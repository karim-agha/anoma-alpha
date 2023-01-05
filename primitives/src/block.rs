use {
  crate::{b58::ToBase58String, Transaction},
  alloc::{vec, vec::Vec},
  multihash::{Hasher, Multihash, MultihashDigest, Sha3_256},
  once_cell::sync::OnceCell,
  serde::{Deserialize, Serialize},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Block {
  pub height: u64,
  pub parent: Multihash,
  pub transactions: Vec<Transaction>,

  #[serde(skip)]
  hash_cache: OnceCell<Multihash>,
}

impl Block {
  pub fn new(parent: &Block, transactions: Vec<Transaction>) -> Self {
    Self {
      height: parent.height + 1,
      parent: *parent.hash(),
      transactions,
      hash_cache: Default::default(),
    }
  }

  pub fn zero() -> Self {
    Self {
      height: 0,
      parent: Multihash::default(),
      transactions: vec![],
      hash_cache: OnceCell::new(),
    }
  }

  pub fn hash(&self) -> &Multihash {
    self.hash_cache.get_or_init(|| {
      let mut hasher = Sha3_256::default();
      hasher.update(&rmp_serde::to_vec(self).unwrap());
      multihash::Code::Sha3_256.wrap(hasher.finalize()).unwrap()
    })
  }
}

impl core::fmt::Debug for Block {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    f.debug_struct("Block")
      .field("height", &self.height)
      .field("parent", &self.parent.to_b58())
      .field("hash", &self.hash().to_b58())
      .field("transactions", &self.transactions)
      .finish()
  }
}
