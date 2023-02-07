use {
  crate::{io::start_network, settings::SystemSettings},
  anoma_network::topic::Topic,
  anoma_predicates_sdk::Predicate,
  anoma_primitives::{Block, Code, Intent, Param, PredicateTree},
  anoma_sdk::{
    BlockchainWatcher,
    ExpressionPattern,
    InMemoryStateStore,
    ParamPattern,
    Query,
  },
  clap::Parser,
  futures::StreamExt,
  rmp_serde::{from_slice, to_vec},
  std::{future::ready, num::NonZeroUsize},
  tracing::info,
};

mod io;
mod settings;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  tracing_subscriber::fmt::init();

  let opts = SystemSettings::parse();
  info!("Solver options: {opts:?}");

  let (txs, blocks, intents) = start_network(&opts)?;

  let mut blocks = blocks
    .filter_map(|bytes| ready(from_slice::<Block>(&bytes).ok()))
    .boxed();
  let mut intents = intents
    .filter_map(|bytes| ready(from_slice::<Intent>(&bytes).ok()))
    .boxed();

  // Wait for some block on p2p to base our state off it
  let recent_block = blocks.next().await.expect("blocks stream closed");
  info!("First observed block: {recent_block:?}");

  // This will accumulate global blockchain state changes from incoming blocks
  let chain_state: &'static mut InMemoryStateStore = Box::leak(Box::default());

  #[allow(clippy::box_default)]
  BlockchainWatcher::new(
    NonZeroUsize::new(64).unwrap(),
    chain_state,
    Box::leak(Box::new(InMemoryStateStore::default())),
    std::iter::once(recent_block),
    blocks,
  )?;

  loop {
    tokio::select! {
      Some(intent) = intents.next() => {
        info!("received an intent: {intent:?}");
        try_match_and_fill_intent(intent, &txs);
      }
    }
  }
}

fn try_match_and_fill_intent(intent: Intent, _txtopic: &Topic) {
  let expression = &intent.expectations;

  if let Some(_matches) = expression.matches(create_project_intent_pattern()) {
    // todo
  } else if let Some(_matches) =
    expression.matches(create_matching_pool_donation_intents_pattern())
  {
    // todo
  } else if let Some(_matches) =
    expression.matches(create_project_donation_intent_pattern())
  {
    // todo
  }
}

fn create_project_intent_pattern() -> PredicateTree<Query> {
  PredicateTree::And(
    Box::new(PredicateTree::Id(Predicate {
      code: Code::AccountRef(
        "/stdpred".parse().expect("validated at compile time"),
        "bytes_equal".into(),
      ),
      params: vec![
        ParamPattern::ProposalRef("project_name".into()),
        ParamPattern::Inline("project_state".into()),
      ],
    })),
    Box::new(PredicateTree::Id(Predicate {
      code: Code::AccountRef(
        "/stdpred".parse().expect("validated at compile time"),
        "uint_equal".into(),
      ),
      params: vec![
        ParamPattern::ProposalRef("wallet_address".into()),
        ParamPattern::Exact(Param::Inline(
          to_vec(&0u64).expect("validated at compile time"),
        )),
      ],
    })),
  )
}

fn create_matching_pool_donation_intents_pattern() -> PredicateTree<Query> {
  PredicateTree::And(
    Box::new(PredicateTree::Id(Predicate {
      code: Code::AccountRef(
        "/stdpred".parse().expect("validated at compile time"),
        "uint_less_than_by".into(),
      ),
      params: vec![
        ParamPattern::AccountRef("donor_balance_before".into()),
        ParamPattern::ProposalRef("donor_balance_after".into()),
        ParamPattern::Inline("donor_amount".into()),
      ],
    })),
    Box::new(PredicateTree::Id(Predicate {
      code: Code::AccountRef(
        "/stdpred".parse().expect("validated at compile time"),
        "uint_greater_than_by".into(),
      ),
      params: vec![
        ParamPattern::AccountRef("treasury_balance_before".into()),
        ParamPattern::ProposalRef("treasury_balance_after".into()),
        ParamPattern::Inline("treasury_amount".into()),
      ],
    })),
  )
}

fn create_project_donation_intent_pattern() -> PredicateTree<Query> {
  PredicateTree::And(
    Box::new(PredicateTree::And(
      Box::new(PredicateTree::Id(Predicate {
        code: Code::AccountRef(
          "/stdpred".parse().expect("validated at compile time"),
          "uint_greater_than_by".into(),
        ),
        params: vec![
          ParamPattern::ProposalRef("project_wallet_after".into()),
          ParamPattern::AccountRef("project_wallet_before".into()),
          ParamPattern::Inline("donor_amount".into()),
        ],
      })),
      Box::new(PredicateTree::Id(Predicate {
        code: Code::AccountRef(
          "/stdpred".parse().expect("validated at compile time"),
          "uint_less_than_by".into(),
        ),
        params: vec![
          ParamPattern::ProposalRef("donor_wallet_after".into()),
          ParamPattern::AccountRef("donor_wallet_before".into()),
          ParamPattern::Inline("donor_amount".into()),
        ],
      })),
    )),
    Box::new(PredicateTree::Id(Predicate {
      code: Code::AccountRef(
        "/stdpred".parse().expect("validated at compile time"),
        "bytes_equal".into(),
      ),
      params: vec![
        ParamPattern::ProposalRef("donation_address".into()),
        ParamPattern::Inline("donation_state".into()),
      ],
    })),
  )
}
