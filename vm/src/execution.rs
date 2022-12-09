use {
  crate::{collect, State, StateDiff},
  anoma_primitives::{Expanded, Predicate, Transaction},
  thiserror::Error,
};

#[derive(Debug, Error)]
pub enum Error {
  #[error("State access error: {0}")]
  State(#[from] collect::Error),
}

pub fn execute(
  tx: Transaction,
  state: &impl State,
) -> Result<StateDiff, Error> {
  // those changes will be applied if all predicates
  // evaluate to true in intents and mutated accounts.
  // the resulting type is a StateDiff that is ready
  // to be applied to global replicated blockchain
  // state.
  let output = collect::outputs(state, &tx)?;

  // This context object is passed to every account and intent predicate
  // during evaluation stage. It contains all account mutations proposed
  // by the transaction and all calldata attached to intents.
  let context = collect::predicate_context(state, &tx)?;

  // Those are predicates of accounts that are mutated by this
  // transaction. They include immediate predicates of the mutated
  // accounts and all their parent accounts. For each mutated account
  // all its and its ancestor accounts predicates must evaluate to
  // true before a mutation is accepted into the global blockchain state.
  let account_preds = collect::account_predicates(state, &context, &tx)?;

  // Those are predicates of all intents in the transaction. They all must
  // evaluate to true for a transaction before any account mutations are
  // allowed.
  let intent_preds = collect::intents_predicates(state, &context, tx)?;

  println!("output: {output:?}");
  println!("account_preds: {account_preds:?}");
  println!("intent_preds: {intent_preds:?}");
  println!("context: {context:?}");

  todo!()
}

fn _invoke(
  _predicate: Predicate<Expanded>,
  _tx: Transaction<Expanded>,
) -> Result<bool, Error> {
  todo!()
}
