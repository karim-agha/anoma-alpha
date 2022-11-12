mod behviour;
mod codec;
mod stream;
pub mod topic;
mod upgrade;
mod wire;

pub use topic::Topic;
use {
  behviour::Behaviour,
  futures::Stream,
  libp2p::{
    core::upgrade::Version,
    dns::TokioDnsConfig,
    identity::Keypair,
    noise::{self, NoiseConfig, X25519Spec},
    tcp::{GenTcpConfig, TokioTcpTransport},
    yamux::YamuxConfig,
    Multiaddr,
    Swarm,
    Transport,
  },
  std::{
    collections::HashMap,
    pin::Pin,
    task::{Context, Poll},
  },
  thiserror::Error,
  tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
  wire::AddressablePeer,
};

pub(crate) type Channel<T> = (UnboundedSender<T>, UnboundedReceiver<T>);

#[derive(Debug, Error)]
pub enum Error {
  #[error("Topic already joined")]
  TopicAlreadyJoined,
}

/// Represents a network level event that is emitted
/// by the networking module. Events are ordered by their
/// occurance time and accessed by polling the network stream.
#[derive(Debug, PartialEq, Eq)]
pub enum Event {
  /// Emitted when the network discovers new public address pointing to the
  /// current node.
  LocalAddressDiscovered(Multiaddr),
}

/// Network wide configuration across all topics.
#[derive(Debug)]
pub struct Config {
  // Maximum size of a message, this applies to
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
  /// addresses on port 44661.
  pub listen_addrs: Vec<Multiaddr>,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      max_transmit_size: 64 * 1024, // 64KB
      shuffle_hops_count: 3,
      forward_join_hops_count: 3,
      listen_addrs: vec![
        "/ip4/0.0.0.0/tcp/44661".parse().unwrap(),
        "/ip6/::/tcp/44661".parse().unwrap(),
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
pub struct Network {
  /// Global network-level configuration
  config: Config,

  /// Local identity of the current local node and all its known addresses.
  this: AddressablePeer,

  /// All joined topic addressed by their topic name.
  /// Each topic is its own instance of HyParView overlay.
  topics: HashMap<String, Topic>,

  /// Libp2p network state driver and event loop
  swarm: Swarm<Behaviour>,

  /// Network-global events.
  events: Channel<Event>,
}

impl Default for Network {
  fn default() -> Self {
    Self::new(Config::default(), Keypair::generate_ed25519())
  }
}

impl Network {
  /// Instanciates a network object with non-default configuration.
  ///
  /// We want to strive to minimize the instances where non-default
  /// network settings are used (outside of unit tests). If you find
  /// yourself repeatedly setting a config value when instantiating
  /// the network, consider making it a default value of the config
  /// object.
  pub fn new(config: Config, keypair: Keypair) -> Self {
    let transport = {
      let transport = TokioDnsConfig::system(TokioTcpTransport::new(
        GenTcpConfig::new().port_reuse(true).nodelay(true),
      ))
      .expect("Failed to create TCP transport layer");

      let noise_keys = noise::Keypair::<X25519Spec>::new()
        .into_authentic(&keypair)
        .expect("Signing libp2p-noise static DH keypair failed.");

      transport
        .upgrade(Version::V1)
        .authenticate(NoiseConfig::xx(noise_keys).into_authenticated())
        .multiplex(YamuxConfig::default())
        .boxed()
    };

    let swarm =
      Swarm::new(transport, Behaviour::new(), keypair.public().into());

    Self {
      swarm,
      config,
      topics: HashMap::new(),
      events: unbounded_channel(),
      this: AddressablePeer {
        peer_id: keypair.public().into(),
        addresses: vec![], // none discovered yet
      },
    }
  }

  /// Joins a new topic on this network.
  ///
  /// The config value specifies mainly the topic name and
  /// a list of bootstrap peers. If the bootstrap list is empty
  /// then this node will not dial into any peers but listen on
  /// incoming connections on that topic. It will not receive or
  /// send any values unless at least one other node connects to it.
  pub async fn join(&mut self, config: topic::Config) -> Result<&Topic, Error> {
    if self.topics.contains_key(&config.name) {
      return Err(Error::TopicAlreadyJoined);
    }

    self
      .topics
      .insert(config.name.clone(), Topic::new(self.this.clone()));
    Ok(self.topics.get(&config.name).unwrap())
  }
}

/// Used to poll for events generated by the network.
impl Stream for Network {
  type Item = Event;

  fn poll_next(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Self::Item>> {
    let (_, rx) = &mut self.events;
    rx.poll_recv(cx)
  }
}
