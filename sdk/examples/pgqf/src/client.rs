use {
  crate::{
    io::{
      install_bytecode,
      send_and_confirm_intents,
      send_and_confirm_transaction,
      start_network,
    },
    settings::SystemSettings,
  },
  anoma_network::topic::Topic,
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
  ed25519_dalek::{Keypair, Signer},
  futures::{future::join_all, StreamExt},
  model::{Campaign, Donation},
  multihash::Multihash,
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  tracing_subscriber::fmt::init();

  let opts = SystemSettings::parse();
  info!("Client options: {opts:?}");

  let (transactions, blocks, intents) = start_network(&opts)?;
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

  let (wallets, addresses): (Vec<_>, Vec<_>) =
    prepare_wallets_and_token_program(10, &transactions, &mut watcher)
      .await?
      .into_iter()
      .unzip();

  info!(
    "Generated the following wallets with 10k tokens in each: {addresses:?}"
  );

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
  let treasury_address: Address = "/token/usdc/spring-2023.eth".parse()?;
  send_and_confirm_transaction(
    create_treasury(treasury_address.clone())?,
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
      "/token/usdc/spring-2023.eth".parse()?,
      *watcher.most_recent_block().await.hash(),
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
    create_donation_to_matching_pool_intents(
      1200,
      &wallets[0],
      treasury_address.clone(),
      *watcher.most_recent_block().await.hash(),
    )?,
    &intents,
    &mut watcher,
  )
  .await?;
  info!("Donated 1200 to matching pool in tx: {tx:?}");
  matching_pool_amount += 1200;

  // second batch of donations to the matching pool
  let tx = send_and_confirm_intents(
    [
      create_donation_to_matching_pool_intents(
        1800,
        &wallets[1],
        treasury_address.clone(),
        *watcher.most_recent_block().await.hash(),
      )?,
      create_donation_to_matching_pool_intents(
        1200,
        &wallets[2],
        treasury_address.clone(),
        *watcher.most_recent_block().await.hash(),
      )?,
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

  for i in 0..N {
    let project = *projects.choose(&mut rand::thread_rng()).unwrap();
    let amount = rand::thread_rng().gen_range(10, 100);
    let donation_intent = create_project_donation_intent(
      project,
      amount,
      format!("/token/usdc/little-wallet-{i}.eth").parse()?,
      *watcher.most_recent_block().await.hash(),
    )?;
    if intents.gossip(to_vec(&donation_intent)?).is_ok() {
      info!("Donated {amount} to {project}");
      donations_intents.push(*donation_intent.hash());

      donation_amounts
        .entry(project)
        .and_modify(|e| *e += amount)
        .or_insert(amount);
    }

    // random delay
    tokio::time::sleep(Duration::from_millis(
      rand::thread_rng().gen_range(100, 1500),
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
          params: vec![
            Param::AccountRef(wallet),
            Param::AccountRef(Address::new("/pgqf/spring-2023")?),
          ],
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
  blockhash: Multihash,
) -> anyhow::Result<Transaction> {
  Ok(Transaction::new(
    // expect that for this campaign there is already a treasury token account
    // created.
    vec![Intent::new(
      blockhash,
      PredicateTree::Id(Predicate {
        code: Code::AccountRef("/stdpred".parse()?, "uint_equal".into()),
        params: vec![
          Param::AccountRef(treasury.clone()),
          Param::Inline(to_vec(&0u64)?),
        ],
      }),
    )],
    [(
      // create the campaign account
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
///
/// The intent expects that at /pgqf/<camaign>/<project-id> there will be
/// an account created that is an empty map. The map will contain later on
/// a collection of donations identified by donor wallet address.
fn create_project_intents(
  name: &str,
  blockhash: Multihash,
) -> anyhow::Result<impl Iterator<Item = Intent>> {
  Ok(std::iter::once(Intent::new(
    blockhash,
    PredicateTree::And(
      // this intent expects a new account to be created for this project
      // with an empty map of donations,
      Box::new(PredicateTree::Id(Predicate {
        code: Code::AccountRef("/stdpred".parse()?, "is_map_empty".into()),
        params: vec![Param::ProposalRef(
          format!("/pgqf/spring-2023/{name}").parse()?,
        )],
      })),
      // it also expects the existing list of projects to include the
      // newly added project in the top-level campaign account projects map.
      Box::new(PredicateTree::Id(Predicate {
        code: Code::AccountRef("/stdpred".parse()?, "key_equals".into()),
        params: vec![
          Param::ProposalRef("/pgqf/spring-2023".parse()?),
          Param::Inline(to_vec(name)?),
          Param::Inline(to_vec(&0u64)?),
        ],
      })),
    ),
  )))
}

/// Transfers funds from a sender to the treasury without allocating
/// the amount to any particular project. This way those funds become
/// part of the matching pool.
fn create_donation_to_matching_pool_intents(
  amount: u64,
  from: &Keypair,
  treasury: Address,
  blockhash: Multihash,
) -> anyhow::Result<impl Iterator<Item = Intent>> {
  let from_addr: Address = format!(
    "/token/usdc/{}",
    bs58::encode(&from.public.to_bytes()).into_string()
  )
  .parse()?;

  let mut intent = Intent::new(
    blockhash,
    PredicateTree::And(
      Box::new(PredicateTree::Id(Predicate {
        code: Code::AccountRef("/stdpred".parse()?, "uint_less_than_by".into()),
        params: vec![
          Param::AccountRef(from_addr.clone()),
          Param::ProposalRef(from_addr),
          Param::Inline(to_vec(&amount)?),
        ],
      })),
      Box::new(PredicateTree::Id(Predicate {
        code: Code::AccountRef(
          "/stdpred".parse()?,
          "uint_greater_than_by".into(),
        ),
        params: vec![
          Param::AccountRef(treasury.clone()),
          Param::ProposalRef(treasury),
          Param::Inline(to_vec(&amount)?),
        ],
      })),
    ),
  );

  // sign the intent with the donor private key
  // for the payment to be authorized by the token predicates
  intent.calldata.insert(
    bs58::encode(&from.public.to_bytes()).into_string(),
    from
      .sign(intent.signing_hash().to_bytes().as_slice())
      .to_bytes()
      .to_vec(),
  );

  Ok(std::iter::once(intent))
}

fn create_project_donation_intent(
  name: &str,
  amount: u64,
  from: Address,
  blockhash: Multihash,
) -> anyhow::Result<Intent> {
  let donation_id = from.to_string().replace('/', "-");
  let donation_address: Address =
    format!("/pgqf/spring-2023/{name}/{donation_id}").parse()?;
  let project_wallet: Address =
    format!("/token/usdc/project-{name}.eth").parse()?;

  Ok(Intent::new(
    blockhash,
    PredicateTree::And(
      Box::new(PredicateTree::And(
        Box::new(PredicateTree::Id(Predicate {
          code: Code::AccountRef(
            "/stdpred".parse()?,
            "uint_greater_than_by".into(),
          ),
          params: vec![
            Param::ProposalRef(project_wallet.clone()),
            Param::AccountRef(project_wallet),
            Param::Inline(to_vec(&amount)?),
          ],
        })),
        Box::new(PredicateTree::Id(Predicate {
          code: Code::AccountRef(
            "/stdpred".parse()?,
            "uint_less_than_by".into(),
          ),
          params: vec![
            Param::ProposalRef(from.clone()),
            Param::AccountRef(from),
            Param::Inline(to_vec(&amount)?),
          ],
        })),
      )),
      Box::new(PredicateTree::Id(Predicate {
        code: Code::AccountRef("/stdpred".parse()?, "bytes_equal".into()),
        params: vec![
          Param::ProposalRef(donation_address),
          Param::Inline(to_vec(&Donation::default())?),
        ],
      })),
    ),
  ))
}

fn create_funding_redistribution_intent(
  _matching_pool_amount: u64,
  _donation_amounts: HashMap<&str, u64>,
  _blockhash: Multihash,
) -> Intent {
  todo!();
}

fn _verify_redistribution(_tx: Transaction) -> anyhow::Result<()> {
  todo!();
}

/// This function installs the Token program and creates a token called "USDC".
/// It also generates a given number of wallets and preloads each of them with
/// 10k tokens. It configures token balance addresses with realistic predicates
/// that require signatures on spending.
///
/// Those are supposed to be wallets owned by users. Donations are sent from
/// those wallets. Rules of the treasury spending predicates do not apply on
/// them.
#[allow(dead_code)]
pub async fn prepare_wallets_and_token_program(
  count: usize,
  transactions_topic: &Topic,
  watcher: &mut BlockchainWatcher,
) -> anyhow::Result<Vec<(Keypair, Address)>> {
  // generate as many wallets as the caller wants
  let wallets: Vec<_> = (0..count)
    .map(|_| Keypair::generate(&mut rand::thread_rng()))
    .collect();

  // for each keypair, generate its token wallet address.
  let wallet_addresses: Vec<_> = wallets
    .iter()
    .map(|w| {
      Address::new(format!(
        "/token/usdc/{}",
        bs58::encode(&w.public.to_bytes()).into_string()
      ))
      .expect("should never fail. validated at compile time")
    })
    .collect();

  let wallets: Vec<_> = wallets
    .into_iter()
    .zip(wallet_addresses.into_iter())
    .collect();

  // Create a transaction that will create all those wallets on
  // chain in the global state, with realistic spending scenarios
  // e.g. validating signatures on spending, etc.

  // Here we're doing a simple trick by first creating the wallet balance
  // accounts and THEN installing the global token predicate, to avoid
  // having to go through the whole mint/transfer scenario that is already
  // tested in VM tests.
  // For each wallet we create an account prefunded with 10k.
  let tx = Transaction::new(
    vec![],
    wallets
      .iter()
      .map(|(kp, addr)| {
        (
          addr.clone(),
          AccountChange::CreateAccount(Account {
            state: to_vec(&10000u64).expect("valid at compile time"),
            predicates: PredicateTree::Or(
              Box::new(PredicateTree::Id(Predicate {
                code: Code::AccountRef(
                  "/stdpred".parse().expect("valid at compile time"),
                  "uint_greater_than_equal".into(),
                ),
                params: vec![
                  Param::ProposalRef(addr.clone()),
                  Param::AccountRef(addr.clone()),
                ],
              })),
              Box::new(PredicateTree::Id(Predicate {
                code: Code::AccountRef(
                  "/stdpred".parse().expect("valid at compile time"),
                  "require_ed25519_signature".into(),
                ),
                params: vec![Param::Inline(kp.public.to_bytes().to_vec())],
              })),
            ),
          }),
        )
      })
      .collect(),
  );

  send_and_confirm_transaction(tx, transactions_topic, watcher).await?;

  // now create a fake USDC token with the correct total supply
  let total_supply = count as u64 * 10000u64;
  let mint_keypair = Keypair::generate(&mut rand::thread_rng());
  let token_address = Address::new("/token/usdx").unwrap();
  send_and_confirm_transaction(
    Transaction::new(
      vec![],
      [(
        token_address.clone(),
        AccountChange::CreateAccount(Account {
          state: to_vec(&total_supply)?,
          predicates: PredicateTree::And(
            Box::new(PredicateTree::Id(Predicate {
              // token contract predicate
              code: Code::AccountRef(
                "/token".parse().unwrap(),
                "predicate".into(),
              ),
              params: vec![
                // input params as per documentation:

                // self address, used to identity child wallet balances
                // accounts
                Param::Inline(to_vec(&token_address).unwrap()),
                // mint authority, signature to authorize minting and burning
                // tokens
                Param::Inline(to_vec(&mint_keypair.public.as_bytes()).unwrap()),
                // reference to an account where the total supply value is
                // stored. we're going to store it in the
                // top-level account itself
                Param::AccountRef(token_address.clone()),
              ],
            })),
            Box::new(PredicateTree::Id(Predicate {
              code: Code::AccountRef(
                "/stdpred".parse().unwrap(),
                "constant".into(),
              ),
              params: vec![Param::Inline(to_vec(&false).unwrap())],
            })),
          ),
        }),
      )]
      .into(),
    ),
    transactions_topic,
    watcher,
  )
  .await?;

  // now we're ready to install the token bytecode and start applying regular
  // token rules

  info!("Installing Token Library...");
  send_and_confirm_transaction(
    install_bytecode(
      "/token".parse()?,
      include_bytes!(
        "../../../../target/wasm32-unknown-unknown/release/examples/token.wasm"
      ),
    )?,
    transactions_topic,
    watcher,
  )
  .await?;
  info!("Token Library installed");

  Ok(wallets)
}
