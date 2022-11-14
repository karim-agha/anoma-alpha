//! HyParView: a membership protocol for reliable gossip-based broadcast
//! Leitão, João & Pereira, José & Rodrigues, Luís. (2007). 419-429.
//! 10.1109/DSN.2007.56.

use {
  crate::{
    wire::{
      Action,
      AddressablePeer,
      Disconnect,
      ForwardJoin,
      Join,
      Message,
      Neighbor,
      Shuffle,
      ShuffleReply,
    },
    Channel,
    Command,
  },
  bytes::Bytes,
  futures::Stream,
  libp2p::{Multiaddr, PeerId},
  std::{
    collections::HashSet,
    pin::Pin,
    task::{Context, Poll},
  },
  thiserror::Error,
  tokio::sync::mpsc::UnboundedSender,
};

#[derive(Debug, Error)]
pub enum Error {}

#[derive(Debug)]
pub struct Config {
  pub name: String,
  pub bootstrap: Vec<Multiaddr>,
}

#[derive(Debug)]
pub enum Event {
  MessageReceived(PeerId, Message),
  LocalAddressDiscovered(Multiaddr),
  ActivePeerConnected(PeerId),
  ActivePeerDisconnected(PeerId),
}

/// A topic represents an instance of HyparView p2p overlay.
pub struct Topic {
  topic_config: Config,
  network_config: crate::Config,
  this_node: AddressablePeer,
  events: Channel<Event>,
  cmdtx: UnboundedSender<Command>,
  active_peers: HashSet<AddressablePeer>,
  passive_peers: HashSet<AddressablePeer>,
  pending_peers: HashSet<AddressablePeer>,
}

// Public API
impl Topic {
  /// Propagate a message to connected active peers
  pub fn gossip(&self, data: Bytes) {
    for peer in &self.active_peers {
      self
        .cmdtx
        .send(Command::SendMessage {
          peer: peer.peer_id,
          msg: Message {
            topic: self.topic_config.name.clone(),
            action: Action::Gossip(data.clone()),
          },
        })
        .expect("receiver is closed");
    }
  }
}

// internal api
impl Topic {
  pub(crate) fn new(
    topic_config: Config,
    network_config: crate::Config,
    this_node: AddressablePeer,
    cmdtx: UnboundedSender<Command>,
  ) -> Self {
    // dial all bootstrap nodes
    for addr in topic_config.bootstrap.iter() {
      cmdtx
        .send(Command::Connect(addr.clone()))
        .expect("lifetime of network should be longer than topic");
    }

    Self {
      topic_config,
      network_config,
      this_node,
      cmdtx,
      events: Channel::new(),
      active_peers: HashSet::new(),
      passive_peers: HashSet::new(),
      pending_peers: HashSet::new(),
    }
  }

  /// Called when the network layer has a new event for this topic
  pub(crate) fn inject_event(&mut self, event: Event) {
    self.events.send(event);
  }

  /// The active views of all nodes create an overlay that is used for message
  /// dissemination. Links in the overlay are symmetric, this means that each
  /// node keeps an open TCP connection to every other node in its active
  /// view.
  ///
  /// The active view is maintained using a reactive strategy, meaning nodes are
  /// remove when they fail.
  fn active(&self) -> impl Iterator<Item = &AddressablePeer> {
    self.active_peers.iter()
  }

  /// The goal of the passive view is to maintain a list of nodes that can be
  /// used to replace failed members of the active view. The passive view is
  /// not used for message dissemination.
  ///
  /// The passive view is maintained using a cyclic strategy. Periodically, each
  /// node performs shuffle operation with one of its neighbors in order to
  /// update its passive view.
  fn passive(&self) -> impl Iterator<Item = &AddressablePeer> {
    self.passive_peers.iter()
  }

  /// Checks if a peer is already in the active view of this topic.
  /// This is used to check if we need to send JOIN message when
  /// the peer is dialed, peers that are active will not get
  /// a JOIN request, otherwise the network will go into endless
  /// join/forward churn.
  fn is_active(&self, peer: &PeerId) -> bool {
    self.active_peers.contains(&AddressablePeer {
      peer_id: *peer,
      addresses: HashSet::new(),
    })
  }

  /// Overconnected nodes are ones where the active view
  /// has a full set of nodes in it.
  fn overconnected(&self) -> bool {
    self.active_peers.len() >= self.network_config.max_active_view_size()
  }

