use {
  crate::settings::SystemSettings,
  anoma_network as network,
  anoma_primitives::Block,
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

// (locks, intents) topic handles
fn start_network(settings: &SystemSettings) -> anyhow::Result<(Topic, Topic)> {
  let mut network = Network::new(
    Config {
      listen_addrs: settings.p2p_addrs(),
      ..Default::default()
    },
    Keypair::generate_ed25519(),
  )?;

  let blocks_topic = network.join(topic::Config {
    name: format!("/{}/blocks", settings.network_id()),
    bootstrap: settings.peers(),
  })?;

  let intents_topic = network.join(topic::Config {
    name: format!("/{}/intents", settings.network_id()),
    bootstrap: Default::default(),
  })?;

  tokio::spawn(network.runloop());
  Ok((blocks_topic, intents_topic))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  tracing_subscriber::fmt::init();

  let opts = SystemSettings::parse();
  info!("Client options: {opts:?}");

  let (blocks, _intents_topic) = start_network(&opts)?;
  let mut blocks = blocks.map(|bytes| from_slice::<Block>(&bytes));

  loop {
    tokio::select! {
      Some(Ok(block)) = blocks.next() => {
        info!("received a block: {block:#?}");
      }
    }
  }
}
