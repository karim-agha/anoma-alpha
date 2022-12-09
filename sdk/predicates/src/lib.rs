#![cfg_attr(target_family = "wasm", no_std)]

mod builtins;

#[cfg(target_family = "wasm")]
extern "C" {
  pub fn syscall_terminate();
}

pub use {
  anoma_predicates_sdk_macros::{initialize_library, predicate},
  anoma_primitives::{
    Address,
    Expanded,
    ExpandedAccountChange,
    ExpandedParam,
    Predicate,
    PredicateContext,
  },
};

#[cfg(not(target_family = "wasm"))]
mod build;

#[cfg(not(target_family = "wasm"))]
pub use build::configure_build;
