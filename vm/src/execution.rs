#![allow(clippy::result_large_err)]

use {
  crate::{collect, State, StateDiff},
  anoma_primitives::{
    Address,
    Expanded,
    Predicate,
    PredicateContext,
    PredicateTree,
    Transaction,
  },
  multihash::{Code, MultihashDigest},
  rayon::prelude::*,
  rmp_serde::{encode, to_vec},
  std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
  thiserror::Error,
  wasmer::{
    imports,
    CompileError,
    Cranelift,
    ExportError,
    Function,
    FunctionEnv,
    FunctionEnvMut,
    Imports,
    Instance,
    InstantiationError,
    Memory,
    MemoryAccessError,
    MemoryError,
    MemoryType,
    Module,
    RuntimeError,
    Store,
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

  #[error("WASM memory allocation error: {0}")]
  Memory(#[from] MemoryError),

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
  cache: &impl State,
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
  match parallel_invoke_predicates(&context, combined, cache) {
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
  cache: &impl State,
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

        match invoke(&context, &pred, cache) {
          Ok(true) => Ok(pred),
          Ok(false) => Err(Error::Rejected(pred)),
          Err(e) => {
            // on predicate crash, cancel everything
            cancelled.store(true, Ordering::Release);
            println!("Predicate error: {e:?}, {pred:?}");
            Err(e)
          },
        }
      }).reduce(|p| p, not, and, or).map(|_| ())
    })
    .reduce_with(and) // top-level preds
    .unwrap_or(Ok(()))
}

fn invoke(
  context: &[u8],
  predicate: &Predicate<Expanded>,
  cache: &impl State,
) -> Result<bool, Error> {
  let compiler = Cranelift::default();
  let mut store = Store::new(compiler);

  let codehash = Code::Sha3_256.digest(&predicate.code.code);
  let cachekey = Address::new(format!(
    "/predcache/{}",
    bs58::encode(codehash.to_bytes()).into_string()
  ))
  .expect("format validate at compile time");
  let module = match cache.get(&cachekey) {
    Some(val) => match unsafe { Module::deserialize(&store, val.state) } {
      Ok(module) => module,
      Err(_) => Module::from_binary(&store, &predicate.code.code)?,
    },
    None => Module::from_binary(&store, &predicate.code.code)?,
  };

  let memory = Memory::new(&mut store, MemoryType::new(32, None, false))?;
  let imports = syscalls(&mut store, &memory);
  let instance = Instance::new(&mut store, &module, &imports)?;

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
    Err(Error::Rejected(mut p)) => {
      let mut not: String = "not(".into();
      not.push_str(&p.code.entrypoint);
      not.push(')');
      p.code.entrypoint = not;
      Ok(p)
    }
    Err(e) => Err(e),
  }
}

fn and<T>(a: Result<T, Error>, b: Result<T, Error>) -> Result<T, Error> {
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

fn or<T>(a: Result<T, Error>, b: Result<T, Error>) -> Result<T, Error> {
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

fn syscalls(store: &mut Store, memory: &Memory) -> Imports {
  let env = FunctionEnv::new(store, memory.clone());

  imports! {
    "env" => {
      "memory" => memory.clone(),
      "syscall_debug_log" => Function::new_typed_with_env(store, &env, debug_log)
    }
  }
}

#[cfg(debug_assertions)]
fn debug_log(env: FunctionEnvMut<Memory>, ptr: u32, len: u32) {
  use wasmer::AsStoreRef;
  let mut buffer = vec![0u8; len as usize];
  env
    .data()
    .view(&env.as_store_ref())
    .read(ptr as u64, &mut buffer)
    .expect("SDK debug log function is not packing the message correctly");
  let message: String = rmp_serde::from_slice(&buffer)
    .expect("SDK debug log function is not packing the message correctly");
  println!("VM debug log: {message}");
}

#[cfg(not(debug_assertions))]
fn debug_log(_: FunctionEnvMut<Memory>, _: u32, _: u32) {
  // noop in non-debug builds
}
