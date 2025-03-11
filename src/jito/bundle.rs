use std::{
    sync::Arc,
    env
};
use dotenv::dotenv;
use tokio::time::Duration;
use reqwest::Client as HttpClient;
use solana_sdk::{
    account::Account,
    address_lookup_table::AddressLookupTableAccount, 
    instruction::Instruction, 
    pubkey::Pubkey, 
    signature::{Keypair, Signer},
    address_lookup_table::state::AddressLookupTable
};

use solana_client::rpc_client::RpcClient;
use crate::jito::jito::JitoBundle;
use crate::pumpfun::pump::PumpFun;
use crate::config::{RPC_URL, JITO_TIP_AMOUNT, MAX_RETRIES, ORCHESTRATOR_URL};
use crate::params::{CreateTokenMetadata, KeypairWithAmount};
use crate::solana::{
    utils::{load_keypair, transfer_ix, build_transaction}, 
    lut::{create_lut, extend_lut, verify_lut_ready},
};

use super::help::build_bundle_txs;

pub async fn process_bundle(
    keypairs_with_amount: Vec<KeypairWithAmount>,
    dev_keypair_with_amount: KeypairWithAmount,
    mint: &Keypair,
    requester_pubkey: String,
    token_metadata: CreateTokenMetadata
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

    let dev_keypair_path = format!("accounts/{}/{}.json", requester_pubkey, dev_keypair_with_amount.keypair.pubkey());

    let loaded_dev_keypair = load_keypair(&dev_keypair_path).unwrap();

    let payer: Arc<Keypair> = Arc::new(loaded_dev_keypair);

    let pumpfun_client = PumpFun::new(payer);

    println!("Creating lut with admin public key:  {}", admin_kp.pubkey());
    println!("Keypairs with amount: {:?}", keypairs_with_amount);
    
    println!("Admin keypair balance: {}", client.get_balance(&admin_kp.pubkey()).unwrap_or(0));
    println!("Attempting to create LUT...");

    let lut: (solana_sdk::pubkey::Pubkey, solana_sdk::signature::Signature) = create_lut(&client, &admin_kp).unwrap();

    let lut_pubkey = lut.0;

    let mut retries = 5;
    while retries > 0 {
        match verify_lut_ready(&client, &lut.0) {
            Ok(true) => {
                println!("LUT is ready");
                break;
            },
            Ok(false) => {
                tokio::time::sleep(Duration::from_millis(500)).await;
                retries -= 1;
            },
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

    //Create atas for the keypairs
    let mut ata_ixs: Vec<Instruction> = Vec::new();

    for keypair in keypairs_with_amount.iter() {
        let ix = pumpfun_client.create_ata(&keypair.keypair.pubkey(), &mint.pubkey());
        ata_ixs.push(ix);
        pubkeys_for_lut.push(keypair.keypair.pubkey());
        let ata_pubkey = pumpfun_client.get_ata(&keypair.keypair.pubkey(), &mint.pubkey());
        println!("ATA pubkey: {:?}", ata_pubkey);
        pubkeys_for_lut.push(ata_pubkey);
    }

    //Extend lut with addresses & attached token accounts
    let extended_lut = extend_lut(&client, &admin_kp, lut.0, &pubkeys_for_lut).unwrap();

    println!("LUT extended with addresses: {:?}", extended_lut);
    //STEP 2: Transfer funds needed from admin to dev + keypairs in a bundle 

    println!("Amount of lamports to transfer to dev: {}", dev_keypair_with_amount.amount);
    let admin_to_dev_ix = transfer_ix(&admin_kp.pubkey(), &dev_keypair_with_amount.keypair.pubkey(), dev_keypair_with_amount.amount);
    let admin_to_keypair_ixs: Vec<Instruction> = keypairs_with_amount.iter().map(|keypair| transfer_ix(&admin_kp.pubkey(), &keypair.keypair.pubkey(), keypair.amount)).collect();
    let jito_tip_ix = jito.get_tip_ix(admin_kp.pubkey()).await.unwrap();

    //Instructions to send sol from admin to dev + keypairs 
    let mut instructions: Vec<Instruction> = vec![admin_to_dev_ix];
    instructions.extend(admin_to_keypair_ixs);
    instructions.push(jito_tip_ix);
    
    println!("LUT address: {:?}", lut.0);
    println!("Addresses: {:?}", keypairs_with_amount.iter().map(|keypair| keypair.keypair.pubkey()).collect::<Vec<Pubkey>>());

    let raw_account: Account = client.get_account(&lut_pubkey).unwrap();
    let address_lookup_table = AddressLookupTable::deserialize(&raw_account.data).unwrap();

    println!("Address lookup table: {:?}", address_lookup_table);

    let address_lookup_table_account = AddressLookupTableAccount {
        key: lut_pubkey,
        addresses: address_lookup_table.addresses.to_vec(),
    };

    let tx = build_transaction(&client, &instructions, &vec![&admin_kp], address_lookup_table_account.clone());
    println!("Transaction built");
    //let signature = client.send_and_confirm_transaction_with_spinner(&tx).unwrap();

    //Sending transaction to fund wallets from admin. 
    //TODO: Check if this is complete. might require tip instruction, signature to tx, and confirmation that bundle is complete
    let _ = jito.one_tx_bundle(tx).await.unwrap();

    tokio::time::sleep(Duration::from_secs(20)).await; //Sleep for 20 seconds to ensure that lut extended + that addresses have their sol received 
    
    //Step 3.5 -> Create accounts for the keypairs 
    let tip_ix = jito.get_tip_ix(admin_kp.pubkey()).await.unwrap();
    ata_ixs.push(tip_ix);
    println!("Beginning creation of attached token accounts... {}", ata_ixs.len());
    let atas_tx = build_transaction(&client, &ata_ixs, &vec![&admin_kp, &dev_keypair_with_amount.keypair], address_lookup_table_account.clone());
    let _ = jito.one_tx_bundle(atas_tx).await.unwrap();
    println!("Attached token accounts created");

    //Step 4: Create and extend lut for the bundle 
    let other_balances = keypairs_with_amount.iter().map(|keypair| client.get_balance(&keypair.keypair.pubkey()).unwrap()).collect::<Vec<u64>>();
    println!("Other balances: {:?}", other_balances);


    //Step 5: Prepare mint instruction and buy instructions as well as tip instruction 
    let mint_pubkey = mint.pubkey();

    let transactions = build_bundle_txs(dev_keypair_with_amount, mint, keypairs_with_amount, lut_pubkey, mint_pubkey, token_metadata).await;
    // Send the bundle....
    println!("Attempting to submit bundle...");

    let _ = jito.submit_bundle(transactions, mint.pubkey(), Some(&pumpfun_client)).await.unwrap();

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
