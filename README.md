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

For an overview of Anoma concepts consult the [whitepaper](https://github.com/anoma/whitepaper/blob/main/whitepaper.pdf). We will walk through an example application that represents a token and implements transfers between user wallets.

We start by creating a token account with address `0xBTCToken1`. The token account will have the total token supply stored in its data section along with entries on which public keys are allowed to mint or burn new tokens.

Then we have two user wallet addresses (externally owned accounts): `0xWallet1` and `0xWallet2`. `0xWallet1` wants to transfer 5 BTC to `0xWallet2`.

We have a method for deriving new account addresses from externally owned accounts specified in the [Account struct](primitives/src/account.rs). Derived addresses are never on the Ed25519 curve, and that ensures that they will never have a private key that corresponds to their address.

Wallet balances of `0xBTCToken` for wallets `0xWallet1` and `0xWallet2` are stored in derived account addresses computed using `(0xWallet1).derive(0xBTCToken)` and `(0xWallet2).derive(0xBTCToken)`. Let's call them `0xWallet1BTC` and `0xWallet2BTC`.

We will have three sets of [validity predicates](primitives/src/account.rs):
- `0xBTCToken`:
  - ensure that the sum of balances before the transfer is equal to the sum of tokens after the transfer (no new tokens are created) in both wallet accounts.
- `0xWallet1BTC`:
  - If the new balance of the wallet account is less than the old balance, make sure that a signature of `0xWallet1` is attached to the transaction. The address of `0xWallet1` is stored in the `params` field of the predicate instance attached to the `0xWallet1BTC`.
- `0xWallet2BTC`:
  - If the new balance of the wallet account is less than the old balance, make sure that a signature of `0xWallet2` is attached to the transaction. The address of `0xWallet2` is stored in the `params` field of the predicate instance attached to the `0xWallet2BTC`.

All validity predicates for all accounts are represented as WASM bytecode that exports the following function:

```
   validate(Context, Transaction, OldState, NewState) -> bool
```
where:
  - `Context`: the value of the "params" field
  - `Transaction`: the transaction that is trying to modify account state
  - `OldState`: the state of the account prior to state modification
  - `NewState`: the desired new state of the account.
  - return value: `true` to permit state change, otherwise `false`

Wallet owner of `0xWallet1` initiates the transfer by sending [Intents](primitives/src/intent.rs). tbd.