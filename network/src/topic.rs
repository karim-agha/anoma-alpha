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
  rand::{
    distributions::Standard,
    rngs::StdRng,
    seq::IteratorRandom,
    thread_rng,
    Rng,
    SeedableRng,
  },
  std::{
    collections::{HashMap, HashSet},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Instant,
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
  Tick,
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

  /// When the last time a shuffle operation happened on this topic
  ///
  /// This also includes attemtps to shuffle that resulted in not
  /// performing the operation due to Config::shuffle_probability.
  last_shuffle: Instant,

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

  /// Peers that have originated a SHUFFLE that are not in
  /// the current active view.
  ///
  /// When replying to the shuffle, a new temporary connection
  /// is established with the originator, the shuffle reply is
  /// sent and then immediately the connection is closed.
  pending_shuffle_replies: HashMap<PeerId, ShuffleReply>,
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
        last_shuffle: Instant::now(),
        active_peers: HashMap::new(),
        passive_peers: HashMap::new(),
        pending_joins: HashSet::new(),
        pending_neighbours: HashSet::new(),
        pending_shuffle_replies: HashMap::new(),
        pending_dials: topic_config.bootstrap.iter().cloned().collect(),
        topic_config,
      })),
    }
  }

  /// Called when the network layer has a new event for this topic
  pub(crate) fn inject_event(&mut self, event: Event) {
    let mut inner = self.inner.write();

    if !matches!(event, Event::Tick) {
      info!("{}: {event:?}", inner.topic_config.name);
    }

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
      Event::Tick => inner.handle_tick(),
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

  fn handle_pending_neighbours(&mut self, peer: &AddressablePeer) {
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

  fn handle_pending_shuffle_replies(&mut self, peer: &AddressablePeer) {
    if let Some(reply) = self.pending_shuffle_replies.remove(&peer.peer_id) {
      // this peer originated a shuffle operation and this
      // is a temporary connection connection just to reply
      // with our unique peer info.
      self.send_message(
        peer.peer_id,
        Message::new(
          self.topic_config.name.clone(),
          Action::ShuffleReply(reply),
        ),
      );

      increment_counter!("shuffle_reply");

      for addr in &peer.addresses {
        self.pending_dials.remove(addr);
      }

      // send & close connection
      self.disconnect(peer.peer_id);
    }
  }

  fn handle_non_pending_connects(&mut self, peer: &AddressablePeer) {
    if self.starved()
      && !self.pending_joins.contains(&peer.peer_id)
      && !self.pending_neighbours.contains(&peer.peer_id)
      && !self.pending_shuffle_replies.contains_key(&peer.peer_id)
    {
      for addr in &peer.addresses {
        if self.pending_dials.remove(addr) {
          increment_counter!(
            "sent_join",
            "topic" => self.topic_config.name.clone()
          );

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
  }

  /// Invoked when a connection is established with a remote peer.
  /// When a node is dialed, we don't know its identity, only the
  /// address we dialed it at. If it happens to be one of the nodes
  /// that we have dialed into from this topic, send it a "JOIN"
  /// message if our active view is not full yet.
  fn handle_peer_connected(&mut self, peer: AddressablePeer) {
    self.handle_non_pending_connects(&peer);
    self.handle_pending_neighbours(&peer);
    self.handle_pending_shuffle_replies(&peer);

    gauge!("active_view_size", self.active_peers.len() as f64);
    gauge!("passive_view_size", self.passive_peers.len() as f64);
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

    gauge!("active_view_size", self.active_peers.len() as f64);
    gauge!("passive_view_size", self.passive_peers.len() as f64);
  }

  fn handle_tick(&mut self) {
    if self.last_shuffle.elapsed() > self.network_config.shuffle_interval {
      self.initiate_shuffle();
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

    gauge!("active_view_size", self.active_peers.len() as f64);
    gauge!("passive_view_size", self.passive_peers.len() as f64);
  }

  fn remove_from_passive_view(&mut self, peer: PeerId) {
    self.passive_peers.remove(&peer);

    gauge!("passive_view_size", self.passive_peers.len() as f64);
  }

  fn try_add_to_active_view(&mut self, peer: AddressablePeer) {
    if self.starved() {
      self.add_to_active_view(peer);
    } else {
      self.add_to_passive_view(peer);
    }

    gauge!("active_view_size", self.active_peers.len() as f64);
    gauge!("passive_view_size", self.passive_peers.len() as f64);
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

    gauge!("active_view_size", self.active_peers.len() as f64);
    gauge!("passive_view_size", self.passive_peers.len() as f64);
  }

  fn remove_from_active_view(&mut self, peer: PeerId) {
    if self.is_active(&peer) {
      self.disconnect(peer);
      self.active_peers.remove(&peer);
    }

    gauge!("active_view_size", self.active_peers.len() as f64);
  }

  /// Called whenever this node gets a chance to learn about new peers.
  ///
  /// If the active view is not saturated, it will randomly pick a peer
  /// from the passive view and try to add it to the active view.
  fn try_replenish_active_view(&mut self) {
    if self.starved() {
      if let Some(random) = self
        .passive_peers
        .values()
        .choose(&mut thread_rng())
        .cloned()
      {
        for addr in random {
          // dial the node and on connect,
          // if still starving JOIN request will be sent
          // see handle_peer_connected for more context.
          self.dial(addr.clone());
        }
      }
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
      self.ban(sender);
      return;
    }

    if self.active_peers.contains_key(&sender) {
      return; // already joined
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

    let recv_peer_ids: HashSet<_> =
      msg.peers.iter().map(|p| p.peer_id).collect();

    let local_peer_ids: HashSet<_> = self
      .active_peers
      .keys()
      .chain(self.passive_peers.keys())
      .cloned()
      .collect();

    let resp_unique_peers: HashSet<_> = local_peer_ids
      .difference(&recv_peer_ids) // local \ rec
      .cloned()
      .collect();

    gauge!("shuffle_unique_peers", resp_unique_peers.len() as f64);

    let new_peers: HashSet<_> = recv_peer_ids
      .difference(&local_peer_ids) // rec \ local
      .cloned()
      .collect();

    gauge!("shuffle_new_peers", new_peers.len() as f64);

    // all peers are either in our passive view,
    // our local active view, or the list of peers
    // we have just learned about from the shuffle
    // message. Given a peer ID construct a complete
    // peer info from either of those sources.
    macro_rules! collect_peer_addrs {
      ($peer_id:expr) => {
        self
          .passive_peers
          .get(&$peer_id)
          .map(Clone::clone)
          .or_else(|| self.active_peers.get(&$peer_id).map(Clone::clone))
          .or_else(|| {
            Some(
              msg
                .peers
                .get(&AddressablePeer {
                  peer_id: $peer_id,
                  addresses: HashSet::new(),
                })
                .expect("the only remaining possibility")
                .addresses
                .clone(),
            )
          })
          .expect("querying all possible sources")
      };
    }

    // Respond to the shuffle initiator with a list of
    // unique peers that we know about and were not
    // present in their shuffle.
    let shuffle_reply = ShuffleReply {
      peers: resp_unique_peers
        .into_iter()
        .map(|peer_id| AddressablePeer {
          peer_id,
          addresses: collect_peer_addrs!(peer_id),
        })
        .collect(),
    };

    // the new passive view is a random sample over
    // what this node knew before the shuffle and
    // new information learned during this shuffle.
    self.passive_peers = local_peer_ids
      .into_iter()
      .chain(new_peers.into_iter())
      .choose_multiple(
        // random std sample
        &mut thread_rng(),
        self.network_config.max_passive_view_size(),
      )
      .into_iter()
      .map(|peer_id| (peer_id, collect_peer_addrs!(peer_id)))
      .collect();

    // if the initiator is one of our active peers,
    // then just respond to it on the open link.
    if self.is_active(&msg.origin.peer_id) {
      self.send_message(
        msg.origin.peer_id,
        Message::new(
          self.topic_config.name.clone(),
          Action::ShuffleReply(shuffle_reply),
        ),
      );

      increment_counter!("shuffle_reply");
    } else {
      // otherwise, open a short-lived connection to
      // the initiator and send the reply.
      self
        .pending_shuffle_replies
        .insert(msg.origin.peer_id, shuffle_reply);

      for addr in &msg.origin.addresses {
        self.dial(addr.clone());
      }
    }

    // forward the shuffle message until
    // hop count reaches shuffle max hops
    if msg.hop < self.network_config.shuffle_hops_count {
      for peer in self.active_peers.keys() {
        self.send_message(
          *peer,
          Message::new(
            self.topic_config.name.clone(),
            Action::Shuffle(Shuffle {
              hop: msg.hop + 1,
              origin: msg.origin.clone(),
              peers: msg.peers.clone(),
            }),
          ),
        )
      }
    }

    self.try_replenish_active_view();
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

    let new_peers: HashSet<_> = self
      .passive_peers
      .keys()
      .collect::<HashSet<_>>()
      .difference(&msg.peers.iter().map(|p| &p.peer_id).collect::<HashSet<_>>())
      .map(|p| **p)
      .collect();

    gauge!("shuffle_reply_new", new_peers.len() as f64);

    self.passive_peers = self
      .passive_peers
      .keys()
      .chain(new_peers.iter()) // merge what we know with new knowledg
      .choose_multiple( // and sample a random subset of it
        &mut thread_rng(),
        self.network_config.max_passive_view_size(),
      )
      .into_iter()
      .map(|id| {
        (
          *id,
          self
            .passive_peers
            .get(id)
            .map(Clone::clone)
            .or_else(|| {
              Some(
                msg
                  .peers
                  .get(&AddressablePeer {
                    peer_id: *id,
                    addresses: Default::default(),
                  })
                  .expect("covered all sources")
                  .addresses
                  .clone(),
              )
            })
            .expect("all sources covered"),
        )
      })
      .collect();

    self.try_replenish_active_view();
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

impl TopicInner {
  fn initiate_shuffle(&mut self) {
    self.last_shuffle = Instant::now();

    // range [0, 1)
    let toss: f32 = StdRng::from_entropy().sample(Standard);
    if (1.0 - toss) > self.network_config.shuffle_probability {
      return; // not this time.
    }

    increment_counter!("shuffles");

    // This is the list of peers that we will exchange
    // with other peers during our shuffle operation.
    let peers_sample = self
      .active_peers
      .keys()
      .chain(self.passive_peers.keys())
      .choose_multiple(
        &mut thread_rng(),
        self.network_config.shuffle_sample_size,
      );

    gauge!("shuffle_size", peers_sample.len() as f64);

    // chose a random peer from the active view to initiate the shuffle with
    if let Some(peer) = self.active_peers.keys().choose(&mut thread_rng()) {
      self.send_message(
        *peer,
        Message::new(
          self.topic_config.name.clone(),
          Action::Shuffle(Shuffle {
            hop: 0,
            origin: self.this_node.clone(),
            peers: peers_sample
              .into_iter()
              .map(|peer_id| AddressablePeer {
                peer_id: *peer_id,
                addresses: self
                  .active_peers
                  .get(peer_id)
                  .or_else(|| self.passive_peers.get(peer_id))
                  .expect("")
                  .clone(),
              })
              .collect(),
          }),
        ),
      );
    }
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
