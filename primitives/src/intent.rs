use {
  crate::{PredicateTree, ToBase58String},
  alloc::{collections::BTreeMap, string::String, vec::Vec},
  core::fmt::Debug,
  multihash::{Hasher, Multihash, MultihashDigest, Sha3_256},
  once_cell::sync::OnceCell,
  serde::{Deserialize, Serialize},
};

/// Intents are partial transactions created by users describing what state
/// transition they want to achieve.
#[derive(Clone, Serialize, Deserialize)]
pub struct Intent {
  /// Hash of a block within the last 2 epochs.
  /// Intents that have this value pointing to a
  /// block that is older then 2 epochs are expired
  /// and rejected by the chain.
  pub recent_blockhash: Multihash,
  pub expectations: PredicateTree,

  /// If any of the calldata entries is a signature,
  /// it should sign the recent_blockhash value.
  pub calldata: BTreeMap<String, Vec<u8>>,

  #[serde(skip)]
  hash_cache: OnceCell<Multihash>,
}

impl Intent {
  pub fn new(
    recent_blockhash: Multihash,
    expectations: PredicateTree,
    calldata: BTreeMap<String, Vec<u8>>,
  ) -> Self {
    Self {
      recent_blockhash,
      expectations,
      calldata,
      hash_cache: OnceCell::new(),
    }
  }
}

impl Debug for Intent {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    f.debug_struct("Intent")
      .field("expectations", &self.expectations)
      .field("calldata", &self.calldata)
      .field("hash", &self.hash().to_b58())
      .finish()
  }
}

impl Intent {
  /// Hash of all elements except signatures.
  ///
  /// This value is used to compute signatures attached to an intent.
  /// This value is computed only once on first call and then cached
  /// for subsequent invocations.
  pub fn hash(&self) -> &Multihash {
    self.hash_cache.get_or_init(|| {
      let mut hasher = Sha3_256::default();
      hasher.update(&bincode::serialize(&self.expectations).unwrap());
      hasher.update(&bincode::serialize(&self.calldata).unwrap());
      multihash::Code::Sha3_256.wrap(hasher.finalize()).unwrap()
    })
  }
}
