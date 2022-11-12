use {
  crate::{stream::SubstreamHandler, wire::Command, Channel, Event},
  libp2p::{
    core::{connection::ConnectionId, transport::ListenerId},
    multiaddr::Protocol,
    swarm::{NetworkBehaviour, NetworkBehaviourAction, PollParameters},
    Multiaddr,
    PeerId,
  },
  std::{
    net::{Ipv4Addr, Ipv6Addr},
    task::{Context, Poll},
  },
  tokio::sync::mpsc::unbounded_channel,
};

pub struct Behaviour {
  events: Channel<Event>,
}

impl Behaviour {
  pub(crate) fn new() -> Self {
    Self {
      events: unbounded_channel(),
    }
  }
}

impl NetworkBehaviour for Behaviour {
  type ConnectionHandler = SubstreamHandler;
  type OutEvent = Event;

  fn new_handler(&mut self) -> Self::ConnectionHandler {
    todo!()
  }

  fn inject_event(
    &mut self,
    peer_id: PeerId,
    connection: ConnectionId,
    event: Command,
  ) {
    todo!()
  }

  fn inject_new_listen_addr(&mut self, _: ListenerId, addr: &Multiaddr) {
    // it does not make sense to advertise localhost addresses to remote nodes
    if !is_local_address(addr) {
      let (tx, _) = &self.events;
      tx.send(Event::LocalAddressDiscovered(addr.clone()))
        .expect("sender and receiver are owned by this instance");
    }
  }

  fn poll(
    &mut self,
    cx: &mut Context<'_>,
    _: &mut impl PollParameters,
  ) -> Poll<NetworkBehaviourAction<Self::OutEvent, Self::ConnectionHandler>> {
    let (_, rx) = &mut self.events;
    if let Poll::Ready(Some(event)) = rx.poll_recv(cx) {
      return Poll::Ready(NetworkBehaviourAction::GenerateEvent(event));
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
