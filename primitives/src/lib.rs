#![cfg_attr(target_family = "wasm", no_std)]

extern crate alloc;

mod account;
mod address;
mod b58;
mod intent;
mod predicate;
mod transaction;

use {
  core::fmt::Debug,
  serde::{Deserialize, Serialize},
};

pub trait Repr: Debug + Clone + Serialize + Eq + PartialEq {
  type Param: Debug
    + Clone
    + PartialEq
    + Eq
    + Serialize
    + core::hash::Hash
    + for<'de> Deserialize<'de>;
  type Code: Debug
    + Clone
    + PartialEq
    + Eq
    + Serialize
    + core::hash::Hash
    + for<'de> Deserialize<'de>;
  type AccountChange: Debug
    + Clone
    + PartialEq
    + Serialize
    + core::hash::Hash
    + for<'de> Deserialize<'de>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Expanded;
impl Repr for Expanded {
  type AccountChange = ExpandedAccountChange;
  type Code = ExpandedCode;
  type Param = ExpandedParam;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Exact;
impl Repr for Exact {
  type AccountChange = AccountChange;
  type Code = Code;
  type Param = Param;
}

pub use {account::*, address::*, intent::*, predicate::*, transaction::*};
