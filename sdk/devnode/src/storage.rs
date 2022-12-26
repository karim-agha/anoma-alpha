use {
  anoma_primitives::{Account, Address},
  anoma_vm::{State, StateDiff},
  once_cell::sync::OnceCell,
  rmp_serde::{from_slice, to_vec},
  std::path::PathBuf,
};

pub struct OnDiskStateStore {
  tree: sled::Tree,
}

impl OnDiskStateStore {
  pub fn new(path: &PathBuf, name: &str) -> Result<Self, sled::Error> {
    static DB: OnceCell<sled::Db> = OnceCell::new();
    Ok(Self {
      tree: DB
        .get_or_init(|| sled::open(path).expect("failed to open db"))
        .open_tree(name)?,
    })
  }
}

impl State for OnDiskStateStore {
  fn get(&self, address: &Address) -> Option<Account> {
    match self.tree.get(address.to_string()).expect("db io error") {
      Some(bytes) => Some(from_slice(&bytes).expect("db corrupt")),
      None => None,
    }
  }

  fn apply(&mut self, diff: StateDiff) {
    for (acc, item) in diff.iter() {
      match item {
        Some(val) => {
          self
            .tree
            .insert(acc.to_string(), to_vec(val).expect("serialization failed"))
            .expect("db error");
        }
        None => {
          self.tree.remove(acc.to_string()).expect("db error");
        }
      }
    }
    self.tree.flush().expect("db tree flush failed");
  }
}
