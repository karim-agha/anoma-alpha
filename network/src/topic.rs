//! HyParView: a membership protocol for reliable gossip-based broadcast
//! Leitão, João & Pereira, José & Rodrigues, Luís. (2007). 419-429.
//! 10.1109/DSN.2007.56.

use {
  crate::{wire::AddressablePeer, Channel},
  futures::Stream,
  libp2p::{Multiaddr, PeerId},
  std::{
    pin::Pin,
    task::{Context, Poll},
  },
  thiserror::Error,
  tokio::sync::mpsc::{error::SendError, unbounded_channel},
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
  identity: AddressablePeer,
  events: Channel<Event>,
}

impl Topic {
  pub(crate) fn new(identity: AddressablePeer) -> Self {
    let (tx, rx) = unbounded_channel();
    Self {
      events: (tx, rx),
      identity,
    }
  }

  pub(crate) fn inject_event(
    &self,
    event: Event,
  ) -> Result<(), SendError<Event>> {
    let (tx, _) = &self.events;
    tx.send(event)
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
    let (_, rx) = &mut self.events;
    match rx.poll_recv(cx) {
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
