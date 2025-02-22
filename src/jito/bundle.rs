use std::{
    sync::Arc,
    collections::HashMap,
    env
};
use dotenv::dotenv;
use tokio::time::Duration;
use reqwest::Client as HttpClient;

use solana_sdk::{
    pubkey::Pubkey,
    transaction::VersionedTransaction,
    rent::Rent,
    signature::{Keypair, Signer},
    address_lookup_table::AddressLookupTableAccount,
    instruction::Instruction,
};

use solana_client::rpc_client::RpcClient;


use crate::jito::jito::JitoBundle;
use crate::pumpfun::pump::PumpFun;
use crate::config::{RPC_URL, FEE_AMOUNT, BUFFER_AMOUNT, JITO_TIP_AMOUNT, MAX_RETRIES, ORCHESTRATOR_URL};
use crate::params::{CreateTokenMetadata, KeypairWithAmount};
use crate::solana::{
    utils::{load_keypair, transfer_ix, build_transaction}, 
    lut::{create_lut, extend_lut, verify_lut_ready},
    helper::pack_instructions,
};

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

    let mut retries = 5;
    while retries > 0 {
        match verify_lut_ready(&client, &lut.0) {
            Ok(true) => break,
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

    //Extend lut with addresses 
    let extended_lut = extend_lut(&client, &admin_kp, lut.0, &keypairs_with_amount.iter().map(|keypair| keypair.keypair.pubkey()).collect::<Vec<Pubkey>>()).unwrap();


    println!("LUT extended with addresses: {:?}", extended_lut);
    //STEP 2: Transfer funds needed from admin to dev + keypairs in a bundle 

    println!("Amount of lamports to transfer to dev: {}", dev_keypair_with_amount.amount);
    let admin_to_dev_ix = transfer_ix(&admin_kp.pubkey(), &dev_keypair_with_amount.keypair.pubkey(), dev_keypair_with_amount.amount);
    let admin_to_keypair_ixs: Vec<Instruction> = keypairs_with_amount.iter().map(|keypair| transfer_ix(&admin_kp.pubkey(), &keypair.keypair.pubkey(), keypair.amount)).collect();
    let jito_tip_ix = jito.get_tip_ix(admin_kp.pubkey()).await.unwrap();

    //Instructions to send sol from admin to dev + keypairs 
    let mut instructions = admin_to_keypair_ixs;
    instructions.extend([admin_to_dev_ix, jito_tip_ix]);
    
    println!("LUT address: {:?}", lut.0);
    println!("Addresses: {:?}", keypairs_with_amount.iter().map(|keypair| keypair.keypair.pubkey()).collect::<Vec<Pubkey>>());

    let final_lut = AddressLookupTableAccount {
        key: lut.0,
        addresses: keypairs_with_amount.iter().map(|keypair| keypair.keypair.pubkey()).collect(),
    };

    let tx = build_transaction(&client, &instructions, vec![&admin_kp], final_lut);
    
    println!("Transaction built");
    //let signature = client.send_and_confirm_transaction_with_spinner(&tx).unwrap();

    //Sending transaction to fund wallets from admin. 
    //TODO: Check if this is complete. might require tip instruction, signature to tx, and confirmation that bundle is complete
    let _ = jito.one_tx_bundle(tx).await.unwrap();
    
    //Close lut - TODO add as side job to diminish waiting time for the user 
    //close_lut(&client, &self.admin_kp, lut.0);

    
    //Step 4: Create and extend lut for the bundle 

    let bundle_lut = create_lut(&client, &dev_keypair_with_amount.keypair).unwrap();
    let bundle_lut_pubkey = bundle_lut.0;
    let _ = extend_lut(&client, &dev_keypair_with_amount.keypair, bundle_lut.0, &keypairs_with_amount.iter().map(|keypair| keypair.keypair.pubkey()).collect::<Vec<Pubkey>>()).unwrap(); 

     // Verify LUT is ready before using
    let mut retries = 5;
    while retries > 0 {
        if verify_lut_ready(&client, &bundle_lut.0).unwrap(){
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        retries -= 1;
    }

    if retries == 0 {
       println!("LUT not ready after maximum retries");
    }

    let final_bundle_lut = AddressLookupTableAccount {
        key: bundle_lut.0,
        addresses: keypairs_with_amount.iter().map(|keypair| keypair.keypair.pubkey()).collect(),
    };

    //Step 5: Prepare mint instruction and buy instructions as well as tip instruction 

    let mut instructions: Vec<Instruction> = Vec::new();
    println!("Mint keypair: {:?}", mint.pubkey());

    let mint_ix = pumpfun_client.create_instruction(&mint, token_metadata).await.unwrap();

    instructions.push(mint_ix);

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
            None
            )
            .await
            .unwrap();
    
        instructions.extend(dev_buy_ixs);
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
            None
            )
            .await
            .unwrap();

        instructions.extend(buy_ixs);
    }

    //Step 6: Prepare tip instruction 

    let jito_tip_ix = jito.get_tip_ix(dev_keypair_with_amount.keypair.pubkey()).await.unwrap();
    instructions.push(jito_tip_ix);
    //Step 7: Bundle instructions into transactions
    
    let packed_txs = pack_instructions(instructions, &final_bundle_lut);
    println!("Packed transactions: {:?}", packed_txs.len());
    println!("Packed transactions. Needed keypairs for: {:?}", packed_txs[0].signers);
    println!("Packed transactions. Needed accounts for: {:?}", packed_txs[0].accounts);

    let mut transactions: Vec<VersionedTransaction> = Vec::new();
    // Create a map of pubkey to keypair for all possible signers
    let mut signers_map: HashMap<Pubkey, &Keypair> = HashMap::new();

    signers_map.insert(dev_keypair_with_amount.keypair.pubkey(), &dev_keypair_with_amount.keypair);

    for keypair in &keypairs_with_amount {
        signers_map.insert(keypair.keypair.pubkey(), &keypair.keypair);
    }
    
    signers_map.insert(mint.pubkey(), &mint);

    // Process each packed transaction
    for packed_tx in packed_txs {
        // Collect required signers' keypairs
        let mut tx_signers = Vec::new();
        for required_signer in &packed_tx.signers {
            if let Some(kp) = signers_map.get(required_signer) {
                println!("Adding signer: {:?}", kp.pubkey());
                tx_signers.push(*kp);
            } else {
                println!("Missing keypair for required signer: {:?}", required_signer);
                return Err("Missing required signer keypair".into());
            }
        }
        println!("Signers: {:?}", tx_signers.iter().map(|kp| kp.pubkey()).collect::<Vec<Pubkey>>());
        // Build the transaction with the collected signers
        let tx = build_transaction(
            &client,
            &packed_tx.instructions,
            tx_signers,
            final_bundle_lut.clone(),
        );
        transactions.push(tx);
    }

    // Send the bundle....
    println!("Attempting to submit bundle...");
    match jito.submit_bundle(transactions).await {
        Ok(_) => println!("Bundle submitted successfully"),
        Err(e) => {
            eprintln!("Failed to submit bundle: {:?}", e);
            return Err(e.to_string().into());
        }
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
    println!("Bundle lut: {:?}", bundle_lut_pubkey);
    Ok(bundle_lut_pubkey)
}
