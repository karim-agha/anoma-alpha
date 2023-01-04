use {
  anoma_network::multiaddr::{Multiaddr, Protocol},
  clap::Parser,
  std::{collections::HashSet, net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr}},
};

#[derive(Debug, Parser)]
pub struct SystemSettings {
  /// Network identifier
  #[clap(short, long, default_value = "localnet", value_name = "IDENTIFIER")]
  network_id: String,

  /// TCP port for accepting p2p connections
  #[clap(long, short, default_value = "44667", value_name = "PORT")]
  p2p_port: u16,

  /// IP addresses for accepting p2p and RPC connections
  #[clap(long, short, 
    value_name = "ADDRESS",
    default_values_t = vec![
      IpAddr::V4(Ipv4Addr::UNSPECIFIED), 
      IpAddr::V6(Ipv6Addr::UNSPECIFIED)])]
  ip: Vec<IpAddr>,

  /// Address of a known peer to bootstrap p2p networking from
  #[clap(long)]
  peer: Vec<SocketAddr>,
}

impl SystemSettings {
  pub fn network_id(&self) -> &str {
    &self.network_id
  }
  
  /// Those peers are used as first bootstrap nodes to join
  /// the p2p gossip network. At the moment all topics use the
  /// same bootstrap peers, although the network API allows for
  /// per-topic peer sets.
  pub fn peers(&self) -> HashSet<Multiaddr> {
    self
      .peer
      .iter()
      .map(|addr| {
        let mut maddr = Multiaddr::empty();
        maddr.push(match *addr {
          SocketAddr::V4(addr) => Protocol::Ip4(*addr.ip()),
          SocketAddr::V6(addr) => Protocol::Ip6(*addr.ip()),
        });
        maddr.push(Protocol::Tcp(addr.port()));
        maddr
      })
      .collect()
  }

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
}
