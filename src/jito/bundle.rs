use dotenv::dotenv;
use reqwest::Client as HttpClient;
use solana_sdk::{
    account::Account,
    address_lookup_table::state::AddressLookupTable,
    address_lookup_table::AddressLookupTableAccount,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use std::{env, sync::Arc};
use tokio::time::Duration;

use super::help::build_bundle_txs;
use crate::jito::jito::JitoBundle;
use crate::params::KeypairWithAmount;
use crate::pumpfun::pump::PumpFun;
use crate::solana::{
    lut::{create_lut, extend_lut, verify_lut_ready},
    utils::{build_transaction, load_keypair, transfer_ix},
};
use crate::{
    config::{JITO_TIP_AMOUNT, MAX_RETRIES, ORCHESTRATOR_URL, RPC_URL},
    solana::lut::extend_lut_size,
};
use pumpfun_cpi::instruction::Create;
use solana_client::rpc_client::RpcClient;

pub async fn process_bundle(
    keypairs_with_amount: Vec<KeypairWithAmount>,
    dev_keypair_with_amount: KeypairWithAmount,
    mint: &Keypair,
    requester_pubkey: String,
    token_metadata: Create,
) -> Result<Pubkey, Box<dyn std::error::Error + Send + Sync>> {
    dotenv().ok();
    let admin_keypair_path = env::var("ADMIN_KEYPAIR").unwrap();
    let admin_kp = load_keypair(&admin_keypair_path).unwrap();

    let client = RpcClient::new(RPC_URL);

    let jito_rpc = RpcClient::new_with_commitment(
        RPC_URL.to_string(),
        solana_sdk::commitment_config::CommitmentConfig::confirmed(),
    );

    let jito = JitoBundle::new(jito_rpc, MAX_RETRIES, JITO_TIP_AMOUNT);

    let dev_keypair_path = format!(
        "accounts/{}/{}.json",
        requester_pubkey,
        dev_keypair_with_amount.keypair.pubkey()
    );

    let loaded_dev_keypair = load_keypair(&dev_keypair_path).unwrap();

    let payer: Arc<Keypair> = Arc::new(loaded_dev_keypair);

    let pumpfun_client = PumpFun::new(payer);

    println!("Creating lut with admin public key:  {}", admin_kp.pubkey());
    println!("Keypairs with amount: {:?}", keypairs_with_amount);

    println!(
        "Admin keypair balance: {}",
        client.get_balance(&admin_kp.pubkey()).unwrap_or(0)
    );
    println!("Attempting to create LUT...");

    let lut: (solana_sdk::pubkey::Pubkey, solana_sdk::signature::Signature) =
        create_lut(&client, &admin_kp).unwrap();

    let lut_pubkey = lut.0;

    let mut retries = 5;
    while retries > 0 {
        match verify_lut_ready(&client, &lut.0) {
            Ok(true) => {
                println!("LUT is ready");
                break;
            }
            Ok(false) => {
                tokio::time::sleep(Duration::from_millis(500)).await;
                retries -= 1;
            }
            Err(e) => {
                println!("Error verifying LUT: {:?}", e);
                return Err(Box::new(e));
            }
        }
    }

    if retries == 0 {
        return Err("LUT not ready after maximum retries".into());
    }

    let mut pubkeys_for_lut: Vec<Pubkey> = Vec::new();

    pubkeys_for_lut.push(admin_kp.pubkey());
    pubkeys_for_lut.push(mint.pubkey());
    pubkeys_for_lut.push(dev_keypair_with_amount.keypair.pubkey());

    for keypair in keypairs_with_amount.iter() {
        pubkeys_for_lut.push(keypair.keypair.pubkey());
        let ata_pubkey = pumpfun_client.get_ata(&keypair.keypair.pubkey(), &mint.pubkey());
        println!("ATA pubkey: {:?}", ata_pubkey);
        pubkeys_for_lut.push(ata_pubkey);
    }

    let dev_ata_pubkey =
        pumpfun_client.get_ata(&dev_keypair_with_amount.keypair.pubkey(), &mint.pubkey());
    pubkeys_for_lut.push(dev_ata_pubkey);

    let extend_tx_size = extend_lut_size(&client, &admin_kp, lut_pubkey, &pubkeys_for_lut).unwrap();
    if extend_tx_size > 1232 {
        //Split lut into two vectors
        let split_point = pubkeys_for_lut.len() / 2;
        let first_half = pubkeys_for_lut[..split_point].to_vec();
        let second_half = pubkeys_for_lut[split_point..].to_vec();
        //Extend lut with addresses & attached token accounts
        let extended_lut = extend_lut(&client, &admin_kp, lut.0, &first_half).unwrap();
        println!("LUT extended once: {:?}", extended_lut);
        //Extend lut with addresses & attached token accounts
        let extended_lut = extend_lut(&client, &admin_kp, lut.0, &second_half).unwrap();
        println!("LUT extended twice: {:?}", extended_lut);
    } else {
        //Extend lut with addresses & attached token accounts
        let extended_lut = extend_lut(&client, &admin_kp, lut.0, &pubkeys_for_lut).unwrap();
        println!("LUT extended with addresses: {:?}", extended_lut);
    }
    //STEP 2: Transfer funds needed from admin to dev + keypairs in a bundle

    println!(
        "Amount of lamports to transfer to dev: {}",
        dev_keypair_with_amount.amount
    );

    let admin_to_dev_ix = transfer_ix(
        &admin_kp.pubkey(),
        &dev_keypair_with_amount.keypair.pubkey(),
        dev_keypair_with_amount.amount,
    );
    let admin_to_keypair_ixs: Vec<Instruction> = keypairs_with_amount
        .iter()
        .map(|keypair| {
            transfer_ix(
                &admin_kp.pubkey(),
                &keypair.keypair.pubkey(),
                keypair.amount,
            )
        })
        .collect();
    let jito_tip_ix = jito.get_tip_ix(admin_kp.pubkey()).await.unwrap();

    //Instructions to send sol from admin to dev + keypairs
    let mut instructions: Vec<Instruction> = vec![admin_to_dev_ix];
    instructions.extend(admin_to_keypair_ixs);
    instructions.push(jito_tip_ix);

    println!("LUT address: {:?}", lut.0);
    println!(
        "Addresses: {:?}",
        keypairs_with_amount
            .iter()
            .map(|keypair| keypair.keypair.pubkey())
            .collect::<Vec<Pubkey>>()
    );

    let raw_account: Account = client.get_account(&lut_pubkey).unwrap();
    let address_lookup_table = AddressLookupTable::deserialize(&raw_account.data).unwrap();

    println!("Address lookup table: {:?}", address_lookup_table);

    let address_lookup_table_account = AddressLookupTableAccount {
        key: lut_pubkey,
        addresses: address_lookup_table.addresses.to_vec(),
    };
    let tx = build_transaction(
        &client,
        &instructions,
        vec![&admin_kp],
        address_lookup_table_account.clone(),
        &admin_kp,
    );

    let tx_size: usize = bincode::serialized_size(&tx).unwrap() as usize;
    println!("Transaction size: {:?}", tx_size);

    if tx_size > 1232 {
        // Split instructions into two vectors
        let instructions_len = instructions.len();
        let split_point = instructions_len / 2;

        let mut first_instructions: Vec<Instruction> = instructions[..split_point].to_vec();
        let jito_tip_ix = jito.get_tip_ix(admin_kp.pubkey()).await.unwrap();
        first_instructions.push(jito_tip_ix);
        let second_instructions = instructions[split_point..].to_vec();

        // Build and send first transaction
        let first_tx = build_transaction(
            &client,
            &first_instructions,
            vec![&admin_kp],
            address_lookup_table_account.clone(),
            &admin_kp,
        );
        let _ = jito.one_tx_bundle(first_tx).await.unwrap();

        // Build and send second transaction
        let second_tx = build_transaction(
            &client,
            &second_instructions,
            vec![&admin_kp],
            address_lookup_table_account.clone(),
            &admin_kp,
        );
        let _ = jito.one_tx_bundle(second_tx).await.unwrap();
    } else {
        //Sending transaction to fund wallets from admin.
        //TODO: Check if this is complete. might require tip instruction, signature to tx, and confirmation that bundle is complete
        let _ = jito.one_tx_bundle(tx).await.unwrap();
    }

    println!("Transaction built");
    //let signature = client.send_and_confirm_transaction_with_spinner(&tx).unwrap();

    let mut dev_balance = client
        .get_balance(&dev_keypair_with_amount.keypair.pubkey())
        .unwrap();
    while dev_balance < dev_keypair_with_amount.amount {
        tokio::time::sleep(Duration::from_secs(3)).await;
        dev_balance = client
            .get_balance(&dev_keypair_with_amount.keypair.pubkey())
            .unwrap();
    }

    //Step 4: Create and extend lut for the bundle
    let other_balances = keypairs_with_amount
        .iter()
        .map(|keypair| client.get_balance(&keypair.keypair.pubkey()).unwrap())
        .collect::<Vec<u64>>();
    println!("Other balances: {:?}", other_balances);
    let balances_to_buy = other_balances
        .iter()
        .map(|balance| balance - 1_900_000)
        .collect::<Vec<u64>>();
    println!("Balances to buy: {:?}", balances_to_buy);

    // Print difference between intended buy amount and actual balance
    for (i, keypair) in keypairs_with_amount.iter().enumerate() {
        let actual_balance = other_balances[i];
        let intended_amount = keypair.amount;
        println!(
            "Wallet {}: Intended buy amount: {}, Actual balance: {}, Difference: {}",
            keypair.keypair.pubkey(),
            intended_amount,
            actual_balance,
            if actual_balance >= intended_amount {
                actual_balance - intended_amount
            } else {
                println!("WARNING: Actual balance less than intended buy amount!");
                intended_amount - actual_balance
            }
        );
    }

    //Step 5: Prepare mint instruction and buy instructions as well as tip instruction
    let mint_pubkey = mint.pubkey();

    let transactions = build_bundle_txs(
        dev_keypair_with_amount,
        mint,
        keypairs_with_amount,
        lut_pubkey,
        mint_pubkey,
        token_metadata,
        &admin_kp,
    )
    .await;

    // Split transactions into batches of 5
    let chunks: Vec<_> = transactions.chunks(5).collect();
    let total_chunks = chunks.len();
    let last_chunk_size = chunks.last().map_or(0, |chunk| chunk.len());

    println!("Total number of chunks: {}", total_chunks);
    println!("Number of transactions in last chunk: {}", last_chunk_size);

    // Only send first chunk for testing
    if let Some(first_chunk) = chunks.first() {
        println!(
            "Attempting to submit first bundle of {} transactions...",
            first_chunk.len()
        );
        let chunk_vec = first_chunk.to_vec();
        let _ = jito
            .submit_bundle(chunk_vec, mint.pubkey(), Some(&pumpfun_client))
            .await
            .unwrap();
    }

    println!("Making callback to orchestrator...");
    // Fire and forget the callback
    let callback_payload = serde_json::json!({
        "mint": mint.pubkey().to_string(),
    });

    tokio::spawn(async move {
        if let Err(e) = HttpClient::new()
            .post(ORCHESTRATOR_URL)
            .json(&callback_payload)
            .send()
            .await
        {
            eprintln!("Failed to send completion signal: {}", e);
        }
    });

    println!("Bundle completed");
    println!("Bundle lut: {:?}", address_lookup_table_account);
    Ok(address_lookup_table_account.key)
}
