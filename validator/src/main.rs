use {
  crate::cli::CliOptions,
  anoma_network as network,
  clap::Parser,
  network::Network,
  tracing::info,
  tracing_subscriber::FmtSubscriber,
};

mod cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  tracing::subscriber::set_global_default(FmtSubscriber::new())?;

  let opts = CliOptions::parse();
  info!("Validator options: {opts:?}");

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

  // This topic is used to publish full (solved) transactions
  // to validators. It also is used to listen on transactions
  // published by other solvers to discard intents solved by
  // other solvers from the mempool.
  let transactions_topic = network.join(network::topic::Config {
    name: "/testnet-1/transactions".into(),
    bootstrap: opts.peers(),
  })?;

  transactions_topic.gossip(vec![4u8, 5, 6].into());

  // run the network runloop in the background forever.
  tokio::spawn(network.runloop()).await?;
  Ok(())
}
