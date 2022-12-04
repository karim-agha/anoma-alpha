# Anoma Predicates Rust SDK

## Design decisions
- Marshalling data between VM runtime and WASM code happens over [MessagePack](https://msgpack.org). This standard is implemented in 50+ of different languages and keeps the door open to having future non-rust SDKs, especially for client platforms like mobile and web.
  
- By the time a predicate is called, all parameters are resolved (such as `AccountRef`, `CalldataRef`, `ProposalRef`). Predicates themselves are not allowed to read arbitary accounts. The reason for this is to enable safe deterministic parallel transaction execution. By limiting external reads to a well-known set of addresses we can tell whether there will be a read/write overlap between two transactions and make optimal execution scheduling decisions.


