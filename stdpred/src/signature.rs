use {
  alloc::vec::Vec,
  anoma_predicates_sdk::{predicate, ExpandedParam, PredicateContext},
  ed25519_dalek::{PublicKey, Signature, Verifier},
};

/// Verifies that the transaction includes an intent that contains a signature
/// for a given public key. The signature should be in calldata under a string
/// key that is the base58 representation of the required signing pubkey.
#[predicate]
fn require_ed25519_signature(
  params: &Vec<ExpandedParam>,
  context: &PredicateContext,
) -> bool {
  assert_eq!(params.len(), 1);
  let pubkey = PublicKey::from_bytes(params[0].data()).expect("invalid pubkey");
  let expected_calldata_key = bs58::encode(&pubkey.as_bytes()).into_string();
  for (hash, calldata) in &context.calldata {
    if let Some(signature) = calldata.get(&expected_calldata_key) {
      if let Ok(signature) = Signature::from_bytes(signature) {
        return pubkey.verify(&hash.to_bytes(), &signature).is_ok();
      }
    }
  }
  false
}
