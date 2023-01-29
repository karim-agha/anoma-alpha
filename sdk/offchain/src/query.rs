use {
  anoma_primitives::{Address, Basic, Code, Param, PredicateTree, Repr},
  serde::{Deserialize, Serialize},
  std::collections::HashMap,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ParamPattern {
  Any,
  Exact(Param),
  Inline(String),
  AccountRef(String),
  ProposalRef(String),
  CalldataRef(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MatchValue {
  Address(Address),
  Bytes(Vec<u8>),
  String(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Query;
impl Repr for Query {
  type AccountChange = ();
  type Code = Code;
  type Param = ParamPattern;
}

/// Used to match expression tree patterns in intents.
/// The primary use case is for solvers to identify intents they are interested
/// in solving from the incoming intent gossip stream.
pub trait ExpressionPattern {
  fn matches(
    &self,
    template: PredicateTree<Query>,
  ) -> Option<HashMap<String, MatchValue>>;
}

impl ExpressionPattern for PredicateTree<Basic> {
  fn matches(
    &self,
    template: PredicateTree<Query>,
  ) -> Option<HashMap<String, MatchValue>> {
    type BasicTree = PredicateTree<Basic>;
    type QueryTree = PredicateTree<Query>;

    fn find_matches(
      basic: &BasicTree,
      template: &QueryTree,
      matches: &mut HashMap<String, MatchValue>,
    ) -> bool {
      match (basic, template) {
        (BasicTree::Id(b_pred), QueryTree::Id(p_pred)) => {
          if b_pred.code == p_pred.code
            && b_pred.params.len() == p_pred.params.len()
          {
            for (left, right) in b_pred.params.iter().zip(p_pred.params.iter())
            {
              match (left, right) {
                (_, ParamPattern::Any) => {} // match but no captures
                (left_p, ParamPattern::Exact(right_p)) => {
                  return left_p == right_p;
                }
                (Param::Inline(val), ParamPattern::Inline(name)) => {
                  matches.insert(name.clone(), MatchValue::Bytes(val.clone()));
                }
                (Param::AccountRef(val), ParamPattern::AccountRef(name)) => {
                  matches
                    .insert(name.clone(), MatchValue::Address(val.clone()));
                }
                (Param::ProposalRef(val), ParamPattern::ProposalRef(name)) => {
                  matches
                    .insert(name.clone(), MatchValue::Address(val.clone()));
                }
                (Param::CalldataRef(val), ParamPattern::CalldataRef(name)) => {
                  matches.insert(name.clone(), MatchValue::String(val.clone()));
                }
                _ => return false,
              }
            }

            true
          } else {
            false
          }
        }
        (BasicTree::Not(left), QueryTree::Not(right)) => {
          find_matches(left, right, matches)
        }
        (
          BasicTree::And(left_left, left_right),
          QueryTree::And(right_left, right_right),
        ) => {
          find_matches(left_left, right_left, matches)
            && find_matches(left_right, right_right, matches)
        }
        (
          BasicTree::Or(left_left, left_right),
          QueryTree::Or(right_left, right_right),
        ) => {
          find_matches(left_left, right_left, matches)
            && find_matches(left_right, right_right, matches)
        }
        _ => false, // different tree shapes, not a match
      }
    }

    let mut matches = HashMap::new();
    match find_matches(self, &template, &mut matches) {
      true => Some(matches),
      false => None,
    }
  }
}

#[cfg(test)]
mod tests {
  use {
    crate::{query::MatchValue, ExpressionPattern, ParamPattern, Query},
    anoma_primitives::{Basic, Code, Param, Predicate, PredicateTree},
  };

  #[test]
  fn matching_single_predicate() {
    let pred = PredicateTree::<Basic>::Id(Predicate {
      code: Code::AccountRef("/stdpred".parse().unwrap(), "pred1".into()),
      params: vec![
        Param::AccountRef("/address1".parse().unwrap()),
        Param::Inline(b"someval".to_vec()),
      ],
    });

    let pattern = PredicateTree::<Query>::Id(Predicate {
      code: Code::AccountRef("/stdpred".parse().unwrap(), "pred1".into()),
      params: vec![
        ParamPattern::AccountRef("param1".into()),
        ParamPattern::Inline("param2".into()),
      ],
    });

    let result = pred.matches(pattern);
    assert!(result.is_some());
    let result = result.unwrap();

    assert_eq!(
      result.get("param1").unwrap(),
      &MatchValue::Address("/address1".parse().unwrap())
    );

    assert_eq!(
      result.get("param2").unwrap(),
      &MatchValue::Bytes(b"someval".to_vec())
    );
  }

  #[test]
  fn matching_single_predicate_any() {
    let pred = PredicateTree::<Basic>::Id(Predicate {
      code: Code::AccountRef("/stdpred".parse().unwrap(), "pred1".into()),
      params: vec![
        Param::AccountRef("/address1".parse().unwrap()),
        Param::Inline(b"someval".to_vec()),
      ],
    });

    let pattern = PredicateTree::<Query>::Id(Predicate {
      code: Code::AccountRef("/stdpred".parse().unwrap(), "pred1".into()),
      params: vec![ParamPattern::Any, ParamPattern::Inline("param2".into())],
    });

    let result = pred.matches(pattern);
    assert!(result.is_some());
    let result = result.unwrap();

    assert_eq!(
      result.get("param2").unwrap(),
      &MatchValue::Bytes(b"someval".to_vec())
    );
  }

  #[test]
  fn matching_single_predicate_negative() {
    let pred = PredicateTree::<Basic>::Id(Predicate {
      code: Code::AccountRef("/stdpred".parse().unwrap(), "pred1".into()),
      params: vec![
        Param::AccountRef("/address1".parse().unwrap()),
        Param::Inline(b"someval".to_vec()),
      ],
    });

    let pattern = PredicateTree::<Query>::Id(Predicate {
      code: Code::AccountRef("/stdpred".parse().unwrap(), "pred2".into()),
      params: vec![ParamPattern::Any, ParamPattern::Inline("param2".into())],
    });

    let result = pred.matches(pattern);
    assert!(result.is_none());
  }

  #[test]
  fn matching_predicate_tree() {
    let pred = PredicateTree::And(
      Box::new(PredicateTree::<Basic>::Id(Predicate {
        code: Code::AccountRef("/stdpred".parse().unwrap(), "pred1".into()),
        params: vec![
          Param::AccountRef("/address1".parse().unwrap()),
          Param::Inline(b"someval".to_vec()),
        ],
      })),
      Box::new(PredicateTree::<Basic>::Id(Predicate {
        code: Code::AccountRef("/stdpred".parse().unwrap(), "pred2".into()),
        params: vec![
          Param::AccountRef("/address2".parse().unwrap()),
          Param::Inline(b"someval2".to_vec()),
        ],
      })),
    );

    let pattern = PredicateTree::And(
      Box::new(PredicateTree::<Query>::Id(Predicate {
        code: Code::AccountRef("/stdpred".parse().unwrap(), "pred1".into()),
        params: vec![
          ParamPattern::AccountRef("param1".into()),
          ParamPattern::Inline("param2".into()),
        ],
      })),
      Box::new(PredicateTree::<Query>::Id(Predicate {
        code: Code::AccountRef("/stdpred".parse().unwrap(), "pred2".into()),
        params: vec![
          ParamPattern::AccountRef("param3".into()),
          ParamPattern::Inline("param4".into()),
        ],
      })),
    );

    let result = pred.matches(pattern);
    assert!(result.is_some());
    let result = result.unwrap();

    assert_eq!(
      result.get("param1").unwrap(),
      &MatchValue::Address("/address1".parse().unwrap())
    );
    assert_eq!(
      result.get("param3").unwrap(),
      &MatchValue::Address("/address2".parse().unwrap())
    );
    assert_eq!(
      result.get("param2").unwrap(),
      &MatchValue::Bytes(b"someval".to_vec())
    );
    assert_eq!(
      result.get("param4").unwrap(),
      &MatchValue::Bytes(b"someval2".to_vec())
    );
  }

  #[test]
  fn matching_predicate_tree_any() {
    let pred = PredicateTree::And(
      Box::new(PredicateTree::<Basic>::Id(Predicate {
        code: Code::AccountRef("/stdpred".parse().unwrap(), "pred1".into()),
        params: vec![
          Param::AccountRef("/address1".parse().unwrap()),
          Param::Inline(b"someval".to_vec()),
        ],
      })),
      Box::new(PredicateTree::<Basic>::Id(Predicate {
        code: Code::AccountRef("/stdpred".parse().unwrap(), "pred2".into()),
        params: vec![
          Param::AccountRef("/address2".parse().unwrap()),
          Param::Inline(b"someval2".to_vec()),
        ],
      })),
    );

    let pattern = PredicateTree::And(
      Box::new(PredicateTree::<Query>::Id(Predicate {
        code: Code::AccountRef("/stdpred".parse().unwrap(), "pred1".into()),
        params: vec![
          ParamPattern::AccountRef("param1".into()),
          ParamPattern::Any,
        ],
      })),
      Box::new(PredicateTree::<Query>::Id(Predicate {
        code: Code::AccountRef("/stdpred".parse().unwrap(), "pred2".into()),
        params: vec![
          ParamPattern::AccountRef("param3".into()),
          ParamPattern::Inline("param4".into()),
        ],
      })),
    );

    let result = pred.matches(pattern);
    assert!(result.is_some());
    let result = result.unwrap();

    assert_eq!(
      result.get("param1").unwrap(),
      &MatchValue::Address("/address1".parse().unwrap())
    );
    assert_eq!(
      result.get("param3").unwrap(),
      &MatchValue::Address("/address2".parse().unwrap())
    );
    assert_eq!(
      result.get("param4").unwrap(),
      &MatchValue::Bytes(b"someval2".to_vec())
    );
  }

  #[test]
  fn matching_predicate_tree_negative() {
    let pred = PredicateTree::And(
      Box::new(PredicateTree::<Basic>::Id(Predicate {
        code: Code::AccountRef("/stdpred".parse().unwrap(), "pred1".into()),
        params: vec![
          Param::AccountRef("/address1".parse().unwrap()),
          Param::Inline(b"someval".to_vec()),
        ],
      })),
      Box::new(PredicateTree::<Basic>::Id(Predicate {
        code: Code::AccountRef("/stdpred".parse().unwrap(), "pred2".into()),
        params: vec![
          Param::AccountRef("/address2".parse().unwrap()),
          Param::Inline(b"someval2".to_vec()),
        ],
      })),
    );

    let pattern = PredicateTree::And(
      Box::new(PredicateTree::<Query>::Id(Predicate {
        code: Code::AccountRef("/stdpred".parse().unwrap(), "pred1".into()),
        params: vec![ParamPattern::AccountRef("param1".into())],
      })),
      Box::new(PredicateTree::<Query>::Id(Predicate {
        code: Code::AccountRef("/stdpred".parse().unwrap(), "pred2".into()),
        params: vec![
          ParamPattern::AccountRef("param3".into()),
          ParamPattern::Inline("param4".into()),
        ],
      })),
    );

    let result = pred.matches(pattern);
    assert!(result.is_none());
  }
}
