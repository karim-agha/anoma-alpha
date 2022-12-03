mod runtime;
mod state;

pub use {
  runtime::{evaluate, Error as RuntimeError},
  state::{InMemoryStateStore, State, StateDiff},
};
