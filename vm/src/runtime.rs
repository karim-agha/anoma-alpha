use {
  crate::State,
  anoma_primitives::{PopulatedPredicate, Transaction, Trigger},
  thiserror::Error,
};

#[derive(Debug, Error)]
pub enum Error {}

pub fn execute(
  _predicate: PopulatedPredicate,
  _trigger: Trigger,
  _tx: Transaction,
  _state: &dyn State,
) -> Result<bool, Error> {
  todo!()
}

#[cfg(test)]
mod tests {
  use {
    anoma_primitives::{
      Account,
      AccountChange,
      Address,
      Code,
      Intent,
      Param,
      Predicate,
      PredicateTree,
      Transaction,
    },
    multihash::Multihash,
    std::collections::BTreeMap,
  };

  fn _create_token_app_account() -> Account {
    // Account {
    //   state: rmp_serde::to_vec(&1000), // total supply
    //   predicates: PredicateTree::Id(()),
    // }
    todo!()
  }

  fn _create_transfer_transaction(
    recent_blockhash: Multihash,
  ) -> anyhow::Result<Transaction> {
    // Intent

    // bob decremented by 10 USDA:
    let expectation_bob_tokens_decremented = Predicate {
      code: Code::AccountRef(
        Address::new("/predicates/std")?,
        "uint_less_than_by".into(),
      ),
      params: vec![
        Param::ProposalRef(Address::new(
          "/token/usda/0x0239d39F0c3F9b26cF728bC6b09872C090935E9f",
        )?),
        Param::AccountRef(Address::new(
          "/token/usda/0x0239d39F0c3F9b26cF728bC6b09872C090935E9f",
        )?),
        Param::Inline(rmp_serde::to_vec(&10u64)?),
      ],
    };

    let expectation_alice_tokens_incremented = Predicate {
      code: Code::AccountRef(
        Address::new("/predicates/std")?,
        "uint_greater_than_by".into(),
      ),
      params: vec![
        Param::ProposalRef(Address::new(
          "/token/usda/0x736b6858924eeEBE82a6269baC237255e42DE2B0",
        )?),
        Param::AccountRef(Address::new(
          "/token/usda/0x736b6858924eeEBE82a6269baC237255e42DE2B0",
        )?),
        Param::Inline(rmp_serde::to_vec(&10u64)?),
      ],
    };

    let calldata: BTreeMap<_, _> =
      [("signature".into(), b"bob-signature".to_vec())]
        .into_iter()
        .collect();

    // send 5 USDA tokens from 0x0239d... to 0x736b6...
    let intent = Intent::new(
      recent_blockhash,
      PredicateTree::And(
        Box::new(PredicateTree::Id(expectation_bob_tokens_decremented)),
        Box::new(PredicateTree::Id(expectation_alice_tokens_incremented)),
      ),
      calldata,
    );

    // Transaction
    //

    let decrement_bob_token_balance = (
      Address::new("/token/usda/0x0239d39F0c3F9b26cF728bC6b09872C090935E9f")?,
      AccountChange::ReplaceState(rmp_serde::to_vec(&5u64)?),
    );

    let increment_alice_token_balance = (
      Address::new("/token/usda/0x736b6858924eeEBE82a6269baC237255e42DE2B0")?,
      AccountChange::ReplaceState(rmp_serde::to_vec(&16u64)?),
    );

    let transaction = Transaction {
      intents: vec![intent],
      proposals: [
        decrement_bob_token_balance, //
        increment_alice_token_balance,
      ]
      .into_iter()
      .collect(),
    };

    Ok(transaction)
  }
}
