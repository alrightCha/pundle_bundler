use std::fs;
use std::str::FromStr;
use solana_sdk::{signer::Signer, system_instruction};
use solana_sdk::instruction::Instruction;
use solana_sdk::signer::keypair::Keypair;
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use crate::config::RPC_URL;
use jito_sdk_rust::JitoJsonRpcSDK;
use crate::solana::utils::{get_slot_and_blockhash, load_keypair}; // For parsing JSON files
use std::env;
use dotenv::dotenv;
use solana_sdk::transaction::Transaction;
use serde_json::json;

pub async fn recursive_pay(from: String, mint: String, lamports: Option<u64>, with_admin_transfer: bool) -> bool {
    dotenv().ok();

    let client = RpcClient::new(RPC_URL);
    let jito_sdk = JitoJsonRpcSDK::new("https://mainnet.block-engine.jito.wtf/api/v1", None);

    let admin_keypair_path = env::var("ADMIN_KEYPAIR").unwrap();

    let recipient: Keypair = load_keypair(&admin_keypair_path).unwrap();
    let user_recipient: Pubkey = Pubkey::from_str(&from).unwrap();

    let final_recipient: Pubkey = match with_admin_transfer {
        true => recipient.pubkey(),
        false => user_recipient,
    };

    let mut remaining_lamports = lamports;
    let mut total_available: u64 = 0;

    // Directory containing keypair JSON files
    let dir_path = format!("accounts/{}/{}", from, mint);

    // Read the directory
    let dir_entries = match fs::read_dir(&dir_path) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("Failed to read directory {}: {}", dir_path, e);
            return false;
        }
    };


    //Collecting the instructions for the transactions 
    let mut ixs: Vec<Instruction> = Vec::new();
    let mut signers: Vec<Keypair> = Vec::new();

    let mut total_wallets_balance = 0;  
    let mut wallet_count = 0; 
    // Iterate over directory entries
    for entry in dir_entries {
        // Break if we've collected enough lamports (only when a specific amount is requested)
        if let Some(remaining) = remaining_lamports {
            if remaining == 0 {
                break;
            }
        }

        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                eprintln!("Failed to read directory entry: {}", e);
                continue;
            }
        };

        // Check if the entry is a file
        if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            let file_path = entry.path();

            let keypair = load_keypair(file_path.to_str().unwrap()).unwrap();

            //skipping if the keypair is the mint 
            if mint == keypair.pubkey().to_string() {
                continue;
            }

            println!("Processing keypair: {:?}", keypair.pubkey());

            let balance = match client.get_balance(&keypair.pubkey()) {
                Ok(balance) => balance,
                Err(e) => {
                    eprintln!("Failed to get balance for keypair {}: {}", keypair.pubkey(), e);
                    continue;
                }
            };

            total_wallets_balance += balance;
            wallet_count += 1;
            println!("Balance: {}", balance);

            if balance < 2000000 {
                println!("Skipping refund for {}", keypair.pubkey());
                continue;
            }

            // Calculate transfer amount based on remaining lamports needed or maximum available
            let available_transfer = balance - 2_000_000;
            
            let transfer_amount = match remaining_lamports {
                Some(remaining) => remaining.min(available_transfer),
                None => available_transfer,
            };

            // Update remaining_lamports if we're collecting a specific amount
            if let Some(remaining) = remaining_lamports.as_mut() {
                *remaining = remaining.saturating_sub(transfer_amount);
            }
            
            total_available += transfer_amount;

            //Creating instruction to transfer the amount to the recipient
            let ix = system_instruction::transfer(
                &keypair.pubkey(),
                &final_recipient,
                transfer_amount,
            );

            //Adding the instruction and signer to the vectors 
            ixs.push(ix);
            signers.push(keypair);
        }
    }

    signers.push(recipient.insecure_clone());

    //Check if we have enough lamports when a specific amount was requested
    if let Some(remaining) = remaining_lamports {
        if remaining > 0 {
            println!("Remaining lamports needed: {}", remaining);
            println!("Total available: {}", total_available);
            println!("Required instructions: {}", ixs.len());
            return false;
        }
    }

    //Gaurd to avoid spam which results in partial loss of funds due to fees
    if !with_admin_transfer {
        println!("Total wallets balance: {}", total_wallets_balance);
        let min_balance_for_transfer = 3000000 * wallet_count as u64;
        if total_wallets_balance < min_balance_for_transfer {
            println!("Not enough balance to transfer");
            return false;
        }
    }

    //Creating instruction to transfer the tip amount to the jito tip account
    let random_tip_account = jito_sdk.get_random_tip_account().await.unwrap();
    let jito_tip_account = Pubkey::from_str(&random_tip_account).unwrap();

    let jito_tip_ix = system_instruction::transfer(
        &recipient.pubkey(),
        &jito_tip_account,
        2000000,
    );

    //Adding the instruction to the vector 
    ixs.push(jito_tip_ix);

    //Getting the blockhash
    let (_, blockhash) = match get_slot_and_blockhash(&client) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("Failed to get blockhash: {}", e);
            return false;
        }
    };

    

        // Create a transaction
    let mut transaction = Transaction::new_with_payer(
        &ixs,
        Some(&recipient.pubkey()),
    );

    let signers: Vec<&Keypair> = signers.iter().map(|k| k).collect();

    transaction.sign(&signers, blockhash);

    // Serialize the transaction
    let serialized_tx = bs58::encode(bincode::serialize(&transaction).unwrap()).into_string();
    
    // Prepare bundle for submission (array of transactions)
    let bundle = json!([serialized_tx]);

    // UUID for the bundle
    let uuid = None;

    // Send bundle using Jito SDK
    println!("Sending bundle with 1 transaction...");
    let _ = jito_sdk.send_bundle(Some(bundle), uuid).await.unwrap();

    true
}