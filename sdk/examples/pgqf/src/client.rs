use {
  crate::settings::SystemSettings,
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

  info!("Installing PGQF predicates...");
  let block = send_and_confirm_transaction(
    install_pgqf_bytecode()?,
    &transactions,
    &mut watcher,
  )
  .await?;
  info!(
    "PGQF installed in block {}",
    bs58::encode(&block.hash().to_bytes()).into_string()
  );

  // first create a campaign:
  let block = send_and_confirm_transaction(
    create_campaign_transaction(campaign_start, campaign_end)?,
    &transactions,
    &mut watcher,
  )
  .await?;

  info!(
    "Campaign created in block: {}",
    bs58::encode(&block.hash().to_bytes()).into_string()
  );

  // then add one project
  let tx = send_and_confirm_intents(
    create_project_intents(
      "project1",
      *watcher.most_recent_block().await.hash(),
    ),
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
      ),
      create_project_intents(
        "project3",
        *watcher.most_recent_block().await.hash(),
      ),
      create_project_intents(
        "project4",
        *watcher.most_recent_block().await.hash(),
      ),
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

/// Gossips an intent through p2p to solvers and awaits a produced block
/// from validators that contains a transaction with this intent.
async fn send_and_confirm_intents<'s>(
  intents: impl Iterator<Item = Intent>,
  intents_topic: &Topic,
  watcher: &mut BlockchainWatcher,
) -> anyhow::Result<Vec<Transaction>> {
  let (hashes, intents): (Vec<_>, Vec<_>) =
    intents.map(|i| (*i.hash(), i)).unzip();
  let hashes = hashes.into_iter().map(|h| watcher.await_intent(h));

  for intent in intents {
    loop {
      info!("sending intent {intent:?}..");
      match intents_topic.gossip(to_vec(&intent)?) {
        Ok(_) => {
          info!("done");
          break;
        }
        Err(topic::Error::NoConnectedPeers) => {
          // wait for this topic to establish connections with
          // other peers and retry.
          info!("awaiting peers...");
          tokio::time::sleep(Duration::from_secs(1)).await;
        }
        Err(e) => return Err(e.into()),
      }
    }
  }

  Ok(
    join_all(hashes)
      .await
      .into_iter()
      .filter_map(|tr| tr.ok())
      .collect(),
  )
}

/// Gossips a transaction through p2p to validators and awaits a produced block
/// containing the transaction.
async fn send_and_confirm_transaction(
  transaction: Transaction,
  transactions_topic: &Topic,
  watcher: &mut BlockchainWatcher,
) -> anyhow::Result<Block> {
  let hash = *transaction.hash();
  loop {
    match transactions_topic.gossip(to_vec(&transaction)?) {
      Ok(_) => {
        info!(
          "Transaction {} sent.",
          bs58::encode(hash.to_bytes()).into_string()
        );
        break;
      }
      Err(topic::Error::NoConnectedPeers) => {
        // wait for this topic to establish connections with
        // other peers and retry.
        info!("awaiting peers...");
        tokio::time::sleep(Duration::from_secs(1)).await;
      }
      Err(e) => return Err(e.into()),
    }
  }

  Ok(watcher.await_transaction(hash).await?)
}

fn install_pgqf_bytecode() -> anyhow::Result<Transaction> {
  Ok(Transaction::new(
    vec![],
    [(
      Address::new("/pgqf")?,
      AccountChange::CreateAccount(Account {
        state: include_bytes!(
          "../../../../target/wasm32-unknown-unknown/release/pgqf_predicates.\
           wasm"
        )
        .to_vec(),
        predicates: PredicateTree::And(
          Box::new(PredicateTree::Id(Predicate {
            code: Code::AccountRef(
              "/stdpred".parse()?,
              "immutable_state".into(),
            ),
            params: vec![Param::Inline(to_vec(&Address::new("/pgqf")?)?)],
          })),
          Box::new(PredicateTree::Id(Predicate {
            code: Code::AccountRef(
              "/stdpred".parse()?,
              "immutable_predicates".into(),
            ),
            params: vec![Param::Inline(to_vec(&Address::new("/pgqf")?)?)],
          })),
        ),
      }),
    )]
    .into(),
  ))
}

fn create_campaign_transaction(
  start_height: u64,
  end_height: u64,
) -> anyhow::Result<Transaction> {
  Ok(Transaction::new(
    vec![],
    [(
      Address::new("/pgqf/spring-2023")?,
      AccountChange::CreateAccount(Account {
        state: b"serialized-value-of-campaign-account-state".to_vec(),
        predicates: PredicateTree::Id(Predicate {
          code: Code::AccountRef("/pgqf".parse()?, "predicate".into()),
          params: vec![
            Param::Inline(to_vec(&start_height)?),
            Param::Inline(to_vec(&end_height)?),
            Param::Inline(to_vec(&Address::new(
              "/token/usdx/pgqf-spring-2023.eth",
            )?)?),
          ],
        }),
      }),
    )]
    .into(),
  ))
}

fn create_project_intents(
  _name: &str,
  blockhash: Multihash,
) -> impl Iterator<Item = Intent> {
  std::iter::once(Intent::new(blockhash, todo!()))
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
