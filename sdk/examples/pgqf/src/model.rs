use {
  anoma_predicates_sdk::Address,
  serde::{Deserialize, Serialize},
  std::collections::HashSet,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Campaign {
  pub starts_at: u64,
  pub ends_at: u64,
  pub projects: HashSet<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Project {
  pub donors: Vec<(Address, u64)>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Donation {}
