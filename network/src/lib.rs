mod behviour;
mod channel;
mod codec;
mod command;
mod stream;
pub mod topic;
mod upgrade;
mod wire;

pub use topic::Topic;

use {
  crate::{channel::Channel, command::Command},
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
    collections::HashMap,
    pin::Pin,
    task::{Context, Poll},
  },
  thiserror::Error,
  tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task::{JoinError, JoinHandle},
  },
  tracing::{error, info, debug},
  wire::AddressablePeer,
};

#[derive(Debug, Error)]
pub enum Error {
  #[error("Topic already joined")]
  TopicAlreadyJoined,

  #[error("IO Error: {0}")]
  Io(#[from] std::io::Error),

  #[error("Transport layer security error: {0}")]
  TlsError(#[from] NoiseError),

  #[error("Transport error: {0}")]
  TransportError(#[from] TransportError<std::io::Error>),
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
  ConnectionClosed(PeerId)
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

  /// All joined topic addressed by their topic name.
  /// Each topic is its own instance of HyParView overlay.
  topics: HashMap<String, Topic>,

  /// Network-global events.
  events: Channel<Event>,

  /// Network background runloop.
  ///
  /// Stored here so users of this type can block on
  /// the network object.
  runloop: JoinHandle<()>,

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
      cmdtx,
      runloop,
      config,
      topics: HashMap::new(),
      events: Channel::new(),
      this: AddressablePeer {
        peer_id,
        addresses: vec![], // none discovered yet
      },
    })
  }

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
            info!("Invoking network command: {command:?}");
            command.execute(&mut swarm);
          }
        };
      }
    }))
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
      return Err(Error::TopicAlreadyJoined);
    }

    let name = config.name.clone();

    self.topics.insert(
      config.name.clone(),
      Topic::new(config, self.this.clone(), self.cmdtx.clone()),
    );
    Ok(self.topics.get(&name).unwrap())
  }

  pub fn connect(&self, addr: Multiaddr) {
    self
      .cmdtx
      .send(Command::Connect(addr))
      .expect("command receiver closed");
  }
}

/// Used to poll for events generated by the network.
impl Stream for Network {
  type Item = Event;

  fn poll_next(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Self::Item>> {
    self.events.poll_recv(cx)
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
