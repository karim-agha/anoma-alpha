use tracing::info;

use {
  crate::{
    behaviour,
    behaviour::Behaviour,
    channel::Channel,
    network::{self, Error},
    wire::Message,
    Config,
  },
  futures::{FutureExt, StreamExt},
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
  std::future::Future,
  tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task::{JoinError, JoinHandle},
  },
  tracing::{debug, error, warn},
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
  handle: JoinHandle<()>,
}

impl Runloop {
  pub fn new(
    config: &Config,
    keypair: Keypair,
    netcmdtx: UnboundedSender<network::Command>,
  ) -> Result<Self, Error> {
    let (tx, rx) = Channel::new().split();
    Ok(Self {
      cmdtx: tx,
      handle: start_network_runloop(config, keypair, rx, netcmdtx)?,
    })
  }

  pub fn send_command(&self, command: Command) {
    self.cmdtx.send(command).expect("runloop thread died");
  }
}

impl Future for Runloop {
  type Output = Result<(), JoinError>;

  fn poll(
    mut self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<Self::Output> {
    self.handle.poll_unpin(cx)
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
          match event {
            SwarmEvent::Behaviour(event) => match event {
              behaviour::Event::MessageReceived(from, msg) =>
                netcmdtx.send(network::Command::AcceptMessage { from, msg })
                  .expect("network should outlive runloop"),
              _ => info!("{event:?}"),
            },
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
