use {
  crate::{builder, BlockStateBuilder},
  anoma_primitives::{Account, Address, Block, Transaction},
  anoma_vm::State,
  dashmap::DashMap,
  futures::{Stream, StreamExt},
  multihash::Multihash,
  std::{num::NonZeroUsize, sync::Arc},
  tokio::sync::{
    oneshot::{self, error::RecvError, Sender},
    RwLock,
  },
  tracing::error,
};

#[derive(Debug, Hash, PartialEq, Eq)]
enum WatchlistKey {
  Intent(Multihash),
  Transaction(Multihash),
  AccountChange(Address),
}

enum WatchlistValue {
  Intent(Transaction),
  Transaction(Block),
  AccountChange(Transaction),
}

/// This type monitors incoming blocks and accumulates state changes.
/// It allows waiting for a specifc intent, transaction or an account change
/// to be included in a block.
pub struct BlockchainWatcher {
  watchlist: Arc<DashMap<WatchlistKey, Sender<WatchlistValue>>>,
  state_builder: Arc<RwLock<BlockStateBuilder<'static>>>,
}

impl BlockchainWatcher {
  #[allow(clippy::result_large_err)]
  pub fn new(
    history_len: NonZeroUsize,
    state: &'static mut dyn State,
    codecache: &'static mut dyn State,
    recent: impl Iterator<Item = Block>,
    stream: impl Stream<Item = Block> + Unpin + Send + 'static,
  ) -> Result<Self, builder::Error> {
    let watchlist =
      Arc::new(DashMap::<WatchlistKey, Sender<WatchlistValue>>::new());

    let state_builder = Arc::new(RwLock::new(BlockStateBuilder::new(
      history_len,
      state,
      codecache,
      recent,
    )?));

    let watchlist_clone = watchlist.clone();
    let state_builder_clone = state_builder.clone();

    tokio::spawn(async move {
      let mut stream = stream;
      let watchlist = watchlist_clone;
      let state_builder = state_builder_clone;
      while let Some(block) = stream.next().await {
        for tx in block.transactions.iter() {
          let txwatchkey = WatchlistKey::Transaction(*tx.hash());
          if let Some((_, signal)) = watchlist.remove(&txwatchkey) {
            if signal
              .send(WatchlistValue::Transaction(block.clone()))
              .is_err()
            {
              error!(
                "Failed signalling awaited transaction {}",
                bs58::encode(tx.hash().to_bytes()).into_string()
              );
            }
          }

          for intent in tx.intents.iter() {
            let intentwatchkey = WatchlistKey::Intent(*intent.hash());
            if let Some((_, signal)) = watchlist.remove(&intentwatchkey) {
              if signal.send(WatchlistValue::Intent(tx.clone())).is_err() {
                error!(
                  "Failed signalling awaited intent {}",
                  bs58::encode(intent.hash().to_bytes()).into_string()
                );
              }
            }
          }

          for account_change in tx.proposals.iter() {
            let accwatchkey =
              WatchlistKey::AccountChange(account_change.0.clone());
            if let Some((_, signal)) = watchlist.remove(&accwatchkey) {
              if signal
                .send(WatchlistValue::AccountChange(tx.clone()))
                .is_err()
              {
                error!(
                  "Failed signalling awaited account change {}",
                  account_change.0
                );
              }
            }
          }
        }

        if let Err(e) = state_builder.write().await.consume(block) {
          error!("block rejected: {e:?}");
        }
      }
    });

    Ok(Self {
      watchlist,
      state_builder,
    })
  }

  pub async fn get(&self, address: &Address) -> Option<Account> {
    self.state_builder.read().await.get(address)
  }

  pub async fn most_recent_block(&self) -> Block {
    self.state_builder.read().await.last().clone()
  }

  pub async fn await_intent(
    &self,
    hash: Multihash,
  ) -> Result<Transaction, RecvError> {
    let key = WatchlistKey::Intent(hash);
    let (tx, rx) = oneshot::channel();
    self.watchlist.insert(key, tx);
    rx.await.map(|v| match v {
      WatchlistValue::Intent(tx) => tx,
      WatchlistValue::Transaction(_) => {
        panic!("bug in blockchain watcher. Incompatible signal type");
      }
      WatchlistValue::AccountChange(_) => {
        panic!("bug in blockchain watcher. Incompatible signal type");
      }
    })
  }

  pub async fn await_transaction(
    &self,
    hash: Multihash,
  ) -> Result<Block, RecvError> {
    let key = WatchlistKey::Transaction(hash);
    let (tx, rx) = oneshot::channel();
    self.watchlist.insert(key, tx);
    rx.await.map(|v| match v {
      WatchlistValue::Intent(_) => {
        panic!("bug in blockchain watcher. Incompatible signal type");
      }
      WatchlistValue::Transaction(block) => block,
      WatchlistValue::AccountChange(_) => {
        panic!("bug in blockchain watcher. Incompatible signal type");
      }
    })
  }

  pub async fn await_account_change(
    &self,
    address: Address,
  ) -> Result<Transaction, RecvError> {
    let key = WatchlistKey::AccountChange(address);
    let (tx, rx) = oneshot::channel();
    self.watchlist.insert(key, tx);
    rx.await.map(|v| match v {
      WatchlistValue::Intent(_) => {
        panic!("bug in blockchain watcher. Incompatible signal type");
      }
      WatchlistValue::Transaction(_) => {
        panic!("bug in blockchain watcher. Incompatible signal type");
      }
      WatchlistValue::AccountChange(tx) => tx,
    })
  }

  pub async fn await_block_height(&self, height: u64) -> Block {
    todo!()
  }

  pub async fn stop(self) {
    todo!();
  }
}
