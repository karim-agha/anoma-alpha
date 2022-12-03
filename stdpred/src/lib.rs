#![no_std]

use anoma_predicates_sdk::{initialize_library, predicate, Param, Transaction};

initialize_library!();

#[predicate]
fn constant(_params: &[Param], _transaction: &Transaction) -> bool {
  true
}

#[predicate]

fn uint_greater_than_by(_params: &[Param], _transaction: &Transaction) -> bool {
  true
}

#[predicate]
fn uint_less_than_by(_params: &[Param], _transaction: &Transaction) -> bool {
  true
}

#[predicate]
fn verify_ed25519_signature(
  _params: &[Param],
  _transaction: &Transaction,
) -> bool {
  true
}
