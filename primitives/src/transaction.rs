use {
  crate::{Account, Address, Intent, PredicateTree},
  alloc::vec::Vec,
  core::fmt::Debug,
  ed25519_dalek::Signature,
  serde::{Deserialize, Serialize},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccountChange {
  CreateAccount(Account),
  ReplaceState(Vec<u8>),
  ReplacePredicates(PredicateTree),
  DeleteAccount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
  pub intents: Vec<Intent>,

  /// Proposals for new contents of accounts under given addresses.
  ///
  /// If all predicates in all involved accounts and their parents
  /// evaluate to true, then the account contents will be replaced by
  /// this value.
  pub proposals: Vec<(Address, AccountChange)>,

  pub producer: (Address, Signature),
}

#[cfg(test)]
mod tests {
  use {
    super::AccountChange,
    crate::{
      address,
      predicate::{Code, Param},
      Account,
      Address,
      Intent,
      Predicate,
      PredicateTree,
      Transaction,
    },
    ed25519_dalek::Signature,
    multihash::Multihash,
  };

  struct MockBlockchain {}

  impl MockBlockchain {
    fn get_recent_blockhash(&self) -> Multihash {
      todo!()
    }

    fn execute_transaction(&mut self, _tx: Transaction) {
      todo!()
    }

    fn read_account(&self, _address: Address) -> Option<Account> {
      todo!()
    }

    fn set_account(&mut self, _address: Address, _account: Account) {
      todo!()
    }
  }

  fn create_mock_blockchain() -> Result<MockBlockchain, address::Error> {
    let mut blockchain = MockBlockchain {};

    // make the entire /predicates/std namespace immutable
    // here we have the standard predicate library that is
    // defined in genesis and users cannot add predicates
    // under this namespace to avoid malicious attempts at
    // adding seemingly "trusted" predicates here.
    blockchain.set_account(Address::new("/predicates/std")?, Account {
      state: vec![],
      predicates: PredicateTree::Id(Predicate {
        code: Code::AccountRef(Address::new("/predicates/std/const")?),
        params: vec![Param::Inline(b"\0".to_vec())], // immutable
      }),
    });

    blockchain.set_account(Address::new("/predicates/std/const")?, Account {
      state: b"wasm-bytecode-always-return-valueof-param-0".to_vec(),
      predicates: PredicateTree::Id(Predicate {
        // immutable account
        code: Code::Inline(b"wasm-bytecode-always-return-false".to_vec()),
        params: vec![],
      }),
    });

    blockchain.set_account(
      Address::new("/predicates/std/uint-less-than-by")?,
      Account {
        state: b"todo-wasm-bytecode".to_vec(),
        predicates: PredicateTree::Id(Predicate {
          code: Code::AccountRef(Address::new("/predicates/std/const")?),
          params: vec![Param::Inline(b"\0".to_vec())], // immutable
        }),
      },
    );

    blockchain.set_account(
      Address::new("/predicates/std/uint-greater-than-by")?,
      Account {
        state: b"todo-wasm-bytecode".to_vec(),
        predicates: PredicateTree::Id(Predicate {
          code: Code::AccountRef(Address::new("/predicates/std/const")?),
          params: vec![Param::Inline(b"\0".to_vec())], // immutable
        }),
      },
    );

    Ok(blockchain)
  }

  #[test]
  #[ignore]
  fn token_transfer() -> Result<(), address::Error> {
    // original balances:
    // bob [0x0239d...]: 15 USDA
    // alice [0x736b6...]: 6 USDA

    let mut blockchain = create_mock_blockchain()?;

    // usually retreived from RPC
    let recent_blockhash = blockchain.get_recent_blockhash();

    // Intent
    //

    // bob decremented by 10 USDA:
    let expectation_bob_tokens_decremented = Predicate {
      code: Code::AccountRef(Address::new(
        "/predicates/std/uint-less-than-by",
      )?),
      params: vec![
        Param::ProposalRef(Address::new(
          "/token/usda/0x0239d39F0c3F9b26cF728bC6b09872C090935E9f",
        )?),
        Param::AccountRef(Address::new(
          "/token/usda/0x0239d39F0c3F9b26cF728bC6b09872C090935E9f",
        )?),
        Param::Inline(10u128.to_le_bytes().to_vec()),
      ],
    };

    let expectation_alice_tokens_incremented = Predicate {
      code: Code::AccountRef(Address::new(
        "/predicates/std/uint-greater-than-by",
      )?),
      params: vec![
        Param::ProposalRef(Address::new(
          "/token/usda/0x736b6858924eeEBE82a6269baC237255e42DE2B0",
        )?),
        Param::AccountRef(Address::new(
          "/token/usda/0x736b6858924eeEBE82a6269baC237255e42DE2B0",
        )?),
        Param::Inline(10u128.to_le_bytes().to_vec()),
      ],
    };

    let calldata: Vec<_> = [("signature".into(), b"bob-signature".to_vec())]
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
      AccountChange::ReplaceState(5u128.to_le_bytes().to_vec()),
    );

    let increment_alice_token_balance = (
      Address::new("/token/usda/0x736b6858924eeEBE82a6269baC237255e42DE2B0")?,
      AccountChange::ReplaceState(16u128.to_le_bytes().to_vec()),
    );

    let transaction = Transaction {
      intents: vec![intent],
      proposals: vec![
        decrement_bob_token_balance,
        increment_alice_token_balance,
      ],
      producer: (
        Address::new("/wallet/0xf573d99385C05c23B24ed33De616ad16a43a0919")?,
        Signature::from_bytes(&[]).unwrap(),
      ),
    };

    let bob_pre = blockchain
      .read_account(Address::new(
        "/token/usda/0x0239d39F0c3F9b26cF728bC6b09872C090935E9f",
      )?)
      .unwrap();

    let alice_pre = blockchain
      .read_account(Address::new(
        "/token/usda/0x0239d39F0c3F9b26cF728bC6b09872C090935E9f",
      )?)
      .unwrap();

    // verify balances before executing transaction
    assert_eq!(bob_pre.state, 15u128.to_le_bytes().to_vec());
    assert_eq!(alice_pre.state, 6u128.to_le_bytes().to_vec());

    // invoke
    blockchain.execute_transaction(transaction);

    let bob_post = blockchain
      .read_account(Address::new(
        "/token/usda/0x0239d39F0c3F9b26cF728bC6b09872C090935E9f",
      )?)
      .unwrap();

    let alice_post = blockchain
      .read_account(Address::new(
        "/token/usda/0x0239d39F0c3F9b26cF728bC6b09872C090935E9f",
      )?)
      .unwrap();

    // verify balances before executing transaction
    assert_eq!(bob_post.state, 10u128.to_le_bytes().to_vec());
    assert_eq!(alice_post.state, 11u128.to_le_bytes().to_vec());

    Ok(())
  }
}
