use {
  crate::storage::OnDiskStateStore,
  anoma_network::multiaddr::{Protocol, Multiaddr},
  anoma_vm::{InMemoryStateStore, State},
  clap::Parser,
  humantime::Duration,
  std::{
    path::PathBuf,
    net::SocketAddr,
    net::{IpAddr, Ipv4Addr, Ipv6Addr}
  },
};

/// Anoma Local Devnode
/// 
/// A single node instance of Anoma validator for local use with
/// no consensus algorithm for dev, CI and test scenarios.
#[derive(Debug, Parser)]
pub struct SystemSettings {
  /// Network identifier
  #[clap(short, long, 
    default_value = "localnet", 
    value_name = "IDENTIFIER")]
  network_id: String,

  /// TCP port for accepting p2p connections
  #[clap(long, short, 
    default_value = "44668", 
    value_name = "PORT")]
  p2p_port: u16,

  /// TCP port for accepting HTTP RPC requests
  #[clap(long, short, 
    default_value = "8080", 
    value_name = "PORT")]
  rpc_port: u16,

  /// IP addresses for accepting p2p and RPC connections
  #[clap(long, short, 
    value_name = "ADDRESS",
    default_values_t = vec![
      IpAddr::V4(Ipv4Addr::UNSPECIFIED), 
      IpAddr::V6(Ipv6Addr::UNSPECIFIED)])]
  ip: Vec<IpAddr>,

  /// Block production interval
  #[clap(long, short = 't', 
    value_name = "DURATION",
    default_value = "2s")]
  block_time: Duration,

  /// Optional directory for persistent state storage.
  #[clap(long, short, value_name = "PATH")]
  data_dir: Option<PathBuf>
}

impl SystemSettings {
  pub fn p2p_addrs(&self) -> Vec<Multiaddr> {
    self
      .ip
      .iter()
      .map(|addr| {
        let mut maddr = Multiaddr::empty();
        maddr.push(match *addr {
          IpAddr::V4(addr) => Protocol::Ip4(addr),
          IpAddr::V6(addr) => Protocol::Ip6(addr),
        });
        maddr.push(Protocol::Tcp(self.p2p_port));
        maddr
      })
      .collect()
  }

  pub fn rpc_addrs(&self) -> Vec<SocketAddr> {
    self.ip
      .iter()
      .cloned()
      .map(|ip| SocketAddr::new(ip, self.rpc_port))
      .collect()
  }

  pub fn state_storage(&self) -> anyhow::Result<Box<dyn State>> {
    Ok(match &self.data_dir {
        Some(path) => Box::new(OnDiskStateStore::new(path, "state")?),
        None => Box::new(InMemoryStateStore::default()),
    })
  }

  pub fn blocks_storage(&self) -> anyhow::Result<Box<dyn State>> {
    Ok(match &self.data_dir {
        Some(path) => Box::new(OnDiskStateStore::new(path, "blocks")?),
        None => Box::new(InMemoryStateStore::default()),
    })
  }

  pub fn cache_storage(&self) -> anyhow::Result<Box<dyn State>> {
    Ok(match &self.data_dir {
        Some(path) => Box::new(OnDiskStateStore::new(path, "cache")?),
        None => Box::new(InMemoryStateStore::default()),
    })
  }

  pub fn network_id(&self) -> &str {
    &self.network_id
  }

  pub fn block_time(&self) -> std::time::Duration {
    self.block_time.into()
  }
}
