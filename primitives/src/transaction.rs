use {
  crate::{Account, Address, Exact, Intent, PredicateTree, Repr},
  alloc::{collections::BTreeMap, vec::Vec},
  core::fmt::Debug,
  multihash::{Hasher, Multihash, MultihashDigest, Sha3_256},
  once_cell::sync::OnceCell,
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
  /// evaluate to true, then the account contents will be repmvxfvm laced by
  /// this value.
  pub proposals: BTreeMap<Address, R::AccountChange>,

  #[serde(skip)]
  hash_cache: OnceCell<Multihash>,
}

impl<R: Repr> Transaction<R> {
  pub fn new(
    intents: Vec<Intent<R>>,
    proposals: BTreeMap<Address, R::AccountChange>,
  ) -> Self {
    Self {
      intents,
      proposals,
      hash_cache: OnceCell::new(),
    }
  }

  pub fn hash(&self) -> &Multihash {
    self.hash_cache.get_or_init(|| {
      let mut hasher = Sha3_256::default();
      hasher.update(&rmp_serde::to_vec(self).unwrap());
      multihash::Code::Sha3_256.wrap(hasher.finalize()).unwrap()
    })
  }
}

impl<R: Repr> PartialEq for Transaction<R> {
  fn eq(&self, other: &Self) -> bool {
    self.hash() == other.hash()
  }
}

impl<R: Repr> Eq for Transaction<R> {}

impl<R: Repr> core::hash::Hash for Transaction<R> {
  fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
    self.hash().hash(state)
  }
}
