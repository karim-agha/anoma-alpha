use {
  crate::{execution, State, StateDiff},
  anoma_primitives::{Address, Code, Param, Transaction},
  std::collections::{HashMap, HashSet, VecDeque},
};

/// Runs multiple transactions in parallel, while preserving read/write
/// dependency ordering. This function is usually called on all transactions
/// within one block in the blockchain.
///
/// Produces a list of results that contain either a state diff on successfull
/// transaction execution or an error explaining why a tx failed. The resulting
/// collection of results is in the same order as the input txs.
pub fn execute_many(
  state: &impl State,
  txs: impl Iterator<Item = Transaction>,
) -> Vec<Result<StateDiff, execution::Error>> {
  // all txs with their account r/w dependencies
  let mut refs: VecDeque<(_, _)> = txs
    .map(|tx| (*tx.hash(), TransactionRefs::new(&tx, state)))
    .collect();

  // identify all r/w dependencies for this tx ordering
  let mut deps = HashMap::new(); // adjacency list
  while let Some((thishash, thisrefs)) = refs.pop_back() {
    deps.insert(thishash, {
      let mut thisdeps = HashSet::new();
      for (hash, r) in refs.iter() {
        if thisrefs.depends_on(r) {
          thisdeps.insert(*hash);
        }
      }
      thisdeps
    });
  }
  
  todo!()
}

/// Specifies the list of all accounts that a transaction will read or write to.
/// This is used when scheduling transactions for execution in parallel.
#[derive(Debug)]
struct TransactionRefs {
  reads: HashSet<Address>,
  writes: HashSet<Address>,
}

impl TransactionRefs {
  pub fn depends_on(&self, other: &Self) -> bool {
    self.reads.iter().any(|addr| other.writes.contains(addr))
      || self.writes.iter().any(|addr| other.writes.contains(addr))
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
