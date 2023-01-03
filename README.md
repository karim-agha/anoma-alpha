# Anoma Blockchain Alpha Version

This milestone aims to reproduce the following topology:

```
 ┌────────┐                     ┌───────────┐   │   ┌───────┐
 │Solver 1├────────┐  ┌─────────┤Validator 1│   │   │       │
 └────────┘        │  │         └───────────┘   │   │Block 0│
                   │  │                         │   │       │
 ┌────────┐        │  │         ┌───────────┐   │   └───┬───┘
 │Solver 2│        ▼  ▼   ┌─────┤Validator 2│   │       │
 └────────┴──►P2P Gossip◄─┘     └───────────┘   │   ┌───▼───┐
              ▲   ▲ ▲  ▲                        │   │       │
              │   │ │  │        ┌───────────┐   │   │Block 1│
              │   │ │  └────────┤Validator 3│   │   │       │
              │   │ │           └───────────┘   │   └───┬───┘
              │   │ │                           │       │
              │   │ │           ┌───────────┐   │   ┌───▼───┐
              │   │ └───────────┤Validator 4│   │   │       │
              │   │             └───────────┘   │   │Block 2│
              │   │                             │   │       │
  ┌──────────┬┘ ┌─┴────────┐                    │   └───────┘
  │  RPC 1   │  │  RPC 2   │                    │      ...
  └────▲─────┘  └──▲───▲───┘                    │       │
       │           │   │                        │       │
       │           │   │        ┌──────────┐    │   ┌───▼───┐
       │           │   └───────►│Blockchain│    │   │       │
       │           │            │          │    │   │Block N│
       ▼           ▼            │ Explorer │    │   │       │
    Client 1     Client 2       └──────────┘    │   └───────┘
```

## Build instructions

In the root directory execute the following command:
```
$ make
```

To run unit tests run the following command in the root directory:

```
$ make test
```

## Nodes

### Solver
Solvers listen on intents (partial transactions) received through gossip and try to find a state mutation that satisfies all intents and validity predicates of the app and accounts targeted by intents.

### RPC
RPC nodes expose HTTP api for external clients that allows:
  - Receiving partial transactions (intents) from external clients and gossiping them to solvers through p2p.
  - Listen on the p2p network for newly propagaged blocks and store locally the latest state of the chain.
  - Inspecting current blockchain state, used mostly by explorers and apps for checking status of the chain (e.g confirming a transaction, querying a block, etc.)

### Validator

Validators package complete transactions into blocks and try to achieve consensus on the canonical block in the blockchain. The also propagate blocks across nodes using gossip.

Validators may optionally expose an RPC interface for clients to interact with the chain and that makes them also RPC nodes (they will have two roles).

### Explorer

Explorers use RPC nodes to inspect the contents of the blockchain:
  - Blocks
  - Transactions
  - State
  - Stats

## Libraries

### Primitives

This crate defines basic types used across all types of nodes in the network, such as keypairs, addresses, etc. More details about this crate are [here](primitives/README.md).

### Network

This crate implements the P2P gossip mechanism used by all nodes that participate in the gossip. More details about this crate are [here](network/README.md).

### SDK

#### Predicates Rust SDK

This crate is a Rust-based SDK for building WASM predicates that validate state on-chain. More details about this crate are [here](sdk/predicates/README.md).

#### Solver Rust SDK

This crate is a Rust-based SDK for building intent solvers that fulfill intents and create transactions. More details about this create are [here](sdk/solver/README.md).

#### Local Devnode

This crate implements a local single-machine validator node with no consensus for development and test scenarios. More details about this crate are [here](sdk/devnode/README.md).

### Standard Predicate Library

This crate implements the Standard Predicates Library; a set of reusable foundational predicates that are shipped with the chain and defined in genesis. More details are [here](stdpred/README.md).

### Anoma Virtual Machine

This crate implements the virtual machine; responsible for executing WASM compiled predicates. More details are [here](vm/README.md).

## Programming model

For an overview of Anoma intent-centric model consult the [whitepaper](https://github.com/anoma/whitepaper/blob/main/whitepaper.pdf). Take a look also at the SDK and the Standard Predicate Library. Examples can be found in the [sdk examples](sdk/predicates/examples/) directory and in [VM tests](vm/tests/).