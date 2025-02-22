use std::fs;
use std::str::FromStr;
use solana_sdk::message::{v0::Message, VersionedMessage};
use solana_sdk::{signer::Signer, system_instruction};
use solana_sdk::transaction::VersionedTransaction;
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use crate::config::RPC_URL;
use crate::solana::utils::{get_slot_and_blockhash, load_keypair}; // For parsing JSON files

pub async fn refund_keypairs(from: String, recipient: String, mint: String) {
    println!("Refunding keypairs");
    let client = RpcClient::new(RPC_URL);
    let recipient = Pubkey::from_str(&recipient).unwrap();

    // Directory containing keypair JSON files
    let dir_path = format!("accounts/{}", from);

    // Read the directory
    let dir_entries = match fs::read_dir(&dir_path) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("Failed to read directory {}: {}", dir_path, e);
            return;
        }
    };

    // Iterate over directory entries
    for entry in dir_entries {
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

            if mint == keypair.pubkey().to_string() {
                continue;
            }

            println!("Keypair: {:?}", keypair.pubkey());

            // Process the keypair (refund logic)
            let balance = match client.get_balance(&keypair.pubkey()) {
                Ok(balance) => balance,
                Err(e) => {
                    eprintln!("Failed to get balance for keypair {}: {}", keypair.pubkey(), e);
                    continue;
                }
            };

            println!("Balance: {}", balance);

            if balance < 2000000 {
                println!("Skipping refund for {}", keypair.pubkey());
                continue;
            }

            let ix = system_instruction::transfer(
                &keypair.pubkey(),
                &recipient,
                balance - 2000000,
            );

            let (_, blockhash) = match get_slot_and_blockhash(&client) {
                Ok(result) => result,
                Err(e) => {
                    eprintln!("Failed to get blockhash: {}", e);
                    continue;
                }
            };

            let message = match Message::try_compile(
                &keypair.pubkey(),
                &[ix],
                &[],
                blockhash,
            ) {
                Ok(message) => message,
                Err(e) => {
                    eprintln!("Failed to compile message: {}", e);
                    continue;
                }
            };

            let versioned_message = VersionedMessage::V0(message);

            let tx = match VersionedTransaction::try_new(versioned_message, &[&keypair]) {
                Ok(tx) => tx,
                Err(e) => {
                    eprintln!("Failed to create transaction: {}", e);
                    continue;
                }
            };

            match client.send_and_confirm_transaction_with_spinner(&tx) {
                Ok(_) => println!("Refund successful for {}", keypair.pubkey()),
                Err(e) => eprintln!("Failed to send transaction: {}", e),
            }
        }
    }
}