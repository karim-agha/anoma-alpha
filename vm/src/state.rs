use {
  anoma_primitives::{Account, Address},
  serde::{Deserialize, Serialize},
  std::collections::{BTreeMap, BTreeSet, HashMap},
};

/// Represents a change in Blockchain Accounts state.
///
/// Statediff are meant to be accumulated and logically the entire
/// state of the blockchain is the result of cumulative application
/// of consecutive state diffs.
///
/// A transaction produces a statediff, blocks produce state diffs
/// which are all its transactions state diffs merged together.
/// If all blocks state diffs are also merged together, then the
/// resulting state diff would represent the entire state of the system.
///
/// StateDiff is also the basic unit of state sync through IPFS/bitswap.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateDiff {
  upserts: BTreeMap<Address, Account>,
  deletes: BTreeSet<Address>,
}

impl StateDiff {
  /// Inserts or updates an account under a given address.
  ///
  /// If the state diff had an account stored under this address
  /// then the old value is returned, otherwise `None` is returned.
  pub fn set(&mut self, address: Address, account: Account) -> Option<Account> {
    self.deletes.remove(&address);
    self.upserts.insert(address, account)
  }

  /// Removes an account under a given address.
  ///
  /// If the state diff contained an account at the given address
  /// then the removed value is returned, otherwise `None`.
  pub fn remove(&mut self, address: &Address) -> Option<Account> {
    self.deletes.insert(address.clone());
    self.upserts.remove(address)
  }

  /// Merges a state diff with a newer diff.
  ///
  /// Applying the resulting diff is equivalent to
  /// applyting the two merged diff consecutively on
  /// any state store.
  pub fn merge(self, newer: StateDiff) -> StateDiff {
    let mut upserts = self.upserts;
    let mut deletes = self.deletes;
    for (addr, acc) in newer.upserts {
      deletes.remove(&addr);
      upserts.insert(addr, acc);
    }
    for addr in newer.deletes {
      deletes.insert(addr);
    }
    StateDiff { upserts, deletes }
  }

  /// Iterate over all account changes in a state diff.
  ///
  /// There are two variants of changes:
  ///   1. (Address, Account) => Means that account under a given address was
  ///      created or changed its contents.
  ///   2. (Address, None) => Means that account under a given address was
  ///      deleted.
  pub fn iter(&self) -> impl Iterator<Item = (&Address, Option<&Account>)> {
    self
      .upserts
      .iter()
      .map(|(addr, acc)| (addr, Some(acc)))
      .chain(self.deletes.iter().map(|addr| (addr, None)))
  }
}

impl State for StateDiff {
  fn get(&self, address: &Address) -> Option<Account> {
    self.upserts.get(address).cloned()
  }

  fn apply(&mut self, diff: StateDiff) {
    *self = std::mem::take(self).merge(diff);
  }
}

pub trait State {
  fn get(&self, address: &Address) -> Option<Account>;
  fn apply(&mut self, diff: StateDiff);
}

#[derive(Debug, Default)]
pub struct InMemoryStateStore {
  data: HashMap<Address, Account>,
}

impl InMemoryStateStore {
  pub fn iter(&self) -> impl Iterator<Item = (&Address, &Account)> {
    self.data.iter()
  }
}

impl State for InMemoryStateStore {
  fn get(&self, address: &Address) -> Option<Account> {
    self.data.get(address).cloned()
  }

  fn apply(&mut self, diff: StateDiff) {
    for (k, v) in diff.upserts {
      self.data.insert(k, v);
    }

    for addr in diff.deletes {
      self.data.remove(&addr);
    }
  }
}

#[cfg(test)]
mod tests {
  use {
    crate::{state::StateDiff, InMemoryStateStore, State},
    anoma_primitives::{Account, Address, Code, Predicate, PredicateTree},
  };

  #[test]
  fn statediff_smoke() -> anyhow::Result<()> {
    let mut store = InMemoryStateStore::default();

    assert_eq!(store.iter().count(), 0);

    let mut diff1 = StateDiff::default();
    diff1.set(Address::new("/test/addr1")?, Account {
      state: vec![0, 1],
      predicates: PredicateTree::Id(Predicate {
        code: Code::Inline(b"some-code".to_vec()),
        params: vec![],
      }),
    });

    diff1.set(Address::new("/test/addr2")?, Account {
      state: vec![2, 3],
      predicates: PredicateTree::Id(Predicate {
        code: Code::Inline(b"some-code".to_vec()),
        params: vec![],
      }),
    });

    store.apply(diff1);

    assert_eq!(store.iter().count(), 2);
    assert_eq!(store.get(&"/test/addr1".parse()?).unwrap().state, vec![
      0, 1
    ]);

    assert_eq!(store.get(&"/test/addr2".parse()?).unwrap().state, vec![
      2, 3
    ]);

    let mut diff2 = StateDiff::default();
    diff2.remove(&"/test/addr1".parse()?);

    store.apply(diff2);

    assert_eq!(store.iter().count(), 1);
    assert!(store.get(&"/test/addr1".parse()?).is_none());

    Ok(())
  }
}
