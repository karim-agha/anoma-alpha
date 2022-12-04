use anoma_predicates_sdk::{predicate, PopulatedParam, Transaction, Trigger};

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
