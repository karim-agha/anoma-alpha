use {
  crate::{Account, Address, Exact, Intent, PredicateTree, Repr},
  alloc::{collections::BTreeMap, vec::Vec},
  core::fmt::Debug,
  serde::{Deserialize, Serialize},
};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub enum AccountChange {
  CreateAccount(Account),
  ReplaceState(Vec<u8>),
  ReplacePredicates(PredicateTree<Exact>),
  DeleteAccount,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub enum ExpandedAccountChange {
  CreateAccount(Account),
  ReplaceState {
    current: Vec<u8>,
    proposed: Vec<u8>,
  },
  ReplacePredicates {
    current: PredicateTree<Exact>,
    proposed: PredicateTree<Exact>,
  },
  DeleteAccount {
    current: Account,
  },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction<R: Repr = Exact> {
  /// The intents that this transaction is trying to satisfy.
  ///
  /// Intents also carry authorizations to modify an account in
  /// for of signatures and other types of calldata.
  pub intents: Vec<Intent<R>>,

  /// Proposals for new contents of accounts under given addresses.
  ///
  /// If all predicates in all involved accounts and their parents
  /// evaluate to true, then the account contents will be replaced by
  /// this value.
  pub proposals: BTreeMap<Address, R::AccountChange>,
}
