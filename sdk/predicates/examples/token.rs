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
//!   - account_ref(self)
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
//!           intents, identified by base58 Repr of its pubkey.
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

use {
  anoma_predicates_sdk::{
    predicate,
    Address,
    ExpandedAccountChange,
    ExpandedParam,
    ExpandedTransaction,
    Trigger,
  },
  ed25519_dalek::{PublicKey, Signature, Verifier},
  serde::{Deserialize, Serialize},
};

#[derive(Debug, Serialize, Deserialize)]
enum TokenState {
  V1 { total_supply: u64 },
}

#[derive(Debug, Serialize, Deserialize)]
enum WalletBalanceState {
  V1 { amount: u64 },
}

#[predicate]
fn predicate(
  params: &[ExpandedParam],
  trigger: &Trigger,
  tx: &ExpandedTransaction,
) -> bool {
  assert_eq!(params.len(), 3);
  assert!(
    matches!(trigger, Trigger::Proposal(_)),
    "this predicate is only valid on accounts and cannot be used in intents"
  );

  let mut argit = params.iter();

  let self_addr: Address =
    rmp_serde::from_slice(argit.next().expect("asserted").data())
      .expect("invalid self address param format");

  let mint_auth: PublicKey =
    rmp_serde::from_slice(argit.next().expect("asserted").data())
      .expect("invalid public key param");

  let current_total_supply = match argit.next().expect("asserted") {
    ExpandedParam::AccountRef(addr, state) => {
      assert_eq!(self_addr, *addr, "invalid token state account address");
      read_total_supply(state)
    }
    _ => panic!("Expecting an AccountRef to the token address"),
  };

  let (pre, post) = sum_balances(&self_addr, tx);

  if pre == post {
    // total supply didn't change, so we don't need any signature from
    // the mint authority. We're done here, and intent predicates will
    // verify that their individual balances are correct for the proposed
    // wallet balance changes in the transaction.
    true
  } else {
    // total supply changed, need to make sure that the global token account
    // has an updated total supply that reflects the delta of pre & post
    // balances and that this change in the token supply value is authorized
    // by the mint authority.
    if !is_signed_by_mint_auth(&mint_auth, tx) {
      return false;
    }

    let total_supply_proposal = match tx.proposals.get(&self_addr) {
      Some(v) => v,
      None => return false, // tx did not update the total supply, fail.
    };

    // now veriy that the new total supply value is correct.
    let expected_new_supply = current_total_supply + post - pre;

    if pre < post {
      match total_supply_proposal {
        // this is the case for the very first mint of this token
        ExpandedAccountChange::CreateAccount(acc) => {
          read_total_supply(&acc.state) == expected_new_supply
        }

        // verify that the new total supply is equal to the increase in
        // balances sum in this transaction
        ExpandedAccountChange::ReplaceState { proposed, .. } => {
          read_total_supply(proposed) == expected_new_supply
        }

        // Global token accounts are expected to have immutable predicates
        // after they are created
        ExpandedAccountChange::ReplacePredicates { .. } => false,

        // tokens minted, can't delete this token type before burning
        // all circulating tokens.
        ExpandedAccountChange::DeleteAccount { .. } => false,
      }
    } else {
      match total_supply_proposal {
        // Can't burn tokens from a token that does not exist yet
        ExpandedAccountChange::CreateAccount(_) => false,

        // verify that the new total supply is equal to the increase in
        // balances sum in this transaction
        ExpandedAccountChange::ReplaceState { proposed, .. } => {
          read_total_supply(proposed) == expected_new_supply
        }

        // Global token accounts are expected to have immutable predicates
        // after they are created
        ExpandedAccountChange::ReplacePredicates { .. } => false,

        // token account can be deleted only if all tokens were burnt
        ExpandedAccountChange::DeleteAccount { .. } => expected_new_supply == 0,
      }
    }
  }
}

fn sum_balances(token_addr: &Address, tx: &ExpandedTransaction) -> (u64, u64) {
  let balance = |state| -> u64 {
    let state: WalletBalanceState = rmp_serde::from_slice(state)
      .expect("invalid token balance account state");
    match state {
      WalletBalanceState::V1 { amount } => amount,
    }
  };

  let pre_sum = tx
    .proposals
    .iter()
    .filter(|(addr, _)| token_addr.is_parent_of(addr))
    .fold(0, |acc, (_, change)| {
      acc
        + (match change {
          ExpandedAccountChange::ReplaceState { current, .. } => {
            balance(current)
          }
          ExpandedAccountChange::DeleteAccount { current } => {
            balance(&current.state)
          }
          _ => 0,
        })
    });

  let post_sum = tx
    .proposals
    .iter()
    .filter(|(addr, _)| token_addr.is_parent_of(addr))
    .fold(0, |acc, (_, change)| {
      acc
        + (match change {
          ExpandedAccountChange::CreateAccount(acc) => balance(&acc.state),
          ExpandedAccountChange::ReplaceState { proposed, .. } => {
            balance(proposed)
          }
          _ => 0,
        })
    });

  (pre_sum, post_sum)
}

fn is_signed_by_mint_auth(
  mint_auth: &PublicKey,
  tx: &ExpandedTransaction,
) -> bool {
  let calldata_key = bs58::encode(mint_auth.as_bytes()).into_string();
  for intent in &tx.intents {
    if let Some(signature) = intent.calldata.get(&calldata_key) {
      if let Ok(signature) = Signature::from_bytes(signature) {
        if mint_auth
          .verify(&intent.hash().to_bytes(), &signature)
          .is_ok()
        {
          return true;
        }
      }
    }
  }
  false
}

fn read_total_supply(state: &[u8]) -> u64 {
  let token_state: TokenState =
    rmp_serde::from_slice(state).expect("invalid token account state format");
  match token_state {
    TokenState::V1 { total_supply } => total_supply,
  }
}