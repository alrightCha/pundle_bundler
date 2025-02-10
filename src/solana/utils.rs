use solana_sdk::{
    address_lookup_table::AddressLookupTableAccount,
    hash::Hash, 
    instruction::Instruction,
    message::{v0::Message, VersionedMessage},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    transaction::VersionedTransaction,
    system_instruction
};
use solana_client::rpc_client::RpcClient;
use std::result::Result::Ok;
use anyhow::Result;

use serde_json;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::io::{BufWriter, Write};

//Create a keypair under the keys/requester/keypair_pubkey.json directory
pub fn create_keypair(requester: &String) -> Result<Keypair, Box<dyn std::error::Error>> {
    let keypair = Keypair::new();

    // Create keys directory if it doesn't exist
    std::fs::create_dir_all("accounts")?;
    
    // Create requester directory inside keys
    let requester_dir = format!("accounts/{}", requester.to_string());
    std::fs::create_dir_all(&requester_dir)?;

    // Create file path with keypair public key as filename
    let file_path = format!("{}/{}.json", requester_dir, keypair.pubkey());
    
    println!("Saving keypair to path: {}", file_path);  // Debug print
    
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
    let main_transfer_ix: Instruction = system_instruction::transfer(
        &from,
        &to,
        amount,
    );

    main_transfer_ix
}

pub fn build_transaction(
    client: &RpcClient,
    ixes: &[Instruction],
    keypairs: Vec<&Keypair>, // Accept a vector of keypairs
    lut: AddressLookupTableAccount,
) -> VersionedTransaction {
    // Ensure there is at least one keypair to use as the payer
    if keypairs.is_empty() {
        panic!("At least one keypair is required to build a transaction.");
    }

    // Use the first keypair as the payer
    let payer = keypairs[0];

    let (_, blockhash) = get_slot_and_blockhash(client).unwrap();
    
    // Compile the message with the payer's public key
    let message = Message::try_compile(
        &payer.pubkey(),
        ixes,
        &[lut],
        blockhash,
    ).unwrap();
    
    let versioned_message = VersionedMessage::V0(message);

    // Create a vector of references to the keypairs for signing
    let signers: Vec<&Keypair> = keypairs.iter().map(|kp| *kp).collect();
    // Create the transaction with all keypairs as signers
    let tx = VersionedTransaction::try_new(versioned_message, &signers).unwrap();

    tx
}

pub fn get_slot_and_blockhash(client: &RpcClient) -> Result<(u64, Hash), Box<dyn std::error::Error>> {
    let blockhash = client.get_latest_blockhash()?;
    let slot = client.get_slot()?;
    Ok((slot, blockhash))
}
//TODO: Add a function that receives instructions and returns a an array of transactions, 
//Make sure the transactions take as many instructions as possible to be efficient 

