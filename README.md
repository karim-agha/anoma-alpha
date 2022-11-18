# Anoma Blockchain prototye

This prototype aims to reproduce the following topology:

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

To run this topology make sure that you have Docker and `docker-compose` installed on your system and run:

```
$ docker-compose up --build
```

This command will configure and run the topology described in the above diagram and expose two RPC HTTP interfaces on ports `8081` and `8082` and a blockchain explorer on port `8083`.

To monitor various metrics recorded by telemetry navigate your browser to http://localhost:10000.

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


## Programming model

For an overview of Anoma intent-centric model consult the [whitepaper](https://github.com/anoma/whitepaper/blob/main/whitepaper.pdf).