  /// Starved topics are ones where the active view
  /// doesn't have a minimum set of nodes in it.
  fn starved(&self) -> bool {
    self.active_peers.len() < self.network_config.min_active_view_size()
  }

  /// Initiates a graceful disconnect from an active peer.
  fn disconnect(&self, peer: PeerId) {
    todo!()
  }
}

// HyParView protocol message handlers
impl Topic {
  /// Handles JOIN messages.
  ///
  /// When a JOIN request arrives, the receiving node will create a new
  /// FORWARDJOIN and send it to peers in its active view, which
  /// in turn will forward it to all peers in their active view. The forward
  /// operation will repeat to all further active view for N hops. N is set
  /// in the config object ([`forward_join_hops_count`]).
  ///
  /// Each node receiving JOIN or FORWARDJOIN request will send a NEIGHBOR
  /// request to the node attempting to join the topic overlay if its active
  /// view is not saturated. Except nodes on the last hop, if they are saturated
  /// they will move a random node from the active view to their passive view
  /// and establish an active connection with the initiator.
  fn consume_join(&mut self, sender: PeerId, msg: Join) {
    todo!()
  }

  /// Handles FORWARDJOIN messages.
  ///
  /// Each node receiving FORWARDJOIN checks if its active view is full,
  /// and if there is still space for new nodes, establishes an active
  /// connection with the initiating peer by sending it NEIGHBOR message.
  ///
  /// Then it increments the hop counter on the FORWARDJOIN message and
  /// sends it to all its active peers. This process repeats for N steps
  /// (configured in [`forward_join_hops_count]).
  ///
  /// Nodes on the last hop MUST establish an active view with the initiator,
  /// even if they have to move one of their active connections to passive mode.
  fn consume_forward_join(&mut self, sender: PeerId, msg: ForwardJoin) {
    todo!()
  }

  /// Handles NEIGHBOR messages.
  ///
  /// This message is send when a node wants to establish an active connection
  /// with the receiving node. This message is sent as a response to JOIN and
  /// FORWARDJOIN messages initiated by the peer wanting to join the overlay.
  ///
  /// This message is also sent to nodes that are being moved from passive view
  /// to the active view.
  fn consume_neighbor(&mut self, sender: PeerId, msg: Neighbor) {
    todo!()
  }

  /// Handles DISCONNECT messages.
  ///
  /// Nodes receiving this message are informed that the sender is removing
  /// them from their active view. Which also means that the sender should
  /// also be removed from the receiving node's active view.
  fn consume_disconnect(&mut self, sender: PeerId, msg: Disconnect) {
    todo!()
  }

  /// Handles SHUFFLE messages.
  ///
  /// Every given interval [`Config::shuffle_interval`] a subset of all
  /// nodes ([`Config::shuffle_probability`]) will send a SHUFFLE message
  /// to one randomly chosen peer in its active view and increment the hop
  /// counter.
  ///
  /// Each node that receives a SHUFFLE message will forward it to all its
  /// active peers and increment the hop counter on every hop. When a SHUFFLE
  /// message is received by a peer it adds all unique nodes that are not known
  /// to the peer to its passive view. This is a method of advertising and
  /// discovery of new nodes on the p2p network.
  ///
  /// Each node that receives a SHUFFLE message that replies with SHUFFLEREPLY
  /// with a sample of its own active and passive nodes that were not present
  /// in the SHUFFLE message.
  fn consume_shuffle(&mut self, sender: PeerId, msg: Shuffle) {
    todo!()
  }

  /// Handles SHUFFLEREPLY messages.
  ///
  /// Those messages are sent as responses to SHUFFLE messages to the originator
  /// of the SHUFFLE message. The SHUFFLEREPLY message should contain a sample
  /// of local node's known active and passive peers that were not present in
  /// the received SHUFFLE message.
  fn consume_shuffle_reply(&mut self, sender: PeerId, msg: ShuffleReply) {
    todo!()
  }
}

impl Stream for Topic {
  type Item = Event;

  fn poll_next(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Self::Item>> {
    let pollres = self.events.poll_recv(cx);

    if let Poll::Ready(Some(ref event)) = pollres {
      match event {
        Event::MessageReceived(_, _) => todo!(),
        Event::LocalAddressDiscovered(addr) => {
          self.this_node.addresses.insert(addr.clone());
        }
        Event::ActivePeerConnected(_) => todo!(),
        Event::ActivePeerDisconnected(_) => todo!(),
      }
    }

    pollres
  }
}
