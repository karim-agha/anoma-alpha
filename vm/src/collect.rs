use {
  crate::{State, StateDiff},
  anoma_primitives::{
    Account,
    AccountChange,
    Address,
    Calldata,
    Code,
    Expanded,
    ExpandedAccountChange,
    ExpandedCode,
    ExpandedParam,
    Param,
    Predicate,
    PredicateContext,
    PredicateTree,
    Transaction,
  },
  std::collections::{BTreeMap, HashMap},
  thiserror::Error,
};

#[derive(Debug, Clone, Error)]
pub enum Error {
  #[error("Trying to create an account ({0}) that already exists.")]
  AccountAlreadyExists(Address),

  #[error("Trying to mutate an account ({0}) that does not exist.")]
  AccountDoesNotExist(Address),

  #[error(
    "Predicate is referencing code from an account ({0}) that does not exist \
     in {1:?}"
  )]
  CodeDoesNotExist(Address, Predicate),

  #[error(
    "Predicate is referencing an account ({0}) that does not exist in {1:?}"
  )]
  AccountRefDoesNotExist(Address, Predicate),

  #[error(
    "Predicate is referencing a proposal for address ({0}) that is not \
     proposed by the transaction in {1:?}"
  )]
  ProposalDoesNotExist(Address, Predicate),

  #[error(
    "Predicate is referencing calldata with key '{0}' that is not found in \
     the transaction in {1:?}"
  )]
  CalldataNotFound(String, Predicate),
}

/// in case all predicates evaluate successfully on mutated
/// accounts and intents, then this is the set of state
/// diff that will be applied to the replicated blockchain
/// global state.
pub fn outputs(
  state: &impl State,
  transaction: &Transaction,
) -> Result<StateDiff, Error> {
  let mut output = StateDiff::default();
  for (addr, change) in &transaction.proposals {
    let addr = addr.clone();
    match change {
      AccountChange::CreateAccount(acc) => {
        if state.get(&addr).is_some() {
          return Err(Error::AccountAlreadyExists(addr.clone()));
        }
        output.set(addr, acc.clone());
      }
      AccountChange::ReplaceState(s) => {
        if let Some(acc) = state.get(&addr) {
          output.set(addr, Account {
            state: s.clone(),
            predicates: acc.predicates,
          });
        } else {
          return Err(Error::AccountDoesNotExist(addr));
        }
      }
      AccountChange::ReplacePredicates(p) => {
        if let Some(acc) = state.get(&addr) {
          output.set(addr, Account {
            state: acc.state,
            predicates: p.clone(),
          });
        } else {
          return Err(Error::AccountDoesNotExist(addr));
        }
      }
      AccountChange::DeleteAccount => {
        if state.get(&addr).is_none() {
          return Err(Error::AccountDoesNotExist(addr));
        } else {
          output.remove(&addr);
        }
      }
    }
  }

  Ok(output)
}

/// Prepares the transaction context that is passed as an
/// argument to every predicate triggered by a transaction.
pub fn predicate_context(
  state: &impl State,
  transaction: &Transaction,
) -> Result<PredicateContext, Error> {
  Ok(PredicateContext {
    calldata: transaction
      .intents
      .iter()
      .map(|intent| (*intent.hash(), intent.calldata.clone()))
      .collect(),
    proposals: {
      let mut proposals = BTreeMap::new();
      for (addr, change) in &transaction.proposals {
        proposals.insert(
          addr.clone(), //
          expand_account_change(addr.clone(), state, change)?,
        );
      }
      proposals
    },
  })
}

