//! HyParView: a membership protocol for reliable gossip-based broadcast
//! Leitão, João & Pereira, José & Rodrigues, Luís. (2007). 419-429.
//! 10.1109/DSN.2007.56.

use {
  crate::{
    channel::Channel,
    network::Command,
    wire::{
      Action,
      AddressablePeer,
      Disconnect,
      ForwardJoin,
      Join,
      Message,
      Neighbour,
      Shuffle,
      ShuffleReply,
    },
  },
  bytes::Bytes,
  futures::Stream,
  libp2p::{Multiaddr, PeerId},
  metrics::{gauge, increment_counter},
  parking_lot::RwLock,
  rand::seq::IteratorRandom,
  std::{
    collections::{HashMap, HashSet},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
  },
  thiserror::Error,
  tokio::sync::mpsc::UnboundedSender,
  tracing::{debug, info},
};

#[derive(Debug, Error)]
pub enum Error {}

#[derive(Debug)]
pub struct Config {
  pub name: String,
  pub bootstrap: HashSet<Multiaddr>,
}

#[derive(Debug)]
pub enum Event {
  MessageReceived(PeerId, Message),
  LocalAddressDiscovered(Multiaddr),
  PeerConnected(AddressablePeer),
  PeerDisconnected(PeerId, bool), // (peer, graceful)
}

/// Here the topic implementation lives. It is in an internal
/// struct because the public interface must be Send + Sync so
/// it can be moved across different threads. Access to this
/// type is protected by an RW lock on the outer public type.
struct TopicInner {
  /// Topic specific config.s
  topic_config: Config,

  /// Network wide config.
  network_config: crate::Config,

  /// Cryptographic identity of the current node and all known TCP
  /// addresses by which it can be reached. This list of addreses
  /// is updated everytime the network layer discovers a new one.
  this_node: AddressablePeer,

  /// Events emitted to listeners on new messages received on this topic.
  outmsgs: Channel<Bytes>,

  /// Commands to the network layer
  cmdtx: UnboundedSender<Command>,

  /// The active views of all nodes create an overlay that is used for message
  /// dissemination. Links in the overlay are symmetric, this means that each
  /// node keeps an open TCP connection to every other node in its active
  /// view.
  ///
  /// The active view is maintained using a reactive strategy, meaning nodes
  /// are remove when they fail.
  active_peers: HashMap<PeerId, HashSet<Multiaddr>>,

  /// The goal of the passive view is to maintain a list of nodes that can be
  /// used to replace failed members of the active view. The passive view is
  /// not used for message dissemination.
  ///
  /// The passive view is maintained using a cyclic strategy. Periodically,
  /// each node performs shuffle operation with one of its neighbors in order
  /// to update its passive view.
  passive_peers: HashMap<PeerId, HashSet<Multiaddr>>,

  /// Peers that we have dialed by address only without knowing their
  /// identity. This is used to send JOIN messages once a connection
  /// is established.
  pending_dials: HashSet<Multiaddr>,

  /// Peers that we have dialed because we want to add them to the active
  /// peers. Peers in this collection will be sent NEIGHBOUR message when
  /// a connection to them is established.
  pending_neighbours: HashSet<PeerId>,

  /// Peers that have received a JOIN message from us.
  ///
  /// This is to prevent spamming the same peer multiple times
  /// with JOIN requests if they add us to their active view.
  pending_joins: HashSet<PeerId>,
}

/// A topic represents an instance of HyparView p2p overlay.
/// This type is cheap to copy and can be safely moved across
/// different threads for example to listen on topic messages
/// on a background thread.
#[derive(Clone)]
pub struct Topic {
  inner: Arc<RwLock<TopicInner>>,
}

