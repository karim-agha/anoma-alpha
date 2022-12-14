use {
  anoma_primitives::{Account, Address, Code, Param, Predicate, PredicateTree},
  anoma_vm::{InMemoryStateStore, State, StateDiff},
  common::{create_initial_blockchain_state, precache_predicates_bytecode},
  ed25519_dalek::Keypair,
  multihash::MultihashDigest,
  rmp_serde::to_vec,
  std::time::Instant,
};

mod common;

#[test]
fn mint_then_transfers() -> anyhow::Result<()> {
  let mint_keypair = Keypair::generate(&mut rand::thread_rng());
  let recent_blockhash = multihash::Code::Sha3_256.digest(b"test2");

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

  let doner_keypair = Keypair::generate(&mut rand::thread_rng());
  let doner_address = "/token/usdx/rich_guy1.eth".parse()?;

  let population: Vec<_> = (0..500)
    .into_iter()
    .map(|i| {
      (
        Address::new(format!("/token/usdx/wallet-{i}.eth")).unwrap(),
        Keypair::generate(&mut rand::thread_rng()),
      )
    })
    .collect();

  let half = population.len() / 2;
  let mut doner_balance = 1000000;
  let mut txs = Vec::with_capacity(population.len());

  txs.push(common::token_ops::mint(
    doner_balance,
    &doner_address,
    &doner_keypair.public,
    &mint_keypair,
    recent_blockhash,
    &store,
  )?);

  for (acc, keypair) in population.iter().take(half) {
    let recipient_balance_after = 10;
    doner_balance -= recipient_balance_after;
    txs.push(common::token_ops::transfer_unchecked(
      &doner_address,
      &doner_keypair,
      doner_balance,
      acc,
      &keypair.public,
      recipient_balance_after,
      true,
      recent_blockhash,
    )?)
  }

  for i in 0..half {
    txs.push(common::token_ops::transfer_unchecked(
      &population[i].0,
      &population[i].1,
      5,
      &population[half + i].0,
      &population[half + i].1.public,
      5,
      true,
      recent_blockhash,
    )?);
  }

  let started = Instant::now();
  let results = anoma_vm::execute_many(&store, &cache, txs.into_iter());
  println!("elapsed: {:?}", started.elapsed());

  assert_eq!(results.len(), 501);
  for result in results {
    assert!(result.is_ok());
    store.apply(result.unwrap());
  }

  for (acc, _) in population {
    assert_eq!(store.get(&acc).unwrap().state, to_vec(&(5)).unwrap());
  }

  Ok(())
}

#[test]
fn many_independent_transfers() -> anyhow::Result<()> {
  // in this test we don't want to have any sequencial dependencies between txs
  // and all of them have to run in parallel
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

  let mut diff = StateDiff::default();
  let population: Vec<_> = (0..1000)
    .into_iter()
    .map(|i| {
      (
        Address::new(format!("/token/usdx/wallet-{i}.eth")).unwrap(),
        Keypair::generate(&mut rand::thread_rng()),
      )
    })
    .collect();

  // prepare a state where half of the accounts have
  // 500 tokens each, then run 500 transactions transfering
  // 250 tokens from the first half to the second half.
  // all those transactions should run in parallel because
  // there are no read/write dependencies between them.

  for (acc, keypair) in population.iter().take(500) {
    diff.set(acc.clone(), Account {
      state: to_vec(&500)?,
      predicates: PredicateTree::Or(
        Box::new(PredicateTree::Id(Predicate {
          code: Code::AccountRef(
            "/predicates/std".parse()?,
            "uint_greater_than_equal".into(),
          ),
          params: vec![
            Param::ProposalRef(acc.clone()),
            Param::AccountRef(acc.clone()),
          ],
        })),
        Box::new(PredicateTree::Id(Predicate {
          // If proposed balance is not greater that current balance
          // then require a signature to authorize spending
          code: Code::AccountRef(
            "/predicates/std".parse()?,
            "require_ed25519_signature".into(),
          ),
          params: vec![Param::Inline(keypair.public.to_bytes().to_vec())],
        })),
      ),
    });
  }
  store.apply(diff);

  let mut txs = Vec::with_capacity(500);
  for i in 0..500 {
    txs.push(common::token_ops::transfer(
      250,
      &population[i].0,
      &population[i].1,
      &population[i + 500].0,
      &population[i + 500].1.public,
      recent_blockhash,
      &store,
    )?)
  }

  let started = Instant::now();
  let results = anoma_vm::execute_many(&store, &cache, txs.into_iter());
  assert_eq!(results.len(), 500);
  println!("elapsed: {:?}", started.elapsed());

  for result in results {
    assert!(result.is_ok());
    store.apply(result.unwrap());
  }

  for (acc, _) in population {
    assert_eq!(store.get(&acc).unwrap().state, to_vec(&(250)).unwrap());
  }

  Ok(())
}
