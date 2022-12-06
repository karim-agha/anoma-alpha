use {
  alloc::{str::FromStr, string::String},
  core::fmt::{Debug, Display},
  serde::{Deserialize, Serialize},
};

#[derive(Debug, Clone, PartialEq)]
pub enum AddressError {
  EmptyPath,
  EmptyPathSegment,
  MissingStartingSlash,
  InvalidEndingSlash,
  InvalidCharacter(char),
}

impl core::fmt::Display for AddressError {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    <Self as Debug>::fmt(self, f)
  }
}

// https://github.com/rust-lang/rust/issues/103765
#[cfg(not(target_family = "wasm"))]
impl std::error::Error for AddressError {}

#[derive(Clone)]
pub struct AncestorIterator {
  current: Address,
}

impl AncestorIterator {
  fn new(addr: Address) -> Self {
    Self { current: addr }
  }
}

impl Iterator for AncestorIterator {
  type Item = Address;

  fn next(&mut self) -> Option<Self::Item> {
    let path = &self.current.0;
    let slash_pos = path.len()
      - 1
      - path
        .chars()
        .rev()
        .position(|c| c == '/')
        .expect("address constructor is allowing invalid addresses");

    if slash_pos == 0 {
      None
    } else {
      self.current = Address(path.chars().take(slash_pos).collect());
      Some(self.current.clone())
    }
  }
}

/// Represents an address of an account.
#[derive(
  Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord,
)]
pub struct Address(String);

impl Address {
  /// Creates new address from a string.
  ///
  /// Valid paths are alphanumeric strings and slashes /.
  /// Paths must start with a slash and cannot end with a slash.
  ///
  /// Paths are hierarchical, e.g. /a/b is a child of /a
  /// so any modification attempt to /a/b will also trigger
  /// /a validity predicates as well as /a/b before it is allowed
  /// to go through.
  pub fn new(path: impl AsRef<str>) -> Result<Self, AddressError> {
    let path: String = path.as_ref().into();

    let mut segment_len = 0;
    let mut chars = path.chars();

    if let Some(first) = chars.next() {
      if first != '/' {
        return Err(AddressError::MissingStartingSlash);
      }
    } else {
      return Err(AddressError::EmptyPath);
    }

    for c in chars {
      if c == '/' {
        if segment_len == 0 {
          return Err(AddressError::EmptyPathSegment);
        }
        segment_len = 0;
      } else {
        segment_len += 1;
      }

      if !(c.is_alphanumeric() || c == '/') {
        return Err(AddressError::InvalidCharacter(c));
      }
    }

    if segment_len == 0 {
      return Err(AddressError::InvalidEndingSlash);
    }

    Ok(Self(path))
  }

  pub fn ancestors(&self) -> AncestorIterator {
    AncestorIterator::new(self.clone())
  }

  pub fn combine(
    &self,
    segment: impl AsRef<str>,
  ) -> Result<Self, AddressError> {
    let mut combined = self.0.clone();
    combined.push('/');
    combined.push_str(segment.as_ref());
    Address::new(combined)
  }

  pub fn is_parent_of(&self, other: &Self) -> bool {
    let mut prefix = self.0.clone();
    prefix.push('/');
    other.0.starts_with(&prefix)
  }
}

impl Display for Address {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    write!(f, "{}", self.0.as_str())
  }
}

impl Debug for Address {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    write!(f, "address({})", &self.0)
  }
}

impl From<Address> for String {
  fn from(pk: Address) -> Self {
    bs58::encode(pk.0).into_string()
  }
}

impl FromStr for Address {
  type Err = AddressError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    Address::new(s)
  }
}

#[cfg(test)]
mod tests {
  use crate::{address::AddressError, Address};

  #[test]
  fn construction() {
    assert!(Address::new("/token").is_ok());
    assert!(Address::new("/token/usda").is_ok());
    assert!(Address::new("/token/usda/walletaddr1").is_ok());

    assert_eq!(Address::new(""), Err(AddressError::EmptyPath));
    assert_eq!(
      Address::new("token"),
      Err(AddressError::MissingStartingSlash)
    );
    assert_eq!(
      Address::new("/token/"),
      Err(AddressError::InvalidEndingSlash)
    );
    assert_eq!(Address::new("//token"), Err(AddressError::EmptyPathSegment));
    assert_eq!(
      Address::new("/inval$id"),
      Err(AddressError::InvalidCharacter('$'))
    );
  }

  #[test]
  fn ancestors() -> Result<(), AddressError> {
    let address = Address::new("/token/usda/walletaddr1")?;
    let ancestors = address.ancestors();

    let mut ancestors_vec = vec![];
    for anc in ancestors {
      ancestors_vec.push(anc);
    }

    assert_eq!(ancestors_vec.len(), 2);
    assert_eq!(Address::new("/token")?, ancestors_vec[1]);
    assert_eq!(Address::new("/token/usda")?, ancestors_vec[0]);

    Ok(())
  }

  #[test]
  fn combine() -> Result<(), AddressError> {
    let token = Address::new("/token")?;
    let token_usda = token.combine("usda")?;
    let token_usda_wallet = token_usda.combine("walletaddr1")?;

    assert_eq!(token.ancestors().count(), 0);
    assert_eq!(token_usda.ancestors().count(), 1);
    assert_eq!(token_usda_wallet.ancestors().count(), 2);

    assert_eq!(token.to_string(), "/token".to_string());
    assert_eq!(token_usda.to_string(), "/token/usda".to_string());
    assert_eq!(
      token_usda_wallet.to_string(),
      "/token/usda/walletaddr1".to_string()
    );

    Ok(())
  }
}
