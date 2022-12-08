use {
  crate::{Address, Exact, ExpandedAccountChange, Repr},
  alloc::{boxed::Box, string::String, vec::Vec},
  core::fmt::Debug,
  multihash::Multihash,
  serde::{Deserialize, Serialize},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Param {
  Inline(Vec<u8>),
  AccountRef(Address),
  ProposalRef(Address),
  CalldataRef(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExpandedParam {
  Inline(Vec<u8>),
  AccountRef(Address, Vec<u8>),
  ProposalRef(Address, ExpandedAccountChange),
  CalldataRef(String, Vec<u8>),
}

impl ExpandedParam {
  pub fn data(&self) -> &[u8] {
    match self {
      Self::Inline(v) => v,
      Self::AccountRef(_, v) => v,
      Self::ProposalRef(_, ac) => match ac {
        ExpandedAccountChange::CreateAccount(acc) => &acc.state,
        ExpandedAccountChange::ReplaceState { proposed, .. } => proposed,
        ExpandedAccountChange::ReplacePredicates { .. } => &[],
        ExpandedAccountChange::DeleteAccount { .. } => &[],
      },
      Self::CalldataRef(_, v) => v,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Code {
  /// If the predicate code is inlined then it must export a predicate
  /// named "predicate" and it will be the entrypoint.
  Inline(Vec<u8>),
  AccountRef(Address, String), // (address, entrypoint)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExpandedCode {
  pub code: Vec<u8>,
  pub entrypoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Predicate<R: Repr = Exact> {
  pub code: R::Code,
  pub params: Vec<R::Param>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PredicateTree<R: Repr = Exact> {
  Id(Predicate<R>),
  Not(Box<PredicateTree<R>>),
  And(Box<PredicateTree<R>>, Box<PredicateTree<R>>),
  Or(Box<PredicateTree<R>>, Box<PredicateTree<R>>),
}

impl<R: Repr> PredicateTree<R> {
  /// Applies a function to all predicates in the tree and returns a new
  /// tree with the same structure and modified predicates.
  pub fn map<O: Repr, F>(self, op: F) -> PredicateTree<O>
  where
    F: Fn(Predicate<R>) -> Predicate<O> + Clone,
  {
    match self {
      PredicateTree::Id(p) => PredicateTree::<O>::Id(op(p)),
      PredicateTree::Not(pt) => PredicateTree::<O>::Not(Box::new(pt.map(op))),
      PredicateTree::And(l, r) => PredicateTree::<O>::And(
        Box::new(l.map(op.clone())),
        Box::new(r.map(op)),
      ),
      PredicateTree::Or(l, r) => {
        PredicateTree::<O>::Or(Box::new(l.map(op.clone())), Box::new(r.map(op)))
      }
    }
  }

  pub fn try_map<O: Repr, F, E>(self, op: F) -> Result<PredicateTree<O>, E>
  where
    F: Fn(Predicate<R>) -> Result<Predicate<O>, E> + Clone,
  {
    Ok(match self {
      PredicateTree::Id(p) => PredicateTree::<O>::Id(match op(p) {
        Ok(res) => res,
        Err(e) => return Err(e),
      }),
      PredicateTree::Not(pt) => {
        PredicateTree::<O>::Not(Box::new(pt.try_map(op)?))
      }
      PredicateTree::And(l, r) => PredicateTree::<O>::And(
        Box::new(l.try_map(op.clone())?),
        Box::new(r.try_map(op)?),
      ),
      PredicateTree::Or(l, r) => PredicateTree::<O>::Or(
        Box::new(l.try_map(op.clone())?),
        Box::new(r.try_map(op)?),
      ),
    })
  }
}

/// Specifies the reason a predicate is being invoked.
///
/// Prediactes are invoked for two reasons:
///
/// 1. When they are part of an intent, that is included in a transaction, to
/// check if intent expectations are satisfied.
///
/// 2. When an account is mutated, to check
/// if the newly proposed account value satisfies the account requirements or
/// any of its parents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Trigger {
  Intent(Multihash),
  Proposal(Address),
}

#[cfg(test)]
mod tests {
  use crate::{
    Code,
    Exact,
    Expanded,
    ExpandedCode,
    ExpandedParam,
    Param,
    Predicate,
    PredicateTree,
  };

  #[test]
  fn predicate_tree_map() {
    let intput_tree =
      PredicateTree::<Exact>::Not(Box::new(PredicateTree::And(
        Box::new(PredicateTree::Or(
          Box::new(PredicateTree::Id(Predicate {
            code: Code::Inline(b"code-1".to_vec()),
            params: vec![Param::Inline(b"param-1".to_vec())],
          })),
          Box::new(PredicateTree::Id(Predicate {
            code: Code::AccountRef(
              "/address/one".parse().unwrap(),
              "entrypoint-1".into(),
            ),
            params: vec![
              Param::AccountRef("/address/two".parse().unwrap()),
              Param::AccountRef("/address/three".parse().unwrap()),
              Param::ProposalRef("/address/five".parse().unwrap()),
            ],
          })),
        )),
        Box::new(PredicateTree::And(
          Box::new(PredicateTree::Not(Box::new(PredicateTree::Id(
            Predicate {
              code: Code::Inline(b"code-2".to_vec()),
              params: vec![Param::CalldataRef("calldata-1".into())],
            },
          )))),
          Box::new(PredicateTree::Id(Predicate {
            code: Code::AccountRef(
              "/address/four".parse().unwrap(),
              "entrypoint-2".into(),
            ),
            params: vec![],
          })),
        )),
      )));

    let expected_output_tree =
      PredicateTree::<Expanded>::Not(Box::new(PredicateTree::And(
        Box::new(PredicateTree::Or(
          Box::new(PredicateTree::Id(Predicate {
            code: ExpandedCode {
              code: b"CODE-1".to_vec(),
              entrypoint: "DEFAULT_ENTRYPOINT".into(),
            },
            params: vec![ExpandedParam::Inline(b"param-1".to_vec())],
          })),
          Box::new(PredicateTree::Id(Predicate {
            code: ExpandedCode {
              code: b"/ADDRESS/ONE".to_vec(),
              entrypoint: "ENTRYPOINT-1".into(),
            },
            params: vec![
              ExpandedParam::AccountRef(
                "/address/two".parse().unwrap(),
                b"/ADDRESS/TWO".to_vec(),
              ),
              ExpandedParam::AccountRef(
                "/address/three".parse().unwrap(),
                b"/ADDRESS/THREE".to_vec(),
              ),
              ExpandedParam::ProposalRef(
                "/address/five".parse().unwrap(),
                crate::ExpandedAccountChange::ReplaceState {
                  current: vec![],
                  proposed: b"/ADDRESS/FIVE".to_vec(),
                },
              ),
            ],
          })),
        )),
        Box::new(PredicateTree::And(
          Box::new(PredicateTree::Not(Box::new(PredicateTree::Id(
            Predicate {
              code: ExpandedCode {
                code: b"CODE-2".to_vec(),
                entrypoint: "DEFAULT_ENTRYPOINT".into(),
              },
              params: vec![ExpandedParam::CalldataRef(
                "calldata-1".into(),
                b"CALLDATA-1".to_vec(),
              )],
            },
          )))),
          Box::new(PredicateTree::Id(Predicate {
            code: ExpandedCode {
              code: b"/ADDRESS/FOUR".to_vec(),
              entrypoint: "ENTRYPOINT-2".into(),
            },
            params: vec![],
          })),
        )),
      )));

    let actual_output_tree = intput_tree.map(|pred| Predicate::<Expanded> {
      code: match &pred.code {
        Code::Inline(b) => ExpandedCode {
          code: String::from_utf8(b.clone())
            .unwrap()
            .to_uppercase()
            .as_bytes()
            .to_vec(),
          entrypoint: "DEFAULT_ENTRYPOINT".into(),
        },
        Code::AccountRef(a, e) => ExpandedCode {
          code: a.to_string().to_uppercase().as_bytes().to_vec(),
          entrypoint: e.to_uppercase(),
        },
      },
      params: pred
        .params
        .iter()
        .map(|p| match p {
          Param::Inline(v) => ExpandedParam::Inline(v.clone()),
          Param::AccountRef(a) => ExpandedParam::AccountRef(
            a.clone(),
            a.to_string().to_uppercase().as_bytes().to_vec(),
          ),
          Param::ProposalRef(p) => ExpandedParam::ProposalRef(
            p.clone(),
            crate::ExpandedAccountChange::ReplaceState {
              current: vec![],
              proposed: p.to_string().to_uppercase().as_bytes().to_vec(),
            },
          ),
          Param::CalldataRef(c) => ExpandedParam::CalldataRef(
            c.clone(),
            c.to_uppercase().as_bytes().to_vec(),
          ),
        })
        .collect(),
    });

    assert_eq!(expected_output_tree, actual_output_tree);
  }

  #[test]
  fn predicate_tree_try_map() {
    
  }
}
