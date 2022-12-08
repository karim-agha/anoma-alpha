use {
  crate::State,
  anoma_primitives::{
    AccountChange,
    Address,
    Code,
    Exact,
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
pub enum ReferenceError {
  #[error("Referenced account not found under address {0}")]
  Account(Address),

  #[error(
    "Predicate is expecting a proposal for account address {0} but it was not \
     proposed by the transaction"
  )]
  Proposal(Address),

  #[error(
    "Predicate is expecting a calldata parameter with key {0} but it was not \
     found in the intent."
  )]
  Calldata(String),
}

/// Packages all proposals for account mutations along with the existing
/// value of the mutated account. This is done so that predicates within
/// a transaction could reason about the proposed account value and its current
/// value without accessing any external state.
fn package_proposals(
  proposals: BTreeMap<Address, AccountChange>,
  state: &impl State,
) -> Result<BTreeMap<Address, ExpandedAccountChange>, ReferenceError> {
  let mut expanded_proposals = BTreeMap::new();
  for (addr, proposal) in proposals.into_iter() {
    expanded_proposals.insert(addr.clone(), match proposal {
      AccountChange::CreateAccount(acc) => {
        ExpandedAccountChange::CreateAccount(acc)
      }
      AccountChange::ReplaceState(newval) => {
        ExpandedAccountChange::ReplaceState {
          current: match state.get(&addr) {
            Some(acc) => acc.state,
            None => return Err(ReferenceError::Account(addr)),
          },
          proposed: newval,
        }
      }
      AccountChange::ReplacePredicates(preds) => {
        ExpandedAccountChange::ReplacePredicates {
          current: match state.get(&addr) {
            Some(acc) => acc.predicates,
            None => return Err(ReferenceError::Account(addr)),
          },
          proposed: preds,
        }
      }
      AccountChange::DeleteAccount => ExpandedAccountChange::DeleteAccount {
        current: match state.get(&addr) {
          Some(acc) => acc,
          None => return Err(ReferenceError::Account(addr)),
        },
      },
    });
  }
  Ok(expanded_proposals)
}

/// Packages intents in self-contained objects with all their parameters and
/// code resolved.
///
/// The packaged object has all global state entries needed to evaluate
/// predicates inside intents without accessing any external state.
fn package_intents(
  intents: Vec<Intent<Exact>>,
  proposals: &BTreeMap<Address, ExpandedAccountChange>,
  state: &impl State,
) -> Result<Vec<Intent<Expanded>>, ReferenceError> {
  let mut packaged_intents = Vec::with_capacity(intents.len());
  for intent in intents.into_iter() {
    packaged_intents.push(Intent::<Expanded>::with_calldata(
      intent.recent_blockhash,
      intent.expectations.try_map(|pred| {
        Ok(Predicate::<Expanded> {
          code: match pred.code {
            Code::Inline(code) => ExpandedCode {
              code,
              entrypoint: "predicate".into(),
            },
            Code::AccountRef(addr, entrypoint) => ExpandedCode {
              code: match state.get(&addr) {
                Some(acc) => acc.state,
                None => return Err(ReferenceError::Account(addr)),
              },
              entrypoint,
            },
          },
          params: {
            let mut expanded_preds = Vec::with_capacity(pred.params.len());
            for param in pred.params {
              expanded_preds.push(match param {
                Param::Inline(v) => ExpandedParam::Inline(v),
                Param::AccountRef(addr) => ExpandedParam::AccountRef(
                  addr.clone(),
                  match state.get(&addr) {
                    Some(acc) => acc.state,
                    None => return Err(ReferenceError::Account(addr)),
                  },
                ),
                Param::ProposalRef(addr) => ExpandedParam::ProposalRef(
                  addr.clone(),
                  match proposals.get(&addr) {
                    Some(change) => change.clone(),
                    None => return Err(ReferenceError::Proposal(addr)),
                  },
                ),
                Param::CalldataRef(key) => ExpandedParam::CalldataRef(
                  key.clone(),
                  match intent.calldata.get(&key) {
                    Some(val) => val.clone(),
                    None => return Err(ReferenceError::Calldata(key)),
                  },
                ),
              });
            }
            expanded_preds
          },
        })
      })?,
      intent.calldata,
    ));
  }
  Ok(packaged_intents)
}

/// This function takes a transaction as it is received from clients and
/// packages it in a self contained transaction object with all its account
/// references resolved.
///
/// This is done so that when executing this packaged transaction, not further
/// state access is needed and it could be scheduled in parallel with other
/// transactions.
///
/// See the [`Expanded`] and [`Exact`] types in [`anoma-primitives`] for more
/// info.
pub fn package_transaction(
  tx: Transaction<Exact>,
  state: &impl State,
) -> Result<Transaction<Expanded>, ReferenceError> {
  let proposals = package_proposals(tx.proposals, state)?;
  let intents = package_intents(tx.intents, &proposals, state)?;
  Ok(Transaction::<Expanded> { intents, proposals })
}


#[cfg(test)]
mod tests {

  #[test]
  fn package_smoke() {

  }

  #[test]
  fn package_negative() {
    
  }
}