//! This example illustrates how to build a token using Anoma Predicates SDK.
//!
//! A token consists of:
//!   1. one top-level account that governs the token behaviour
//!   2. many sub-accounts of the top-level account that contain
//!      balances of individual wallets. Wallet balance accounts
//!      also are responsible for the spending authorization logic
//!      of those accounts tokens.
//!
//! If we were to build a USDX token then the logic would look as following:
//!
//! /token account
//!   constructor params:
//!   - none
//!
//!   account state:
//!   - WASM bytecode containing predicates for token instance accounts
//!
//!   validity predicate:
//!   - stdpred/immutable_state && stdpred/immutable_predicates
//!
//! /token/usdx account
//!   constructor params:
//!   - token account top-level address ("/token/usdx")
//!   - mint_authority public key
//!
//!   account state:
//!   - mint_authority_public_key
//!   - total_supply
//!
//! validity predicate:
//!   1. for each /token/usdx/* proposal:
//!       1. sum their current_balance
//!       2. sum their proposed_balance
//!   2.
//!     2.a - assert that sum_current_balance == sum_proposed_balance
//!       or
//!     2.b - assert that mint_authority_pubkey signature is in one of the
//!           intents, identified by base58 representation of its pubkey.
//!         - assert that the difference in sum is reflected in the total_supply
//!           value in /token/usdx
//!   3. stdpred/immutable_predicates
//!
//! /token/usdx/* wallet accounts
//!   example addresses:
//!   - /token/usdx/0x0239d39F0c3F9b26cF728bC6b09872C090935E9f
//!   - /token/usdx/example.eth
//!
//!   account state:
//!   - u64 value specifying balance of this wallet
//!
//!   constructor params:
//!   - owner public key
//!
//!   validity predicate:
//!     if proposed_balance < current_balance || validity predicates modified {
//!       assert that at least one of the intents has a calldata
//!       entry named after the owner public key in base58 and it
//!       holds a bytesting that is a valid signature for the containing
//!       intent sha3 hash of (recent_blockhash || predicate_tree)
//!     } else {
//!       always allow, everyone is happy to receive tokens
//!     }
//!
//! The spending authorization logic of the balance accounts could varry
//! between different accounts, some might want to have a multisig, others
//! may chose to use a single pubkey, or a password. For that purpose the
//! validity predicates on the token account will not be implemented as part
//! of the token VPs, instead they will be constructed on a case by case
//! basis when new accounts are created and assembled from predicates from
//! the standard predicate library.
//!
//! To make the token validity predicates reusable across many different
//! token instances, we will deploy this code at /token and each token
//! instance will reference the validity predicate in /token with token
//! specific constructor params. The /token account VP will only ensure
//! that the contents of the /token account is immutable by giving it
//! a predicate from the standard predicate library that always returns
//! true if the modified account is equal to "/token".
//!
//! All state in accounts in serialized using MessagePack format.
