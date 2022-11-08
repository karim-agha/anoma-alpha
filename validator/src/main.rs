use {anoma_network as network, network::Network};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  let _network = Network::new(network::Config::default());

  Ok(())
}
