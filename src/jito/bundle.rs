use core::time;
use dotenv::dotenv;
use reqwest::Client as HttpClient;
use solana_sdk::{
    account::Account,
    address_lookup_table::{state::AddressLookupTable, AddressLookupTableAccount},
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::VersionedTransaction,
};
use std::{env, sync::Arc, thread::sleep};
use tokio::time::Duration;

use super::help::BundleTransactions;
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
        "accounts/{}/{}/{}.json",
        requester_pubkey,
        mint.pubkey().to_string(),
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
    let num_txs = (extend_tx_size + 1231) / 1232; // Round up division
    if num_txs > 1 {
        let chunk_size = (pubkeys_for_lut.len() + num_txs - 1) / num_txs; // Round up division
        for (i, chunk) in pubkeys_for_lut.chunks(chunk_size).enumerate() {
            let extended_lut = extend_lut(&client, &admin_kp, lut.0, &chunk.to_vec()).unwrap();
            println!("LUT extended part {}/{}: {:?}", i + 1, num_txs, extended_lut);
        }
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
    println!("Transfer Transaction size: {:?}", tx_size);

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
    let mut txs_builder: BundleTransactions = BundleTransactions::new(
        dev_keypair_with_amount.keypair,
        mint,
        lut_pubkey,
        keypairs_with_amount,
    );

    //Submitting first bundle
    let first_bundle: Vec<VersionedTransaction> = txs_builder
        .collect_first_bundle_txs(dev_keypair_with_amount.amount, token_metadata)
        .await;

    if txs_builder.has_delayed_bundle() {
        let mint_pubkey = mint.pubkey();
        let admin_kp = load_keypair(&admin_keypair_path).unwrap();
        let payer: Arc<Keypair> = Arc::new(admin_kp);
        let pumpfun_client = PumpFun::new(payer);
        let rpc = RpcClient::new(RPC_URL);
        let jito = JitoBundle::new(rpc, MAX_RETRIES, JITO_TIP_AMOUNT);

        tokio::spawn(async move {
            //Check every 500ms if token is live, then build and submit rest of txs
            let mut is_live: bool = false;
            let start_time = std::time::Instant::now();

            while !is_live {
                if start_time.elapsed() > Duration::from_secs(120) {
                    println!("Timeout reached after 2 minutes, killing process");
                    return;
                }

                let live = pumpfun_client.is_token_live(&mint_pubkey).await;
                if live {
                    is_live = true;
                } else {
                    sleep(Duration::from_millis(500));
                }
            }
            let late_txs = txs_builder.collect_rest_txs().await;
            let late_txs_chunks: Vec<Vec<VersionedTransaction>> = late_txs.chunks(5).map(|c| c.to_vec()).collect();

            print!("We received {:?} late bundles", late_txs_chunks.len());

            for chunk in late_txs_chunks {
                // Only send first chunk for testing
                let _ = jito
                    .submit_bundle(chunk, mint_pubkey, Some(&pumpfun_client))
                    .await
                    .unwrap();
            }
        });
    }

    // Only send first chunk for testing
    let _ = jito
        .submit_bundle(first_bundle, mint.pubkey(), Some(&pumpfun_client))
        .await
        .unwrap();

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
