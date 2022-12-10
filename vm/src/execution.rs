#![allow(clippy::result_large_err)]

use {
  crate::{collect, limits::LimitingTunables, State, StateDiff},
  anoma_primitives::{
    Expanded,
    Predicate,
    PredicateContext,
    PredicateTree,
    Transaction,
  },
  rayon::prelude::*,
  rmp_serde::{encode, to_vec},
  std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
  thiserror::Error,
  wasmer::{
    imports,
    BaseTunables,
    CompileError,
    Cranelift,
    ExportError,
    Instance,
    InstantiationError,
    MemoryAccessError,
    Module,
    Pages,
    RuntimeError,
    Store,
    Target,
    TypedFunction,
    WasmPtr,
  },
};

#[derive(Debug, Error)]
pub enum Error {
  #[error("State access error: {0}")]
  State(#[from] collect::Error),

  #[error("Rejected by predicate {0:?}")]
  Rejected(Predicate<Expanded>),

  #[error("Predicate evaluation cancelled by other failed predicates")]
  Cancelled,

  #[error("WASM bytecode compilation error: {0}")]
  Compile(#[from] CompileError),

  #[error("WASM instantitation error: {0}")]
  Instantiation(#[from] InstantiationError),

  #[error("WASM export error: {0}")]
  Export(#[from] ExportError),

  #[error("Failed to serialize context for predicate: {0}")]
  Encoding(#[from] encode::Error),

  #[error("WASM execution error: {0}")]
  Execution(#[from] RuntimeError),

  #[error("WASM memory access error: {0}")]
  MemoryAccess(#[from] MemoryAccessError),

  #[error("WASM predicate returned an unexpected value: {0}")]
  InvalidReturnValue(u32),
}

/// Executes a transaction
///
/// This function will identify all nessesary predicates that need
/// to be executed for this transaction, then execute them for the
/// current blockchain state and the proposed values and returns
/// a StateDiff object that can be applied to global blockchain
/// state if all predicates evaluate to true.
pub fn execute(
  tx: Transaction,
  state: &impl State,
) -> Result<StateDiff, Error> {
  // those changes will be applied if all predicates
  // evaluate to true in intents and mutated accounts.
  // the resulting type is a StateDiff that is ready
  // to be applied to global replicated blockchain
  // state.
  let state_diff = collect::outputs(state, &tx)?;

  // This context object is passed to every account and intent predicate
  // during evaluation stage. It contains all account mutations proposed
  // by the transaction and all calldata attached to intents.
  let context = collect::predicate_context(state, &tx)?;

  // Those are predicates of accounts that are mutated by this
  // transaction. They include immediate predicates of the mutated
  // accounts and all their parent accounts. For each mutated account
  // all its and its ancestor accounts predicates must evaluate to
  // true before a mutation is accepted into the global blockchain state.
  let account_preds = collect::accounts_predicates(state, &context, &tx)?;

  // Those are predicates of all intents in the transaction. They all must
  // evaluate to true for a transaction before any account mutations are
  // allowed.
  let intent_preds = collect::intents_predicates(state, &context, tx)?;

  // merge both sets of predicates into one parallel iterator
  let combined = account_preds
    .into_par_iter()
    .chain(intent_preds.into_par_iter());

  // on success return the resulting state diff of this tx
  match parallel_invoke_predicates(&context, combined) {
    Ok(()) => Ok(state_diff),
    Err(e) => Err(e),
  }
}

/// Runs a set of predicates in parallel and returns Ok(()) if all of
/// them successfully ran to completion and returned true.
///
/// Otherwise if any predicate crashes, then all other predicate will
/// be cancelled and the reason for the failure will be returned.
fn parallel_invoke_predicates(
  context: &PredicateContext,
  predicates: impl ParallelIterator<Item = PredicateTree<Expanded>>,
) -> Result<(), Error> {
  let context = to_vec(&context)?;
  let cancelled = Arc::new(AtomicBool::new(false));

  predicates
    .into_par_iter()
    .map(|tree| {
      if cancelled.load(Ordering::Acquire) {
        return Err(Error::Cancelled);
      }

      tree.map(|pred| {
        if cancelled.load(Ordering::Acquire) {
          return Err(Error::Cancelled);
        }

        match invoke(&context, &pred) {
          Ok(true) => Ok(pred),
          Ok(false) => Err(Error::Rejected(pred)),
          Err(e) => {
            // on predicate crash, cancel everything
            cancelled.store(true, Ordering::Release);
            Err(e)
          },
        }
      }).reduce(|p| p, not, and, or).map(|_| ())
    })
    .reduce_with(|a, b| match (a, b) { // && all top-levl predicates
      (Ok(_), Ok(_)) => Ok(()),
      (Err(e), Ok(_)) => Err(e),
      (Ok(_), Err(e)) => Err(e),
      (Err(Error::Cancelled), Err(e)) => Err(e), // skip cancelled
      (Err(e), Err(Error::Cancelled)) => Err(e), // skip cancelled
      (Err(e1), Err(_)) => Err(e1),              // randomy pick one :-)
    })
    // this case happens when creating a new account
    // that has no predicates attached to any of its
    // ancestors, then there are no account predicates
    // gating this write.
    .unwrap_or(Ok(()))
}

fn invoke(
  context: &[u8],
  predicate: &Predicate<Expanded>,
) -> Result<bool, Error> {
  let compiler = Cranelift::default();
  let mut store = Store::new_with_tunables(
    compiler,
    LimitingTunables::new(
      BaseTunables::for_target(&Target::default()),
      Pages(100),
    ),
  );

  let imports = imports! {};
  let module = Module::from_binary(&store, &predicate.code.code)?;
  let instance = Instance::new(&mut store, &module, &imports)?;
  let memory = instance.exports.get_memory("memory")?;

  let allocate_fn = instance
    .exports
    .get_typed_function::<u32, WasmPtr<u8>>(&store, "__allocate")?;

  let context_fn = instance
    .exports
    .get_typed_function::<(WasmPtr<u8>, u32), WasmPtr<u8>>(
      &store,
      "__ingest_context",
    )?;

  let params_fn = instance
    .exports
    .get_typed_function::<(WasmPtr<u8>, u32), WasmPtr<u8>>(
      &store,
      "__ingest_params",
    )?;

  let entrypoint_fn = instance
    .exports
    .get_typed_function::<(WasmPtr<u8>, WasmPtr<u8>), u32>(
      &store,
      &predicate.code.entrypoint,
    )?;

  let mut deliver_data =
    |data: &[u8],
     ingest_fn: TypedFunction<(WasmPtr<u8>, u32), WasmPtr<u8>>|
     -> Result<WasmPtr<u8>, RuntimeError> {
      let data_len = data.len() as u32;
      let raw_ptr = allocate_fn.call(&mut store, data_len)?;
      let ptr_offset = raw_ptr.offset() as u64;
      // copy data to wasm instance memory
      memory.view(&store).write(ptr_offset, data)?;

      // instantiate object in sdk-specific object model
      ingest_fn.call(&mut store, raw_ptr, data_len)
    };

  let context_ptr = deliver_data(context, context_fn)?;
  let params_ptr = deliver_data(&to_vec(&predicate.params)?, params_fn)?;

  match entrypoint_fn.call(&mut store, params_ptr, context_ptr)? {
    0 => Ok(false),
    1 => Ok(true),
    r => Err(Error::InvalidReturnValue(r)),
  }
}

fn not(
  val: Result<Predicate<Expanded>, Error>,
) -> Result<Predicate<Expanded>, Error> {
  match val {
    Ok(mut p) => {
      let mut not: String = "not(".into();
      not.push_str(&p.code.entrypoint);
      not.push(')');
      p.code.entrypoint = not;
      Err(Error::Rejected(p))
    }
    Err(Error::Rejected(p)) => Ok(p),
    Err(e) => Err(e),
  }
}

fn and(
  a: Result<Predicate<Expanded>, Error>,
  b: Result<Predicate<Expanded>, Error>,
) -> Result<Predicate<Expanded>, Error> {
  match (a, b) {
    (Ok(p), Ok(_)) => Ok(p),
    (Ok(_), Err(Error::Rejected(p))) => Err(Error::Rejected(p)),
    (Err(Error::Rejected(p)), Ok(_)) => Err(Error::Rejected(p)),
    (Err(Error::Cancelled), Err(e)) => Err(e),
    (Err(e), Err(Error::Cancelled)) => Err(e),
    (Err(e), _) => Err(e),
    (_, Err(e)) => Err(e),
  }
}

fn or(
  a: Result<Predicate<Expanded>, Error>,
  b: Result<Predicate<Expanded>, Error>,
) -> Result<Predicate<Expanded>, Error> {
  match (a, b) {
    (Ok(p), Ok(_)) => Ok(p),
    (Ok(p), Err(Error::Rejected(_))) => Ok(p),
    (Err(Error::Rejected(_)), Ok(p)) => Ok(p),
    (Err(Error::Cancelled), Err(e)) => Err(e),
    (Err(e), Err(Error::Cancelled)) => Err(e),
    (Err(e), _) => Err(e),
    (_, Err(e)) => Err(e),
  }
}
