use {
  alloc::{collections::BTreeMap, vec::Vec},
  anoma_predicates_sdk::{predicate, ExpandedParam, PredicateContext},
};

#[predicate]
fn is_empty_map(params: &Vec<ExpandedParam>, _: &PredicateContext) -> bool {
  assert_eq!(params.len(), 1);

  let mut it = params.iter();
  let first = it.next().expect("asserted").data();

  let set: BTreeMap<Vec<u8>, Vec<u8>> = rmp_serde::from_slice(first)
    .expect("Invalid collection format. Expecting BTreeMap<Vec<u8>, Vec<u8>>.");

  set.is_empty()
}

#[predicate]
fn contains_key(params: &Vec<ExpandedParam>, _: &PredicateContext) -> bool {
  assert_eq!(params.len(), 2);

  let mut it = params.iter();
  let haystack = it.next().expect("asserted").data();
  let needle = it.next().expect("asserted").data();

  let map: BTreeMap<Vec<u8>, Vec<u8>> = rmp_serde::from_slice(haystack)
    .expect("Invalid collection format. Expecting BTreeMap<Vec<u8>, Vec<u8>>.");

  map.contains_key(needle)
}


#[predicate]
fn key_equals(params: &Vec<ExpandedParam>, _: &PredicateContext) -> bool {
  assert_eq!(params.len(), 2);

  let mut it = params.iter();
  let haystack = it.next().expect("asserted").data();
  let needle = it.next().expect("asserted").data();
  let value = it.next().expect("asserted").data();

  let map: BTreeMap<Vec<u8>, Vec<u8>> = rmp_serde::from_slice(haystack)
    .expect("Invalid collection format. Expecting BTreeMap<Vec<u8>, Vec<u8>>.");

  map.get(needle).map(|v| v == value).unwrap_or(false)
}
