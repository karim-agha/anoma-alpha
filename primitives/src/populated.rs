use {
  crate::Address,
  alloc::{boxed::Box, collections::BTreeMap, string::String, vec::Vec},
  multihash::Multihash,
  serde::{Deserialize, Serialize},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PopulatedParam {
  Inline(Vec<u8>),
  AccountRef(Address, Vec<u8>),
  ProposalRef(Address, Vec<u8>),
  CalldataRef(String, Vec<u8>),
}

impl PopulatedParam {
  pub fn data(&self) -> &[u8] {
    match self {
      PopulatedParam::Inline(v) => v,
      PopulatedParam::AccountRef(_, v) => v,
      PopulatedParam::ProposalRef(_, v) => v,
      PopulatedParam::CalldataRef(_, v) => v,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PopulatedCode {
  pub code: Vec<u8>,
  pub entrypoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PopulatedPredicate {
  pub code: PopulatedCode,
  pub params: Vec<PopulatedParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PopulatedPredicateTree {
  Id(PopulatedPredicate),
  Not(Box<PopulatedPredicateTree>),
  And(Box<PopulatedPredicateTree>, Box<PopulatedPredicateTree>),
  Or(Box<PopulatedPredicateTree>, Box<PopulatedPredicateTree>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PopulatedIntent {
  pub recent_blockhash: Multihash,
  pub expectations: PopulatedPredicateTree,
  pub calldata: BTreeMap<String, Vec<u8>>,
  pub hash: Multihash,
}
