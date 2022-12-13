use {
  multihash::MultihashDigest,
  wasmer::{Cranelift, Module, Store},
};

pub mod token_ops;

use {
  anoma_primitives::{Account, Address, Code, Param, Predicate, PredicateTree},
  anoma_vm::{State, StateDiff},
  ed25519_dalek::PublicKey,
  rmp_serde::to_vec,
};

/// Creates a statediff that has the standard predicates library
/// installed in sate at '/predicates/std'. Almost everything
/// will require this.
///
/// This function could fail if the wasm binary is not present
/// in the build directory. if you encounter this error, then
/// from the root of this project run:
///
/// ```bash
/// $ cargo build \
///     --package stdpred \
///     --target wasm32-unknown-unknown \
///     --release
/// ```
/// Then a WASM binary will be produced under this address
pub fn install_standard_library() -> StateDiff {
  let stdaddr: Address = "/predicates/std".parse().unwrap();
  let mut stdpred_bytecode = StateDiff::default();
  stdpred_bytecode.set(stdaddr.clone(), Account {
    state: include_bytes!(
      "../../../target/wasm32-unknown-unknown/release/stdpred.wasm"
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

/// Creates a statediff that has the Token bytecode deployed at
/// '/token', ready to create instances of it under /token/*.
///
/// This step assumes that the standard predicate library is
/// already installed in state.
///
/// This function could fail if the token WASM binary is not
/// present in the build directory, if you encounter this error
/// then from the root of this project run:
///
/// ```bash
/// $ cargo build \
///     --package anoma-predicates-sdk \
///     --example token \
///     --target wasm32-unknown-unknown \
///     --release
/// ```
pub fn install_token_bytecode() -> StateDiff {
  let stdaddr: Address = "/predicates/std".parse().unwrap();
  let tokenaddr: Address = "/token".parse().unwrap();
  let mut token_bytecode = StateDiff::default();
  token_bytecode.set("/token".parse().unwrap(), Account {
    state: include_bytes!(
      "../../../target/wasm32-unknown-unknown/release/examples/token.wasm"
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

pub fn create_usdx_token(mint_authority: PublicKey) -> StateDiff {
  let tokenaddr: Address = "/token/usdx".parse().unwrap();
  let mut usdx_token = StateDiff::default();
  usdx_token.set("/token/usdx".parse().unwrap(), Account {
    state: to_vec(&0u64).unwrap(),
    predicates: PredicateTree::And(
      Box::new(PredicateTree::Id(Predicate {
        // token contract predicate
        code: Code::AccountRef("/token".parse().unwrap(), "predicate".into()),
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

pub fn create_initial_blockchain_state(mint_authority: PublicKey) -> StateDiff {
  let mut state = StateDiff::default();
  state.apply(install_standard_library());
  state.apply(install_token_bytecode());
  state.apply(create_usdx_token(mint_authority));
  state
}

pub fn precache_predicates_bytecode(
  state: &impl State,
  addr: &Address,
) -> StateDiff {
  let bytecode = state.get(addr).expect("bytecode not found").state;
  let codehash = multihash::Code::Sha3_256.digest(&bytecode);

  let compiler = Cranelift::default();
  let store = Store::new(compiler);
  let compiled = Module::from_binary(&store, &bytecode)
    .expect("compilation failed")
    .serialize()
    .expect("compiled wasm serialization failed");

  let mut diff = StateDiff::default();
  diff.set(
    format!(
      "/predcache/{}",
      bs58::encode(codehash.to_bytes()).into_string()
    )
    .parse()
    .expect("validated at compile time"),
    Account {
      state: compiled.to_vec(),
      predicates: PredicateTree::Id(Predicate {
        code: Code::Inline(vec![]),
        params: vec![],
      }),
    },
  );
  diff
}
