use {
  anoma_primitives::{Address, Code, Exact, Param, PredicateTree, Repr},
  serde::{Deserialize, Serialize},
  std::collections::HashMap,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ParamPattern {
  Exact(Param),
  Inline(String),
  AccountRef(String),
  ProposalRef(String),
  CalldataRef(String),
}

#[derive(Debug, Clone)]
pub enum MatchValue {
  Address(Address),
  Bytes(Vec<u8>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Query;
impl Repr for Query {
  type AccountChange = ();
  type Code = Code;
  type Param = ParamPattern;
}

/// Used to match expression tree patterns in intents.
/// The primary use case is for solvers to identify intents they are interested
/// in solving from the incoming intent gossip stream.
pub trait ExpressionPattern {
  fn matches(
    &self,
    template: PredicateTree<Query>,
  ) -> Option<HashMap<String, MatchValue>>;
}

impl ExpressionPattern for PredicateTree<Exact> {
  fn matches(
    &self,
    _template: PredicateTree<Query>,
  ) -> Option<HashMap<String, MatchValue>> {
    None
  }
}
