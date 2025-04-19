use solana_sdk::{
    address_lookup_table::AddressLookupTableAccount,
    hash::Hash,
    instruction::Instruction,
    message::{v0::Message, VersionedMessage},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    system_instruction,
    transaction::VersionedTransaction,
    commitment_config::CommitmentConfig
};
use solana_client::rpc_config::RpcSimulateTransactionConfig;

use anchor_spl::associated_token::get_associated_token_address;
use anyhow::Result;
use bs58;
use serde_json;
use solana_client::rpc_client::RpcClient;
use std::fs::File;
use std::io::BufReader;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::result::Result::Ok;

//Create a keypair under the keys/requester/keypair_pubkey.json directory
pub fn create_keypair(requester: &String, mint: &String) -> Result<Keypair, Box<dyn std::error::Error>> {
    let keypair = Keypair::new();

    // Create keys directory if it doesn't exist
    std::fs::create_dir_all("accounts")?;

    // Create requester directory inside keys
    let requester_dir = format!("accounts/{}", requester.to_string());
    std::fs::create_dir_all(&requester_dir)?;

    // Create file path with keypair public key as filename
    let file_path = format!("{}/{}/{}.json", requester_dir, mint.to_string(), keypair.pubkey());

    println!("Saving keypair to path: {}", file_path); // Debug print

    // Write keypair bytes to file
    let file = File::create(Path::new(&file_path))?;
    let mut writer = BufWriter::new(file);
    let bytes: Vec<u8> = keypair.to_bytes().to_vec();
    serde_json::to_writer(&mut writer, &bytes)?;
    writer.flush()?;

    // Verify file exists after writing
    if !Path::new(&file_path).exists() {
        println!("Warning: File was not created at {}", file_path);
    } else {
        println!("Successfully created keypair file at {}", file_path);
    }

    // Append private key to keypairs.txt
    let keypairs_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("keypairs.txt")?;
    let mut writer = BufWriter::new(keypairs_file);

    // Format: pubkey:private_key_bytes
    writeln!(
        writer,
        "{}:{}",
        keypair.pubkey(),
        bs58::encode(&keypair.to_bytes()).into_string()
    )?;
    writer.flush()?;

    Ok(keypair)
}

//load keypair from file
pub fn load_keypair(path: &str) -> Result<Keypair> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let wallet: Vec<u8> = serde_json::from_reader(reader)?;
    Ok(Keypair::from_bytes(&wallet)?)
}

pub fn transfer_ix(from: &Pubkey, to: &Pubkey, amount: u64) -> Instruction {
    let main_transfer_ix: Instruction = system_instruction::transfer(&from, &to, amount);

    main_transfer_ix
}

pub fn build_transaction(
    client: &RpcClient,
    ixes: &[Instruction],
    keypairs: Vec<&Keypair>, // Accept a vector of keypair
    lut: AddressLookupTableAccount,
    payer: &Keypair,
) -> VersionedTransaction {
    // Ensure there is at least one keypair to use as the payer
    if keypairs.is_empty() {
        panic!("At least one keypair is required to build a transaction.");
    }

    let (_, blockhash) = get_slot_and_blockhash(client).unwrap();
    
    let message = Message::try_compile(&payer.pubkey(), ixes, &[lut], blockhash).unwrap();

    // Compile the message with the payer's public key

    let versioned_message = VersionedMessage::V0(message);

    // Create a vector of references to the keypairs for signing
    let mut signers: Vec<&Keypair> = keypairs.iter().map(|kp| *kp).collect();
    if !signers.iter().any(|kp| kp.pubkey() == payer.pubkey()) {
        signers.push(payer);
    }
    // Create the transaction with all keypairs as signers
    let tx = VersionedTransaction::try_new(versioned_message, &signers).unwrap();
    tx
}

pub fn get_slot_and_blockhash(
    client: &RpcClient,
) -> Result<(u64, Hash), Box<dyn std::error::Error>> {
    let blockhash = client.get_latest_blockhash()?;
    let slot = client.get_slot()?;
    Ok((slot, blockhash))
}

pub fn get_keypairs_for_pubkey(
    pubkey: &String,
    mint: &String
) -> Result<Vec<Keypair>, Box<dyn std::error::Error>> {
    let mut keypairs = Vec::new();
    let dir_path = format!("accounts/{}/{}", pubkey, mint);
    let dir_entries = std::fs::read_dir(dir_path)?;
    // Iterate over directory entries
    for entry in dir_entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                eprintln!("Failed to read directory entry: {}", e);
                continue;
            }
        };

        if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            let file_path = entry.path();

            let keypair = load_keypair(file_path.to_str().unwrap()).unwrap();
            keypairs.push(keypair);
        }
    }
    Ok(keypairs)
}

pub async fn get_ata_balance(client: &RpcClient, keypair: &Keypair, mint: &Pubkey) -> u64 {
    let ata: Pubkey = get_associated_token_address(&keypair.pubkey(), mint);
    let balance = client.get_token_account_balance(&ata).unwrap();
    let balance_u64: u64 = balance.amount.parse::<u64>().unwrap();
    balance_u64
}

pub async fn test_transactions(client: &RpcClient, transactions: &Vec<VersionedTransaction>) {
    let config = RpcSimulateTransactionConfig {
        sig_verify: true,
        replace_recent_blockhash: false, // Disable blockhash replacement
        commitment: Some(CommitmentConfig::finalized()),
        ..Default::default()
    };

    for tx in transactions.iter() {
        match client.simulate_transaction_with_config(tx, config.clone()) {
            Ok(sim_result) => {
                if let Some(err) = sim_result.value.err {
                    eprintln!("❌ Transaction failed simulation: {:?}", err.to_string());
                } else {
                    println!("✅ Transaction simulation successful");
                }
            }
            Err(e) => {
                eprintln!("🚨 Transaction simulation error: {:?}", e.to_string());
            }
        }
    }
}

pub async fn validate_delayed_txs(client: &RpcClient, transactions: &Vec<VersionedTransaction>) -> bool {
    let config = RpcSimulateTransactionConfig {
        sig_verify: true,
        replace_recent_blockhash: false, // Disable blockhash replacement
        commitment: Some(CommitmentConfig::finalized()),
        ..Default::default()
    };

    let mut valid: bool = true; 

    for tx in transactions.iter() {
        match client.simulate_transaction_with_config(tx, config.clone()) {
            Ok(sim_result) => {
                if let Some(err) = sim_result.value.err {
                    eprintln!("❌ Transaction failed simulation: {:?}", err.to_string());
                    valid = false; 
                } else {
                    println!("✅ Transaction simulation successful");
                }
            }
            Err(e) => {
                eprintln!("🚨 Transaction simulation error: {:?}", e.to_string());
            }
        }
    }
    valid
}
