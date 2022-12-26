mod common;
use {
  anoma_primitives::{Address, Code, Param, Predicate, PredicateTree},
  anoma_vm::{InMemoryStateStore, State},
  common::{create_initial_blockchain_state, precache_predicates_bytecode},
  ed25519_dalek::Keypair,
  multihash::MultihashDigest,
  rmp_serde::to_vec,
};

#[test]
fn mint_tokens() -> anyhow::Result<()> {
  let mint_keypair = Keypair::generate(&mut rand::thread_rng());
  let recent_blockhash = multihash::Code::Sha3_256.digest(b"test1");

  let mut store = InMemoryStateStore::default();
  store.apply(create_initial_blockchain_state(mint_keypair.public));

  let mut cache = InMemoryStateStore::default();
  cache.apply(precache_predicates_bytecode(
    &store,
    &"/token".parse().unwrap(),
  ));
  cache.apply(precache_predicates_bytecode(
    &store,
    &"/stdpred/v1".parse().unwrap(),
  ));

  let wallet1keypair = Keypair::generate(&mut rand::thread_rng());
  let mint_tx = common::token_ops::mint(
    1000,
    &"/token/usdx/wallet1.eth".parse()?,
    &wallet1keypair.public,
    &mint_keypair,
    recent_blockhash,
    &store,
  )?;

  // run transaction in the VM and get state diff
  let outdiff = anoma_vm::execute(mint_tx, &store, &cache)?;

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
          "/stdpred/v1".parse()?,
          "uint_greater_than_equal".into(),
        ),
        params: vec![
          Param::ProposalRef("/token/usdx/wallet1.eth".parse()?),
          Param::AccountRef("/token/usdx/wallet1.eth".parse()?),
        ],
      })),
      Box::new(PredicateTree::Id(Predicate {
        code: Code::AccountRef(
          "/stdpred/v1".parse()?,
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
        code: Code::AccountRef("/token".parse().unwrap(), "predicate".into()),
        params: vec![
          Param::Inline(to_vec(&tokenaddr).unwrap()),
          Param::Inline(to_vec(&mint_keypair.public).unwrap()),
          Param::AccountRef(tokenaddr.clone()),
        ],
      })),
      Box::new(PredicateTree::Id(Predicate {
        code: Code::AccountRef(
          "/stdpred/v1".parse().unwrap(),
          "immutable_predicates".into(),
        ),
        params: vec![Param::Inline(to_vec(&tokenaddr).unwrap())],
      })),
    )
  );

  // apply first mint state diff to state
  store.apply(outdiff);

  let second_mint = common::token_ops::mint(
    500,
    &"/token/usdx/wallet1.eth".parse()?,
    &wallet1keypair.public,
    &mint_keypair,
    recent_blockhash,
    &store,
  )?;

  // second mint tx
  store.apply(anoma_vm::execute(second_mint, &store, &cache)?);

  // prev mint 1000 + second mint 500
  assert_eq!(
    rmp_serde::from_slice::<u64>(
      &store
        .get(&"/token/usdx/wallet1.eth".parse()?)
        .unwrap()
        .state
    )?,
    1500u64
  );

  // total supply also updated
  assert_eq!(
    rmp_serde::from_slice::<u64>(
      &store.get(&"/token/usdx".parse()?).unwrap().state
    )?,
    1500u64
  );
  Ok(())
}
