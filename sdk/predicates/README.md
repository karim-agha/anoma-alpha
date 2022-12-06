# Anoma Predicates Rust SDK

## Design decisions
- Marshalling data between VM runtime and WASM code happens over [MessagePack](https://msgpack.org). This standard is implemented in 50+ of different languages and keeps the door open to having future non-rust SDKs, especially for client platforms like mobile and web.
  
- By the time a predicate is called, all parameters are resolved (such as `AccountRef`, `CalldataRef`, `ProposalRef`). Predicates themselves are not allowed to read arbitary accounts. The reason for this is to enable safe deterministic parallel transaction execution. By limiting external reads to a well-known set of addresses we can tell whether there will be a read/write overlap between two transactions and make optimal execution scheduling decisions.


## Example
Here is an example predicate example implemented using the SDK:

```rust
#[predicate]
fn uint_equal(params: &[ParamValue], _: &Trigger, _: &Transaction) -> bool {
  assert_eq!(params.len(), 2);

  let mut it = params.iter();
  
  let first: u64 = rmp_serde::from_slice(
    it.next()
      .expect("asserted")
      .data()).expect("invalid argument format");

  let second: u64 = rmp_serde::from_slice(
    it.next()
      .expect("asserted")
      .data()).expect("invalid argument format");

  first == second
}
```

This gets compiled to WASM and uploaded to an account on-chain then referenced by intents. Alternativelly it can be embedded in an intent directly if it is not used often by many predicates and you want to save on gas costs. The [standard predicate library](../../stdpred/README.md) is built using this SDK.