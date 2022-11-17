use {
  crate::cli::CliOptions,
  anoma_network as network,
  clap::Parser,
  futures::StreamExt,
  network::Network,
  std::time::Duration,
  tracing::info,
  tracing_subscriber::FmtSubscriber,
};

mod cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  tracing::subscriber::set_global_default(FmtSubscriber::new())?;

  let opts = CliOptions::parse();
  info!("Solver options: {opts:?}");

  let mut network = Network::default();

  // This topic is used for listening on
  // partial transactions (intents) that could potentially
  // be solved by this instance of the solver and turned into
  // complete transactions.
  let intents_topic = network.join(network::topic::Config {
    name: "/testnet-1/intents".into(),
    bootstrap: opts.peers(),
  })?;

  intents_topic.gossip(vec![1u8, 2, 3].into());

  tokio::spawn(async move {
    let mut intents_topic = intents_topic;
    while let Some(e) = intents_topic.next().await {
      info!("intents: {e:?}");
    }
  });

  // This topic is used to publish full (solved) transactions
  // to validators. It also is used to listen on transactions
  // published by other solvers to discard intents solved by
  // other solvers from the mempool.
  let transactions_topic = network.join(network::topic::Config {
    name: "/testnet-1/transactions".into(),
    bootstrap: opts.peers(),
  })?;

  transactions_topic.gossip(vec![4u8, 5, 6].into());

  tokio::spawn(async move {
    let mut transactions_topic = transactions_topic;
    while let Some(e) = transactions_topic.next().await {
      info!("transactions: {e:?}");
    }
  });

  // run the network runloop in the background.
  tokio::spawn(network.runloop());

  // kill process after 60 seconds.
  tokio::time::sleep(Duration::from_secs(60)).await;
  Ok(())
}
