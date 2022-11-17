use {
  crate::{
    channel::Channel,
    stream::SubstreamHandler,
    wire::{AddressablePeer, Message},
    Config,
  },
  libp2p::{
    core::{connection::ConnectionId, transport::ListenerId, ConnectedPoint},
    multiaddr::Protocol,
    swarm::{
      NetworkBehaviour,
      NetworkBehaviourAction,
      NotifyHandler,
      PollParameters,
    },
    Multiaddr,
    PeerId,
  },
  std::{
    net::{Ipv4Addr, Ipv6Addr},
    task::{Context, Poll},
  },
  tracing::debug,
};

/// Represents a behaviour level event that is emitted
/// by the protocol. Events are ordered by their occurance
/// time and accessed by polling the network stream.
#[derive(Debug, Clone)]
pub(crate) enum Event {
  /// Emitted when the network discovers new public address pointing to the
  /// current node.
  LocalAddressDiscovered(Multiaddr),

  /// Emitted when a connection is created between two peers.
  ///
  /// This is emitted only once regardless of the number of HyParView
  /// overlays the two peers share. All overlapping overlays share the
  /// same connection.
  ConnectionEstablished {
    /// True if this node initiated the connection, otherwise false.
    dialer: bool,

    /// Address and identity of the remote peer.
    peer: AddressablePeer,
  },

  /// Emitted when a connection is closed between two peers.
  ///
  /// This is emitted when the last HyparView overlay between the two
  /// peers is destroyed and they have no common topics anymore. Also
  /// emitted when the connection is dropped due to transport layer failure.
  ConnectionClosed(PeerId),

  /// Emitted when a message is received on the wire from a connected peer.
  MessageReceived(PeerId, Message),
}

pub(crate) struct Behaviour {
  config: Config,
  events: Channel<Event>,
  outmsgs: Channel<(PeerId, Message)>,
}

impl Behaviour {
  pub fn new(config: Config) -> Self {
    Self {
      config,
      events: Channel::new(),
      outmsgs: Channel::new(),
    }
  }

  pub fn send_to(&self, peer: PeerId, msg: Message) {
    self.outmsgs.send((peer, msg));
  }
}

impl NetworkBehaviour for Behaviour {
  type ConnectionHandler = SubstreamHandler;
  type OutEvent = Event;

  fn new_handler(&mut self) -> Self::ConnectionHandler {
    SubstreamHandler::new(&self.config)
  }

  fn inject_event(
    &mut self,
    peer_id: PeerId,
    connection: ConnectionId,
    event: Message,
  ) {
    debug!("injecting event from {peer_id:?} [conn {connection:?}]: {event:?}");
    self.events.send(Event::MessageReceived(peer_id, event));
  }

  /// Informs the behaviour about a newly established connection to a peer.
  fn inject_connection_established(
    &mut self,
    peer_id: &PeerId,
    _: &ConnectionId,
    endpoint: &ConnectedPoint,
    _: Option<&Vec<Multiaddr>>,
    other_established: usize,
  ) {
    // signal only if it is the first connection to this peer,
    // otherwise it will be immediately closed by libp2p as it
    // will exceed the maximum allowed connections between peers (1).s
    if other_established == 0 {
      self.events.send(Event::ConnectionEstablished {
        dialer: matches!(endpoint, ConnectedPoint::Dialer { .. }),
        peer: AddressablePeer {
          peer_id: *peer_id,
          addresses: [endpoint.get_remote_address().clone()]
            .into_iter()
            .collect(),
        },
      });
    }
  }

  /// Informs the behaviour about a closed connection to a peer.
  ///
  /// A call to this method is always paired with an earlier call to
  /// [`NetworkBehaviour::inject_connection_established`] with the same peer ID,
  /// connection ID and endpoint.
  fn inject_connection_closed(
    &mut self,
    peerid: &PeerId,
    _: &ConnectionId,
    _: &ConnectedPoint,
    _: SubstreamHandler,
    remaining_established: usize,
  ) {
    if remaining_established == 0 {
      self.events.send(Event::ConnectionClosed(*peerid));
    }
  }

  fn inject_new_listen_addr(&mut self, _: ListenerId, addr: &Multiaddr) {
    // it does not make sense to advertise localhost addresses to remote nodes
    if !is_local_address(addr) {
      self
        .events
        .send(Event::LocalAddressDiscovered(addr.clone()));
    }
  }

  fn poll(
    &mut self,
    cx: &mut Context<'_>,
    _: &mut impl PollParameters,
  ) -> Poll<NetworkBehaviourAction<Self::OutEvent, Self::ConnectionHandler>> {
    // propagate any generated events to the network API.
    if let Poll::Ready(Some(event)) = self.events.poll_recv(cx) {
      return Poll::Ready(NetworkBehaviourAction::GenerateEvent(event));
    }

    // Send next message from outbound queue by forwarding it to the
    // connection handler associated with the given peer id.
    if let Poll::Ready(Some((peer, msg))) = self.outmsgs.poll_recv(cx) {
      return Poll::Ready(NetworkBehaviourAction::NotifyHandler {
        peer_id: peer,
        handler: NotifyHandler::Any,
        event: msg,
      });
    }

    Poll::Pending
  }
}

/// This handles the case when the swarm api starts listening on
/// 0.0.0.0 and one of the addresses is localhost. Localhost is
/// meaningless when advertised to remote nodes, so its omitted
/// when counting local addresses
fn is_local_address(addr: &Multiaddr) -> bool {
  addr.iter().any(|p| {
    // fileter out all localhost addresses
    if let Protocol::Ip4(addr) = p {
      addr == Ipv4Addr::LOCALHOST || addr == Ipv4Addr::UNSPECIFIED
    } else if let Protocol::Ip6(addr) = p {
      addr == Ipv6Addr::LOCALHOST || addr == Ipv6Addr::UNSPECIFIED
    } else {
      false
    }
  })
}
