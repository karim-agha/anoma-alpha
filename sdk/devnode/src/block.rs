use {
  anoma_primitives::Transaction,
  serde::{Deserialize, Serialize},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Block {
  pub transactions: Vec<Transaction>,
}
