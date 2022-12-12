use {
  anoma_vm::{InMemoryStateStore, State},
  common::create_initial_blockchain_state,
  ed25519_dalek::Keypair,
  multihash::MultihashDigest,
};

mod common;

#[test]
fn make_1000_transfers() -> anyhow::Result<()> {
  let mint_keypair = Keypair::generate(&mut rand::thread_rng());
  let recent_blockhash = multihash::Code::Sha3_256.digest(b"test2");

  let mut store = InMemoryStateStore::default();
  store.apply(create_initial_blockchain_state(mint_keypair.public));

  let doner_keypair = Keypair::generate(&mut rand::thread_rng());
  let doner_address = "/token/usdx/rich_guy1.eth".parse()?;

  store.apply(anoma_vm::execute(
    common::token_ops::mint(
      10000,
      &doner_address,
      &doner_keypair.public,
      &mint_keypair,
      recent_blockhash,
      &store,
    )?,
    &store,
  )?);

  Ok(())
}
