use {
  crate::{mempool::Mempool, settings::SystemSettings},
  anoma_network::{
    topic::{self, Topic},
    Config,
    Keypair,
    Network,
  },
  anoma_primitives::Block,
  anoma_sdk::BlockStateBuilder,
  anoma_vm::InMemoryStateStore,
  clap::Parser,
  futures::StreamExt,
  rmp_serde::{from_slice, to_vec},
  std::num::NonZeroUsize,
  tokio::time::{interval, MissedTickBehavior},
  tracing::{info, warn},
};

mod mempool;
mod settings;

// (transactions, blocks) topic handles
fn start_network(settings: &SystemSettings) -> anyhow::Result<(Topic, Topic)> {
  let mut network = Network::new(
    Config {
      listen_addrs: settings.p2p_addrs(),
      ..Default::default()
    },
    Keypair::generate_ed25519(),
  )?;

  let txs_topic = network.join(topic::Config {
    name: format!("/{}/transactions", settings.network_id()),
    bootstrap: Default::default(),
  })?;

  let blocks_topic = network.join(topic::Config {
    name: format!("/{}/blocks", settings.network_id()),
    bootstrap: Default::default(),
  })?;

  tokio::spawn(network.runloop());
  Ok((txs_topic, blocks_topic))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  tracing_subscriber::fmt::init();

  // gather CLI parameters
  let settings = SystemSettings::parse();
  info!("startup settings: {settings:#?}");

  // start network and get topic handles for txs and blocks
  let (mut txs_topic, blocks_topic) = start_network(&settings)?;

  // start time-based block production trigger
  let mut interval = interval(settings.block_time());
  interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

  let history_length = NonZeroUsize::new(64).expect("compile time constant");
  let mut code_cache = InMemoryStateStore::default();
  let mut state_store = InMemoryStateStore::default();
  let mut mempool = Mempool::new(BlockStateBuilder::new(
    history_length,
    &mut state_store,
    &mut code_cache,
    std::iter::once(Block::zero()),
  )?);

  loop {
    tokio::select! {
      Some(tx) = txs_topic.next() => {
        if let Ok(tx) = from_slice(&tx) {
          mempool.consume(tx);
        }
      }
      _ = interval.tick() => {
        let block = mempool.produce();
        info!("produced block {} (#{}) on top of {} with {} transactions.",
          bs58::encode(&block.hash().to_bytes()).into_string(),
          block.height,
          bs58::encode(&block.parent.to_bytes()).into_string(),
          block.transactions.len());

        // broadcast through p2p to all other nodes
        if let Err(e) = blocks_topic.gossip(to_vec(&block)?) {
          warn!("failed to gossip block: {e:?}");
        }
      }
    }
  }
}
