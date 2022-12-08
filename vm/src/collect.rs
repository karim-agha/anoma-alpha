use {
  crate::{State, StateDiff},
  anoma_primitives::{
    Account,
    AccountChange,
    Address,
    Code,
    Expanded,
    ExpandedAccountChange,
    ExpandedCode,
    ExpandedParam,
    Intent,
    Param,
    Predicate,
    Transaction,
  },
  std::collections::BTreeMap,
  thiserror::Error,
};

#[derive(Debug, Error)]
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
/// mutations that will be applied to the replicated blockchain
/// state.
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

/// Given a transaction object that contains references to external
/// accounts in the blockchain state, this function will produce an
/// expanded version of the transaction that contains all the referenced
/// accounts state.
///
/// The expanded transaction object can successfully execute all included
/// preducates without further access to the global state, and can be safely
/// executed concurrently with other transactions as long as read/write
/// dependencies are preserved.
pub fn references(
  state: &impl State,
  transaction: Transaction,
) -> Result<Transaction<Expanded>, Error> {
  let mut proposals = BTreeMap::new();
  let mut intents = Vec::with_capacity(transaction.intents.len());

  for (addr, change) in transaction.proposals {
    proposals.insert(
      addr.clone(), //
      expand_account_change(addr, state, &change)?,
    );
  }

  for intent in transaction.intents {
    intents.push(expand_intent(intent, &proposals, state)?);
  }

  Ok(Transaction::<Expanded> { intents, proposals })
}

/// Intents reference various accounts, calldata, proposals, etc.
/// This function takes an intent object and turns it in to an
/// expanded intent with all references resolved and fetched, so it
/// can evaluate its conditions without firther access to global state.
fn expand_intent(
  intent: Intent,
  proposals: &BTreeMap<Address, ExpandedAccountChange>,
  state: &impl State,
) -> Result<Intent<Expanded>, Error> {
  Ok(Intent::<Expanded>::with_calldata(
    intent.recent_blockhash,
    intent.expectations.try_map(|pred| {
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
                None => {
                  return Err(Error::AccountRefDoesNotExist(addr, pred_e))
                }
              },
              Param::ProposalRef(addr) => ExpandedParam::ProposalRef(
                addr.clone(),
                match proposals.get(&addr) {
                  Some(change) => change.clone(),
                  None => {
                    return Err(Error::ProposalDoesNotExist(addr, pred_e))
                  }
                },
              ),
              Param::CalldataRef(key) => match intent.calldata.get(&key) {
                Some(val) => ExpandedParam::CalldataRef(key, val.clone()),
                None => return Err(Error::CalldataNotFound(key, pred_e)),
              },
            });
          }
          params
        },
      })
    })?,
    intent.calldata,
  ))
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
