use anchor_spl::associated_token::get_associated_token_address;
use reqwest::Client as HttpClient;

use solana_sdk::{
    address_lookup_table::AddressLookupTableAccount,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::VersionedTransaction,
};

use super::help::BundleTransactions;
use crate::jito::jito::JitoBundle;
use crate::params::KeypairWithAmount;
use crate::pumpfun::pump::PumpFun;
use crate::solana::lut::create_lut;
use crate::solana::utils::validate_delayed_txs;
use crate::warmup::token_manager::TokenManager;
use crate::{
    config::{JITO_TIP_AMOUNT, MAX_RETRIES, ORCHESTRATOR_URL, RPC_URL},
    solana::utils::get_admin_keypair,
};
use pumpfun_cpi::instruction::Create;
use solana_client::rpc_client::RpcClient;
use solana_sdk::address_lookup_table::state::AddressLookupTable;
use std::{sync::Arc, thread::sleep};
use tokio::time::Duration;

pub async fn process_bundle(
    keypairs_with_amount: Vec<KeypairWithAmount>,
    dev_keypair_with_amount: KeypairWithAmount,
    mint: &Keypair,
    token_metadata: Create,
    priority_fee: u64,
    jito_fee: u64,
    with_delay: bool,
) -> Result<Pubkey, Box<dyn std::error::Error + Send + Sync>> {
    let admin_kp = get_admin_keypair();

    let client = RpcClient::new(RPC_URL);

    let jito = JitoBundle::new(MAX_RETRIES, JITO_TIP_AMOUNT);

    let payer: Arc<Keypair> = Arc::new(admin_kp.insecure_clone());

    let pumpfun_client = PumpFun::new(payer);

    let mut pubkeys_for_lut: Vec<Pubkey> = Vec::new();

    let tip_account: Pubkey = jito.get_tip_account().await;

    pubkeys_for_lut.push(admin_kp.pubkey());

    //Adding other addresses to lut
    let extra_addresses: Vec<Pubkey> = pumpfun_client.get_addresse_for_lut(&mint.pubkey()).await;
    pubkeys_for_lut.extend(extra_addresses);
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

    let mut all_transfers: Vec<KeypairWithAmount> = Vec::new();

    let dev = KeypairWithAmount {
        keypair: dev_keypair_with_amount.keypair.insecure_clone(),
        amount: dev_keypair_with_amount.amount.clone(),
    };

    all_transfers.push(dev);

    for wallet in keypairs_with_amount.iter() {
        let new_transfer = KeypairWithAmount {
            keypair: wallet.keypair.insecure_clone(),
            amount: wallet.amount.clone(),
        };
        all_transfers.push(new_transfer);
    }

    let mut shadow_manager = TokenManager::new();
    //Init hashmaps and wsol holding for admin
    shadow_manager.init_alloc_ixs(&all_transfers).await;

    //Extend LUT with rest of addresses
    let extension: Vec<Pubkey> = shadow_manager.get_lut_extension();
    pubkeys_for_lut.extend(extension);

    //Create & Extend LUT
    let lut_pubkey: Pubkey = create_lut(&client, &admin_kp, &pubkeys_for_lut)
        .await
        .unwrap();

    //Ensure LUT is ready
    sleep(Duration::from_secs(5));

    //Build LUT
    let raw_account = client.get_account(&lut_pubkey).unwrap();
    let address_lookup_table = AddressLookupTable::deserialize(&raw_account.data).unwrap();
    let address_lookup_table_account = AddressLookupTableAccount {
        key: lut_pubkey,
        addresses: address_lookup_table.addresses.to_vec(),
    };

    //BUY MEMES; SWAP TO WSOL TO RECIPIENTS; CLOSE WSOL FROM HOP ACCOUNTS TO TOKEN BUYERS
    shadow_manager
        .shadow_bundle(&address_lookup_table_account)
        .await;

    //Step 5: Prepare mint instruction and buy instructions as well as tip instruction
    let mut txs_builder: BundleTransactions = BundleTransactions::new(
        admin_kp,
        dev_keypair_with_amount.keypair,
        mint,
        address_lookup_table_account,
        keypairs_with_amount,
        tip_account,
        with_delay,
        priority_fee,
        jito_fee,
    );

    //Submitting first bundle
    let first_bundle: Vec<Vec<Instruction>> =
        txs_builder.collect_first_bundle_txs(token_metadata).await;

    let delayed_bundle_ixs: Vec<Vec<Instruction>> = txs_builder.collect_rest_txs().await;
    let first_txs = txs_builder.get_txs(&first_bundle, true);

    if txs_builder.has_delayed_bundle() {
        let mint_pubkey = mint.pubkey();
        let admin_kp = get_admin_keypair();
        let payer: Arc<Keypair> = Arc::new(admin_kp);
        let pumpfun_client = PumpFun::new(payer);
        let jito = JitoBundle::new(MAX_RETRIES, JITO_TIP_AMOUNT);

        tokio::spawn(async move {
            let start_time = std::time::Instant::now();

            loop {
                if start_time.elapsed() > Duration::from_secs(120) {
                    println!("Timeout reached after 2 minutes, killing process");
                    return;
                }

                let dev_balance = client.get_token_account_balance(&dev_ata_pubkey);
                if let Ok(dev_balance) = dev_balance {
                    if let Some(dev_bal) = dev_balance.ui_amount {
                        if dev_bal > 0.0 {
                            let late_txs = txs_builder.get_txs(&delayed_bundle_ixs, false);
                            let late_txs_chunks: Vec<Vec<VersionedTransaction>> =
                                late_txs.chunks(5).map(|c| c.to_vec()).collect();
                            print!("We received {:?} late bundles", late_txs_chunks.len());
                            //Implement retry here by checking balance of latest bundle addresses
                            for chunk in late_txs_chunks {
                                // Only send first chunk for testing
                                let _ = jito
                                    .submit_bundle(chunk, mint_pubkey, Some(&pumpfun_client))
                                    .await
                                    .unwrap();
                            }
                        } else {
                            sleep(Duration::from_millis(100));
                        }
                    }
                }
            }
        });
    }

       // Submit first bundle with retries
       let mut success = false;
       for retry in 1..=3 {
           match jito
               .submit_bundle(first_txs.clone(), mint.pubkey(), Some(&pumpfun_client))
               .await
           {
               Ok(_) => {
                   success = true;
                   break;
               }
               Err(error) => {
                   println!("Error submitting first bundle, retry {}/3", retry);
                   println!("Error: {}", error.to_string());
                   if retry < 3 {
                       sleep(Duration::from_secs(1));
                   }
               }
           }
       }
   
       if !success {
           println!("Failed to submit first bundle after 3 retries");
       }


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
