use {
  crate::Address,
  serde::{Deserialize, Serialize},
};

/// Predicates validate state and permit writes to an account.
/// Each predicate points to an account address where the predicate
/// WASM logic is stored, and a set of parameters specific to an instance
/// of the predicate.
///
/// For example, we could have a generic signature verification wasm predicate
/// or a multisig predicate stored under address 0xAAA. When it is attached to
/// an account, its params will have one or more public keys that it will use to
/// verify signatures for specific accounts. So all accounts that want to add
/// signature verification for their state mutations will create a new predicate
/// instance with code = 0xAAA and params = <authorized pubkey>
///
/// Another example are app predicates, so for example a token app account could
/// have a predicate that validates that the sum of tokens before a transfer
/// in both accounts is equal to sum of tokens after the transfer and that
/// sender's signature is attached to the transaction.
///
/// Code running as a predicate is not allowed to modify any state on chain, it
/// has only read access and can returns a boolean value.
///
/// Predicates WASM code must export a function with the following signature:
///
///   validate(Context, Transaction, OldState, NewState) -> bool
///   
///   where:
///     Context: the value of the "params" field
///     Transaction: the transaction that is trying to modify account state
///     OldState: the state of the account prior to state modification
///     NewState: the desired new state of the account.
///     return value: true to permit state change, otherwise false
#[derive(Debug, Serialize, Deserialize)]
pub struct Predicate {
  /// Address of an account that stores the predicate logic in WASM.
  ///
  /// Predicate accounts are be immutable once created, that is achieved by
  /// attaching a predicate that always evaluates to "false" after the WASM
  /// bytecode is stored.
  code: Address,

  /// Parameters to an instance of a predicate for a specific account.
  ///
  /// That parameters specialize a predicate to the context they are used in,
  /// for example by providing specific public keys for for signature
  /// verification, or providing concrete addresses of price oracles, or any
  /// other bytestrings that are understood by the predicate code.
  ///
  /// This bytestring is passed to the predicate as the `Context` parameter.
  params: Vec<u8>,
}

/// Represents the basic unit of storage and state verification in Anoma.
///
/// An account stores arbitary data that must satisfy all its predicates.
/// Accounts may store WASM code that acts as predicates for other accounts.
#[derive(Debug, Serialize, Deserialize)]
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
  state: Vec<u8>,

  /// A set of predicate instances for this specific account.
  ///
  /// All those predicates must evaluate to "true" before a state
  /// change is permitted. Predicate execution order is unspecified
  /// and they can run in parallel. They are read-only functions, so
  /// execution order is irrelevant.
  predicates: Vec<Predicate>,
}
