use {
  anoma_primitives::{Account, Address},
  anoma_vm::{State, StateDiff},
  rmp_serde::{from_slice, to_vec},
  std::path::PathBuf,
};

pub struct OnDiskStateStore {
  db: sled::Db,
}

impl OnDiskStateStore {
  pub fn new(path: &PathBuf) -> Result<Self, sled::Error> {
    Ok(Self {
      db: sled::open(path)?,
    })
  }
}

impl State for OnDiskStateStore {
  fn get(&self, address: &Address) -> Option<Account> {
    match self
      .db
      .get(&to_vec(address).expect(""))
      .expect("db io error")
    {
      Some(bytes) => Some(from_slice(&bytes).expect("db corrupt")),
      None => None,
    }
  }

  fn apply(&mut self, _: StateDiff) {
    todo!()
  }
}