// Public API
impl Topic {
  /// Propagate a message to connected active peers
  pub fn gossip(&self, data: Bytes) {
    let inner = self.inner.read();
    for peer in inner.active_peers.keys() {
      inner
        .cmdtx
        .send(Command::SendMessage {
          peer: *peer,
          msg: Message::new(
            inner.topic_config.name.clone(),
            Action::Gossip(data.clone()),
          ),
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
        .send(Command::Connect {
          addr: addr.clone(),
          topic: topic_config.name.clone(),
        })
        .expect("lifetime of network should be longer than topic");
    }

    Self {
      inner: Arc::new(RwLock::new(TopicInner {
        network_config,
        this_node,
        cmdtx,
        outmsgs: Channel::new(),
        active_peers: HashMap::new(),
        passive_peers: HashMap::new(),
        pending_joins: HashSet::new(),
        pending_neighbours: HashSet::new(),
        pending_dials: topic_config.bootstrap.iter().cloned().collect(),
        topic_config,
      })),
    }
  }

  /// Called when the network layer has a new event for this topic
  pub(crate) fn inject_event(&mut self, event: Event) {
    let mut inner = self.inner.write();
    info!("{}: {event:?}", inner.topic_config.name);
    match event {
      Event::LocalAddressDiscovered(addr) => {
        inner.handle_new_local_address(addr);
      }
      Event::PeerConnected(peer) => {
        inner.handle_peer_connected(peer);
      }
      Event::PeerDisconnected(peer, gracefully) => {
        inner.handle_peer_disconnected(peer, gracefully);
      }
      Event::MessageReceived(peer, msg) => {
        inner.handle_message_received(peer, msg);
      }
    }
  }
}

/// Event handlers
impl TopicInner {
  fn handle_new_local_address(&mut self, addr: Multiaddr) {
    self.this_node.addresses.insert(addr);
  }

  fn handle_message_received(&mut self, sender: PeerId, msg: Message) {
    match msg.action {
      Action::Join(join) => self.consume_join(sender, join),
      Action::ForwardJoin(fj) => self.consume_forward_join(sender, fj),
      Action::Neighbour(n) => self.consume_neighbor(sender, n),
      Action::Shuffle(s) => self.consume_shuffle(sender, s),
      Action::ShuffleReply(sr) => self.consume_shuffle_reply(sender, sr),
      Action::Disconnect(d) => self.consume_disconnect(sender, d),
      Action::Gossip(b) => self.consume_gossip(sender, b),
    }
  }

  /// Invoked when a connection is established with a remote peer.
  /// When a node is dialed, we don't know its identity, only the
  /// address we dialed it at. If it happens to be one of the nodes
  /// that we have dialed into from this topic, send it a "JOIN"
  /// message if our active view is not full yet.
  fn handle_peer_connected(&mut self, peer: AddressablePeer) {
    if self.starved()
      && !self.pending_joins.contains(&peer.peer_id)
      && !self.pending_neighbours.contains(&peer.peer_id)
    {
      for addr in &peer.addresses {
        if self.pending_dials.remove(addr) {
          self
            .cmdtx
            .send(Command::SendMessage {
              peer: peer.peer_id,
              msg: Message::new(
                self.topic_config.name.clone(),
                Action::Join(Join {
                  node: self.this_node.clone(),
                }),
              ),
            })
            .expect("network lifetime > topic");
        }
      }
    }

    if self.pending_neighbours.remove(&peer.peer_id) {
      self.send_message(
        peer.peer_id,
        Message::new(
          self.topic_config.name.clone(),
          Action::Neighbour(Neighbour {
            peer: self.this_node.clone(),
            high_priority: self.active_peers.is_empty(),
          }),
        ),
      );

      for addr in &peer.addresses {
        self.pending_dials.remove(addr);
      }
      self.passive_peers.remove(&peer.peer_id);
    }
  }

  fn handle_peer_disconnected(&mut self, peer: PeerId, gracefully: bool) {
    // if the remote peer disconnected gracefuly move them to the passive view.
    if let Some(addrs) = self.active_peers.remove(&peer) {
      if gracefully {
        self.add_to_passive_view(AddressablePeer {
          peer_id: peer,
          addresses: addrs,
        });
      }
    }
  }
}

impl TopicInner {
  /// Checks if a peer is already in the active view of this topic.
  /// This is used to check if we need to send JOIN message when
  /// the peer is dialed, peers that are active will not get
  /// a JOIN request, otherwise the network will go into endless
  /// join/forward churn.
  fn is_active(&self, peer: &PeerId) -> bool {
    self.active_peers.contains_key(peer)
  }

  /// Starved topics are ones where the active view
  /// doesn't have a minimum set of nodes in it.
  fn starved(&self) -> bool {
    self.active_peers.len() < self.network_config.max_active_view_size()
  }

  /// Initiates a graceful disconnect from an active peer.
  fn disconnect(&mut self, peer: PeerId) {
    self.send_message(
      peer,
      Message::new(
        self.topic_config.name.clone(),
        Action::Disconnect(Disconnect),
      ),
    );

    self
      .cmdtx
      .send(Command::Disconnect {
        peer,
        topic: self.topic_config.name.clone(),
      })
      .expect("topic lifetime < network lifetime");
  }

  fn dial(&mut self, addr: Multiaddr) {
    self.pending_dials.insert(addr.clone());
    self
      .cmdtx
      .send(Command::Connect {
        addr,
        topic: self.topic_config.name.clone(),
      })
      .expect("lifetime of network should be longer than topic");
  }

  fn ban(&self, peer: PeerId) {
    self
      .cmdtx
      .send(Command::BanPeer(peer))
      .expect("topic lifetime < network lifetime");
  }

  fn send_message(&self, peer: PeerId, msg: Message) {
    self
      .cmdtx
      .send(Command::SendMessage { peer, msg })
      .expect("network lifetime > topic lifetime");
  }
}

// add/remove to/from views
impl TopicInner {
  fn add_to_passive_view(&mut self, peer: AddressablePeer) {
    self.remove_from_active_view(peer.peer_id);
    self.passive_peers.insert(peer.peer_id, peer.addresses);

    // if we've reached the passive view limit, remove a random node
    if self.passive_peers.len() > self.network_config.max_passive_view_size() {
      let random = *self
        .passive_peers
        .keys()
        .choose(&mut rand::thread_rng())
        .expect("already checked that it is not empty");
      self.remove_from_passive_view(random);
    }
  }

  fn remove_from_passive_view(&mut self, peer: PeerId) {
    self.passive_peers.remove(&peer);
  }

  fn try_add_to_active_view(&mut self, peer: AddressablePeer) {
    if self.starved() {
      self.add_to_active_view(peer);
    } else {
      self.add_to_passive_view(peer);
    }
  }

  fn add_to_active_view(&mut self, peer: AddressablePeer) {
    if self.is_active(&peer.peer_id) {
      return;
    }

    if peer.peer_id == self.this_node.peer_id {
      return;
    }

    self
      .active_peers
      .insert(peer.peer_id, peer.addresses.clone());

    self.remove_from_passive_view(peer.peer_id);
    self.pending_neighbours.insert(peer.peer_id);
    for addr in peer.addresses {
      // when it connects and is in pending_neighbours,
      // then a NEIGHBOUR message will be sent. see the
      // handle_peer_connected method.
      self.dial(addr);
    }
  }

  fn remove_from_active_view(&mut self, peer: PeerId) {
    if self.is_active(&peer) {
      self.disconnect(peer);
      self.active_peers.remove(&peer);
    }
  }
}

// HyParView protocol message handlers
impl TopicInner {
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
    increment_counter!(
      "received_join",
      "topic" => self.topic_config.name.clone()
    );

    debug!(
      "join request on topic {} from {sender}",
      self.topic_config.name
    );

    if sender != msg.node.peer_id {
      // Seems like an impersonation attempt
      // JOIN messages are not forwarded to
      // other peers as is.
      info!("join: sender != msg.node.peer_id");
      self.ban(sender);
      return;
    }

    if self.active_peers.contains_key(&sender) {
      // its a protocol violation for a peer
      // to send a join request to one of its
      // active peers.
      self.ban(sender);
      return;
    }

    // if not starving add to active, otherwise passive
    self.try_add_to_active_view(msg.node.clone());

    // forward join to all active peers
    for peer in self.active_peers.keys() {
      self.send_message(
        *peer,
        Message::new(
          self.topic_config.name.clone(),
          Action::ForwardJoin(ForwardJoin {
            hop: 1,
            node: msg.node.clone(),
          }),
        ),
      );
    }
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
    increment_counter!(
      "received_forward_join",
      "topic" => self.topic_config.name.clone(),
      "hop" => msg.hop.to_string()
    );

    if sender == msg.node.peer_id {
      // nodes may not send this message for themselves.
      // it has to be innitiated by another peer that received
      // JOIN message.
      self.ban(sender);
      return;
    }

    if msg.node.peer_id == self.this_node.peer_id {
      // cyclic forward join from this node, ignore
      return;
    }

    // if last hop, must create active connection
    if msg.hop == self.network_config.forward_join_hops_count {
      if !self.starved() {
        // our active view is full, need to free up a slot
        let random = *self
          .active_peers
          .keys()
          .choose(&mut rand::thread_rng())
          .expect("already checked that it is not empty");

        // move the unlucky node to passive view
        self.add_to_passive_view(AddressablePeer {
          peer_id: random,
          addresses: self
            .active_peers
            .get(&random)
            .expect("chosen by random from existing values")
            .clone(),
        });
      }
    } else {
      for peer in self.active_peers.keys() {
        self.send_message(
          *peer,
          Message::new(
            self.topic_config.name.clone(),
            Action::ForwardJoin(ForwardJoin {
              hop: msg.hop + 1,
              node: msg.node.clone(),
            }),
          ),
        )
      }
    }

    self.try_add_to_active_view(msg.node);
  }

  /// Handles NEIGHBOR messages.
  ///
  /// This message is send when a node wants to establish an active connection
  /// with the receiving node. This message is sent as a response to JOIN and
  /// FORWARDJOIN messages initiated by the peer wanting to join the overlay.
  ///
  /// This message is also sent to nodes that are being moved from passive view
  /// to the active view.
  fn consume_neighbor(&mut self, sender: PeerId, msg: Neighbour) {
    increment_counter!(
      "received_neighbor",
      "topic" => self.topic_config.name.clone()
    );

    if sender != msg.peer.peer_id {
      // impersonation attempt. ban sender
      self.ban(sender);
      return;
    }

    self.pending_joins.remove(&sender);
    self.pending_joins.remove(&msg.peer.peer_id);

    if !self.starved() && msg.high_priority {
      // our active view is full, need to free up a slot
      let random = *self
        .active_peers
        .keys()
        .choose(&mut rand::thread_rng())
        .expect("already checked that it is not empty");
      self.remove_from_active_view(random);
    }

    if self.starved() {
      self.add_to_active_view(msg.peer);
    } else {
      self.disconnect(sender);
    }
  }

  /// Handles DISCONNECT messages.
  ///
  /// Nodes receiving this message are informed that the sender is removing
  /// them from their active view. Which also means that the sender should
  /// also be removed from the receiving node's active view.
  fn consume_disconnect(&mut self, sender: PeerId, _: Disconnect) {
    increment_counter!(
      "received_disconnect",
      "topic" => self.topic_config.name.clone()
    );

    self
      .cmdtx
      .send(Command::Disconnect {
        topic: self.topic_config.name.clone(),
        peer: sender,
      })
      .expect("topic lifetime < network lifetime");
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
  fn consume_shuffle(&mut self, _sender: PeerId, msg: Shuffle) {
    increment_counter!(
      "received_shuffle",
      "topic" => self.topic_config.name.clone(),
      "peers_count" => msg.peers.len().to_string()
    );

    todo!()
  }

  /// Handles SHUFFLEREPLY messages.
  ///
  /// Those messages are sent as responses to SHUFFLE messages to the originator
  /// of the SHUFFLE message. The SHUFFLEREPLY message should contain a sample
  /// of local node's known active and passive peers that were not present in
  /// the received SHUFFLE message.
  fn consume_shuffle_reply(&mut self, _sender: PeerId, msg: ShuffleReply) {
    increment_counter!(
      "received_shuffle_reply",
      "topic" => self.topic_config.name.clone(),
      "peers_count" => msg.peers.len().to_string()
    );

    todo!()
  }

  /// Invoked when a content is gossiped to this node.
  ///
  /// Those messages are emitted to listeners on this topic events.
  /// The message id is a randomly generated identifier by the originating
  /// node and is used to ignore duplicate messages.
  fn consume_gossip(&mut self, _sender: PeerId, msg: Bytes) {
    gauge!(
      "gossip_size", msg.len() as f64,
      "topic" => self.topic_config.name.clone());
    self.outmsgs.send(msg);
  }
}

impl Stream for Topic {
  type Item = Bytes;

  fn poll_next(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Self::Item>> {
    self.inner.write().outmsgs.poll_recv(cx)
  }
}
