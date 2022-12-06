use {
  crate::{Address, Exact, Repr},
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
pub enum ExpandedParam {
  Inline(Vec<u8>),
  AccountRef(Address, Vec<u8>),
  ProposalRef(Address, Vec<u8>),
  CalldataRef(String, Vec<u8>),
}

impl ExpandedParam {
  pub fn data(&self) -> &[u8] {
    match self {
      Self::Inline(v) => v,
      Self::AccountRef(_, v) => v,
      Self::ProposalRef(_, v) => v,
      Self::CalldataRef(_, v) => v,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Code {
  Inline(Vec<u8>),
  AccountRef(Address, String), // (address, entrypoint)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandedCode {
  pub code: Vec<u8>,
  pub entrypoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Predicate<R: Repr = Exact> {
  pub code: R::Code,
  pub params: Vec<R::Param>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PredicateTree<R: Repr = Exact> {
  Id(Predicate<R>),
  Not(Box<PredicateTree<R>>),
  And(Box<PredicateTree<R>>, Box<PredicateTree<R>>),
  Or(Box<PredicateTree<R>>, Box<PredicateTree<R>>),
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
