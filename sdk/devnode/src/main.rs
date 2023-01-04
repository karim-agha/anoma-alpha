use {
  crate::{mempool::Mempool, settings::SystemSettings},
  anoma_network::{
    topic::{self, Topic},
    Config,
    Keypair,
    Network,
  },
  anoma_primitives::{Account, Block, Code, Param, Predicate, PredicateTree},
  anoma_vm::StateDiff,
  clap::Parser,
  futures::StreamExt,
  multihash::MultihashDigest,
  rmp_serde::{from_slice, to_vec},
  tokio::time::{interval, MissedTickBehavior},
  tracing::{info, warn},
  wasmer::{Cranelift, Module, Store},
};

mod mempool;
mod settings;
mod storage;

// (transactions, blocks) topic handles
fn start_network(settings: &SystemSettings) -> anyhow::Result<(Topic, Topic)> {
  let mut network = Network::new(
    Config {
      listen_addrs: settings.p2p_addrs(),
      ..Default::default()
    },
    Keypair::generate_ed25519(),
  )?;

  let txs_topic = network.join(topic::Config {
    name: format!("/{}/transactions", settings.network_id()),
    bootstrap: Default::default(),
  })?;

  let blocks_topic = network.join(topic::Config {
    name: format!("/{}/blocks", settings.network_id()),
    bootstrap: Default::default(),
  })?;

  tokio::spawn(network.runloop());
  Ok((txs_topic, blocks_topic))
}

fn precompile_predicates(diff: &StateDiff) -> anyhow::Result<StateDiff> {
  let wasm_sig = b"\0asm";
  let mut output = StateDiff::default();
  for (_, change) in diff.iter() {
    if let Some(change) = change {
      if change.state.starts_with(wasm_sig) {
        let compiler = Cranelift::default();
        let store = Store::new(compiler);
        if let Ok(compiled) = Module::from_binary(&store, &change.state) {
          let codehash = multihash::Code::Sha3_256.digest(&change.state);
          let serialized = compiled.serialize()?;
          output.set(
            format!(
              "/predcache/{}",
              bs58::encode(codehash.to_bytes()).into_string()
            )
            .parse()?,
            Account {
              state: serialized.to_vec(),
              predicates: PredicateTree::Id(Predicate {
                code: Code::Inline(vec![]),
                params: vec![],
              }),
            },
          );
        }
      }
    }
  }
  Ok(output)
}

fn persist_block(height: u64, blockdata: &[u8]) -> anyhow::Result<StateDiff> {
  let mut output = StateDiff::default();
  output.set("/block_num".parse()?, Account {
    state: to_vec(&height)?,
    predicates: PredicateTree::Id(Predicate {
      code: Code::AccountRef("/stdpred/v1".parse()?, "constant".into()),
      params: vec![Param::Inline(to_vec(&false)?)],
    }),
  });

  output.set(format!("/block/{height}").parse()?, Account {
    state: blockdata.to_vec(),
    predicates: PredicateTree::Id(Predicate {
      code: Code::AccountRef("/stdpred/v1".parse()?, "constant".into()),
      params: vec![Param::Inline(to_vec(&false)?)],
    }),
  });
  Ok(output)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  tracing_subscriber::fmt::init();

  // gather CLI parameters
  let settings = SystemSettings::parse();
  info!("startup settings: {settings:#?}");

  // get instances of stores, it can be either
  // an in-memory ephemeral storage if no data directory
  // is provided by cli or persistent on-disk store otherwise.
  let mut code_cache = settings.cache_storage()?;
  let mut state_store = settings.state_storage()?;
  let mut blocks_store = settings.blocks_storage()?;

  // start network and get topic handles for txs and blocks
  let (mut txs_topic, blocks_topic) = start_network(&settings)?;

  let mut mempool = Mempool::default();
  let mut interval = interval(settings.block_time());
  interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

  // get last known block #:
  let height: u64 = blocks_store
    .get(&"/block_num".parse()?)
    .map(|a| from_slice(&a.state).expect("corrupt db"))
    .unwrap_or_default();

  let mut lastblock = blocks_store
    .get(&format!("/block/{height}").parse()?)
    .map(|a| from_slice(&a.state).expect("corrupt db"))
    .unwrap_or(Block::zero());

  loop {
    tokio::select! {
      Some(tx) = txs_topic.next() => {
        if let Ok(tx) = from_slice(&tx) {
          mempool.consume(tx);
        }
      }
      _ = interval.tick() => {
        let (block, statediff) = mempool.produce(&*state_store, &*code_cache, &lastblock);
        info!("produced block {} (#{}) with {} transactions and {} account mutations",
          bs58::encode(&block.hash().to_bytes()).into_string(),
          block.height,
          block.transactions.len(),
          statediff.iter().count());

        // serialize block bytes
        let blockbytes = to_vec(&block)?;
        lastblock = block;

        code_cache.apply(precompile_predicates(&statediff)?);
        state_store.apply(statediff);
        blocks_store.apply(persist_block(height, &blockbytes)?);

        // broadcast through p2p to all other nodes
        if let Err(e) = blocks_topic.gossip(blockbytes) {
          warn!("failed to gossip block: {e:?}");
        }
      }
    }
  }
}
