use {
  crate::{mempool::Mempool, settings::SystemSettings},
  anoma_network::{topic, topic::Topic, Config, Network},
  anoma_vm::{InMemoryStateStore, State, StateDiff},
  clap::Parser,
  futures::StreamExt,
  rmp_serde::{from_slice, to_vec},
  tokio::time::{interval, MissedTickBehavior},
  tracing::{info, subscriber::set_global_default},
  tracing_subscriber::FmtSubscriber,
};

mod block;
mod mempool;
mod settings;
mod storage;

// (transactions, blocks) topic handles
fn start_network(settings: &SystemSettings) -> anyhow::Result<(Topic, Topic)> {
  let mut network = Network::new(
    Config {
      listen_addrs: settings.p2p_addrs(),
      ..Default::default()
    },
    anoma_network::Keypair::generate_ed25519(),
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

fn precompile_predicates(_diff: &StateDiff) -> StateDiff {
  StateDiff::default()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  // configure logging
  set_global_default(FmtSubscriber::new())?;

  // gather CLI parameters
  let settings = SystemSettings::parse();
  info!("startup settings: {settings:#?}");

  // get an instance of state store, it can be either
  // an in-memory ephemeral storage if no data directory
  // is provided by cli or persistent on-disk store otherwise.
  let mut state_store = settings.storage()?;

  // stores precompiled predicates
  let mut code_cache = InMemoryStateStore::default();

  // start network and get topic handles for txs and blocks
  let (mut txs_topic, blocks_topic) = start_network(&settings)?;

  let mut mempool = Mempool::default();
  let mut interval = interval(settings.block_time());
  interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

  loop {
    tokio::select! {
      Some(tx) = txs_topic.next() => {
        if let Ok(tx) = from_slice(&tx) {
          mempool.consume(tx);
        }
      }
      _ = interval.tick() => {
        let (block, statediff) = mempool.produce(&*state_store, &code_cache);
        info!("produced block with {} transactions and {} account mutations",
          block.transactions.len(), statediff.iter().count());
        code_cache.apply(precompile_predicates(&statediff));
        state_store.apply(statediff);
        blocks_topic.gossip(to_vec(&block)?);
      }
    }
  }
}
