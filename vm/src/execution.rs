use {
  crate::{collect, State, StateDiff},
  anoma_primitives::{
    Expanded,
    Predicate,
    PredicateContext,
    PredicateTree,
    Transaction,
  },
  rayon::{join, prelude::*},
  std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
  thiserror::Error,
};

#[derive(Debug, Clone, Error)]
pub enum Error {
  #[error("State access error: {0}")]
  State(#[from] collect::Error),

  #[error("Rejected by predicate {0:?}")]
  Rejected(Predicate<Expanded>),

  #[error("Predicate evaluation cancelled by other failed predicates")]
  Cancelled,
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

  println!("output: {state_diff:?}");
  println!("account_preds: {account_preds:?}");
  println!("intent_preds: {intent_preds:?}");
  println!("context: {context:?}");

  // run both sets of predicates in parallel
  // and parallelize execution within each set
  let (account_result, intents_result) = join(
    || parallel_invoke_predicates(&context, &account_preds),
    || parallel_invoke_predicates(&context, &intent_preds),
  );

  // both sides must evaluate successfully
  // otherwise return the error that caused
  // evaluation to fail.
  match (account_result, intents_result) {
    (Ok(()), Ok(())) => Ok(state_diff),
    (Err(e), _) => Err(e),
    (_, Err(e)) => Err(e),
  }
}

/// Runs a set of predicates in parallel and returns Ok(()) if all of
/// them successfully ran to completion and returned true.
///
/// Otherwise if any predicate fails (returns false or crashes), then
/// all other predicate will be cancelled and the reason
/// for the failure will be returned.
fn parallel_invoke_predicates(
  context: &PredicateContext,
  predicates: &[PredicateTree<Expanded>],
) -> Result<(), Error> {
  let cancelled = Arc::new(AtomicBool::new(false));
  predicates
    .into_par_iter()
    .map(|tree| {
      if cancelled.load(Ordering::Acquire) {
        return Err(Error::Cancelled);
      }

      let mut output = Ok(());
      tree.for_each(&mut |pred| {
        if !cancelled.load(Ordering::Acquire) {
          let result = match invoke(pred, context) {
            Ok(true) => Ok(()),
            Ok(false) => Err(Error::Rejected(pred.clone())),
            Err(e) => Err(e),
          };

          if let Err(e) = result {
            // on first error cancel evaluating all
            // remaining predicates in the predicates set,
            // and store the reason why evaluation failed
            if let Ok(true) = cancelled.compare_exchange(
              false,
              true,
              Ordering::Release,
              Ordering::Acquire,
            ) {
              output = Err(e);
            }
          }
        }
      });

      output
    })
    .reduce_with(|prev, current| match (prev, current) {
      (Ok(_), Ok(_)) => Ok(()),
      (Err(e), Ok(_)) => Err(e),
      (Ok(_), Err(e)) => Err(e),
      (Err(Error::Cancelled), Err(e)) => Err(e), // skip cancelled
      (Err(e), Err(Error::Cancelled)) => Err(e), // skip cancelled
      (Err(e1), Err(_)) => Err(e1),              /* randomy pick one of the
                                                   * two failures :-) */
    })
    .unwrap_or(Ok(())) // no predicates
}

fn invoke(
  _predicate: &Predicate<Expanded>,
  _context: &PredicateContext,
) -> Result<bool, Error> {
  todo!()
}
