use {
  crate::{Account, Address, Intent, PredicateTree, Trigger},
  alloc::{collections::BTreeMap, vec::Vec},
  core::fmt::Debug,
  serde::{Deserialize, Serialize},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccountChange {
  CreateAccount(Account),
  ReplaceState(Vec<u8>),
  ReplacePredicates(PredicateTree),
  DeleteAccount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
  /// The intents that this transaction is trying to satisfy.
  ///
  /// Intents also carry authorizations to modify an account in
  /// for of signatures and other types of calldata.
  pub intents: Vec<Intent>,

  /// Proposals for new contents of accounts under given addresses.
  ///
  /// If all predicates in all involved accounts and their parents
  /// evaluate to true, then the account contents will be replaced by
  /// this value.
  pub proposals: BTreeMap<Address, AccountChange>,
}

#[derive(Debug, Clone)]
pub enum TriggerRef<'a> {
  Intent(&'a Intent),
  Proposal(&'a Address, &'a AccountChange),
}

impl Transaction {
  /// Looks up a predicate trigger in the transaction.
  ///
  /// This may invoked by predicates to find out the context in which
  /// they are called, like for example in the case of signature validation,
  /// where we need to know which hash should be used to validate a signature.
  pub fn get(&self, trigger: &Trigger) -> Option<TriggerRef<'_>> {
    match trigger {
      Trigger::Intent(hash) => {
        for intent in &self.intents {
          if intent.hash() == hash {
            return Some(TriggerRef::Intent(intent));
          }
        }
        None
      }
      Trigger::Proposal(address) => {
        for (addr, change) in &self.proposals {
          if addr == address {
            return Some(TriggerRef::Proposal(addr, change));
          }
        }
        None
      }
    }
  }
}
