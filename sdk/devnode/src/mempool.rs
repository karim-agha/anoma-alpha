use {
  anoma_sdk::BlockStateBuilder,
  anoma_primitives::{Block, Transaction},
};

pub struct Mempool<'s> {
  txs: Vec<Transaction>,
  blocks: BlockStateBuilder<'s>,
}

impl<'s> Mempool<'s> {
  pub fn new(block_consumer: BlockStateBuilder<'s>) -> Self {
    Self {
      txs: vec![],
      blocks: block_consumer,
    }
  }

  pub fn consume(&mut self, tx: Transaction) {
    self.txs.push(tx);
  }

  pub fn produce(&mut self) -> Block {
    let txs = std::mem::take(&mut self.txs);
    let parent = self.blocks.last();
    let block = Block::new(parent, txs);
    self
      .blocks
      .consume(block.clone())
      .expect("height and parent verified here");

    block
  }
}
