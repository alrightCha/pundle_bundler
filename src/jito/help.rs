use super::jito::JitoBundle;
use crate::pumpfun::pump::PumpFun;
use crate::solana::utils::build_transaction;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    address_lookup_table::state::AddressLookupTable,
    address_lookup_table::AddressLookupTableAccount,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::VersionedTransaction,
    sysvar::rent::Rent,
};
use std::collections::HashSet;
use crate::config::{MAX_RETRIES, JITO_TIP_AMOUNT, RPC_URL};
use std::sync::Arc;
use crate::params::{CreateTokenMetadata, KeypairWithAmount};
use crate::config::{BUFFER_AMOUNT, FEE_AMOUNT, MAX_TX_PER_BUNDLE};

/*
keypairs_with_amount: Vec<KeypairWithAmount>,
dev_keypair_with_amount: KeypairWithAmount,
mint: Keypair,
requester_pubkey: String,
token_metadata: CreateTokenMetadata
*/

pub async fn build_bundle_txs(dev_with_amount: KeypairWithAmount, mint_keypair: &Keypair, others_with_amount: Vec<KeypairWithAmount>, lut_pubkey: Pubkey, mint_pubkey: Pubkey, token_metadata: CreateTokenMetadata) -> Vec<VersionedTransaction> {
    println!("Building bundle txs...");
    let client = RpcClient::new(RPC_URL);

    let jito_rpc = RpcClient::new_with_commitment(
        RPC_URL.to_string(),
        solana_sdk::commitment_config::CommitmentConfig::confirmed(),
    );

    let jito = JitoBundle::new(jito_rpc, MAX_RETRIES, JITO_TIP_AMOUNT);
    let dev: Keypair = dev_with_amount.keypair.insecure_clone();
    let payer: Arc<Keypair> = Arc::new(dev);

    let mut pumpfun_client = PumpFun::new(payer);

    let raw_account = client.get_account(&lut_pubkey).unwrap();
    let address_lookup_table = AddressLookupTable::deserialize(&raw_account.data).unwrap();

    let address_lookup_table_account = AddressLookupTableAccount {
        key: lut_pubkey,
        addresses: address_lookup_table.addresses.to_vec(),
    };

    //BUILD INSTRUCTIONS

    //calculating the max amount of lamports to buy with
    let rent = Rent::default();
    let rent_exempt_min = rent.minimum_balance(0);

    let to_subtract: u64 = rent_exempt_min + FEE_AMOUNT + BUFFER_AMOUNT;

    let to_sub_for_dev: u64 = to_subtract.clone() + JITO_TIP_AMOUNT;

    let final_dev_buy_amount = dev_with_amount.amount - to_sub_for_dev;

    let mint_ix = pumpfun_client
        .create_instruction(&mint_keypair, token_metadata)
        .await
        .unwrap();

    let tip_ix = jito
    .get_tip_ix(dev_with_amount.keypair.pubkey())
    .await
    .unwrap();

    let dev_ix = pumpfun_client
        .buy_ixs(
            &mint_pubkey,
            &dev_with_amount.keypair,
            final_dev_buy_amount,
            None,
            true,
        )
        .await
        .unwrap();

    //TODO: add to params
    let mint_pubkey: &Pubkey = &mint_keypair.pubkey();

    let mut transactions: Vec<VersionedTransaction> = Vec::new();

    let mut current_tx_ixs : Vec<Instruction> = Vec::new(); 

    current_tx_ixs.extend(dev_ix);
    current_tx_ixs.push(mint_ix);
    current_tx_ixs.push(tip_ix);

    for (index, keypair) in others_with_amount.iter().enumerate() {
        let balance = client.get_balance(&keypair.keypair.pubkey()).unwrap();
        if balance < keypair.amount {
            println!(
                "Keypair {} has insufficient balance. Skipping buy.",
                keypair.keypair.pubkey()
            );
            continue;
        }

        let final_buy_amount = keypair.amount - to_subtract;

        //Return buy instructions
        let new_ixs =pumpfun_client
        .buy_ixs(mint_pubkey, &keypair.keypair, final_buy_amount, None, transactions.len() < MAX_TX_PER_BUNDLE)
        .await
        .unwrap();
        //let mut maybe_ixs: Vec<Instruction> = Vec::new(); 

        //for ix in &current_tx_ixs {
        //    maybe_ixs.push(ix.clone());
        //}

        //maybe_ixs.extend(buy_ixs);

        let mut unique_signers: HashSet<Pubkey> = HashSet::new();

        let mut all_ixs: Vec<Instruction> = Vec::new();

        for ix in &current_tx_ixs {
            for acc in ix.accounts.iter().filter(|acc| acc.is_signer) {
                unique_signers.insert(acc.pubkey);
            }
            all_ixs.push(ix.clone());
        }

        let mut tx_signers: Vec<&Keypair> = Vec::new();

        for signer in unique_signers {
            if let Some(kp) = others_with_amount.iter().find(|kp| kp.keypair.pubkey() == signer) {
                tx_signers.push(&kp.keypair);
            }
            if signer == dev_with_amount.keypair.pubkey() {
                tx_signers.push(&dev_with_amount.keypair);
            }
            if signer == mint_keypair.pubkey() {
                tx_signers.push(&mint_keypair);
            }
        }

        for ix in &new_ixs {
            all_ixs.push(ix.clone());
        }

        let mut maybe_ix_unique_signers: HashSet<Pubkey> = HashSet::new();
        for ix in &all_ixs {
            for acc in ix.accounts.iter().filter(|acc| acc.is_signer) {
                maybe_ix_unique_signers.insert(acc.pubkey);
            }
        }

        let mut maybe_ix_tx_signers: Vec<&Keypair> = Vec::new();
        for signer in maybe_ix_unique_signers {
            if let Some(kp) = others_with_amount.iter().find(|kp| kp.keypair.pubkey() == signer) {
                maybe_ix_tx_signers.push(&kp.keypair);
            }
            if signer == dev_with_amount.keypair.pubkey() {
                maybe_ix_tx_signers.push(&dev_with_amount.keypair);
            }
            if signer == mint_keypair.pubkey() {
                maybe_ix_tx_signers.push(&mint_keypair);
            }
        }

        let maybe_tx = build_transaction(&client, &all_ixs, &maybe_ix_tx_signers, address_lookup_table_account.clone());

        let size: usize = bincode::serialized_size(&maybe_tx).unwrap() as usize;

        //Add new ixs to current tx instructions if size below 1232, else create new tx and reset current tx instructions, and add new ixs to it
        if size > 1232 {
            let new_tx = build_transaction(&client, &current_tx_ixs, &tx_signers, address_lookup_table_account.clone());
            transactions.push(new_tx);
            println!("Added new tx to transactions, with size {}", size);
            current_tx_ixs = vec![];
            current_tx_ixs.extend(new_ixs);
            if index % 5 == 0 {
                println!("Adding tip ix to current tx instructions");
                let tip_ix = jito.get_tip_ix(dev_with_amount.keypair.pubkey()).await.unwrap();
                current_tx_ixs.push(tip_ix);
            }
        }else{
            println!("Adding new ixs to current tx instructions, current size: {}", size);
            current_tx_ixs.extend(new_ixs);
        }
        println!("With {} instructions", current_tx_ixs.len());
        println!("Transaction size: {:?}", size);
    }

    if current_tx_ixs.len() > 0 {
        let mut unique_signers: HashSet<Pubkey> = HashSet::new();
        for ix in &current_tx_ixs {
            for acc in ix.accounts.iter().filter(|acc| acc.is_signer) {
                unique_signers.insert(acc.pubkey);
            }
        }

        let mut tx_signers: Vec<&Keypair> = Vec::new();
        for signer in unique_signers {
            if let Some(kp) = others_with_amount.iter().find(|kp| kp.keypair.pubkey() == signer) {
                tx_signers.push(&kp.keypair);
            }
            if signer == dev_with_amount.keypair.pubkey() {
                tx_signers.push(&dev_with_amount.keypair);
            }
            if signer == mint_keypair.pubkey() {
                tx_signers.push(&mint_keypair);
            }
        }
        transactions.push(build_transaction(&client, &current_tx_ixs, &tx_signers, address_lookup_table_account.clone()));
    }

    /*

    TODO:

    -Test the build_transaction functions with the wallets

    -Play with the helper function and make it recursive
            - Make it detect when we are in first bundle
            - Make it add a tip instruction to every 5th transaction
            - Make it split instructions into transactions correctly, and maximize instructions per transactino respecting size limit

    ---> Goal is to make the helper function work correctly, and that it passes with_stimulate true only for the first 5 transactions

    -Submit multiple bundles and await for the first bundle

    ---> Goal is to ensure that the first bundle passes, second bundles

     */

    transactions
}


