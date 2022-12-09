use anoma_predicates_sdk::{
  predicate,
  Address,
  ExpandedAccountChange,
  ExpandedParam,
  PredicateContext,
};

/// Always return a const true or false.
///
/// Parameters:
///   0: Boolean value that is always returned by this predicate
#[predicate]
fn constant(params: &[ExpandedParam], _: &PredicateContext) -> bool {
  assert_eq!(params.len(), 1);
  rmp_serde::from_slice(params.iter().next().expect("asserted").data())
    .expect("invalid argument format")
}

/// Forbids any changes to the specified account state but allows changes to its
/// children or its predicates
///
/// Parameters:
///   0. Address of the immutable account
#[predicate]
fn immutable_state(
  params: &[ExpandedParam],
  context: &PredicateContext,
) -> bool {
  assert_eq!(params.len(), 1);

  // make sure that the change is targetting this account not any of its
  // children
  let target: Address =
    rmp_serde::from_slice(params.first().expect("asserted").data())
      .expect("invalid predicate param");

  if let Some(change) = context.proposals.get(&target) {
    if matches!(change, ExpandedAccountChange::ReplaceState { .. }) {
      return false;
    }
  }

  true
}

/// Forbids any changes to the specified account predicates while permitting
/// changes to its state. Also any changes to its children are permitted.
///
/// Parameters:
///   0. Address of the immutable predicates account
#[predicate]
fn immutable_predicates(
  params: &[ExpandedParam],
  context: &PredicateContext,
) -> bool {
  assert_eq!(params.len(), 1);

  // make sure that the change is targetting this account not any of its
  // children
  let target: Address =
    rmp_serde::from_slice(params.first().expect("asserted").data())
      .expect("invalid predicate param");

  if let Some(change) = context.proposals.get(&target) {
    if matches!(change, ExpandedAccountChange::ReplacePredicates { .. }) {
      return false;
    }
  }

  true
}
