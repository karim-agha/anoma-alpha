use {
  anoma_primitives::Address,
  anoma_vm::{InMemoryStateStore, State},
  common::create_initial_blockchain_state,
  ed25519_dalek::Keypair,
  multihash::MultihashDigest,
  rand::Rng,
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

  let population: Vec<_> = (0..100)
    .into_iter()
    .map(|i| {
      (
        Address::new(format!("/token/usdx/wallet-{i}.eth")).unwrap(),
        Keypair::generate(&mut rand::thread_rng()),
      )
    })
    .collect();

  let half = population.len() / 2;
  let mut txs = Vec::with_capacity(population.len());

  for _ in 0..1000 {
    let rand_sender = rand::thread_rng().gen_range(0, population.len());
    let rand_recipient = rand::thread_rng().gen_range(0, population.len());
    txs.push(common::token_ops::transfer_unchecked(
      40,
      &population[rand_sender].0,
      &population[rand_sender].1,
      &population[rand_recipient].0,
      &population[rand_recipient].1.public,
      recent_blockhash,
    )?)
  }

  txs.push(common::token_ops::mint(
    10000,
    &doner_address,
    &doner_keypair.public,
    &mint_keypair,
    recent_blockhash,
    &store,
  )?);

  for (addr, keypair) in population.iter().take(half) {
    txs.push(common::token_ops::transfer(
      100,
      &doner_address,
      &doner_keypair,
      addr,
      &keypair.public,
      recent_blockhash,
      &store,
    )?);
  }

  for i in 0..half {
    txs.push(common::token_ops::transfer_unchecked(
      40,
      &population[i].0,
      &population[i].1,
      &population[half + i].0,
      &population[half + i].1.public,
      recent_blockhash,
    )?)
  }

  for _ in 0..1000 {
    let rand_sender = rand::thread_rng().gen_range(0, population.len());
    let rand_recipient = rand::thread_rng().gen_range(0, population.len());
    txs.push(common::token_ops::transfer_unchecked(
      40,
      &population[rand_sender].0,
      &population[rand_sender].1,
      &population[rand_recipient].0,
      &population[rand_recipient].1.public,
      recent_blockhash,
    )?)
  }

  anoma_vm::execute_many(&store, txs.into_iter());

  Ok(())
}
