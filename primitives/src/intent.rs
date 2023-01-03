use {
  crate::{b58::ToBase58String, Exact, PredicateTree, Repr},
  alloc::{collections::BTreeMap, string::String, vec::Vec},
  core::fmt::Debug,
  multihash::{Hasher, Multihash, MultihashDigest, Sha3_256},
  once_cell::sync::OnceCell,
  serde::{Deserialize, Serialize},
};

/// Represents a list of arbitary key-value data attached
/// to an intent. This can carry things like signatures,
/// or any other arbitrary input parameters to intents, etc.
pub type Calldata = BTreeMap<String, Vec<u8>>;

/// Intents are partial transactions created by users describing what state
/// transition they want to achieve.
#[derive(Clone, Serialize, Deserialize)]
pub struct Intent<R: Repr = Exact> {
  /// Hash of a block within the last 2 epochs.
  /// Intents that have this value pointing to a
  /// block that is older then 2 epochs are expired
  /// and rejected by the chain.
  pub recent_blockhash: Multihash,
  pub expectations: PredicateTree<R>,

  /// If any of the calldata entries is a signature,
  /// it should sign the recent_blockhash value.
  pub calldata: Calldata,

  #[serde(skip)]
  hash_cache: OnceCell<Multihash>,
}

impl<R: Repr> Intent<R> {
  pub fn new(
    recent_blockhash: Multihash,
    expectations: PredicateTree<R>,
  ) -> Self {
    Self {
      recent_blockhash,
      expectations,
      calldata: Calldata::new(),
      hash_cache: OnceCell::new(),
    }
  }

  pub fn with_calldata(
    recent_blockhash: Multihash,
    expectations: PredicateTree<R>,
    calldata: Calldata,
  ) -> Self {
    Self {
      recent_blockhash,
      expectations,
      calldata,
      hash_cache: OnceCell::new(),
    }
  }
}

impl<R: Repr> Debug for Intent<R> {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    f.debug_struct("Intent")
      .field("expectations", &self.expectations)
      .field("calldata", &self.calldata)
      .field("hash", &self.hash().to_b58())
      .finish()
  }
}

impl<R: Repr> Intent<R> {
  /// Hash of the intent that uniquely identitifies it.
  pub fn hash(&self) -> &Multihash {
    self.hash_cache.get_or_init(|| {
      let mut hasher = Sha3_256::default();
      hasher.update(&rmp_serde::to_vec(self).unwrap());
      multihash::Code::Sha3_256.wrap(hasher.finalize()).unwrap()
    })
  }

  /// Hash of the contents of the intent without calldata.
  ///
  /// This hash is used as the message when signatures need
  /// to be attached to intents.
  pub fn signing_hash(&self) -> &Multihash {
    self.hash_cache.get_or_init(|| {
      let mut hasher = Sha3_256::default();
      hasher.update(&rmp_serde::to_vec(&self.recent_blockhash).unwrap());
      hasher.update(&rmp_serde::to_vec(&self.expectations).unwrap());
      multihash::Code::Sha3_256.wrap(hasher.finalize()).unwrap()
    })
  }
}
