use {
  crate::behviour::Behaviour,
  libp2p::{Multiaddr, Swarm},
  tracing::error,
};

#[derive(Debug, Clone)]
pub(crate) enum Command {
  Connect(Multiaddr),
}

impl Command {
  pub fn execute(self, swarm: &mut Swarm<Behaviour>) {
    match self {
      Command::Connect(addr) => swarm_connect(swarm, addr),
    }
  }
}

fn swarm_connect(swarm: &mut Swarm<Behaviour>, addr: Multiaddr) {
  if let Err(e) = swarm.dial(addr) {
    error!("dial error: {e:?}");
  }
}
