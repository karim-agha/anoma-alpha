use {
  anoma_primitives::{Block, Transaction},
  anoma_vm::{execute_many, State, StateDiff},
};

#[derive(Default)]
pub struct Mempool {
  txs: Vec<Transaction>,
}

impl Mempool {
  pub fn consume(&mut self, tx: Transaction) {
    self.txs.push(tx);
  }

  pub fn produce(
    &mut self,
    state: &dyn State,
    cache: &dyn State,
    parent: &Block,
  ) -> (Block, StateDiff) {
    let txs = std::mem::take(&mut self.txs);
    let block = Block::new(parent, txs.clone());
    let results = execute_many(state, cache, txs.into_iter());
    let statediff = results
      .into_iter()
      .filter_map(|res| res.ok())
      .reduce(|acc, e| acc.merge(e))
      .unwrap_or_default();

    (block, statediff)
  }
}
