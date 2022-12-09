use {
  anoma_predicates_sdk::{
    predicate,
    ExpandedParam,
    ExpandedTransaction,
    Trigger,
  },
};

/// Takes two arguments and varifies that they are equal bytestings
#[predicate]
fn bytes_equal(
  params: &[ExpandedParam],
  _: &Trigger,
  _: &ExpandedTransaction,
) -> bool {
  assert_eq!(params.len(), 2);

  let mut it = params.iter();
  let first = it.next().expect("asserted").data();
  let second = it.next().expect("asserted").data();

  first == second
}