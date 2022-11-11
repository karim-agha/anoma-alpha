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
///
/// Each predicate implementation should always document the input format of its
/// [`params`] in its developer reference.
#[derive(Debug, Serialize, Deserialize)]
pub struct Predicate {
  /// Address of an account that stores the predicate logic in WASM.
  ///
  /// Predicate accounts are be immutable once created, that is achieved by
  /// attaching a predicate tree that always evaluates to "false" after the
  /// WASM bytecode is stored.
  ///
  /// As a convention, predicate addresses are generated through the following
  /// formula:
  /// ```
  /// address = sha3(predicate_wasm_bytecode);
  /// ```
  /// however any 32-byte value works as an address, and more human-readable
  /// addresses such as `Equals1xxxxxxxxxxxxxxxxxxxx` in base58 are reserved
  /// for fundamental (builtin) predicates such as Equals, VerifySig, LessThan,
  /// etc.
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

/// A boolean expression tree with predicates on its leaf nodes.
#[derive(Debug, Serialize, Deserialize)]
pub enum PredicateTree {
  Id(Predicate),
  Not(Box<PredicateTree>),
  And(Box<PredicateTree>, Box<PredicateTree>),
  Or(Box<PredicateTree>, Box<PredicateTree>),
}
