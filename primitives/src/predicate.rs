use {
  crate::{Address, Calldata, Exact, ExpandedAccountChange, Repr},
  alloc::{boxed::Box, collections::BTreeMap, string::String, vec::Vec},
  core::fmt::Debug,
  multihash::Multihash,
  serde::{Deserialize, Serialize},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Param {
  Inline(Vec<u8>),
  AccountRef(Address),
  ProposalRef(Address),
  CalldataRef(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
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

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Code {
  /// If the predicate code is inlined then it must export a predicate
  /// named "invoke" and it will be the entrypoint.
  Inline(Vec<u8>),
  AccountRef(Address, String), // (address, entrypoint)
}

impl core::fmt::Debug for Code {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    match self {
      Self::Inline(_) => {
        f.debug_tuple("Inline").field(&"[wasm-bytecode]").finish()
      }
      Self::AccountRef(arg0, arg1) => {
        f.debug_tuple("AccountRef").field(arg0).field(arg1).finish()
      }
    }
  }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ExpandedCode {
  pub code: Vec<u8>,
  pub entrypoint: String,
}

impl core::fmt::Debug for ExpandedCode {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    f.debug_struct("ExpandedCode")
      .field("code", &"[wasm-bytecode]")
      .field("entrypoint", &self.entrypoint)
      .finish()
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Predicate<R: Repr = Exact> {
  pub code: R::Code,
  pub params: Vec<R::Param>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
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

  /// Applies a function to all predicates in the tree and returns a new
  /// tree with the same structure and modified predicates.
  pub fn for_each<F>(&self, op: &mut F)
  where
    F: FnMut(&Predicate<R>),
  {
    match self {
      PredicateTree::Id(p) => op(p),
      PredicateTree::Not(pt) => pt.for_each(op),
      PredicateTree::And(l, r) => {
        l.for_each(op);
        r.for_each(op);
      }
      PredicateTree::Or(l, r) => {
        l.for_each(op);
        r.for_each(op);
      }
    };
  }
}

/// This context object is passed to predicates during evaluation stage.
/// It contains all input key-value pairs attached to predicates and
/// a list of all mutated accounts by a transaction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PredicateContext {
  /// Intent input key-value pair groupped by the intent hash.
  /// Could include things like signature or other arbitrary
  /// input parameters to predicates.
  pub calldata: BTreeMap<Multihash, Calldata>,

  /// Changes to accounts that are modified by a transaction.
  pub proposals: BTreeMap<Address, ExpandedAccountChange>,
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
}
