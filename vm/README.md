# Anoma Virtual Machine

## Running scenario tests

First compile the [standard predicate library](../stdpred/) using into WASM:

```
cargo build --package stdpred --target wasm32-unknown-unknown --release
```

Then compile the [token contract example](../sdk/predicates/examples/token.rs) into WASM:

```
cargo build --package anoma-predicates-sdk --example token --target wasm32-unknown-unknown --release
```

And run the scenario tests:

```
cargo test --release --package anoma-vm -- --show-output
```
