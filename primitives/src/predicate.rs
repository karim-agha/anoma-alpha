use {
  crate::Address,
  alloc::{boxed::Box, string::String, vec::Vec},
  core::fmt::Debug,
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
  AccountRef(Address, String),
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
