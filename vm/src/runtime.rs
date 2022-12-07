use {
  crate::State,
  anoma_primitives::{Expanded, Predicate, Transaction, Trigger},
  thiserror::Error,
};

#[derive(Debug, Error)]
pub enum Error {}

pub fn invoke(
  _predicate: Predicate<Expanded>,
  _trigger: Trigger,
  _tx: Transaction<Expanded>,
  _state: &dyn State,
) -> Result<bool, Error> {
  todo!()
}

#[cfg(test)]
mod tests {
  use {
    crate::{InMemoryStateStore, State, StateDiff},
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
    ed25519_dalek::PublicKey,
    multihash::Multihash,
    rmp_serde::to_vec,
    serde::Serialize,
    std::collections::BTreeMap,
  };

  fn install_standard_library() -> StateDiff {
    let stdaddr: Address = "/predicates/std".parse().unwrap();
    let mut stdpred_bytecode = StateDiff::default();
    stdpred_bytecode.set(stdaddr.clone(), Account {
      state: include_bytes!(
        "../../target/wasm32-unknown-unknown/release/stdpred.wasm"
      )
      .to_vec(),
      predicates: PredicateTree::And(
        Box::new(PredicateTree::Id(Predicate {
          code: Code::AccountRef(stdaddr.clone(), "immutable_state".into()),
          params: vec![Param::Inline(rmp_serde::to_vec(&stdaddr).unwrap())],
        })),
        Box::new(PredicateTree::Id(Predicate {
          code: Code::AccountRef(
            stdaddr.clone(),
            "immutable_predicates".into(),
          ),
          params: vec![Param::Inline(rmp_serde::to_vec(&stdaddr).unwrap())],
        })),
      ),
    });
    stdpred_bytecode
  }

  // assumes std predicates are installed
  fn install_token_bytecode() -> StateDiff {
    let stdaddr: Address = "/predicates/std".parse().unwrap();
    let tokenaddr: Address = "/token".parse().unwrap();
    let mut token_bytecode = StateDiff::default();
    token_bytecode.set("/token".parse().unwrap(), Account {
      state: include_bytes!(
        "../../target/wasm32-unknown-unknown/release/examples/token.wasm"
      )
      .to_vec(),
      predicates: PredicateTree::And(
        // immutable predicates & state
        Box::new(PredicateTree::Id(Predicate {
          code: Code::AccountRef(stdaddr.clone(), "immutable_state".into()),
          params: vec![Param::Inline(to_vec(&tokenaddr).unwrap())],
        })),
        Box::new(PredicateTree::Id(Predicate {
          code: Code::AccountRef(stdaddr, "immutable_predicates".into()),
          params: vec![Param::Inline(to_vec(&tokenaddr).unwrap())],
        })),
      ),
    });
    token_bytecode
  }

  fn create_usdx_token(mint_authority: PublicKey) -> StateDiff {
    #[derive(Debug, Serialize)]
    enum TokenState {
      V1 { total_supply: u64 },
    }

    let tokenaddr: Address = "/token/usdx".parse().unwrap();
    let mut usdx_token = StateDiff::default();
    usdx_token.set("/token/usdx".parse().unwrap(), Account {
      state: to_vec(&TokenState::V1 { total_supply: 0 }).unwrap(),
      predicates: PredicateTree::And(
        Box::new(PredicateTree::Id(Predicate {
          // token contract predicate
          code: Code::AccountRef(tokenaddr.clone(), "predicate".into()),
          params: vec![
            // input params as per documentation:

            // self address, used to identity child wallet balances accounts
            Param::Inline(to_vec(&tokenaddr).unwrap()),
            // mint authority, signature to authorize minting and burning
            // tokens
            Param::Inline(to_vec(&mint_authority).unwrap()),
            // reference to an account where the total supply value is stored.
            // we're going to store it in the top-level account itself
            Param::AccountRef(tokenaddr.clone()),
          ],
        })),
        Box::new(PredicateTree::Id(Predicate {
          code: Code::AccountRef(
            "/predicates/std".parse().unwrap(),
            "immutable_predicates".into(),
          ),
          params: vec![Param::Inline(to_vec(&tokenaddr).unwrap())],
        })),
      ),
    });

    usdx_token
  }

  fn create_initial_blockchain_state(mint_authority: PublicKey) -> impl State {
    let mut state = InMemoryStateStore::default();
    state.apply(install_standard_library());
    state.apply(install_token_bytecode());
    state.apply(create_usdx_token(mint_authority));
    state
  }

  fn create_transfer_transaction(
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
