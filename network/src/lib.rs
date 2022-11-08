use {futures::Stream, thiserror::Error};

#[derive(Debug, Error)]
pub enum NetworkError {}

#[derive(Debug, PartialEq)]
pub enum NetworkEvent {}

#[derive(Debug, Default)]
pub struct NetworkConfig {}

pub struct Network {}

impl Network {
  pub fn new(_config: NetworkConfig) -> Result<Self, NetworkError> {
    todo!()
  }
}

impl Stream for Network {
  type Item = NetworkEvent;

  fn poll_next(
    self: std::pin::Pin<&mut Self>,
    _: &mut std::task::Context<'_>,
  ) -> std::task::Poll<Option<Self::Item>> {
    todo!()
  }
}
