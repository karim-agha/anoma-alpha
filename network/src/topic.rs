//! HyParView: a membership protocol for reliable gossip-based broadcast
//! Leitão, João & Pereira, José & Rodrigues, Luís. (2007). 419-429.
//! 10.1109/DSN.2007.56.

use {
  futures::Stream,
  libp2p::{Multiaddr, PeerId},
  std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
  },
  thiserror::Error,
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
  ActivePeerConnected(PeerId),
  ActivePeerDisconnected(PeerId),
}

/// A topic represents an instance of HyparView p2p overlay.
pub struct Topic {
  events: VecDeque<Event>,
}

impl Stream for Topic {
  type Item = Event;

  fn poll_next(
    mut self: Pin<&mut Self>,
    _: &mut Context<'_>,
  ) -> Poll<Option<Self::Item>> {
    // propagate accumulated events
    if let Some(event) = self.events.pop_back() {
      return Poll::Ready(Some(event));
    }

    Poll::Pending
  }
}
