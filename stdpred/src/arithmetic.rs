use anoma_predicates_sdk::{
  predicate,
  ExpandedParam,
  ExpandedTransaction,
  Trigger,
};

/// Takes two arguments and varifies that they are equal 64bit unsigned
/// integers.
#[predicate]
fn uint_equal(
  params: &[ExpandedParam],
  _: &Trigger,
  _: &ExpandedTransaction,
) -> bool {
  assert_eq!(params.len(), 2);

  let mut it = params.iter();
  let first = slice_to_u64(it.next().expect("asserted").data());
  let second = slice_to_u64(it.next().expect("asserted").data());

  first == second
}

/// Takes two 64bit unsigned int arguments and varifies that first
/// is > than second.
#[predicate]
fn uint_greater_than(
  params: &[ExpandedParam],
  _: &Trigger,
  _: &ExpandedTransaction,
) -> bool {
  assert_eq!(params.len(), 2);

  let mut it = params.iter();
  let first = slice_to_u64(it.next().expect("asserted").data());
  let second = slice_to_u64(it.next().expect("asserted").data());

  first > second
}

/// Takes two 64bit unsigned ints arguments and varifies that first
/// is >= than second.
#[predicate]
fn uint_greater_than_equal(
  params: &[ExpandedParam],
  _: &Trigger,
  _: &ExpandedTransaction,
) -> bool {
  assert_eq!(params.len(), 2);

  let mut it = params.iter();
  let first = slice_to_u64(it.next().expect("asserted").data());
  let second = slice_to_u64(it.next().expect("asserted").data());

  first >= second
}

/// Takes two 64bit unsigned arguments and varifies that first
/// is < than second.
#[predicate]
fn uint_less_than(
  params: &[ExpandedParam],
  _: &Trigger,
  _: &ExpandedTransaction,
) -> bool {
  assert_eq!(params.len(), 2);

  let mut it = params.iter();
  let first = slice_to_u64(it.next().expect("asserted").data());
  let second = slice_to_u64(it.next().expect("asserted").data());

  first < second
}

/// Takes two 64bit unsigned ints arguments and varifies that first
/// is < than second.
#[predicate]
fn uint_less_than_equal(
  params: &[ExpandedParam],
  _: &Trigger,
  _: &ExpandedTransaction,
) -> bool {
  assert_eq!(params.len(), 2);

  let mut it = params.iter();
  let first = slice_to_u64(it.next().expect("asserted").data());
  let second = slice_to_u64(it.next().expect("asserted").data());

  first <= second
}

/// Takes three arguments and verifies that argument at index 0 is greater than
/// arument at index 1 by a constant uint at argument index 2.
#[predicate]
fn uint_greater_than_by(
  params: &[ExpandedParam],
  _: &Trigger,
  _: &ExpandedTransaction,
) -> bool {
  assert_eq!(params.len(), 3);

  let mut it = params.iter();
  let first = slice_to_u64(it.next().expect("asserted").data());
  let second = slice_to_u64(it.next().expect("asserted").data());
  let by = slice_to_u64(it.next().expect("asserted").data());

  first.saturating_sub(second) == by
}

/// Takes three arguments and verifies that argument at index 0 is less than
/// arument at index 1 by a constant uint at argument index 2.
#[predicate]
fn uint_less_than_by(
  params: &[ExpandedParam],
  _: &Trigger,
  _: &ExpandedTransaction,
) -> bool {
  assert_eq!(params.len(), 3);

  let mut it = params.iter();
  let first = slice_to_u64(it.next().expect("asserted").data());
  let second = slice_to_u64(it.next().expect("asserted").data());
  let by = slice_to_u64(it.next().expect("asserted").data());

  second.saturating_sub(first) == by
}

fn slice_to_u64(bytes: &[u8]) -> u64 {
  rmp_serde::from_slice(bytes).expect("invalid argument format")
}
