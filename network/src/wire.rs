//! Defines the wire binary protocol messages structure for p2p communication
//! This protocol implements the following work:
//! https://asc.di.fct.unl.pt/~jleitao/pdf/dsn07-leitao.pdf
//! by Joao Leitao el at.

use {
  libp2p::{Multiaddr, PeerId},
  serde::{Deserialize, Serialize},
};

/// Represents a member of the p2p network
/// with a list of all known physical addresses that
/// can be used to reach it.
#[derive(Debug, Serialize, Deserialize)]
pub struct AddressablePeer {
  /// libp2p encoded version of node [`Address`]
  pub peer_id: PeerId,

  /// All known physical address that can be used
  /// to reach this peer. Not all of them will be
  /// accessible from all locations, so the protocol
  /// will try to connecto to any of the addresses listed here.
  pub addresses: Vec<Multiaddr>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Join {}

#[derive(Debug, Serialize, Deserialize)]
pub struct ForwardJoin {}

#[derive(Debug, Serialize, Deserialize)]
pub struct Neighbor {}

#[derive(Debug, Serialize, Deserialize)]
pub struct Shuffle {}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShuffleReply {}

#[derive(Debug, Serialize, Deserialize)]
pub struct Disconnect {}

#[derive(Debug, Serialize, Deserialize)]
pub enum Command {
  Join(Join),
  ForwardJoin(ForwardJoin),
  Neighbor(Neighbor),
  Shuffle(Shuffle),
  ShuffleReply(ShuffleReply),
  Disconnect(Disconnect),
}
