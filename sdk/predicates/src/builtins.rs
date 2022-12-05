//#![cfg(target_family = "wasm")]

use anoma_primitives::{Param, Trigger};

extern crate alloc;

use {
  alloc::{boxed::Box, vec::Vec},
  anoma_primitives::Transaction,
};

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[export_name = "__allocate"]
pub extern "C" fn allocate(size: u32) -> *mut u8 {
  let mut buf = Vec::with_capacity(size as usize);
  let ptr = buf.as_mut_ptr();
  core::mem::forget(buf);
  ptr
}

#[export_name = "__ingest_transaction"]
pub extern "C" fn ingest_transaction(
  ptr: *mut u8,
  len: usize,
) -> *const Transaction {
  let bytes = unsafe { Vec::from_raw_parts(ptr, len, len) };
  let transaction = Box::new(rmp_serde::from_slice(&bytes).expect(
    "The virtual machine encoded an invalid transaction object. This is a bug \
     in Anoma not in your code.",
  ));
  Box::leak(transaction)
}

#[export_name = "__ingest_trigger"]
pub extern "C" fn ingest_trigger(ptr: *mut u8, len: usize) -> *const Trigger {
  let bytes = unsafe { Vec::from_raw_parts(ptr, len, len) };
  let transaction = Box::new(rmp_serde::from_slice(&bytes).expect(
    "The virtual machine encoded an invalid trigger object. This is a bug in \
     Anoma not in your code.",
  ));
  Box::leak(transaction)
}

#[export_name = "__ingest_params"]
#[allow(improper_ctypes_definitions)] // this is rust to rust across WASM, not rust to C
pub extern "C" fn ingest_params(ptr: *mut u8, len: usize) -> *const [Param] {
  let bytes = unsafe { Vec::from_raw_parts(ptr, len, len) };
  let params: Vec<Param> = rmp_serde::from_slice(&bytes).expect(
    "The virtual machine encoded an invalid params object. This is a bug in \
     Anoma not in your code.",
  );
  Box::leak(params.into_boxed_slice()) as *const _
}
