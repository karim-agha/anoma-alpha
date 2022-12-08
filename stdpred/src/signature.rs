use anoma_predicates_sdk::{
  ed25519::{PublicKey, Signature, Verifier},
  predicate,
  ExpandedParam,
  ExpandedTransaction,
  Trigger,
};

/// Verifies that the transaction includes an intent that contains a signature
/// for a given public key. The signature should be in calldata under a string
/// key that is the base58 representation of the required signing pubkey.
#[predicate]
fn require_ed25519_signature(
  params: &[ExpandedParam],
  _: &Trigger,
  transaction: &ExpandedTransaction,
) -> bool {
  assert_eq!(params.len(), 2);

  let mut args = params.iter();
  let pubkey: PublicKey =
    rmp_serde::from_slice(args.next().expect("asserted").data())
      .expect("invalid public key format");

  let expected_calldata_key = bs58::encode(&pubkey.as_bytes()).into_string();
  for intent in &transaction.intents {
    if let Some(signature) = intent.calldata.get(&expected_calldata_key) {
      if let Ok(signature) = Signature::from_bytes(signature) {
        return pubkey
          .verify(&intent.signing_hash().to_bytes(), &signature)
          .is_ok();
      }
    }
  }

  false
}