/// Retreives a list of all account predicates that need to be invoked
/// for each transaction proposal, along with account's ancestors predicates.
pub fn accounts_predicates(
  state: &impl State,
  context: &PredicateContext,
  transaction: &Transaction,
) -> Result<Vec<PredicateTree<Expanded>>, Error> {
  let mut output = HashMap::new();

  // when predicates on accounts reference calldata entries,
  // the reference calldata entries stored in intents. If intents
  // have overlapping calldata entries, then one of them will be
  // bound to the CalldataRef parameter. The one that gets bound is
  // undetermined as we don't want to make any promises for VM users.
  //
  // Its rare that account predicates reference calldata entries from
  // predicates, but when they do, its most likely a very specific value
  // identified by things like a public key.
  //
  // If account predicates care about which specific intent has a given
  // calldata entry, then they have access to the context object that groups
  // those entries by the containing intent hash.
  let calldata = context
    .calldata
    .values()
    .cloned()
    .reduce(|mut prev, current| {
      prev.extend(current);
      prev
    })
    .unwrap_or_default();

  for addr in transaction.proposals.keys() {
    if !output.contains_key(addr) {
      if let Some(acc) = state.get(addr) {
        output.insert(
          addr.clone(),
          expand_predicate_tree(state, acc.predicates, context, &calldata)?,
        );
      }
    }

    // and predicates of all its ancestors (if any)
    for addr in addr.ancestors() {
      if !output.contains_key(&addr) {
        if let Some(acc) = state.get(&addr) {
          output.insert(
            addr.clone(),
            expand_predicate_tree(state, acc.predicates, context, &calldata)?,
          );
        }
      }
    }
  }

  Ok(output.into_values().collect())
}

pub fn intents_predicates(
  state: &impl State,
  context: &PredicateContext,
  tx: Transaction,
) -> Result<Vec<PredicateTree<Expanded>>, Error> {
  let mut output = Vec::with_capacity(tx.intents.len());
  for intent in tx.intents {
    output.push(expand_predicate_tree(
      state,
      intent.expectations,
      context,
      &intent.calldata,
    )?);
  }
  Ok(output)
}

/// Gathers a predicate tree into a self contained object with
/// all external references to accounts, proposals or calldata
/// resolved and embedded in the expanded representation of
/// predicate tree.
fn expand_predicate_tree(
  state: &impl State,
  tree: PredicateTree,
  context: &PredicateContext,
  calldata: &Calldata,
) -> Result<PredicateTree<Expanded>, Error> {
  tree.try_map(|pred| {
    let pred_e = pred.clone();
    Ok(Predicate::<Expanded> {
      code: match pred.code {
        Code::Inline(wasm) => ExpandedCode {
          code: wasm,
          entrypoint: "invoke".into(),
        },
        Code::AccountRef(addr, entrypoint) => {
          if let Some(acc) = state.get(&addr) {
            ExpandedCode {
              code: acc.state,
              entrypoint,
            }
          } else {
            return Err(Error::CodeDoesNotExist(addr, pred_e));
          }
        }
      },
      params: {
        let mut params = Vec::with_capacity(pred.params.len());
        for param in pred.params {
          params.push(match param {
            Param::Inline(v) => ExpandedParam::Inline(v),
            Param::AccountRef(addr) => match state.get(&addr) {
              Some(acc) => ExpandedParam::AccountRef(addr, acc.state),
              None => return Err(Error::AccountRefDoesNotExist(addr, pred_e)),
            },
            Param::ProposalRef(addr) => ExpandedParam::ProposalRef(
              addr.clone(),
              match context.proposals.get(&addr) {
                Some(change) => change.clone(),
                None => return Err(Error::ProposalDoesNotExist(addr, pred_e)),
              },
            ),
            Param::CalldataRef(key) => match calldata.get(&key) {
              Some(val) => ExpandedParam::CalldataRef(key, val.clone()),
              None => return Err(Error::CalldataNotFound(key, pred_e)),
            },
          });
        }
        params
      },
    })
  })
}

/// For account mutation proposals, this will produce an expanded
/// version of the account change that contains the current and
/// future version of the account being mutated.
fn expand_account_change(
  addr: Address,
  state: &impl State,
  change: &AccountChange,
) -> Result<ExpandedAccountChange, Error> {
  Ok(match change {
    AccountChange::CreateAccount(acc) => {
      ExpandedAccountChange::CreateAccount(acc.clone())
    }
    AccountChange::ReplaceState(s) => match state.get(&addr) {
      Some(acc) => ExpandedAccountChange::ReplaceState {
        current: acc.state,
        proposed: s.clone(),
      },
      None => return Err(Error::AccountDoesNotExist(addr.clone())),
    },
    AccountChange::ReplacePredicates(p) => match state.get(&addr) {
      Some(acc) => ExpandedAccountChange::ReplacePredicates {
        current: acc.predicates,
        proposed: p.clone(),
      },
      None => return Err(Error::AccountDoesNotExist(addr)),
    },
    AccountChange::DeleteAccount => match state.get(&addr) {
      Some(acc) => ExpandedAccountChange::DeleteAccount { current: acc },
      None => return Err(Error::AccountDoesNotExist(addr)),
    },
  })
}
