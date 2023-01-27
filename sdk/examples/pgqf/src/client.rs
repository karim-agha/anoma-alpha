use {
  crate::{
    io::{
      install_bytecode,
      send_and_confirm_intents,
      send_and_confirm_transaction,
    },
    settings::SystemSettings,
  },
  anoma_network as network,
  anoma_predicates_sdk::{Address, Predicate},
  anoma_primitives::{
    Account,
    AccountChange,
    Block,
    Code,
    Intent,
    Param,
    PredicateTree,
    Transaction,
  },
  anoma_sdk::{BlockchainWatcher, InMemoryStateStore},
  clap::Parser,
  futures::{future::join_all, StreamExt},
  model::{Campaign, Project},
  multihash::Multihash,
  network::{
    topic::{self, Topic},
    Config,
    Keypair,
    Network,
  },
  rand::{seq::SliceRandom, Rng},
  rmp_serde::{from_slice, to_vec},
  std::{
    collections::HashMap,
    future::ready,
    num::NonZeroUsize,
    time::Duration,
  },
  tracing::info,
};

mod io;
mod model;
mod settings;

// (blocks, intents) topic handles
fn start_network(
  settings: &SystemSettings,
) -> anyhow::Result<(Topic, Topic, Topic)> {
  let mut network = Network::new(
    Config {
      listen_addrs: settings.p2p_addrs(),
      ..Default::default()
    },
    Keypair::generate_ed25519(),
  )?;

  let blocks_topic = network.join(topic::Config {
    name: format!("/{}/blocks", settings.network_id()),
    bootstrap: settings.peers(),
  })?;

  let intents_topic = network.join(topic::Config {
    name: format!("/{}/intents", settings.network_id()),
    bootstrap: settings.peers(),
  })?;

  let transactions_topic = network.join(topic::Config {
    name: format!("/{}/transactions", settings.network_id()),
    bootstrap: settings.peers(),
  })?;

  tokio::spawn(network.runloop());
  Ok((blocks_topic, transactions_topic, intents_topic))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  tracing_subscriber::fmt::init();

  let opts = SystemSettings::parse();
  info!("Client options: {opts:?}");

  let (blocks, transactions, intents) = start_network(&opts)?;
  let mut blocks = blocks
    .filter_map(|bytes| ready(from_slice::<Block>(&bytes).ok()))
    .boxed();

  // Wait for some block on p2p to base our state off it
  let recent_block = blocks.next().await.expect("blocks stream closed");
  info!("First observed block: {recent_block:?}");

  // the funding campaign will be open to public
  // donations in 10 blocks and will last for 100 blocks.
  let campaign_start = recent_block.height + 10;
  let campaign_end = campaign_start + 100;
  info!("Campaign lifetime [{campaign_start}, {campaign_end}]");

  #[allow(clippy::box_default)]
  let mut watcher = BlockchainWatcher::new(
    NonZeroUsize::new(64).unwrap(),
    Box::leak(Box::new(InMemoryStateStore::default())),
    Box::leak(Box::new(InMemoryStateStore::default())),
    std::iter::once(recent_block),
    blocks,
  )?;

  info!("Installing Standard Predicate Library...");
  send_and_confirm_transaction(
    install_bytecode(
      "/stdpred".parse()?,
      include_bytes!(
        "../../../../target/wasm32-unknown-unknown/release/stdpred.wasm"
      ),
    )?,
    &transactions,
    &mut watcher,
  )
  .await?;
  info!("Standard Predicate Library installed");

  info!("Installing PGQF predicates...");
  send_and_confirm_transaction(
    install_bytecode(
      "/pgqf".parse()?,
      include_bytes!(
        "../../../../target/wasm32-unknown-unknown/release/pgqf_predicates.\
         wasm"
      ),
    )?,
    &transactions,
    &mut watcher,
  )
  .await?;
  info!("PGQF predicates installed",);

  // create treasury wallet
  send_and_confirm_transaction(
    create_treasury("/token/usdc/spring-2023".parse()?)?,
    &transactions,
    &mut watcher,
  )
  .await?;
  info!("Treasury wallet created.");

  // first create a campaign:
  send_and_confirm_transaction(
    create_campaign_transaction(
      campaign_start,
      campaign_end,
      "/token/usdc/spring-2023".parse()?,
    )?,
    &transactions,
    &mut watcher,
  )
  .await?;
  info!("Spring 2023 campaign created");

  // then add one project
  let tx = send_and_confirm_intents(
    create_project_intents(
      "project1",
      *watcher.most_recent_block().await.hash(),
    )?,
    &intents,
    &mut watcher,
  )
  .await?;
  info!("Added project1 in tx: {tx:?}");

  // then add another three projects
  let tx = send_and_confirm_intents(
    [
      create_project_intents(
        "project2",
        *watcher.most_recent_block().await.hash(),
      )?,
      create_project_intents(
        "project3",
        *watcher.most_recent_block().await.hash(),
      )?,
      create_project_intents(
        "project4",
        *watcher.most_recent_block().await.hash(),
      )?,
    ]
    .into_iter()
    .flatten(),
    &intents,
    &mut watcher,
  )
  .await?;
  info!("Added projects 2-4 in tx: {tx:?}");

  // the campaign has not started yet, fund the matching pool
  let mut matching_pool_amount = 0;

  // first donation to the matching pool
  let tx = send_and_confirm_intents(
    create_matching_pool_donation_intents(
      1200,
      *watcher.most_recent_block().await.hash(),
    ),
    &intents,
    &mut watcher,
  )
  .await?;
  info!("Donated 1200 to matching pool in tx: {tx:?}");
  matching_pool_amount += 1200;

  // second batch of donations to the matching pool
  let tx = send_and_confirm_intents(
    [
      create_matching_pool_donation_intents(
        1800,
        *watcher.most_recent_block().await.hash(),
      ),
      create_matching_pool_donation_intents(
        1200,
        *watcher.most_recent_block().await.hash(),
      ),
    ]
    .into_iter()
    .flatten(),
    &intents,
    &mut watcher,
  )
  .await?;
  info!("Donated 3000 to matching pool in tx: {tx:?}");
  matching_pool_amount += 3000;

  // after this future complets, we know that transactions running
  // from this point on will be targetting an ongoing campaign.
  watcher.await_block_height(campaign_start).await?;

  // gossip N random donation intents to random projects
  // with a random amount and store hash of the intent.
  //
  // later we will want to make sure that all intents were
  // included in transactions in blocks.
  const N: usize = 30;
  let mut donations_intents = vec![];
  let projects = &["project1", "project2", "project3", "project4"];
  let mut donation_amounts = HashMap::new();

  for _ in 0..N {
    let project = *projects.choose(&mut rand::thread_rng()).unwrap();
    let amount = rand::thread_rng().gen_range(100..10000);
    let donation_intent = create_project_donation_intent(
      project,
      amount,
      *watcher.most_recent_block().await.hash(),
    );
    if intents.gossip(to_vec(&donation_intent).unwrap()).is_ok() {
      info!("Donated {amount} to {project}");
      donations_intents.push(*donation_intent.hash());

      donation_amounts
        .entry(project)
        .and_modify(|e| *e += amount)
        .or_insert(amount);
    }

    // random delay
    tokio::time::sleep(Duration::from_millis(
      rand::thread_rng().gen_range(100..1500),
    ))
    .await;
  }

  // wait for all donations intents to be included in blocks
  let txs = join_all(
    donations_intents
      .into_iter()
      .map(|ih| watcher.await_intent(ih)),
  )
  .await;

  info!("All project donations confirmed in transactions: {txs:?}");

  // await campaign end block height
  watcher.await_block_height(campaign_end).await?;

  // once the funding campaign is over, redistribute all
  // funds to projects.
  let tx = send_and_confirm_intents(
    std::iter::once(create_funding_redistribution_intent(
      matching_pool_amount,
      donation_amounts,
      *watcher.most_recent_block().await.hash(),
    )),
    &intents,
    &mut watcher,
  )
  .await?;
  info!("Donated 1200 to matching pool in tx: {tx:?}");

  watcher.stop().await;

  Ok(())
}

