use {
  crate::State,
  anoma_primitives::{Address, Code, Param, Transaction},
  std::collections::HashSet,
};

/// Specifies the list of all accounts that a transaction will read or write to.
/// This is used when scheduling transactions for execution in parallel.
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

  pub fn new(tx: &Transaction, state: &impl State) -> Self {
    let mut reads = HashSet::new();
    let mut writes = HashSet::new();

    // collect all writes
    for addr in tx.proposals.keys() {
      // add the account that we want to mutate
      writes.insert(addr.clone());
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
              if !writes.contains(addr) {
                reads.insert(addr.clone());
              }
            };
          }

          if let Code::AccountRef(ref addr, _) = pred.code {
            if !writes.contains(addr) {
              reads.insert(addr.clone());
            }
          }
        });

        // then all references used by predicates of all its ancestors
        for ancestor in addr.ancestors() {
          if let Some(acc) = state.get(&ancestor) {
            acc.predicates.for_each(&mut |pred| {
              for param in &pred.params {
                if let Param::AccountRef(addr) = param {
                  if !writes.contains(addr) {
                    reads.insert(addr.clone());
                  }
                };
              }
              if let Code::AccountRef(ref addr, _) = pred.code {
                if !writes.contains(addr) {
                  reads.insert(addr.clone());
                }
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
            if !writes.contains(addr) {
              reads.insert(addr.clone());
            }
          };
        }

        if let Code::AccountRef(ref addr, _) = pred.code {
          if !writes.contains(addr) {
            reads.insert(addr.clone());
          }
        }
      })
    }

    Self { reads, writes }
  }
}
