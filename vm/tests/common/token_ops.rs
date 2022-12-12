use {
  anoma_primitives::{
    Account,
    AccountChange,
    Address,
    Code,
    Exact,
    Intent,
    Param,
    Predicate,
    PredicateTree,
    Transaction,
  },
  anoma_vm::State,
  ed25519_dalek::{Keypair, PublicKey, Signer},
  multihash::Multihash,
  rmp_serde::{from_slice, to_vec},
};

/// Creates a transaction that mints a given number of USDX tokens to a given
/// wallet account that does not exist yet.
///
/// This operation expects that:
///   1. the standard predicate library is installed in state
///   2. an instance of USDX token exists in state
pub fn mint(
  amount: u64,
  recipient: &Address,
  recipient_pubkey: &PublicKey,
  auth_keypair: &Keypair,
  recent_blockhash: Multihash,
  state: &impl State,
) -> anyhow::Result<Transaction> {
  let new_supply = state
    .get(&"/token/usdx".parse()?)
    .map(|acc| from_slice(&acc.state).unwrap())
    .unwrap_or(0)
    + amount;

  let new_balance = state
    .get(recipient)
    .map(|acc| from_slice(&acc.state).unwrap())
    .unwrap_or(0)
    + amount;

  let mut mint_intent = Intent::new(
    recent_blockhash,
    PredicateTree::<Exact>::And(
      Box::new(PredicateTree::Id(Predicate {
        // expect that the total supply is updated by the mint amount
        code: Code::AccountRef("/predicates/std".parse()?, "uint_equal".into()),
        params: vec![
          Param::ProposalRef("/token/usdx".parse()?),
          Param::Inline(to_vec(&new_supply)?),
        ],
      })),
      Box::new(PredicateTree::Id(Predicate {
        // expect that the minted amount is credited to a wallet
        code: Code::AccountRef("/predicates/std".parse()?, "uint_equal".into()),
        params: vec![
          Param::ProposalRef(recipient.clone()),
          Param::Inline(to_vec(&new_balance)?),
        ],
      })),
    ),
  );

  let recipient_acc_change = match state.get(recipient) {
    Some(_) => AccountChange::ReplaceState(to_vec(&new_balance)?),
    None => AccountChange::CreateAccount(Account {
      // wallet does not exist, create it
      state: to_vec(&new_balance)?,
      predicates: PredicateTree::Or(
        Box::new(PredicateTree::Id(Predicate {
          // The newly created account will requre a
          // signature if the balance is deducted,
          // otherwise its happy to receive tokens
          // without any authorization.
          code: Code::AccountRef(
            "/predicates/std".parse()?,
            "uint_greater_than_equal".into(),
          ),
          params: vec![
            Param::ProposalRef(recipient.clone()),
            Param::AccountRef(recipient.clone()),
          ],
        })),
        Box::new(PredicateTree::Id(Predicate {
          // If proposed balance is not greater that current balance
          // then require a signature to authorize spending
          code: Code::AccountRef(
            "/predicates/std".parse()?,
            "require_ed25519_signature".into(),
          ),
          params: vec![Param::Inline(recipient_pubkey.to_bytes().to_vec())],
        })),
      ),
    }),
  };

  // add mint authority signature to the intent
  let mint_pk_b58 = bs58::encode(auth_keypair.public.as_bytes()).into_string();

  mint_intent.calldata.insert(
    mint_pk_b58,
    auth_keypair
      .sign(mint_intent.signing_hash().to_bytes().as_slice())
      .to_bytes()
      .to_vec(),
  );

  Ok(Transaction::new(
    vec![mint_intent],
    [
      (recipient.clone(), recipient_acc_change),
      (
        "/token/usdx".parse()?, // update total supply
        AccountChange::ReplaceState(to_vec(&new_supply)?),
      ),
    ]
    .into_iter()
    .collect(),
  ))
}

/// Creates a transaction that transfers a given number of USDX tokens between
/// two USDX token wallets.
///
/// This operation expects that:
///   1. the standard predicate library is installed in state
///   2. an instance of USDX token exists in state
pub fn transfer(
  amount: u64,
  sender: &Address,
  sender_keypair: &Keypair,
  recipient: &Address,
  recipient_pubkey: &PublicKey,
  recent_blockhash: Multihash,
  state: &impl State,
) -> anyhow::Result<Transaction> {
  assert!(state.get(sender).is_some());

  let mut transfer_intent = Intent::new(
    recent_blockhash,
    PredicateTree::<Exact>::And(
      Box::new(PredicateTree::Id(Predicate {
        code: Code::AccountRef(
          "/predicates/std".parse()?,
          "uint_less_than_by".into(),
        ),
        params: vec![
          Param::ProposalRef(sender.clone()),
          Param::AccountRef(sender.clone()),
          Param::Inline(to_vec(&amount)?),
        ],
      })),
      Box::new(PredicateTree::Id(Predicate {
        code: Code::AccountRef(
          "/predicates/std".parse()?,
          "uint_greater_than_equal".into(),
        ),
        params: vec![
          Param::ProposalRef(recipient.clone()),
          Param::Inline(to_vec(&amount)?),
        ],
      })),
    ),
  );

  // sign intent by sender
  transfer_intent.calldata.insert(
    bs58::encode(sender_keypair.public.as_bytes()).into_string(),
    sender_keypair
      .sign(transfer_intent.signing_hash().to_bytes().as_slice())
      .to_bytes()
      .to_vec(),
  );

  let new_sender_balance =
    from_slice::<u64>(&state.get(sender).expect("asserted").state)?
      .checked_sub(amount)
      .expect("sender balance too low");

  let new_recipient_balance = state
    .get(recipient)
    .map(|acc| {
      from_slice::<u64>(&acc.state)
        .unwrap()
        .checked_add(amount)
        .expect("that's too much money")
    })
    .unwrap_or(amount);

  let sender_acc_change =
    AccountChange::ReplaceState(to_vec(&new_sender_balance)?);
  let recipient_acc_change = match state.get(recipient) {
    Some(_) => AccountChange::ReplaceState(to_vec(&new_recipient_balance)?),
    None => AccountChange::CreateAccount(Account {
      state: to_vec(&new_recipient_balance)?,
      predicates: PredicateTree::Or(
        Box::new(PredicateTree::Id(Predicate {
          code: Code::AccountRef(
            "/predicates/std".parse()?,
            "uint_greater_than_equal".into(),
          ),
          params: vec![
            Param::ProposalRef(recipient.clone()),
            Param::AccountRef(recipient.clone()),
          ],
        })),
        Box::new(PredicateTree::Id(Predicate {
          // If proposed balance is not greater that current balance
          // then require a signature to authorize spending
          code: Code::AccountRef(
            "/predicates/std".parse()?,
            "require_ed25519_signature".into(),
          ),
          params: vec![Param::Inline(recipient_pubkey.to_bytes().to_vec())],
        })),
      ),
    }),
  };

  Ok(Transaction::new(
    vec![transfer_intent], //
    [
      (recipient.clone(), recipient_acc_change),
      (sender.clone(), sender_acc_change),
    ]
    .into_iter()
    .collect(),
  ))
}
