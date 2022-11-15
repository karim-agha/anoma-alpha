use {
  crate::{
    behaviour,
    channel::Channel,
    runloop::{self, Runloop},
    topic::{self, Topic},
    wire::{AddressablePeer, Message},
    Config,
  },
  futures::{Stream, StreamExt},
  libp2p::{
    identity::Keypair,
    noise::NoiseError,
    Multiaddr,
    PeerId,
    TransportError,
  },
  std::{
    collections::{HashMap, HashSet},
    pin::Pin,
    task::{Context, Poll},
  },
  thiserror::Error,
  tracing::{error, info, warn},
};

#[derive(Debug, Error)]
pub enum Error {
  #[error("IO Error: {0}")]
  Io(#[from] std::io::Error),

  #[error("Transport layer security error: {0}")]
  TlsError(#[from] NoiseError),

  #[error("Transport error: {0}")]
  TransportError(#[from] TransportError<std::io::Error>),

  #[error("Topic {0} already joined")]
  TopicAlreadyJoined(String),
}

/// Commands sent by different components to the network layer
/// instructing it to perform some operation on its network-managed
/// threads and runloops. Examples of types invoking those commands
/// are Topics, Behaviour, etc.
#[derive(Debug, Clone)]
pub(crate) enum Command {
  /// Invoked by topics when adding new peer to the active view.
  Connect { addr: Multiaddr, topic: String },

  /// Invoked by topics when removing peers from the active view.
  Disconnect { peer: PeerId, topic: String },

  /// Sends a message to one peer in the active view of
  /// one of the topics.
  SendMessage { peer: PeerId, msg: Message },

  /// Ingests a message received on the wire
  /// and routes it the the appropriate topic.
  AcceptMessage { from: PeerId, msg: Message },
}

/// This type is the entrypoint to using the network API.
///
/// It is used to configure general network settings such as
/// the underlying transport protocol, encryption scheme, dns
/// lookup and other non-topic specific configuration values.
///
/// An instance of this type is used to join topics and acquire
/// instances of types for interacting with individul topics.
///
/// On the implementation level, this type acts as a multiplexer
/// for topics, routing incoming packets to their appropriate topic
/// instance.
pub struct Network {
  /// Global network-level configuration
  config: Config,

  /// Local identity of the current local node and all its known addresses.
  this: AddressablePeer,

  /// Network-global events.
  events: Channel<behaviour::Event>,

  /// Network background runloop.
  ///
  /// Stored here so users of this type can block on
  /// the network object.
  runloop: Runloop,

  /// Commands sent to the network module by the runloop and topics.
  commands: Channel<Command>,

  /// All joined topic addressed by their topic name.
  /// Each topic is its own instance of HyParView overlay.
  topics: HashMap<String, Topic>,
}

impl Default for Network {
  fn default() -> Self {
    Self::new(Config::default(), Keypair::generate_ed25519())
      .expect("Failed to instantiate network instance using default config")
  }
}

impl Network {
  /// Instanciates a network object.
  pub fn new(config: Config, keypair: Keypair) -> Result<Self, Error> {
    let peer_id = keypair.public().into();
    let commands = Channel::new();

    Ok(Self {
      topics: HashMap::new(),
      events: Channel::new(),
      runloop: Runloop::new(&config, keypair, commands.sender())?,
      this: AddressablePeer {
        peer_id,
        addresses: HashSet::new(), // none discovered yet
      },
      config,
      commands,
    })
  }

  /// Joins a new topic on this network.
  ///
  /// The config value specifies mainly the topic name and
  /// a list of bootstrap peers. If the bootstrap list is empty
  /// then this node will not dial into any peers but listen on
  /// incoming connections on that topic. It will not receive or
  /// send any values unless at least one other node connects to it.
  pub fn join(&mut self, config: topic::Config) -> Result<&Topic, Error> {
    if self.topics.contains_key(&config.name) {
      return Err(Error::TopicAlreadyJoined(config.name));
    }

    let name = config.name.clone();
    self.topics.insert(
      name.clone(),
      Topic::new(
        config,
        self.config.clone(),
        self.this.clone(),
        self.commands.sender(),
      ),
    );
    Ok(self.topics.get(&name).unwrap())
  }

  /// Runs the network event loop.
  ///
  /// This loop must be running all the time to drive the network layer,
  /// this function makes it easy to move the whole network layer to the
  /// background by calling:
  ///
  /// ```rust
  /// tokio::spawn(network.runloop());
  /// ```
  ///
  /// The network object is needed only to join topics, after that all
  /// interactions with the network happen through the [`Topic`] instances
  /// created when calling [`Network::join`].
  pub async fn runloop(mut self) {
    loop {
      while let Some(event) = self.next().await {
        info!("event: {event:?}");
      }
    }
  }
}

impl Network {
  fn append_local_address(&mut self, address: Multiaddr) {
    self.this.addresses.insert(address.clone());

    // update all topics about new local addresses
    for topic in self.topics.values_mut() {
      topic.inject_event(topic::Event::LocalAddressDiscovered(address.clone()));
    }
  }

  fn accept_message(&mut self, from: PeerId, msg: Message) {
    if let Some(topic) = self.topics.get_mut(&msg.topic) {
      // route message to appropriate topic
      topic.inject_event(topic::Event::MessageReceived(from, msg));
    } else {
      // sending messages on unsubscribed topics is a protocol violation
      self.runloop.send_command(runloop::Command::BanPeer(from));
    }
  }
}

/// Drives the network event loop.
///
/// This should not be used directly, use [`Self::runloop`] instead.
impl Stream for Network {
  type Item = ();

  fn poll_next(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Self::Item>> {
    if let Poll::Ready(Some(event)) = self.events.poll_recv(cx) {
      info!("network event: {event:?}");
      match event {
        behaviour::Event::LocalAddressDiscovered(addr) => {
          // always keep track of all known address
          // that point to the current node
          self.append_local_address(addr);
        }
        behaviour::Event::MessageReceived(peer, msg) => {
          if let Some(topic) = self.topics.get_mut(&msg.topic) {
            // forward message to appropriate topic
            topic.inject_event(topic::Event::MessageReceived(peer, msg));
          } else {
            warn!("received message on an unrecognized topic {:?}", msg.topic);
          }
        }
        _ => info!("net event: {event:?}"),
      }
    }

    if let Poll::Ready(Some(command)) = self.commands.poll_recv(cx) {
      info!("network command: {command:?}");
      match command {
        Command::Connect { addr, .. } => {
          self.runloop.send_command(runloop::Command::Connect(addr))
        }
        Command::Disconnect { peer, .. } => self
          .runloop
          .send_command(runloop::Command::Disconnect(peer)),
        Command::SendMessage { peer, msg } => self
          .runloop
          .send_command(runloop::Command::SendMessage { peer, msg }),
        Command::AcceptMessage { from, msg } => self.accept_message(from, msg),
      }
    }

    Poll::Pending
  }
}
