use {
  clap::Parser,
  multiaddr::{Multiaddr, Protocol},
  std::{collections::HashSet, net::SocketAddr},
};

#[derive(Debug, Parser)]
pub struct CliOptions {
  #[clap(
    long,
    help = "address of a known peer to bootstrap p2p networking from"
  )]
  peer: Vec<SocketAddr>,

  #[clap(long, short)]
  secret: String,

  #[clap(long, short)]
  genesis: String,
}

impl CliOptions {
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
}
