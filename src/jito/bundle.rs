use std::{
    sync::Arc,
    collections::{HashMap, HashSet},
    env
};
use dotenv::dotenv;
use tokio::time::Duration;
use reqwest::Client as HttpClient;
use solana_sdk::{
    account::Account, address_lookup_table::AddressLookupTableAccount, instruction::Instruction, pubkey::Pubkey, rent::Rent, signature::{Keypair, Signer}, transaction::VersionedTransaction
};

use solana_client::rpc_client::RpcClient;
use solana_sdk::address_lookup_table::state::AddressLookupTable;

use crate::jito::jito::JitoBundle;
use crate::pumpfun::pump::PumpFun;
use crate::config::{RPC_URL, FEE_AMOUNT, BUFFER_AMOUNT, JITO_TIP_AMOUNT, MAX_RETRIES, ORCHESTRATOR_URL};
use crate::params::{CreateTokenMetadata, KeypairWithAmount};
use crate::solana::{
    utils::{load_keypair, transfer_ix, build_transaction}, 
    lut::{create_lut, extend_lut, verify_lut_ready},
};
use crate::solana::helper::pack_instructions;
use solana_client::rpc_config::RpcSimulateTransactionConfig;
use solana_sdk::commitment_config::CommitmentConfig;

pub async fn process_bundle(
    keypairs_with_amount: Vec<KeypairWithAmount>,
    dev_keypair_with_amount: KeypairWithAmount,
    mint: Keypair,
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
    let mut pumpfun_client = PumpFun::new(payer);
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

    let rest_pbks: Vec<Pubkey> = keypairs_with_amount.iter().map(|keypair| keypair.keypair.pubkey()).collect();
    pubkeys_for_lut.extend(rest_pbks);

    //Extend lut with addresses 
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

    let tx = build_transaction(&client, &instructions, vec![&admin_kp], address_lookup_table_account.clone());
    println!("Transaction built");
    //let signature = client.send_and_confirm_transaction_with_spinner(&tx).unwrap();

    //Sending transaction to fund wallets from admin. 
    //TODO: Check if this is complete. might require tip instruction, signature to tx, and confirmation that bundle is complete
    let _ = jito.one_tx_bundle(tx).await.unwrap();

    //Step 4: Create and extend lut for the bundle 
    
    let other_balances = keypairs_with_amount.iter().map(|keypair| client.get_balance(&keypair.keypair.pubkey()).unwrap()).collect::<Vec<u64>>();
    println!("Other balances: {:?}", other_balances);


    tokio::time::sleep(Duration::from_secs(20)).await; //Sleep for 20 seconds to ensure that lut extended + that addresses have their sol received 
    //Step 5: Prepare mint instruction and buy instructions as well as tip instruction 

    let mut bundle_ixs: Vec<Instruction> = Vec::new();

    println!("Mint keypair: {:?}", mint.pubkey());

    let mint_ix = pumpfun_client.create_instruction(&mint, token_metadata).await.unwrap();

    bundle_ixs.push(mint_ix);

    //calculating the max amount of lamports to buy with 
    let rent = Rent::default();
    let rent_exempt_min = rent.minimum_balance(0);

    let to_subtract: u64 = rent_exempt_min + FEE_AMOUNT + BUFFER_AMOUNT;
   
    let to_sub_for_dev: u64 = to_subtract.clone() + JITO_TIP_AMOUNT;

    let final_dev_buy_amount = dev_keypair_with_amount.amount - to_sub_for_dev;

    println!("Final dev buy amount: {:?}", final_dev_buy_amount);

    let balance = client.get_balance(&dev_keypair_with_amount.keypair.pubkey()).unwrap();

    if balance > to_sub_for_dev {
        let dev_buy_ixs = pumpfun_client.buy_ixs(
            &mint.pubkey(),
            &dev_keypair_with_amount.keypair, 
            final_dev_buy_amount, 
            None,
            true
            )
            .await
            .unwrap();

        bundle_ixs.extend(dev_buy_ixs);
    }else{
        println!("Dev keypair has insufficient balance. Skipping buy.");
    }

    for keypair in keypairs_with_amount.iter() {
        let balance = client.get_balance(&keypair.keypair.pubkey()).unwrap();
        if balance < keypair.amount {
            println!("Keypair {} has insufficient balance. Skipping buy.", keypair.keypair.pubkey());
            continue;
        }
        let mint_pubkey: &Pubkey = &mint.pubkey();

        let final_buy_amount = keypair.amount - to_subtract;
        println!("Final buy amount: {:?}", final_buy_amount);
        let buy_ixs = pumpfun_client.buy_ixs(
            mint_pubkey,
            &keypair.keypair, 
            final_buy_amount, 
            None,
            true
            )
            .await
            .unwrap();

        bundle_ixs.extend(buy_ixs);
    }

    //Step 6: Prepare tip instruction 
    let tip_ix = jito.get_tip_ix(dev_keypair_with_amount.keypair.pubkey()).await.unwrap();
    instructions.push(tip_ix);
    let packed_txs = pack_instructions(bundle_ixs, &address_lookup_table_account);

    println!("Packed transactions: {:?}", packed_txs.len());
    println!("Packed transactions. Needed keypairs for: {:?}", packed_txs[0].signers);
    println!("Packed transactions. Needed accounts for: {:?}", packed_txs[0].accounts);

    //Inserting signers into the hashmap 
    let mut transactions: Vec<VersionedTransaction> = Vec::new();
    // Create a map of pubkey to keypair for all possible signers
    let mut signers_map: HashMap<Pubkey, &Keypair> = HashMap::new();

    signers_map.insert(dev_keypair_with_amount.keypair.pubkey(), &dev_keypair_with_amount.keypair);

    for keypair in &keypairs_with_amount {
        signers_map.insert(keypair.keypair.pubkey(), &keypair.keypair);
    }

    signers_map.insert(admin_kp.pubkey(), &admin_kp);
    signers_map.insert(mint.pubkey(), &mint);

    //Step 7: Bundle instructions into transactions

    // Process each packed transaction
    for (i, packed_tx) in packed_txs.iter().enumerate() {
        // Collect required signers' keypairs
        let mut tx_signers: Vec<&Keypair> = Vec::new();
        // Use a HashSet to deduplicate signers
        let mut unique_signers = HashSet::new();

        for ix in &packed_tx.instructions {
            for acc in ix.accounts.iter().filter(|acc| acc.is_signer) {
                unique_signers.insert(acc.pubkey);
            }
        }

        // Get keypairs for unique signers
        for signer in unique_signers {
            if let Some(kp) = signers_map.get(&signer) {
                tx_signers.push(kp);
            } else {
                println!("Missing keypair for required signer: {:?}", signer);
            }
        }

        // Build the transaction with the collected signers
        let tx = build_transaction(
            &client,
            &packed_tx.instructions,
            tx_signers.clone(),
            address_lookup_table_account.clone(),
        );

        let size: usize = bincode::serialized_size(&tx).unwrap() as usize;

        println!("Taking care of transaction {}", i);
        println!("Signers: {:?}", tx_signers.iter().map(|kp| kp.pubkey()).collect::<Vec<Pubkey>>());
        println!("Transaction size: {}", size);
        transactions.push(tx);
    }

    let config = RpcSimulateTransactionConfig {
        sig_verify: true,
        replace_recent_blockhash: false, // Disable blockhash replacement
        commitment: Some(CommitmentConfig::finalized()),
        ..Default::default()
    };

    for (i, tx) in transactions.iter().enumerate() {
        println!("Simulating transaction {}", i);
        match client.simulate_transaction_with_config(tx, config.clone()) {
            Ok(sim_result) => {
                if let Some(err) = sim_result.value.err {
                    eprintln!("❌ Transaction {} failed simulation: {:?}", i, err);
                } else {
                    println!("✅ Transaction {} simulation successful", i);
                }
            }
            Err(e) => {
                eprintln!("🚨 Transaction {} simulation error: {:?}", i, e);
            }
        }
    }

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
