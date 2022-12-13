use {
  anoma_vm::{InMemoryStateStore, State},
  common::{create_initial_blockchain_state, precache_predicates_bytecode},
  ed25519_dalek::Keypair,
  multihash::MultihashDigest,
  rmp_serde::from_slice,
};

mod common;

#[test]
fn transfer_token() -> anyhow::Result<()> {
  let mint_keypair = Keypair::generate(&mut rand::thread_rng());
  let recent_blockhash = multihash::Code::Sha3_256.digest(b"test3");

  let mut store = InMemoryStateStore::default();
  store.apply(create_initial_blockchain_state(mint_keypair.public));

  let mut cache = InMemoryStateStore::default();
  cache.apply(precache_predicates_bytecode(
    &store,
    &"/token".parse().unwrap(),
  ));
  cache.apply(precache_predicates_bytecode(
    &store,
    &"/predicates/std".parse().unwrap(),
  ));

  let alice_keypair = Keypair::generate(&mut rand::thread_rng());
  let alice_address = &"/token/usdx/alice.eth".parse()?;

  let bob_keypair = Keypair::generate(&mut rand::thread_rng());
  let bob_address = &"/token/usdx/bob.eth".parse()?;

  store.apply(anoma_vm::execute(
    common::token_ops::mint(
      1000,
      alice_address,
      &alice_keypair.public,
      &mint_keypair,
      recent_blockhash,
      &store,
    )?,
    &store,
    &cache,
  )?);

  assert_eq!(
    from_slice::<u64>(&store.get(alice_address).unwrap().state)?,
    1000
  );

  assert!(store.get(bob_address).is_none());

  store.apply(anoma_vm::execute(
    common::token_ops::transfer(
      400,
      alice_address,
      &alice_keypair,
      bob_address,
      &bob_keypair.public,
      recent_blockhash,
      &store,
    )?,
    &store,
    &cache,
  )?);

  assert_eq!(
    from_slice::<u64>(&store.get(alice_address).unwrap().state)?,
    600
  );

  assert_eq!(
    from_slice::<u64>(&store.get(bob_address).unwrap().state)?,
    400
  );

  Ok(())
}
