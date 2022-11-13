//! HyParView: a membership protocol for reliable gossip-based broadcast
//! Leitão, João & Pereira, José & Rodrigues, Luís. (2007). 419-429.
//! 10.1109/DSN.2007.56.

use {
  crate::{wire::AddressablePeer, Channel, Command},
  futures::Stream,
  libp2p::{Multiaddr, PeerId},
  std::{
    pin::Pin,
    task::{Context, Poll},
  },
  thiserror::Error,
  tokio::sync::mpsc::UnboundedSender,
};

#[derive(Debug, Error)]
pub enum Error {}

#[derive(Debug)]
pub struct Config {
  pub name: String,
  pub bootstrap: Vec<Multiaddr>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
  LocalAddressDiscovered(Multiaddr),
  ActivePeerConnected(PeerId),
  ActivePeerDisconnected(PeerId),
}

/// A topic represents an instance of HyparView p2p overlay.
pub struct Topic {
  config: Config,
  identity: AddressablePeer,
  events: Channel<Event>,
  cmdtx: UnboundedSender<Command>,
}

impl Topic {
  pub(crate) fn new(
    config: Config,
    identity: AddressablePeer,
    cmdtx: UnboundedSender<Command>,
  ) -> Self {
    // dial all bootstrap nodes
    for addr in config.bootstrap.iter() {
      cmdtx
        .send(Command::Connect(addr.clone()))
        .expect("lifetime of network should be longer than topic");
    }

    Self {
      events: Channel::new(),
      config,
      identity,
      cmdtx,
    }
  }

  pub(crate) fn inject_event(&self, event: Event) {
    self.events.send(event);
  }
}

impl Topic {
  fn append_local_address(&mut self, address: Multiaddr) {
    if !self.identity.addresses.contains(&address) {
      self.identity.addresses.push(address);
    }
  }
}

impl Stream for Topic {
  type Item = Event;

  fn poll_next(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Self::Item>> {
    match self.events.poll_recv(cx) {
      Poll::Ready(event) => match event {
        Some(event) => {
          match &event {
            Event::LocalAddressDiscovered(addr) => {
              self.append_local_address(addr.clone());
            }
            Event::ActivePeerConnected(_) => todo!(),
            Event::ActivePeerDisconnected(_) => todo!(),
          };
          Poll::Ready(Some(event))
        }
        None => Poll::Ready(None),
      },
      Poll::Pending => Poll::Pending,
    }
  }
}
