use {
  crate::Predicate,
  ed25519_dalek::{Keypair, SecretKey, Signature, Signer},
  multihash::{Hasher, Multihash, MultihashDigest, Sha3_256},
  num::BigUint,
  once_cell::sync::OnceCell,
  serde::{Deserialize, Serialize},
};

/// Intents are partial signatures created by users describing what state
/// transition they want to achieve.
#[derive(Debug, Serialize, Deserialize)]
pub struct Intent {
  /// Opaque bytestrings that need to be understood solvers and
  /// predicates within one app. Solvers use those values as hints
  /// to find solutions that satisfy predicates.
  value: Vec<u8>,

  /// A list of predicates that must evaluate to `true` when included
  /// in a transaction before a state transition is premitted. A transaction
  /// must satisfy all account and intent predicates before it is allowed to
  /// mutate sate in accounts.
  predicates: Vec<Predicate>,

  /// The total amount of fees the intent producer is willing to pay for
  /// relaying and solving this intent. Actual distribution of fees between
  /// relayers (hops) and the solver is described in the envelope that carries
  /// intents between hops.
  ///
  /// Each relayer must make sure to leave enough of the remaining fee for
  /// other relayers to incentivise further relays and all relayers through
  /// all hops must make sure that there is enough fee remaining for the
  /// solver to have an incentive to provide a solution for this intent.
  fees: BigUint,

  /// A list of signatures attached to an intent.
  /// ```
  /// signature = sign(prvkey, self.partial_hash());
  /// ```
  signatures: Vec<Signature>,

  /// Hash of all elements except signatures.
  ///
  /// This value is used to compute signatures attached to an intent.
  /// This value is computed only once on first call and then cached
  /// for subsequent invocations.
  #[serde(skip)]
  partial_hash: OnceCell<Multihash>,
}

impl Intent {
  /// Hash of all elements except signatures.
  ///
  /// This value is used to compute signatures attached to an intent.
  /// This value is computed only once on first call and then cached
  /// for subsequent invocations.
  pub fn partial_hash(&self) -> &Multihash {
    self.partial_hash.get_or_init(|| {
      let mut hasher = Sha3_256::default();
      hasher.update(&bincode::serialize(&self.value).unwrap());
      hasher.update(&bincode::serialize(&self.value).unwrap());
      hasher.update(&bincode::serialize(&self.predicates).unwrap());
      hasher.update(&bincode::serialize(&self.fees).unwrap());
      multihash::Code::Sha3_256.wrap(hasher.finalize()).unwrap()
    })
  }

  /// Given a secret key, it appends a new signature to the intent.
  ///
  /// usage:
  /// ```
  /// let mut intent = Intent { ... };
  /// intent.attach_signature(privkey1);
  /// intent.attach_signature(privkey2);
  /// ```
  pub fn append_signature(&mut self, secret: SecretKey) {
    let keypair = Keypair {
      public: (&secret).into(),
      secret,
    };

    self
      .signatures
      .push(keypair.sign(&self.partial_hash().to_bytes()));
  }
}
