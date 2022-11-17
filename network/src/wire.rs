//! Defines the wire binary protocol messages structure for p2p communication
//! This protocol implements the following work:
//! https://asc.di.fct.unl.pt/~jleitao/pdf/dsn07-leitao.pdf
//! by Joao Leitao el at.

use {
  bytes::Bytes,
  libp2p::{Multiaddr, PeerId},
  serde::{Deserialize, Serialize},
  std::collections::HashSet,
};

/// Represents a member of the p2p network
/// with a list of all known physical addresses that
/// can be used to reach it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressablePeer {
  /// libp2p encoded version of node [`Address`]
  pub peer_id: PeerId,

  /// All known physical address that can be used
  /// to reach this peer. Not all of them will be
  /// accessible from all locations, so the protocol
  /// will try to connecto to any of the addresses listed here.
  pub addresses: HashSet<Multiaddr>,
}

impl Eq for AddressablePeer {}
impl PartialEq for AddressablePeer {
  fn eq(&self, other: &Self) -> bool {
    self.peer_id == other.peer_id
  }
}

impl From<PeerId> for AddressablePeer {
  fn from(value: PeerId) -> Self {
    AddressablePeer {
      peer_id: value,
      addresses: [].into_iter().collect(),
    }
  }
}

impl std::hash::Hash for AddressablePeer {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.peer_id.hash(state);
  }
}

/// Message sent to a bootstrap node to initiate network join
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Join {
  /// Identity and address of the local node that is trying
  /// to join the p2p network.
  pub node: AddressablePeer,
}

/// Message forwarded to active peers of the bootstrap node.
///
/// Nodes that receive this message will attempt to establish
/// an active connection with the node initiating the JOIN
/// procedure. They will send a [`Neighbor`] message to
/// the joining node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardJoin {
  /// Hop counter. Incremented with every network hop.
  hop: u16,

  /// Identity and address of the local node that is trying
  /// to join the p2p network.
  node: AddressablePeer,
}

/// Sent as a response to JOIN, FORWARDJOIN to the initating node,
/// or if a node is being moved from passive to active view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Neighbor {
  /// Identity and address of the peer that is attempting
  /// to add this local node to its active view.
  pub peer: AddressablePeer,

  /// High-priority NEIGHBOR requests are sent iff the sender
  /// has zero peers in their active view.
  pub high_priority: bool,
}

/// This message is sent periodically by a subset of
/// peers to propagate info about peers known to them
/// to other peers in the network.
///
/// This message is forwarded for up to N hops.
/// N is configurable in [`network::Config`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shuffle {
  /// Hop counter. Incremented with every network hop.
  pub hop: u16,

  /// Identity and addresses of the node initiating the shuffle.
  pub origin: AddressablePeer,

  /// A sample of known peers to the shuffle originator.
  pub peers: Vec<AddressablePeer>,
}

/// Sent as a response to SHUFFLE to the shuffle originator.
///
/// Exchanges deduplicated entries about peers known to this
/// local node. This reply is sent by every node that receives
/// the shuffle message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShuffleReply {
  /// A sample of known peers to the local node minus all
  /// nodes listed in the SHUFFLE message.
  pub peers: Vec<AddressablePeer>,
}

/// Instructs a peer to end an active connection with the local node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Disconnect {
  /// If the disconnect is graceful (no protocol violation or network error)
  /// then it is simply moved from the active view to the passive view.
  /// Otherwise the peer is removed from both active and passive views.
  pub graceful: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
  Join(Join),
  ForwardJoin(ForwardJoin),
  Neighbor(Neighbor),
  Shuffle(Shuffle),
  ShuffleReply(ShuffleReply),
  Disconnect(Disconnect),
  Gossip(u128, Bytes),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
  pub topic: String,
  pub action: Action,
}
