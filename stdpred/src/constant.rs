use anoma_predicates_sdk::{
  predicate,
  AccountChange,
  Address,
  PopulatedParam,
  Transaction,
  Trigger,
  TriggerRef,
};

/// Always return a const true or false.
///
/// Parameters:
///   0: Boolean value that is always returned by this predicate
#[predicate]
fn constant(params: &[PopulatedParam], _: &Trigger, _: &Transaction) -> bool {
  assert_eq!(params.len(), 1);
  rmp_serde::from_slice(params.iter().next().expect("asserted").data())
    .expect("invalid argument format")
}

/// Forbids any changes to the specified account but allows changes to its
/// children
///
/// Parameters:
///   0. Address of the immutable account
#[predicate]
fn immutable(
  params: &[PopulatedParam],
  trigger: &Trigger,
  _: &Transaction,
) -> bool {
  assert_eq!(params.len(), 1);
  assert!(matches!(trigger, Trigger::Proposal(_))); // only valid on account predicates

  // make sure that the change is targetting this account not any of its
  // children
  let target: Address =
    rmp_serde::from_slice(params.first().expect("asserted").data())
      .expect("invalid predicate param");

  if let Trigger::Proposal(addr) = trigger {
    // allow changes only to children of this account but not the account itself
    target != *addr
  } else {
    unreachable!("already asserted that its an account predicate");
  }
}

/// Forbids any changes to the specified account predicates while permitting
/// changes to its state. Also any changes to its children are permitted.
///
/// Parameters:
///   0. Address of the immutable predicates account
#[predicate]
fn immutable_predicates(
  params: &[PopulatedParam],
  trigger: &Trigger,
  tx: &Transaction,
) -> bool {
  assert_eq!(params.len(), 1);
  assert!(matches!(trigger, Trigger::Proposal(_))); // only valid on account predicates

  // make sure that the change is targetting this account not any of its
  // children
  let target: Address =
    rmp_serde::from_slice(params.first().expect("asserted").data())
      .expect("invalid predicate param");

  if let Some(TriggerRef::Proposal(addr, change)) = tx.get(trigger) {
    // allow changes only to children of this account but not the account itself
    // allow changes to the account state but not its predicates
    if target == *addr && matches!(change, AccountChange::ReplacePredicates(_))
    {
      return false;
    }
  }
  true
}
