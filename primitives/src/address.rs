use {
  curve25519_dalek::edwards::CompressedEdwardsY,
  ed25519_dalek::{PublicKey, SecretKey},
  multihash::{Hasher, Sha3_256},
  rand::RngCore,
  serde::{Deserialize, Serialize},
  std::{
    fmt::{Debug, Display},
    ops::Deref,
    str::FromStr,
  },
  thiserror::Error,
};

/// Represents an address of an account.
///
/// The same address could either represent a user wallet that
/// has a corresponding private key on the ed25519 curve (externally owned)
/// or a app/contract account that is not on the curve and is writable
/// only by the app owning it.
///
/// Accounts may optionally store data, like balances, etc.
///
/// Here's an example that involves using accounts:
///
///   - Say we have a user identified by address 0xAAA
///   - We also have an asset address identified by address 0xBBB
///   - in this case if we want to get user's account balance we do:
///     - address(0xAAA).derive(0xBBB) gives us 0xCCC
///     - read the contents of 0xCCC
///     - the Validity predicate on the derived account should be:
///       - if the new balance value is lower than current value, assert that
///         the tx contains 0xAAA's signature.
#[derive(
  Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct Address([u8; 32]);

impl Address {
  /// Given a list of seeds this method will generate a new
  /// derived address that is not on the Ed25519 curve
  /// (no private key exists for the resulting address).
  ///
  /// This method is used to generate addresses that are
  /// related to some original address but manipulated by
  /// contracts.
  ///
  /// The same set of seeds will always return the same
  /// derived address, so it can be used as a hashmap
  /// in contracts.
  pub fn derive(&self, seeds: &[&[u8]]) -> Self {
    let mut bump: u64 = 0;
    loop {
      let mut hasher = Sha3_256::default();
      hasher.update(&self.0);
      for seed in seeds.iter() {
        hasher.update(seed);
      }
      hasher.update(&bump.to_le_bytes());
      let key = Address(hasher.finalize().try_into().unwrap());
      if !key.has_private_key() {
        return key;
      } else {
        bump += 1;
      }
    }
  }

  /// Checks if the given address lies on the Ed25519 elliptic curve.
  ///
  /// When true, then it means that there exists a private key that
  /// make up together a valid Ed25519 keypair. Otherwise, when false
  /// it means that there is no corresponding valid private key.
  ///
  /// This is useful in cases we want to make sure that an account
  /// could not be ever modified except by the contract owning it, as
  /// it is not possible to have a signer of a transaction that will
  /// give write access to an account.
  fn has_private_key(&self) -> bool {
    CompressedEdwardsY::from_slice(&self.0)
      .decompress()
      .is_some()
  }
}

impl AsRef<[u8]> for Address {
  fn as_ref(&self) -> &[u8] {
    &self.0
  }
}

impl Deref for Address {
  type Target = [u8];

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl Display for Address {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", bs58::encode(self.0).into_string())
  }
}

impl Debug for Address {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "address({})", bs58::encode(self.0).into_string())
  }
}

impl From<Address> for String {
  fn from(pk: Address) -> Self {
    bs58::encode(pk.0).into_string()
  }
}

impl FromStr for Address {
  type Err = bs58::decode::Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let mut bytes = [0u8; 32];
    bs58::decode(s).into(&mut bytes)?;
    Ok(Self(bytes))
  }
}

impl TryFrom<&str> for Address {
  type Error = bs58::decode::Error;

  fn try_from(value: &str) -> Result<Self, Self::Error> {
    FromStr::from_str(value)
  }
}

impl From<PublicKey> for Address {
  fn from(p: PublicKey) -> Self {
    Self(*p.as_bytes())
  }
}

/// Represents a wallet account on the ed25519 curve that can
/// be controlled by an external wallet and cannot be owned by
/// a program.
pub struct Keypair(ed25519_dalek::Keypair);

impl Keypair {
  pub fn unique() -> Self {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 32];
    rng.fill_bytes(&mut bytes);
    (bytes.as_slice()).try_into().unwrap()
  }

  pub fn public(&self) -> Address {
    self.0.public.into()
  }

  pub fn secret(&self) -> &SecretKey {
    &self.0.secret
  }
}

impl Clone for Keypair {
  fn clone(&self) -> Self {
    Self(ed25519_dalek::Keypair::from_bytes(&self.0.to_bytes()).unwrap())
  }
}

impl Deref for Keypair {
  type Target = ed25519_dalek::Keypair;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl std::fmt::Debug for Keypair {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_tuple("Keypair").field(&self.0.public).finish()
  }
}

impl Display for Keypair {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      "Keypair({})",
      bs58::encode(self.0.public.as_bytes()).into_string()
    )
  }
}

impl From<ed25519_dalek::Keypair> for Keypair {
  fn from(k: ed25519_dalek::Keypair) -> Self {
    Self(k)
  }
}

impl From<Keypair> for ed25519_dalek::Keypair {
  fn from(kp: Keypair) -> Self {
    ed25519_dalek::Keypair::from_bytes(&kp.0.to_bytes()).unwrap()
  }
}

#[derive(Debug, Error)]
pub enum KeypairError {
  #[error("Failed parsing base58 string: {0:?}")]
  Base58ParseError(bs58::decode::Error),

  #[error("Ed25519 error: {0}")]
  Ed25519Error(#[from] ed25519_dalek::ed25519::Error),
}

impl TryFrom<&[u8]> for Keypair {
  type Error = KeypairError;

  fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
    let secret = SecretKey::from_bytes(value)?;
    let public: PublicKey = (&secret).into();
    Ok(Self(ed25519_dalek::Keypair { secret, public }))
  }
}
