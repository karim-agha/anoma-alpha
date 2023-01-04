use {
  crate::settings::SystemSettings,
  anoma_network as network,
  anoma_primitives::{Block, Intent, Transaction},
  clap::Parser,
  futures::{Stream, StreamExt},
  multihash::Multihash,
  network::{
    topic::{self, Topic},
    Config,
    Keypair,
    Network,
  },
  rand::{seq::SliceRandom, Rng},
  rmp_serde::{from_slice, to_vec},
  std::{collections::HashMap, time::Duration},
  tracing::info,
};

mod settings;

type BlocksStream = dyn Stream<Item = Result<Block, rmp_serde::decode::Error>>;

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
  let mut blocks = blocks.map(|bytes| from_slice::<Block>(&bytes));

  // get a recent blockhash for constructing valid intents
  let recent_block = await_next_block(&mut blocks).await?;
  let mut blockhash = *recent_block.hash();

  // the funding campaign will be open to public
  // donations in 10 blocks and will last for 100 blocks.
  let campaign_start = recent_block.height + 10;
  let campaign_end = campaign_start + 100;

  // first create a campaign:
  let tx = send_and_confirm_intents(
    create_campaign_intents(blockhash, campaign_start, campaign_end),
    &intents,
    &mut blocks,
  )
  .await?;
  println!("Campaign created in tx: {tx:?}");

  // then add one project
  let tx = send_and_confirm_intents(
    create_project_intents("project1", blockhash),
    &intents,
    &mut blocks,
  )
  .await?;
  println!("Added project1 in tx: {tx:?}");

  // then add another three projects
  let tx = send_and_confirm_intents(
    [
      create_project_intents("project2", blockhash),
      create_project_intents("project3", blockhash),
      create_project_intents("project4", blockhash),
    ]
    .into_iter()
    .flatten(),
    &intents,
    &mut blocks,
  )
  .await?;
  println!("Added projects 2-4 in tx: {tx:?}");

  // the campaign has not started yet, fund the matching pool
  let mut matching_pool_amount = 0;

  // first donation to the matching pool
  let tx = send_and_confirm_intents(
    create_matching_pool_donation_intents(1200, blockhash),
    &intents,
    &mut blocks,
  )
  .await?;
  println!("Donated 1200 to matching pool in tx: {tx:?}");
  matching_pool_amount += 1200;

  // second batch of donations to the matching pool
  let tx = send_and_confirm_intents(
    [
      create_matching_pool_donation_intents(1800, blockhash),
      create_matching_pool_donation_intents(1200, blockhash),
    ]
    .into_iter()
    .flatten(),
    &intents,
    &mut blocks,
  )
  .await?;
  println!("Donated 3000 to matching pool in tx: {tx:?}");
  matching_pool_amount += 3000;

  // await campaign start block height
  while let Ok(block) = await_next_block(&mut blocks).await {
    if block.height > campaign_start {
      // get a more recent blockhash
      // now we know that transactions running
      // from this point on will be targetting
      // an ongoing campaign.

      blockhash = *block.hash();
      break;
    }
  }

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
    let donation_intent =
      create_project_donation_intent(project, amount, blockhash);
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
  let txs = confirm_intents(donations_intents.into_iter(), &mut blocks).await?;
  println!(
    "All project donations confirmed in transactions: {:?}",
    txs.collect::<Vec<_>>()
  );

  // await campaign end block height
  while let Ok(block) = await_next_block(&mut blocks).await {
    if block.height > campaign_end {
      // get a more recent blockhash
      // now any tx will act on a campaign that is ended.
      blockhash = *block.hash();
      break;
    }
  }

  // once the funding campaign is over, redistribute all
  // funds to projects.
  let tx = send_and_confirm_intents(
    std::iter::once(create_funding_redistribution_intent(
      matching_pool_amount,
      donation_amounts,
      blockhash,
    )),
    &intents,
    &mut blocks,
  )
  .await?;
  println!("Donated 1200 to matching pool in tx: {tx:?}");

  Ok(())
}

/// Gossips an intent through p2p to solvers and awaits a produced block
/// from validators that contains a transaction with this intent.
async fn send_and_confirm_intents(
  _intent: impl Iterator<Item = Intent>,
  _intents_topic: &Topic,
  _blocks_topic: &mut BlocksStream,
) -> anyhow::Result<Transaction> {
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

fn create_campaign_intents(
  blockhash: Multihash,
  _start_height: u64,
  _end_height: u64,
) -> impl Iterator<Item = Intent> {
  std::iter::once(Intent::new(blockhash, todo!()))
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
