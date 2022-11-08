use {futures::Stream, libp2p::Multiaddr, thiserror::Error};

#[derive(Debug, Error)]
pub enum Error {}

#[derive(Debug)]
pub struct Config {
  pub name: String,
  pub bootstrap: Vec<Multiaddr>,
}

#[derive(Debug, PartialEq)]
pub enum Event {}

#[derive(PartialEq)]
pub struct Topic {}

impl Stream for Topic {
  type Item = Event;

  fn poll_next(
    self: std::pin::Pin<&mut Self>,
    _: &mut std::task::Context<'_>,
  ) -> std::task::Poll<Option<Self::Item>> {
    todo!()
  }
}
