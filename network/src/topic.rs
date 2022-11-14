//! HyParView: a membership protocol for reliable gossip-based broadcast
//! Leitão, João & Pereira, José & Rodrigues, Luís. (2007). 419-429.
//! 10.1109/DSN.2007.56.

use {
  crate::{
    wire::{Action, AddressablePeer, Message},
    Channel,
    Command,
  },
  bytes::Bytes,
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

#[derive(Debug)]
pub enum Event {
  MessageReceived(PeerId, Message),
  LocalAddressDiscovered(Multiaddr),
  ActivePeerConnected(PeerId),
  ActivePeerDisconnected(PeerId),
}

/// A topic represents an instance of HyparView p2p overlay.
pub struct Topic {
  config: Config,
  events: Channel<Event>,
  cmdtx: UnboundedSender<Command>,
  active_peers: Vec<AddressablePeer>,
}

impl Topic {
  pub(crate) fn new(config: Config, cmdtx: UnboundedSender<Command>) -> Self {
    // dial all bootstrap nodes
    for addr in config.bootstrap.iter() {
      cmdtx
        .send(Command::Connect(addr.clone()))
        .expect("lifetime of network should be longer than topic");
    }

    Self {
      active_peers: vec![],
      events: Channel::new(),
      config,
      cmdtx,
    }
  }

  pub(crate) fn inject_event(&self, event: Event) {
    self.events.send(event);
  }

  pub fn gossip(&self, data: Bytes) {
    for peer in &self.active_peers {
      self
        .cmdtx
        .send(Command::SendMessage {
          peer: peer.peer_id,
          msg: Message {
            topic: self.config.name.clone(),
            action: Action::Gossip(data.clone()),
          },
        })
        .expect("receiver is closed");
    }
  }
}

impl Stream for Topic {
  type Item = Event;

  fn poll_next(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Self::Item>> {
    self.events.poll_recv(cx)
  }
}
