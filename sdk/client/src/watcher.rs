use {
  crate::{builder, BlockStateBuilder},
  anoma_primitives::{Account, Address, Block, Transaction},
  anoma_vm::State,
  dashmap::DashMap,
  futures::Stream,
  multihash::Multihash,
  std::num::NonZeroUsize,
  tokio::sync::oneshot,
};

#[derive(Debug, Hash, PartialEq, Eq)]
enum WatchlistKey {
  Intent(Multihash),
  Transaction(Multihash),
  AccountChange(Address),
}

/// This type monitors incoming blocks and accumulates state changes.
/// It allows waiting for a specifc intent, transaction or an account change
/// to be included in a block.
pub struct BlockchainWatcher<'s> {
  watchlist: DashMap<WatchlistKey, oneshot::Sender<()>>,
  state_builder: BlockStateBuilder<'s>,
}

impl<'s> BlockchainWatcher<'s> {
  #[allow(clippy::result_large_err)]
  pub fn new(
    history_len: NonZeroUsize,
    state: &'s mut dyn State,
    codecache: &'s mut dyn State,
    recent: impl Iterator<Item = Block>,
    _stream: impl Stream<Item = Block>,
  ) -> Result<Self, builder::Error> {
    Ok(Self {
      watchlist: DashMap::new(),
      state_builder: BlockStateBuilder::new(
        history_len,
        state,
        codecache,
        recent,
      )?,
    })
  }

  pub fn get(&self, address: &Address) -> Option<Account> {
    self.state_builder.get(address)
  }
  
  pub fn most_recent_block(&self) -> &Block {
    todo!()
  }

  pub fn consume(&mut self, block: Block) -> Result<(), builder::Error> {
    todo!()
  }

  pub async fn await_intent(&self, hash: Multihash) -> &Transaction {
    todo!()
  }

  pub async fn await_transaction(&self, hash: Multihash) -> &Block {
    todo!()
  }

  pub async fn await_account_change(&self, address: Address) -> &Transaction {
    todo!()
  }

  pub async fn await_block_height(&self, height: u64) -> &Block {
    todo!()
  }

  pub async fn await_next_block(&self) -> &Block {
    todo!()
  }

  pub async fn stop(self) {
    todo!();
  }
}
