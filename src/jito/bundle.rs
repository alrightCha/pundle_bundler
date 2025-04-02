use dotenv::dotenv;
use reqwest::Client as HttpClient;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::{
    account::Account,
    address_lookup_table::{state::AddressLookupTable, AddressLookupTableAccount},
    hash::Hash,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::{Transaction, VersionedTransaction},
};

use std::{env, sync::Arc, thread::sleep};
use tokio::time::Duration;

use super::help::BundleTransactions;
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
use crate::{jito::jito::JitoBundle, solana::utils::test_transactions};
use pumpfun_cpi::instruction::Create;
use solana_client::rpc_client::RpcClient;

pub async fn process_bundle(
    keypairs_with_amount: Vec<KeypairWithAmount>,
    dev_keypair_with_amount: KeypairWithAmount,
    mint: &Keypair,
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

    let payer: Arc<Keypair> = Arc::new(admin_kp.insecure_clone());

    let pumpfun_client = PumpFun::new(payer);

    let lut: (solana_sdk::pubkey::Pubkey, solana_sdk::signature::Signature) =
        create_lut(&client, &admin_kp).unwrap();

    let lut_pubkey = lut.0;

    let mut retries = 5;
    while retries > 0 {
        match verify_lut_ready(&client, &lut.0) {
            Ok(true) => {
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

    let tip_account: Pubkey = jito.get_tip_account().await;

    pubkeys_for_lut.push(admin_kp.pubkey());

    //Adding tip account to lut
    pubkeys_for_lut.push(tip_account);

    //Adding other addresses to lut
    let extra_addresses: Vec<Pubkey> = pumpfun_client.get_addresse_for_lut(&mint.pubkey()).await;

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
            println!(
                "LUT extended part {}/{}: {:?}",
                i + 1,
                num_txs,
                extended_lut
            );
        }
    } else {
        //Extend lut with addresses & attached token accounts
        let _ = extend_lut(&client, &admin_kp, lut.0, &pubkeys_for_lut).unwrap();
    }
    //STEP 2: Transfer funds needed from admin to dev + keypairs in a bundle

    let admin_to_dev_ix: Instruction = transfer_ix(
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

    let jito_tip_ix = jito
        .get_tip_ix(admin_kp.pubkey(), Some(tip_account))
        .await
        .unwrap();

    let priority_fee_amount = 7_000; // 0.000007 SOL
                                     // Create priority fee instruction
    let set_compute_unit_price_ix =
        ComputeBudgetInstruction::set_compute_unit_price(priority_fee_amount);

    //Instructions to send sol from admin to dev + keypairs
    let mut instructions: Vec<Instruction> = vec![set_compute_unit_price_ix, admin_to_dev_ix];
    instructions.extend(admin_to_keypair_ixs);
    instructions.push(jito_tip_ix);

    println!("LUT address: {:?}", lut.0);

    let raw_account: Account = client.get_account(&lut_pubkey).unwrap();
    let address_lookup_table = AddressLookupTable::deserialize(&raw_account.data).unwrap();

    println!("Address lookup table: {:?}", address_lookup_table);

    let address_lookup_table_account = AddressLookupTableAccount {
        key: lut_pubkey,
        addresses: address_lookup_table.addresses.to_vec(),
    };

    println!("Instructions length : {:?}", instructions.len());

    let fund_tx = build_transaction(
        &client,
        &instructions,
        vec![&admin_kp],
        address_lookup_table_account.clone(),
        &admin_kp,
    );

    let size: usize = bincode::serialized_size(&fund_tx).unwrap() as usize;

    if size <= 1232 {
        jito.submit_bundle(vec![fund_tx], mint.pubkey(), None)
            .await
            .unwrap();
    } else {
        // Calculate number of transactions needed based on size
        let num_transactions = (1232 as f64 / 1232.0).ceil() as usize;
        let instructions_per_tx = instructions.len() / num_transactions;

        // Split instructions into chunks
        let mut chunks: Vec<Vec<Instruction>> = Vec::new();
        let mut start = 0;

        for i in 0..num_transactions {
            let end = if i == num_transactions - 1 {
                instructions.len() // Last chunk gets remaining instructions
            } else {
                start + instructions_per_tx
            };

            let chunk = instructions[start..end].to_vec();

            chunks.push(chunk);
            start = end;
        }

        let mut txs: Vec<VersionedTransaction> = Vec::new();
        // Build and send each transaction
        for chunk in chunks {
            let lut: AddressLookupTableAccount = address_lookup_table_account.clone();
            let tx = build_transaction(&client, &chunk, vec![&admin_kp], lut, &admin_kp);
            txs.push(tx);
        }
        test_transactions(&client, &txs).await;
        let _ = jito.submit_bundle(txs, mint.pubkey(), None).await.unwrap();
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

    //Step 5: Prepare mint instruction and buy instructions as well as tip instruction
    let mut txs_builder: BundleTransactions = BundleTransactions::new(
        admin_kp,
        dev_keypair_with_amount.keypair,
        mint,
        lut_pubkey,
        keypairs_with_amount,
        tip_account,
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
            let start_time = std::time::Instant::now();

            loop {
                if start_time.elapsed() > Duration::from_secs(120) {
                    println!("Timeout reached after 2 minutes, killing process");
                    return;
                }
                let live = pumpfun_client.is_token_live(&mint_pubkey).await;
                if live {
                    let late_txs = txs_builder.collect_rest_txs().await;
                    let late_txs_chunks: Vec<Vec<VersionedTransaction>> =
                        late_txs.chunks(5).map(|c| c.to_vec()).collect();

                    print!("We received {:?} late bundles", late_txs_chunks.len());

                    for chunk in late_txs_chunks {
                        // Only send first chunk for testing
                        let _ = jito
                            .submit_bundle(chunk, mint_pubkey, Some(&pumpfun_client))
                            .await
                            .unwrap();
                    }
                    break;
                } else {
                    sleep(Duration::from_millis(100));
                }
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
        "lut": lut_pubkey.to_string()
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
    println!("Bundle lut: {:?}", lut_pubkey);
    Ok(lut_pubkey)
}
