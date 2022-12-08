use {
  crate::{State, StateDiff},
  anoma_primitives::{Expanded, Predicate, Transaction, Trigger},
  thiserror::Error,
};

#[derive(Debug, Error)]
pub enum Error {}

pub fn execute(_transaction: Transaction, _state: &impl State) -> Result<StateDiff, Error> {
  todo!()
}

fn _invoke(
  _predicate: Predicate<Expanded>,
  _trigger: Trigger,
  _tx: Transaction<Expanded>,
  _state: &dyn State,
) -> Result<bool, Error> {
  todo!()
}
