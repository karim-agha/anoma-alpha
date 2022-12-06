use {
  crate::{
    Account,
    Address,
    Exact,
    Expanded,
    Intent,
    PredicateTree,
    Repr,
    Trigger,
  },
  alloc::{collections::BTreeMap, vec::Vec},
  core::fmt::Debug,
  serde::{Deserialize, Serialize},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccountChange {
  CreateAccount(Account),
  ReplaceState(Vec<u8>),
  ReplacePredicates(PredicateTree<Exact>),
  DeleteAccount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

pub type ExpandedTransaction = Transaction<Expanded>;

#[derive(Debug, Clone)]
pub enum TriggerRef<'a, R: Repr> {
  Intent(&'a Intent<R>),
  Proposal(&'a Address, &'a R::AccountChange),
}

impl<R: Repr> Transaction<R> {
  /// Looks up a predicate trigger in the transaction.
  ///
  /// This may invoked by predicates to find out the context in which
  /// they are called, like for example in the case of signature validation,
  /// where we need to know which hash should be used to validate a signature.
  pub fn get(&self, trigger: &Trigger) -> Option<TriggerRef<'_, R>> {
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
