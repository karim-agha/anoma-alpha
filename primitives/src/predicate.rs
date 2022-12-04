use {
  crate::Address,
  alloc::{boxed::Box, string::String, vec::Vec},
  core::fmt::Debug,
  multihash::Multihash,
  serde::{Deserialize, Serialize},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Param {
  Inline(Vec<u8>),
  AccountRef(Address),
  ProposalRef(Address),
  CalldataRef(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Code {
  Inline(Vec<u8>),
  AccountRef(Address, String), // (address, entrypoint)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Predicate {
  pub code: Code,
  pub params: Vec<Param>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PredicateTree {
  Id(Predicate),
  Not(Box<PredicateTree>),
  And(Box<PredicateTree>, Box<PredicateTree>),
  Or(Box<PredicateTree>, Box<PredicateTree>),
}

/// Specifies the reason a predicate is being invoked.
///
/// Prediactes are invoked for two reasons:
///
/// 1. When they are part of an intent, that is included in a transaction, to
/// check if intent expectations are satisfied.
///
/// 2. When an account is mutated, to check
/// if the newly proposed account value satisfies the account requirements or
/// any of its parents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Trigger {
  Intent(Multihash),
  Proposal(Address),
}
