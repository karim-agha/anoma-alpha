use alloc::string::String;
use alloc::vec::Vec;

pub trait ToBase58String {
  fn to_b58(&self) -> String;
}

impl<const S: usize> ToBase58String for multihash::MultihashGeneric<S> {
  fn to_b58(&self) -> String {
    bs58::encode(self.to_bytes()).into_string()
  }
}

impl ToBase58String for ed25519_dalek::Signature {
  fn to_b58(&self) -> String {
    bs58::encode(self.to_bytes()).into_string()
  }
}

impl ToBase58String for &[u8] {
  fn to_b58(&self) -> String {
    bs58::encode(self).into_string()
  }
}

impl<const N: usize> ToBase58String for [u8; N] {
  fn to_b58(&self) -> String {
    bs58::encode(self).into_string()
  }
}

impl ToBase58String for Vec<u8> {
  fn to_b58(&self) -> String {
    bs58::encode(self).into_string()
  }
}
