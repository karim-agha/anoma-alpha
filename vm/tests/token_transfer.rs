use {
  anoma_primitives::{
    Account,
    AccountChange,
    Address,
    Code,
    Exact,
    Intent,
    Param,
    Predicate,
    PredicateTree,
    Transaction,
  },
  anoma_vm::{InMemoryStateStore, State, StateDiff},
  ed25519_dalek::{Keypair, PublicKey, Signer},
  multihash::MultihashDigest,
  rmp_serde::to_vec,
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
        params: vec![Param::Inline(to_vec(&stdaddr).unwrap())],
      })),
      Box::new(PredicateTree::Id(Predicate {
        code: Code::AccountRef(stdaddr.clone(), "immutable_predicates".into()),
        params: vec![Param::Inline(to_vec(&stdaddr).unwrap())],
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
  let tokenaddr: Address = "/token/usdx".parse().unwrap();
  let mut usdx_token = StateDiff::default();
  usdx_token.set("/token/usdx".parse().unwrap(), Account {
    state: to_vec(&0u64).unwrap(),
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

fn create_initial_blockchain_state(mint_authority: PublicKey) -> StateDiff {
  let mut state = StateDiff::default();
  state.apply(install_standard_library());
  state.apply(install_token_bytecode());
  state.apply(create_usdx_token(mint_authority));
  state
}

#[test]
fn mint_first_batch() -> anyhow::Result<()> {
  let mint_keypair = Keypair::generate(&mut rand::thread_rng());
  let recent_blockhash = multihash::Code::Sha3_256.digest(b"test1");
  let mint_pk_b58 = bs58::encode(mint_keypair.public.as_bytes()).into_string();

  let mut store = InMemoryStateStore::default();
  store.apply(create_initial_blockchain_state(mint_keypair.public));

  let mut mint_intent = Intent::new(
    recent_blockhash,
    PredicateTree::<Exact>::And(
      Box::new(PredicateTree::Id(Predicate {
        // expect that the total supply is updated by the mint amount
        code: Code::AccountRef("/predicates/std".parse()?, "uint_equal".into()),
        params: vec![
          Param::AccountRef("/token/usdx".parse()?),
          Param::Inline(to_vec(&1000u64)?),
        ],
      })),
      Box::new(PredicateTree::Id(Predicate {
        // expect that the minted amount is credited to a wallet
        code: Code::AccountRef("/predicates/std".parse()?, "uint_equal".into()),
        params: vec![
          Param::ProposalRef("/token/usdx/wallet1.eth".parse()?),
          Param::Inline(to_vec(&1000u64)?),
        ],
      })),
    ),
  );

  // add mint authority signature to the intent
  mint_intent.calldata.insert(
    mint_pk_b58,
    mint_keypair
      .sign(mint_intent.signing_hash().to_bytes().as_slice())
      .to_bytes()
      .to_vec(),
  );

  let wallet1keypair = Keypair::generate(&mut rand::thread_rng());

  let tx = Transaction {
    intents: vec![mint_intent],
    proposals: [
      (
        // this account does not exist yet, create it.
        "/token/usdx/wallet1.eth".parse()?,
        AccountChange::CreateAccount(Account {
          state: to_vec(&1000u64)?,
          predicates: PredicateTree::Or(
            Box::new(PredicateTree::Id(Predicate {
              // The newly created account will requre a
              // signature if the balance is deducted,
              // otherwise its happy to receive tokens
              // without any authorization.
              code: Code::AccountRef(
                "/predicates/std".parse()?,
                "uint_greater_than_equal".into(),
              ),
              params: vec![
                Param::AccountRef("/token/usdx/wallet1.eth".parse()?),
                Param::ProposalRef("/token/usdx/wallet1.eth".parse()?),
              ],
            })),
            Box::new(PredicateTree::Id(Predicate {
              // If proposed balance is not greater that current balance
              // then require a signature to authorize spending
              code: Code::AccountRef(
                "/predicates/std".parse()?,
                "require_ed25519_signature".into(),
              ),
              params: vec![Param::Inline(
                wallet1keypair.public.to_bytes().to_vec(),
              )],
            })),
          ),
        }),
      ),
      (
        "/token/usdx".parse()?, // update total supply
        AccountChange::ReplaceState(to_vec(&1000u64)?),
      ),
    ]
    .into_iter()
    .collect(),
  };

  println!("tx: {tx:#?}");

  let outdiff = anoma_vm::execute(tx, &store)?;

  assert_eq!(outdiff.iter().count(), 2);
  assert!(outdiff.get(&"/token/usdx".parse()?).is_some());
  assert!(outdiff.get(&"/token/usdx/wallet1.eth".parse()?).is_some());

  // assert total supply is 1000
  assert_eq!(
    outdiff.get(&"/token/usdx".parse()?).unwrap().state,
    to_vec(&1000u64)?
  );

  // assert recipient wallet has 1000 tokens
  assert_eq!(
    outdiff
      .get(&"/token/usdx/wallet1.eth".parse()?)
      .unwrap()
      .state,
    to_vec(&1000u64)?
  );

  // assert that the newly created recipient wallet
  // has correct predicates set, that require a
  // signature if the balance is deducted
  assert_eq!(
    outdiff
      .get(&"/token/usdx/wallet1.eth".parse()?)
      .unwrap()
      .predicates,
    PredicateTree::Or(
      Box::new(PredicateTree::Id(Predicate {
        code: Code::AccountRef(
          "/predicates/std".parse()?,
          "uint_greater_than_equal".into(),
        ),
        params: vec![
          Param::AccountRef("/token/usdx/wallet1.eth".parse()?),
          Param::ProposalRef("/token/usdx/wallet1.eth".parse()?),
        ],
      })),
      Box::new(PredicateTree::Id(Predicate {
        code: Code::AccountRef(
          "/predicates/std".parse()?,
          "require_ed25519_signature".into(),
        ),
        params: vec![Param::Inline(wallet1keypair.public.to_bytes().to_vec(),)],
      })),
    )
  );

  // assert that the token account has its predicates
  // unchanged after updating the total supply value
  let tokenaddr: Address = "/token/usdx".parse()?;
  assert_eq!(
    outdiff.get(&"/token/usdx".parse()?).unwrap().predicates,
    PredicateTree::And(
      Box::new(PredicateTree::Id(Predicate {
        code: Code::AccountRef(tokenaddr.clone(), "predicate".into()),
        params: vec![
          Param::Inline(to_vec(&tokenaddr).unwrap()),
          Param::Inline(to_vec(&mint_keypair.public).unwrap()),
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
    )
  );

  Ok(())
}