/// Creates an empty wallet that will be used as the treasury for a campaign
/// where funds are stored and then redistributed at the end of the campaign.
/// Spending in this wallet is governed by the "spending" predicate from the
/// PGQF bytecode.
///
/// This should be called for a wallet address that is governed by the /token/x/
/// preds.
fn create_treasury(wallet: Address) -> anyhow::Result<Transaction> {
  Ok(Transaction::new(
    vec![], // no intents
    [(
      wallet.clone(),
      AccountChange::CreateAccount(Account {
        state: to_vec(&0u64)?, // start with zero balance
        predicates: PredicateTree::Id(Predicate {
          code: Code::AccountRef("/pgqf".parse()?, "treasury".into()),
          params: vec![Param::AccountRef(wallet)],
        }),
      }),
    )]
    .into(),
  ))
}

/// Creates a new funding campaign with no projects in it yet.
/// Assigns it a treasury wallet address and makes it governed by the
/// "campaign" predicate from PGQF bytecode.
fn create_campaign_transaction(
  start_height: u64,
  end_height: u64,
  treasury: Address,
) -> anyhow::Result<Transaction> {
  Ok(Transaction::new(
    vec![], // no intents
    [(
      Address::new("/pgqf/spring-2023")?,
      AccountChange::CreateAccount(Account {
        state: to_vec(&Campaign {
          starts_at: start_height,
          ends_at: end_height,
          projects: Default::default(), // start with 0 projects
        })?,
        predicates: PredicateTree::Id(Predicate {
          code: Code::AccountRef("/pgqf".parse()?, "campaign".into()),
          params: vec![
            Param::AccountRef("/pgqf/spring-2023".parse()?),
            Param::AccountRef(treasury),
          ],
        }),
      }),
    )]
    .into(),
  ))
}

/// Creates an intent for solvers that expects a transaction adding a project to
/// the campaign projects list. This should be called before a campaign begins.
/// After a campaign begin, the projects list is frozen.
///
/// The solver will have to figure out all the details of adding this project to
/// the list of projects in the campaign account state and all other plumbing.
fn create_project_intents(
  name: &str,
  blockhash: Multihash,
) -> anyhow::Result<impl Iterator<Item = Intent>> {
  Ok(std::iter::once(Intent::new(
    blockhash,
    PredicateTree::Id(Predicate {
      code: Code::AccountRef("/stdpred".parse()?, "bytes_equal".into()),
      params: vec![
        Param::ProposalRef(format!("/pgqf/spring-2023/{name}").parse()?),
        Param::Inline(to_vec(&Project::default())?),
      ],
    }),
  )))
}

fn create_matching_pool_donation_intents(
  _amount: u64,
  blockhash: Multihash,
) -> impl Iterator<Item = Intent> {
  std::iter::once(Intent::new(blockhash, todo!()))
}

fn create_project_donation_intent(
  _name: &str,
  _amount: u64,
  blockhash: Multihash,
) -> Intent {
  Intent::new(blockhash, todo!())
}

fn create_funding_redistribution_intent(
  matching_pool_amount: u64,
  donation_amounts: HashMap<&str, u64>,
  blockhash: Multihash,
) -> Intent {
  todo!();
}

fn verify_redistribution(tx: Transaction) -> anyhow::Result<()> {
  todo!();
}
