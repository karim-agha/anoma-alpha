use {
  crate::{collect, State, StateDiff},
  anoma_primitives::{Expanded, Predicate, Transaction, Trigger},
  thiserror::Error,
};

#[derive(Debug, Error)]
pub enum Error {
  #[error("State access error: {0}")]
  State(#[from] collect::Error),
}

pub fn execute(
  transaction: Transaction,
  state: &impl State,
) -> Result<StateDiff, Error> {
  // those changes will be applied if all predicates
  // evaluate to true in intents and mutated accounts.
  // the resulting type is a StateDiff that is ready
  // to be applied to global replicated blockchain
  // state if all predicates evaluate to true.
  let output = collect::outputs(state, &transaction)?;

  // collect all referenced state into a self-contained
  // object that has everything it needs to execute all
  // transactions. The resulting type is an expanded transaction.
  let expanded = collect::references(state, transaction)?;
  println!("output: {:?}", output);
  println!("expanded: {:?}", expanded);

  todo!()
}

fn _invoke(
  _predicate: Predicate<Expanded>,
  _trigger: Trigger,
  _tx: Transaction<Expanded>,
) -> Result<bool, Error> {
  todo!()
}
