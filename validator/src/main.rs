use anoma_network::{Network, NetworkConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  let _network = Network::new(NetworkConfig::default())?;

  Ok(())
}
