use anoma_predicates_sdk::{predicate, PopulatedParam, Transaction, Trigger};

/// Always return a const true or false.
///
/// Parameters:
///   0: 1-byte long byte array with values [0] for false or [1] for true.
#[predicate]
fn constant(params: &[PopulatedParam], _: &Trigger, _: &Transaction) -> bool {
  assert_eq!(params.len(), 1);
  let value = params.iter().next().expect("asserted");
  assert_eq!(value.data().len(), 1);

  match value.data()[0] {
    1 => true,
    0 => false,
    v => panic!(
      "Invalid boolen constant in param. Expecting 1 or 0, found {}",
      v
    ),
  }
}
