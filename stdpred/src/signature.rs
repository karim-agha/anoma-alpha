use {
  anoma_predicates_sdk::{
    predicate,
    PopulatedParam,
    Transaction,
    Trigger,
    TriggerRef,
  },
  ed25519_dalek::{PublicKey, Signature, Verifier},
};

/// Takes three arguments and verifies that the contents of argument at index 1
/// is a valid Ed25519 signature by public key at index 0 for the blockhash
/// value in the Intent.
#[predicate]
fn verify_ed25519_signature(
  params: &[PopulatedParam],
  trigger: &Trigger,
  transaction: &Transaction,
) -> bool {
  assert_eq!(params.len(), 2);

  let mut args = params.iter();
  let pubkey = PublicKey::from_bytes(args.next().expect("asserted").data())
    .expect("invalid public key format");
  let signature = Signature::from_bytes(args.next().expect("asserted").data())
    .expect("invalid signature format");

  let trigger = transaction.get(trigger).expect(
    "The virtual machine encoded an invalid trigger reference. This is a bug \
     in Anoma not in your code.",
  );

  let preimage = match trigger {
    TriggerRef::Intent(intent) => intent.hash(),
    TriggerRef::Proposal(_, _) => {
      todo!()
    }
  };

  pubkey.verify(&preimage.to_bytes(), &signature).is_ok()
}
