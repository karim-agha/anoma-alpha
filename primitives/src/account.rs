use {
  crate::PredicateTree,
  alloc::vec::Vec,
  serde::{Deserialize, Serialize},
};

/// Represents the basic unit of storage and state verification in Anoma.
///
/// An account stores arbitary data that must satisfy all its predicates, and
/// its parents predicates before a mutation is allowed to happen.
///
/// Accounts may store WASM code that acts as predicates for other accounts.
///
/// Accounts are organized in a hierarchical path-like structure. For example
/// if there is a token type defined at account "/token/usda" and there is
/// wallet balance account defined at "/token/usda/walletaddress1", then any
/// modification to the latter will trigger validity predicates stored at
/// addresses.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Account {
  /// Arbitary data stored within an account, this could be some token
  /// balance for a user wallet, wasm bytecode that acts as a predicate,
  /// or other app-specific state.
  ///
  /// All accounts in the system are readable by all predicates and external
  /// clients, including solvers, RPC clients, etc. They are also writable by
  /// all clients as long as they satisfy all predicates.
  ///
  /// Access control to accounts is implemented through predicates, so for
  /// example if a user-token-account will have a predicate that validates a
  /// signature, and if it is satisfied then the predicate allows state
  /// modification.
  ///
  /// Any state write will invoke all predicates and occurs only if they all
  /// return true, otherwise state change is rejected.
  pub state: Vec<u8>,

  /// A Boolean Expressions Tree of predicates for an account.
  ///
  /// This bool expression tree of predicates must evaluate to "true" before a
  /// state change is permitted. Predicate execution order is unspecified
  /// and they can run in parallel. Some predicates may not run at all if a
  /// short-circut result is found. They are read-only functions, so
  /// execution order is irrelevant for the final result.
  pub predicates: PredicateTree,
}
