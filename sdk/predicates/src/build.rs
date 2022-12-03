#![cfg(not(target_family = "wasm"))]

pub fn configure_build() {
  let target_family =
    std::env::var("CARGO_CFG_TARGET_FAMILY").expect("set by cargo");

  if target_family == "wasm" {
    println!("cargo:rustc-link-arg=--import-memory");
  }
}
