#![no_std]

//! This example illustrates how to build a Public Goods Quadratic Funding
//! (PGQF) on-chain predicates using Anoma Predicates SDK.
//!
//! There is a corresponding PGQF solver in the Solver SDK that illustrates
//! how to build the solver part of this functionality.
//!
//! PGQF requires the following predicates to be installed on chain prior to
//! deployment:
//!   - Standard Predicates Library
//!   - Token (from the Token example in the Predicates SDK)
//!
//! This predicate controlls the following accounts:
//!
//! - /pgqf state:
//!   - predicate bytecode
//!   predicates:
//!   - stdpred::immutable_state
//!   AND
//!   - stdpred::immutable_predicates
//!
//! - /pgqf/<camaign-id> predicates:
//!   - /pgqd (account-ref(self))
//!   state:
//!   - name
//!   - description
//!   params:
//!   - begins_at
//!   - ends_at
//!   - wallet
//!   - projects_list
//!   - committee_pubkey
//!   
//! - /pgqf/<camaign-id>/project/<project-id> state:
//!   - name
//!   - donors_list
//!   - wallet
//!
//! - /pgqf/<camaign-id>/project/<project-id>/<donor-id> state:
//!   - amount

use {
  alloc::vec::Vec,
  anoma_predicates_sdk::{
    initialize_library,
    predicate,
    ExpandedParam,
    PredicateContext,
  },
};

initialize_library!();

#[predicate]
fn predicate(
  _params: &Vec<ExpandedParam>,
  _context: &PredicateContext,
) -> bool {
  true
}
