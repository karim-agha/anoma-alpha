use {
  crate::settings::SystemSettings,
  anoma_network::{
    topic::{self, Topic},
    Config,
    Keypair,
    Network,
  },
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
  anoma_sdk::BlockchainWatcher,
  futures::future::join_all,
  rmp_serde::to_vec,
  std::time::Duration,
  tracing::info,
};

/// Gossips an intent through p2p to solvers and awaits a produced block
/// from validators that contains a transaction with this intent.
#[allow(dead_code)]
pub async fn send_and_confirm_intents(
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
          info!(
            "Intent {} sent",
            bs58::encode(intent.hash().to_bytes()).into_string()
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
#[allow(dead_code)]
pub async fn send_and_confirm_transaction(
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

#[allow(dead_code)]
pub fn install_bytecode(
  address: Address,
  bytecode: &[u8],
) -> anyhow::Result<Transaction> {
  Ok(Transaction::new(
    vec![],
    [(
      address.clone(),
      AccountChange::CreateAccount(Account {
        state: bytecode.to_vec(),
        predicates: PredicateTree::And(
          Box::new(PredicateTree::Id(Predicate {
            code: Code::AccountRef(
              "/stdpred".parse()?,
              "immutable_state".into(),
            ),
            params: vec![Param::Inline(to_vec(&address)?)],
          })),
          Box::new(PredicateTree::Id(Predicate {
            code: Code::AccountRef(
              "/stdpred".parse()?,
              "immutable_predicates".into(),
            ),
            params: vec![Param::Inline(to_vec(&address)?)],
          })),
        ),
      }),
    )]
    .into(),
  ))
}

/// (transactions, blocks, intents) topic handles
#[allow(dead_code)]
pub fn start_network(
  settings: &SystemSettings,
) -> anyhow::Result<(Topic, Topic, Topic)> {
  let mut network = Network::new(
    Config {
      listen_addrs: settings.p2p_addrs(),
      ..Default::default()
    },
    Keypair::generate_ed25519(),
  )?;

  let txs_topic = network.join(topic::Config {
    name: format!("/{}/transactions", settings.network_id()),
    bootstrap: settings.peers(),
  })?;

  let blocks_topic = network.join(topic::Config {
    name: format!("/{}/blocks", settings.network_id()),
    bootstrap: settings.peers(),
  })?;

  let intents_topic = network.join(topic::Config {
    name: format!("/{}/intents", settings.network_id()),
    bootstrap: settings.peers(),
  })?;

  tokio::spawn(network.runloop());
  Ok((txs_topic, blocks_topic, intents_topic))
}
