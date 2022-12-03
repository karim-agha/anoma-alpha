use {
  crate::State,
  anoma_primitives::{Param, Transaction},
  thiserror::Error,
};

#[derive(Debug, Error)]
pub enum Error {}

pub fn evaluate(
  _bytecode: &[u8],
  _params: &[Param],
  _tx: Transaction,
  _state: &dyn State,
) -> Result<bool, Error> {
  todo!()
}
