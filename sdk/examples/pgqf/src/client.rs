use {
  crate::settings::SystemSettings,
  anoma_client_sdk::{BlockchainWatcher, InMemoryStateStore},
  anoma_network as network,
  anoma_primitives::{Block, Intent, Transaction},
  clap::Parser,
  futures::{future::join_all, Stream, StreamExt},
  multihash::Multihash,
  network::{
    topic::{self, Topic},
    Config,
    Keypair,
    Network,
  },
  rand::{seq::SliceRandom, Rng},
  rmp_serde::{from_slice, to_vec},
  std::{collections::HashMap, num::NonZeroUsize, time::Duration},
  tracing::info,
};

mod settings;

type BlocksStream = dyn Stream<Item = Block>;

// (blocks, intents) topic handles
fn start_network(settings: &SystemSettings) -> anyhow::Result<(Topic, Topic)> {
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
    bootstrap: Default::default(),
  })?;

  tokio::spawn(network.runloop());
  Ok((blocks_topic, intents_topic))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  tracing_subscriber::fmt::init();

  let opts = SystemSettings::parse();
  info!("Client options: {opts:?}");

  let (blocks, intents) = start_network(&opts)?;
  let mut blocks =
    blocks.filter_map(|bytes| async move { from_slice::<Block>(&bytes).ok() });

  let mut code_cache = InMemoryStateStore::default();
  let mut chain_state = InMemoryStateStore::default();

  // get a recent blockhash for constructing valid intents
  let recent_block = await_next_block(&mut blocks).await?;

  // the funding campaign will be open to public
  // donations in 10 blocks and will last for 100 blocks.
  let campaign_start = recent_block.height + 10;
  let campaign_end = campaign_start + 100;

  let mut watcher = BlockchainWatcher::new(
    NonZeroUsize::new(64).unwrap(),
    &mut chain_state,
    &mut code_cache,
    std::iter::once(recent_block),
    blocks,
  )?;

  // first create a campaign:
  let tx = send_and_confirm_intents(
    std::iter::once(create_campaign_intent(
      *watcher.most_recent_block().hash(),
      campaign_start,
      campaign_end,
    )),
    &intents,
    &mut watcher,
  )
  .await?;
  println!("Campaign created in tx: {tx:?}");

  // then add one project
  let tx = send_and_confirm_intents(
    create_project_intents("project1", *watcher.most_recent_block().hash()),
    &intents,
    &mut watcher,
  )
  .await?;
  println!("Added project1 in tx: {tx:?}");

  // then add another three projects
  let tx = send_and_confirm_intents(
    [
      create_project_intents("project2", *watcher.most_recent_block().hash()),
      create_project_intents("project3", *watcher.most_recent_block().hash()),
      create_project_intents("project4", *watcher.most_recent_block().hash()),
    ]
    .into_iter()
    .flatten(),
    &intents,
    &mut watcher,
  )
  .await?;
  println!("Added projects 2-4 in tx: {tx:?}");

  // the campaign has not started yet, fund the matching pool
  let mut matching_pool_amount = 0;

  // first donation to the matching pool
  let tx = send_and_confirm_intents(
    create_matching_pool_donation_intents(
      1200,
      *watcher.most_recent_block().hash(),
    ),
    &intents,
    &mut watcher,
  )
  .await?;
  println!("Donated 1200 to matching pool in tx: {tx:?}");
  matching_pool_amount += 1200;

  // second batch of donations to the matching pool
  let tx = send_and_confirm_intents(
    [
      create_matching_pool_donation_intents(
        1800,
        *watcher.most_recent_block().hash(),
      ),
      create_matching_pool_donation_intents(
        1200,
        *watcher.most_recent_block().hash(),
      ),
    ]
    .into_iter()
    .flatten(),
    &intents,
    &mut watcher,
  )
  .await?;
  println!("Donated 3000 to matching pool in tx: {tx:?}");
  matching_pool_amount += 3000;

  // after this future complets, we know that transactions running
  // from this point on will be targetting an ongoing campaign.
  watcher.await_block_height(campaign_start).await;

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
      *watcher.most_recent_block().hash(),
    );
    if intents.gossip(to_vec(&donation_intent).unwrap()).is_ok() {
      println!("Donated {amount} to {project}");
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

  println!("All project donations confirmed in transactions: {txs:?}");

  // await campaign end block height
  watcher.await_block_height(campaign_end).await;

  // once the funding campaign is over, redistribute all
  // funds to projects.
  let tx = send_and_confirm_intents(
    std::iter::once(create_funding_redistribution_intent(
      matching_pool_amount,
      donation_amounts,
      *watcher.most_recent_block().hash(),
    )),
    &intents,
    &mut watcher,
  )
  .await?;
  println!("Donated 1200 to matching pool in tx: {tx:?}");

  Ok(())
}

/// Gossips an intent through p2p to solvers and awaits a produced block
/// from validators that contains a transaction with this intent.
async fn send_and_confirm_intents<'s>(
  _intent: impl Iterator<Item = Intent>,
  _intents_topic: &Topic,
  _watcher: &mut BlockchainWatcher<'s>,
) -> anyhow::Result<&'s Transaction> {
  todo!();
}

async fn confirm_intents(
  _intents_hashes: impl Iterator<Item = Multihash>,
  _blocks_topic: &mut BlocksStream,
) -> anyhow::Result<impl Iterator<Item = Transaction>> {
  Ok(std::iter::empty())
}

async fn await_next_block(_blocks: &mut BlocksStream) -> anyhow::Result<Block> {
  todo!();
}

fn create_campaign_intent(
  blockhash: Multihash,
  _start_height: u64,
  _end_height: u64,
) -> Intent {
  Intent::new(blockhash, todo!())
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
