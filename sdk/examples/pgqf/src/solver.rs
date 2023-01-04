use {
  crate::settings::SystemSettings,
  anoma_network as network,
  anoma_primitives::{Block, Intent},
  clap::Parser,
  futures::StreamExt,
  network::{
    topic::{self, Topic},
    Config,
    Keypair,
    Network,
  },
  rmp_serde::from_slice,
  tracing::info,
};

mod settings;

// (transactions, blocks, intents) topic handles
fn start_network(
  settings: &SystemSettings,
) -> anyhow::Result<(Topic, Topic, Topic)> {
  let mut network = Network::new(
    Config {
      listen_addrs: settings.p2p_addrs(),
      ..Default::default()
    },
    Keypair::generate_ed25519(),
  )?;

  let txs_topic = network.join(topic::Config {
    name: format!("/{}/transactions", settings.network_id()),
    bootstrap: settings.peers(),
  })?;

  let blocks_topic = network.join(topic::Config {
    name: format!("/{}/blocks", settings.network_id()),
    bootstrap: settings.peers(),
  })?;

  let intents_topic = network.join(topic::Config {
    name: format!("/{}/intents", settings.network_id()),
    bootstrap: Default::default(),
  })?;

  tokio::spawn(network.runloop());
  Ok((txs_topic, blocks_topic, intents_topic))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  tracing_subscriber::fmt::init();

  let opts = SystemSettings::parse();
  info!("Solver options: {opts:?}");

  let (_, blocks, intents) = start_network(&opts)?;

  let mut blocks = blocks.map(|bytes| from_slice::<Block>(&bytes));
  let mut intents = intents.map(|bytes| from_slice::<Intent>(&bytes));

  loop {
    tokio::select! {
      Some(Ok(intent)) = intents.next() => {
        info!("received an intent: {intent:?}");
      }
      Some(Ok(block)) = blocks.next() => {
        info!("received a block: {block:#?}");
      }
    }
  }
}
