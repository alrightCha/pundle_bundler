use std::fs;
use solana_sdk::message::{v0::Message, VersionedMessage};
use solana_sdk::{signer::Signer, system_instruction};
use solana_sdk::transaction::VersionedTransaction;
use solana_client::rpc_client::RpcClient;
use crate::config::RPC_URL;
use crate::solana::utils::{get_slot_and_blockhash, load_keypair}; // For parsing JSON files
use std::env;
use dotenv::dotenv;

pub async fn recursive_pay(from: String, mint: String, lamports: u64) -> Vec<String> {
    dotenv().ok();

    let admin_keypair_path = env::var("ADMIN_KEYPAIR").unwrap();
    let recipient = load_keypair(&admin_keypair_path).unwrap();

    let client = RpcClient::new(RPC_URL);
    let mut collected_signatures = Vec::new();
    let mut remaining_lamports = lamports;

    // Directory containing keypair JSON files
    let dir_path = format!("accounts/{}", from);

    // Read the directory
    let dir_entries = match fs::read_dir(&dir_path) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("Failed to read directory {}: {}", dir_path, e);
            return collected_signatures;
        }
    };

    // Iterate over directory entries
    for entry in dir_entries {
        // Break if we've collected enough lamports
        if remaining_lamports == 0 {
            break;
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

            println!("Balance: {}", balance);

            if balance < 2000000 {
                println!("Skipping refund for {}", keypair.pubkey());
                continue;
            }

            // Calculate transfer amount based on remaining lamports needed
            let available_transfer = balance - 2_000_000;
            let transfer_amount = remaining_lamports.min(available_transfer);

            let ix = system_instruction::transfer(
                &keypair.pubkey(),
                &recipient.pubkey(),
                transfer_amount,
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
                Ok(sig) => {
                    println!("Transfer successful for {} - Amount: {}", keypair.pubkey(), transfer_amount);
                    collected_signatures.push(sig.to_string());
                    remaining_lamports = remaining_lamports.saturating_sub(transfer_amount);
                }
                Err(e) => eprintln!("Failed to send transaction: {}", e),
            }
        }
    }

    collected_signatures
}