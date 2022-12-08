use {
  crate::{State, StateDiff},
  anoma_primitives::{
    Address,
    Expanded,
    Param,
    Predicate,
    Transaction,
    Trigger,
  },
  std::collections::HashSet,
  thiserror::Error,
};

#[derive(Debug, Error)]
pub enum Error {}

pub fn execute(
  _transaction: Transaction,
  _state: &impl State,
) -> Result<StateDiff, Error> {
  todo!()
}

fn _invoke(
  _predicate: Predicate<Expanded>,
  _trigger: Trigger,
  _tx: Transaction<Expanded>,
  _state: &dyn State,
) -> Result<bool, Error> {
  todo!()
}

/// Specifies the list of all accounts that a transaction will read or write to.
#[derive(Debug)]
pub struct TransactionRefs {
  reads: HashSet<Address>,
  writes: HashSet<Address>,
}

impl TransactionRefs {
  pub fn reads(&self) -> impl Iterator<Item = &Address> {
    self.reads.iter()
  }

  pub fn writes(&self) -> impl Iterator<Item = &Address> {
    self.writes.iter()
  }
}

/// Identifies all accounts that will be read or written to by this transaction.
pub fn refs(tx: &Transaction, state: &impl State) -> TransactionRefs {
  let mut references = TransactionRefs {
    reads: HashSet::new(),
    writes: HashSet::new(),
  };

  // collect all writes
  for addr in tx.proposals.keys() {
    // add the account that we want to mutate
    references.writes.insert(addr.clone());
  }

  // if an account is both read and write, then
  // it belongs to the "write" subset, because
  // it is what matters when locking state and
  // scheduling concurrent executions of transactions.

  // collect all reads that will occur when evaluating
  // the validity predicates of the mutatated account and
  // all its ancestors.
  for addr in tx.proposals.keys() {
    // and all references used by its predicates
    if let Some(acc) = state.get(addr) {
      acc.predicates.for_each(&mut |pred| {
        for param in &pred.params {
          if let Param::AccountRef(addr) = param {
            if !references.writes.contains(addr) {
              references.reads.insert(addr.clone());
            }
          };
        }
      });

      // then all references used by predicates of all its ancestors
      for ancestor in addr.ancestors() {
        if let Some(acc) = state.get(&ancestor) {
          acc.predicates.for_each(&mut |pred| {
            for param in &pred.params {
              if let Param::AccountRef(addr) = param {
                if !references.writes.contains(addr) {
                  references.reads.insert(addr.clone());
                }
              };
            }
          });
        }
      }
    }
  }

  // collect all reads that will occur when evaluating
  // intent predicates.
  for intent in &tx.intents {
    intent.expectations.for_each(&mut |pred| {
      for param in &pred.params {
        if let Param::AccountRef(addr) = param {
          if !references.writes.contains(addr) {
            references.reads.insert(addr.clone());
          }
        };
      }
    })
  }

  references
}
