use {
  alloc::{collections::BTreeSet, vec::Vec},
  anoma_predicates_sdk::{predicate, ExpandedParam, PredicateContext},
};

#[predicate]
fn is_empty_set(params: &Vec<ExpandedParam>, _: &PredicateContext) -> bool {
  assert_eq!(params.len(), 1);

  let mut it = params.iter();
  let first = it.next().expect("asserted").data();

  let set: BTreeSet<Vec<u8>> = rmp_serde::from_slice(first)
    .expect("Invalid collection format. Expecting BTreeSet<Vec<u8>>.");

  set.is_empty()
}

#[predicate]
fn contains_element(params: &Vec<ExpandedParam>, _: &PredicateContext) -> bool {
  assert_eq!(params.len(), 2);

  let mut it = params.iter();
  let haystack = it.next().expect("asserted").data();
  let needle = it.next().expect("asserted").data();

  let set: BTreeSet<Vec<u8>> = rmp_serde::from_slice(haystack)
    .expect("Invalid collection format. Expecting BTreeSet<Vec<u8>>.");

  set.contains(needle)
}
