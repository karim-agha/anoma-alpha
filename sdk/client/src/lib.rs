use {
  anoma_primitives::{Account, Address, Block, Code, Predicate, PredicateTree},
  anoma_vm::{execute_many, State, StateDiff},
  multihash::{Multihash, MultihashDigest},
  std::collections::VecDeque,
  thiserror::Error,
  wasmer::{Cranelift, Module, Store},
};

#[derive(Debug, Error)]
pub enum Error {
  #[error("Invalid block height {0}. Expected {1}")]
  InvalidBlockHeight(u64, u64),

  #[error("Invalid block parent {0:?}. Expected {1:?}")]
  InvalidBlockParent(Multihash, Multihash),
}

pub struct BlockStateBuilder<'s> {
  history_len: usize,
  state: &'s mut dyn State,
  codecache: &'s mut dyn State,
  recent: VecDeque<Block>,
}

impl<'s> State for BlockStateBuilder<'s> {
  fn get(&self, address: &Address) -> Option<Account> {
    self.state.get(address)
  }

  fn apply(&mut self, _: StateDiff) {
    unimplemented!(
      "Direct state mutation is not allowed on this type. State mutation in \
       BlockConsumer happens only by consuming blocks."
    )
  }
}

impl<'s> BlockStateBuilder<'s> {
  pub fn new(
    history_len: usize,
    state: &'s mut dyn State,
    codecache: &'s mut dyn State,
    recent: impl Iterator<Item = Block>,
  ) -> Self {
    assert!(history_len > 0);
    let recent: VecDeque<_> = recent.collect();
    assert!(!recent.is_empty());

    Self {
      history_len,
      state,
      codecache,
      recent,
    }
  }

  pub fn last(&self) -> &Block {
    self
      .recent
      .front()
      .expect("asserted that there must be at least one recent block")
  }

  pub fn recent(&self) -> impl Iterator<Item = &Block> {
    self.recent.iter()
  }

  #[allow(clippy::result_large_err)]
  pub fn consume(&mut self, block: Block) -> Result<(), Error> {
    let prev_height = self.last().height;
    let prev_hash = *self.last().hash();

    if prev_hash != block.parent {
      return Err(Error::InvalidBlockParent(block.parent, prev_hash));
    }

    if prev_height + 1 != block.height {
      return Err(Error::InvalidBlockHeight(block.height, prev_height + 1));
    }

    self.recent.push_front(block.clone());
    if self.recent.len() > self.history_len {
      self.recent.pop_back();
    }

    let results = execute_many(
      self.state, //
      self.codecache,
      block.transactions.into_iter(),
    );

    let statediff = results
      .into_iter()
      .filter_map(|res| res.ok())
      .reduce(|acc, e| acc.merge(e))
      .unwrap_or_default();

    self.codecache.apply(try_precompile_predicates(&statediff));
    self.state.apply(statediff);
    Ok(())
  }
}

fn try_precompile_predicates(diff: &StateDiff) -> StateDiff {
  const WASM_SIG: &[u8] = b"\0asm";
  let mut output = StateDiff::default();
  for (_, change) in diff.iter() {
    if let Some(change) = change {
      if change.state.starts_with(WASM_SIG) {
        let compiler = Cranelift::default();
        let store = Store::new(compiler);
        if let Ok(compiled) = Module::from_binary(&store, &change.state) {
          let codehash = multihash::Code::Sha3_256.digest(&change.state);
          if let Ok(serialized) = compiled.serialize() {
            output.set(
              format!(
                "/predcache/{}",
                bs58::encode(codehash.to_bytes()).into_string()
              )
              .parse()
              .expect("constructed at compile time"),
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
  }
  output
}
