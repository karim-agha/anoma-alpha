#![cfg_attr(target_family = "wasm", no_std)]

mod builtins;

extern "C" {
  pub fn syscall_debug_log(ptr: *const u8, len: u32);
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

pub fn debug_log(msg: &str) {
  let serialized = rmp_serde::to_vec(msg).unwrap();
  let ptr = serialized.as_ptr();
  unsafe { syscall_debug_log(ptr, serialized.len() as u32) };
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
      $crate::debug_log(&alloc::format!($($arg)*));
    };
}
