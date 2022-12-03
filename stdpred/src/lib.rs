#![cfg_attr(target_family = "wasm", no_std)]

// panic handler
pub use anoma_predicates_sdk;

#[no_mangle]
pub extern "C" fn r#const(a: u32, b: u32) -> u32 {
  a + b
}

#[no_mangle]
pub extern "C" fn uint_greater_than_by(a: u32, b: u32) -> u32 {
  a + b
}

#[no_mangle]
pub extern "C" fn uint_less_than_by(a: u32, b: u32) -> u32 {
  a + b
}

#[no_mangle]
pub extern "C" fn verify_ed25519_signature(a: u32, b: u32) -> u32 {
  a + b
}
