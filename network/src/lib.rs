mod behviour;
mod channel;
mod codec;
mod stream;
pub mod topic;
mod upgrade;
mod wire;

pub use topic::Topic;

use {
  crate::{channel::Channel},
  behviour::Behaviour,
  futures::{FutureExt, Stream, StreamExt},
  libp2p::{
    PeerId, 
    core::upgrade::Version,
    dns::TokioDnsConfig,
    identity::Keypair,
    noise::{self, NoiseConfig, NoiseError, X25519Spec},
    swarm::{ConnectionLimits, SwarmBuilder, SwarmEvent},
    tcp::{GenTcpConfig, TokioTcpTransport},
    yamux::YamuxConfig,
    Multiaddr,
    Swarm,
    Transport,
    TransportError,
  },
  std::{
    pin::Pin,
    collections::HashMap,
    task::{Context, Poll},
  },
  thiserror::Error,
  tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task::{JoinError, JoinHandle},
  },
  tracing::{error, info, debug, warn},
  wire::{AddressablePeer, Message},
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
  TopicAlreadyJoined(String)
}

/// Represents a network level event that is emitted
/// by the networking module. Events are ordered by their
/// occurance time and accessed by polling the network stream.
#[derive(Debug)]
pub enum Event {
  /// Emitted when the network discovers new public address pointing to the
  /// current node.
  LocalAddressDiscovered(Multiaddr),

  /// Emitted when a connection is created between two peers.
  /// 
  /// This is emitted only once regardless of the number of HyParView
  /// overlays the two peers share. All overlapping overlays share the
  /// same connection.
  ConnectionEstablished(AddressablePeer),

  /// Emitted when a connection is closed between two peers.
  /// 
  /// This is emitted when the last HyparView overlay between the two
  /// peers is destroyed and they have no common topics anymore. Also
  /// emitted when the connection is dropped due to transport layer failure.
  ConnectionClosed(PeerId),

  /// Emitted when a message is received on the wire from a connected peer.
  MessageReceived(PeerId, Message),
}

#[derive(Debug, Clone)]
pub(crate) enum Command {
  Connect(Multiaddr),
  SendMessage { peer: PeerId, msg: Message },
}

/// Network wide configuration across all topics.
#[derive(Debug, Clone)]
pub struct Config {
  /// Maximum size of a message, this applies to
  /// control and payload messages
  pub max_transmit_size: usize,

  /// The number of hops a shuffle message should
  /// travel across the network.
  pub shuffle_hops_count: u16,

  /// The number of hops a FORWARDJOIN message should
  /// travel across the network.
  pub forward_join_hops_count: u16,

  /// Local network addresses this node will listen on for incoming
  /// connections. By default it will listen on all available IPv4 and IPv6
  /// addresses on port 44668.
  pub listen_addrs: Vec<Multiaddr>,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      max_transmit_size: 64 * 1024, // 64KB
      shuffle_hops_count: 3,
      forward_join_hops_count: 3,
      listen_addrs: vec![
        "/ip4/0.0.0.0/tcp/44668".parse().unwrap(),
        "/ip6/::/tcp/44668".parse().unwrap(),
      ],
    }
  }
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
  events: Channel<Event>,

  /// Network background runloop.
  ///
  /// Stored here so users of this type can block on
  /// the network object.
  runloop: JoinHandle<()>,

  /// All joined topic addressed by their topic name.
  /// Each topic is its own instance of HyParView overlay.
  topics: HashMap<String, Topic>,

  /// Channel for sending commands to the network thread.
  cmdtx: UnboundedSender<Command>,
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
    let (cmdtx, cmdrx) = Channel::new().split();
    let runloop = Self::start_network_runloop(&config, keypair, cmdrx)?;

    Ok(Self {
      topics: HashMap::new(),
      events: Channel::new(),
      this: AddressablePeer {
        peer_id,
        addresses: vec![], // none discovered yet
      },
      cmdtx,
      runloop,
      config,
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
    self.topics.insert(name.clone(), Topic::new(config, self.cmdtx.clone()));
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
    if !self.this.addresses.contains(&address) {
      self.this.addresses.push(address);
    }
  }
}

impl Network {
  fn build_swarm(
    config: &Config,
    keypair: Keypair,
  ) -> Result<Swarm<Behaviour>, Error> {
    // TCP transport with DNS resolution, NOISE encryption and Yammux
    // substream multiplexing.
    let transport = {
      let transport = TokioDnsConfig::system(TokioTcpTransport::new(
        GenTcpConfig::new().port_reuse(true).nodelay(true),
      ))?;

      let noise_keys =
        noise::Keypair::<X25519Spec>::new().into_authentic(&keypair)?;

      transport
        .upgrade(Version::V1)
        .authenticate(NoiseConfig::xx(noise_keys).into_authenticated())
        .multiplex(YamuxConfig::default())
        .boxed()
    };

    Ok(
      SwarmBuilder::new(
        transport, //
        Behaviour::new(config.clone()),
        keypair.public().into(),
      )
      // invoke libp2p tasks on current reactor
      .executor(Box::new(|f| {
        tokio::spawn(f); 
      }))
      // If multiple topics have overlapping nodes, 
      // maintain only one connection between peers.
      .connection_limits(
        ConnectionLimits::default().with_max_established_per_peer(Some(1)),
      )
      .build(),
    )
  }

  fn start_network_runloop(
    config: &Config,
    keypair: Keypair,
    mut cmdrx: UnboundedReceiver<Command>,
  ) -> Result<JoinHandle<()>, Error> {
    // Libp2p network state driver and event loop
    let mut swarm = Self::build_swarm(config, keypair)?;

    // instruct the libp2p engine to accept connections
    // on all configured addresses and ports.
    //
    // The actual sockets will open once we start polling
    // the swarm on a separate thread.
    for addr in &config.listen_addrs {
      swarm.listen_on(addr.clone())?;
    }

    Ok(tokio::spawn(async move {
      loop {
        tokio::select! {
          Some(event) = swarm.next() => {
            match event {
              SwarmEvent::Behaviour(event) => info!("Network event: {event:?}"),
              _ => debug!("{event:?}"),
            }
          }

          Some(command) = cmdrx.recv() => {
            debug!("Invoking network command: {command:?}");
            match command {
              Command::Connect(addr) => {
                if let Err(err) = swarm.dial(addr) {
                  error!("Failed to dial peer: {err:?}");
                }
              }
              Command::SendMessage { peer, msg } => {
                swarm.behaviour().send_to(peer, msg);           
              }
            }
          }
        };
      }
    }))
  }
}

/// Used to poll for events generated by the network.
impl Stream for Network {
  type Item = Event;

  fn poll_next(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Self::Item>> {
    let pollres = self.events.poll_recv(cx);

    if let Poll::Ready(Some(event)) = pollres {
      info!("network event: {event:?}");
      match event {
        Event::LocalAddressDiscovered(addr) => {
          // always keep track of all known address 
          // that point to the current node
          self.append_local_address(addr);
        }
        Event::MessageReceived(peer, msg) => { 
          if let Some(topic) = self.topics.get(&msg.topic) {
            // forward message to appropriate topic
            topic.inject_event(topic::Event::MessageReceived(peer, msg));
          } else {
            warn!("received message on an unrecognized topic {:?}", msg.topic);
          }
        },
        _ => { }
      }
    }

    Poll::Pending
  }
}

impl std::future::Future for Network {
  type Output = Result<(), JoinError>;

  fn poll(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Self::Output> {
    self.runloop.poll_unpin(cx)
  }
}
