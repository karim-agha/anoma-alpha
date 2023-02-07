use {
  anoma_predicates_sdk::Address,
  serde::{Deserialize, Serialize},
  std::collections::BTreeSet,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Campaign {
  pub starts_at: u64,
  pub ends_at: u64,
  pub projects: BTreeSet<String>,
}

#[repr(transparent)]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Project(pub BTreeSet<Address>);

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Donation {}
