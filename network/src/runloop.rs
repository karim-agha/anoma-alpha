use {
  crate::{
    behaviour::Behaviour,
    channel::Channel,
    network::{self, Error},
    wire::Message,
    Config,
  },
  futures::StreamExt,
  libp2p::{
    core::upgrade::Version,
    dns::TokioDnsConfig,
    identity::Keypair,
    noise::{self, NoiseConfig, X25519Spec},
    swarm::{ConnectionLimits, SwarmBuilder, SwarmEvent},
    tcp::{GenTcpConfig, TokioTcpTransport},
    yamux::YamuxConfig,
    Multiaddr,
    PeerId,
    Swarm,
    Transport,
  },
  tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
  },
  tracing::{error, warn},
};

/// Low-level network commands.
///
/// At this level of abstraction there is no notion of topics
/// or any other high-level concepts. Here we are dealing with
/// raw connections to peers, sending and receiving streams of
/// bytes.
#[derive(Debug, Clone)]
pub enum Command {
  /// Establishes a long-lived TCP connection with a peer.
  ///
  /// If a connection already exists with the given address,
  /// then its refcount is incremented by 1.
  ///
  /// This happens when a peer is added to the active view
  /// of one of the topics.
  Connect(Multiaddr),

  /// Disconnects from a peer.
  ///
  /// First it will decrement the recount on a connection with
  /// the peer, and if it reaches zero then the connection gets closed.
  Disconnect(PeerId),

  /// Bans a peer from connecting to this node.
  ///
  /// This happens when a violation of the network protocol
  /// is detected. Banning a peer will also automatically forcefully
  /// disconnect it from all topics.
  ///
  /// Trying to connect to a peer on an unexpected topic is also
  /// considered a violation of the protocol and gets the sender
  /// banned.
  BanPeer(PeerId),

  /// Sends a message to one peer in the active view of
  /// one of the topics.
  SendMessage { peer: PeerId, msg: Message },
}

/// Manages the event loop that drives the network layer.
pub(crate) struct Runloop {
  cmdtx: UnboundedSender<Command>,
}

impl Runloop {
  pub fn new(
    config: &Config,
    keypair: Keypair,
    netcmdtx: UnboundedSender<network::Command>,
  ) -> Result<Self, Error> {
    let (tx, rx) = Channel::new().split();
    start_network_runloop(config, keypair, rx, netcmdtx)?;
    Ok(Self { cmdtx: tx })
  }

  pub fn send_command(&self, command: Command) {
    self.cmdtx.send(command).expect("runloop thread died");
  }
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
  cmdrx: UnboundedReceiver<Command>,
  netcmdtx: UnboundedSender<network::Command>,
) -> Result<JoinHandle<()>, Error> {
  // Libp2p network state driver and event loop
  let mut swarm = build_swarm(config, keypair)?;

  // instruct the libp2p engine to accept connections
  // on all configured addresses and ports.
  //
  // The actual sockets will open once we start polling
  // the swarm on a separate thread.
  for addr in &config.listen_addrs {
    swarm.listen_on(addr.clone())?;
  }

  let mut cmdrx = cmdrx;
  Ok(tokio::spawn(async move {
    loop {
      tokio::select! {
        Some(event) = swarm.next() => {
          if let SwarmEvent::Behaviour(event) = event {
            // forward all events to the [`Network`] object and
            // handle it there. This loop is not responsible for
            // any high-level logic except routing commands and
            // events between network foreground and background
            // threads.
            netcmdtx.send(network::Command::InjectEvent(event))
              .expect("network should outlive runloop");
          }
        }

        Some(command) = cmdrx.recv() => {
          match command {
            Command::Connect(addr) => {
              if let Err(err) = swarm.dial(addr) {
                error!("Failed to dial peer: {err:?}");
              }
            }
            Command::Disconnect(peer) => {
              if let Err(()) = swarm.disconnect_peer_id(peer) {
                warn!("trying to disconnect from an unknown peer");
              }
            }
            Command::SendMessage { peer, msg } => {
              swarm.behaviour().send_to(peer, msg);
            }
            Command::BanPeer(peer) => {
              swarm.ban_peer_id(peer);
            }
          }
        }
      };
    }
  }))
}
