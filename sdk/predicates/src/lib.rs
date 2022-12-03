#![cfg_attr(target_family = "wasm", no_std)]

mod builtins;
//pub use builtins::*;

#[cfg(target_family = "wasm")]
extern "C" {
  pub fn syscall_terminate();
  pub fn syscall_read_account(_: u32) -> u32;
}

pub use {
  anoma_predicates_sdk_macros::{initialize_library, predicate},
  anoma_primitives::*,
};

#[cfg(not(target_family = "wasm"))]
mod build;

#[cfg(not(target_family = "wasm"))]
pub use build::configure_build;
