mod config;
mod handlers;
mod jito;
mod jupiter;
mod params;
mod pumpfun;
mod solana;

use pumpfun_cpi::instruction::Create;
use anchor_spl::associated_token::spl_associated_token_account;
use config::{BUFFER_AMOUNT, FEE_AMOUNT, JITO_TIP_AMOUNT, MAX_RETRIES, RPC_URL, MAX_TX_PER_BUNDLE, JITO_TIP_SIZE};
use dotenv::dotenv;
use jito::jito::JitoBundle;
use params::{CreateTokenMetadata, KeypairWithAmount};
use pumpfun::pump::PumpFun;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana::lut::extend_lut;
use solana::{
    refund::refund_keypairs,
    utils::{build_transaction, load_keypair},
};
use solana_client::{rpc_client::RpcClient, rpc_config::RpcSimulateTransactionConfig};
use solana_sdk::{
    address_lookup_table::state::AddressLookupTable,
    address_lookup_table::AddressLookupTableAccount,
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    sysvar::rent::Rent,
    transaction::VersionedTransaction,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    str::FromStr,
    sync::Arc,
};

fn filtering(mint: &Keypair, dev: &Keypair, all: Vec<&Keypair>) -> Vec<Keypair> {
    fn to_remove(to_verif: &Keypair, kp1: &Keypair, kp2: &Keypair) -> bool {
        to_verif.pubkey() != kp1.pubkey() && to_verif.pubkey() != kp2.pubkey()
    }
    let buying_keypairs: Vec<Keypair> = all
        .iter()
        .filter(|keypair| to_remove(*keypair, &mint, &dev))
        .map(|kp| kp.insecure_clone())
        .collect();
    buying_keypairs
}

fn popper(amounts: &mut Vec<u64>) -> u64 {
    amounts.pop().unwrap_or_else(|| 0)
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    let admin_keypair_path = "./funding.json";
    println!("Path: {}", admin_keypair_path);
    let admin = load_keypair(&admin_keypair_path).unwrap();
    let admin_keypair = load_keypair(&admin_keypair_path).unwrap();

    println!("Admin keypair loaded: {}", admin_keypair.pubkey());
    //refund_keypairs("".to_string(), admin_keypair.pubkey().to_string(), "".to_string()).await;

    let client = RpcClient::new(RPC_URL);

    let jito_rpc = RpcClient::new_with_commitment(
        RPC_URL.to_string(),
        solana_sdk::commitment_config::CommitmentConfig::confirmed(),
    );

    let jito = JitoBundle::new(jito_rpc, MAX_RETRIES, JITO_TIP_AMOUNT);

    let dev_pubkey = "p3tUtcdaNepwZjho5U2RXphoovt84ncDyLYqiqNhHaq";
    let mint_pubkey = "pdLrsvJsX4qNjyxV9zYuZ3jcEvaFyka2R9wUEKGQiWg";

    let dev_keypair_path = format!("accounts/{}.json", &dev_pubkey);
    let mint_keypair_path = format!("accounts/{}.json", &mint_pubkey);

    let dev: Keypair = load_keypair(&dev_keypair_path).unwrap();
    let dev_keypair: Keypair = load_keypair(&dev_keypair_path).unwrap();
    let mint_keypair: Keypair = load_keypair(&mint_keypair_path).unwrap();

    let payer: Arc<Keypair> = Arc::new(dev);

    let mut pumpfun_client = PumpFun::new(payer);

    //Creating LUT
    let lut_pubkey: Pubkey =
        Pubkey::from_str("GCSH2bNi8yKHbk4tCeSptvqPKE1YwTv1o35Lnj6zDhWt").unwrap();
    let raw_account = client.get_account(&lut_pubkey).unwrap();
    let address_lookup_table = AddressLookupTable::deserialize(&raw_account.data).unwrap();

    let address_lookup_table_account = AddressLookupTableAccount {
        key: lut_pubkey,
        addresses: address_lookup_table.addresses.to_vec(),
    };

    print!("LUT: {:?}", address_lookup_table_account);
    // Directory containing keypair JSON files

    let dir_path = "accounts/".to_string();

    // Read the directory
    let dir_entries = match fs::read_dir(&dir_path) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("Failed to read directory {}: {}", dir_path, e);
            return;
        }
    };

    // Iterate over directory entries
    let mut kps: Vec<Keypair> = Vec::new();

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
            println!("Found keypair: {:?}", keypair.pubkey().to_string());
            kps.push(keypair);
        }
    }

    let all_keypairs: Vec<&Keypair> = kps.iter().collect();


    let other_buyers: Vec<Keypair> = filtering(&mint_keypair, &dev_keypair, all_keypairs.clone());

    let addresses: Vec<Pubkey> = other_buyers
        .iter()
        .map(|keypair| keypair.pubkey())
        .collect();

    print!("Buying addresses: {:?}", addresses);

    let dev_buy_amount = 300_000_000; //0.3 sol

    let dev_with_amount = KeypairWithAmount {
        keypair: dev_keypair,
        amount: dev_buy_amount,
    };

    let mut splits: Vec<u64> = vec![
        287500000, 287500000, 313892203, 282221559, 297011818, 303085315, 267184146, 261604959,
    ]; // 2.3 sol

    let others_with_amount: Vec<KeypairWithAmount> = other_buyers
        .iter()
        .map(|keypair| KeypairWithAmount {
            keypair: keypair.insecure_clone(),
            amount: popper(&mut splits),
        })
        .collect();

    let token_metadata = Create {
        _name: "yrdy".to_string(),
        _symbol: "yrdy".to_string(),
        _uri: "https://ipfs.io/ipfs/QmRZKP3LWBiDdeoigMXkndnLdxDzzry6Cv8HB4tKRm5mDW".to_string(),
        _creator: dev_with_amount.keypair.pubkey(),
    };

    //TOP UP SHOULD BE HERE

    //BUILD INSTRUCTIONS

    //calculating the max amount of lamports to buy with
    let rent = Rent::default();
    let rent_exempt_min = rent.minimum_balance(0);

    let to_subtract: u64 = rent_exempt_min + FEE_AMOUNT + BUFFER_AMOUNT;

    let to_sub_for_dev: u64 = to_subtract.clone() + JITO_TIP_AMOUNT;

    let final_dev_buy_amount = dev_with_amount.amount - to_sub_for_dev;

    let mint_ix = pumpfun_client.create_instruction(&mint_keypair, token_metadata);

    let tip_ix = jito
    .get_tip_ix(dev_with_amount.keypair.pubkey())
    .await
    .unwrap();

    let dev_ix = pumpfun_client
        .buy_ixs(
            &Pubkey::from_str(mint_pubkey).unwrap(),
            &dev_with_amount.keypair,
            final_dev_buy_amount,
            None,
            true,
        )
        .await
        .unwrap();

    let mut transactions: Vec<VersionedTransaction> = Vec::new();

    let mut current_tx_ixs : Vec<Instruction> = Vec::new(); 
    let priority_fee_amount = 200_000; // 0.000007 SOL
    // Create priority fee instruction
    let set_compute_unit_price_ix = ComputeBudgetInstruction::set_compute_unit_price(priority_fee_amount);
    current_tx_ixs.push(set_compute_unit_price_ix);
    current_tx_ixs.push(mint_ix);
    current_tx_ixs.extend(dev_ix);
    current_tx_ixs.push(tip_ix);

    for (index, keypair) in others_with_amount.iter().enumerate() {
        
        let balance = client.get_balance(&keypair.keypair.pubkey()).unwrap();
        if balance < keypair.amount {
            println!(
                "Keypair {} has insufficient balance. Skipping buy.",
                keypair.keypair.pubkey()
            );
            continue;
        }

        let mint_pubkey: &Pubkey = &mint_keypair.pubkey();

        let final_buy_amount = keypair.amount - to_subtract;

        let buy_ixs = pumpfun_client
            .buy_ixs(mint_pubkey, &keypair.keypair, final_buy_amount, None, transactions.len() < MAX_TX_PER_BUNDLE)
            .await
            .unwrap();
        //let mut maybe_ixs: Vec<Instruction> = Vec::new(); 

        //for ix in &current_tx_ixs {
        //    maybe_ixs.push(ix.clone());
        //}

        //maybe_ixs.extend(buy_ixs);

        let mut unique_signers: HashSet<Pubkey> = HashSet::new();

        let mut all_ixs: Vec<Instruction> = Vec::new();

        for ix in &current_tx_ixs {
            for acc in ix.accounts.iter().filter(|acc| acc.is_signer) {
                unique_signers.insert(acc.pubkey);
            }
            all_ixs.push(ix.clone());
        }

        let mut tx_signers: Vec<&Keypair> = Vec::new();

        for signer in unique_signers {
            if let Some(kp) = all_keypairs.iter().find(|kp| kp.pubkey() == signer) {
                tx_signers.push(kp);
            }
        }

        for ix in &buy_ixs {
            all_ixs.push(ix.clone());
        }

        let mut maybe_ix_unique_signers: HashSet<Pubkey> = HashSet::new();
        for ix in &all_ixs {
            for acc in ix.accounts.iter().filter(|acc| acc.is_signer) {
                maybe_ix_unique_signers.insert(acc.pubkey);
            }
        }

        let mut maybe_ix_tx_signers: Vec<&Keypair> = Vec::new();
        for signer in maybe_ix_unique_signers {
            if let Some(kp) = all_keypairs.iter().find(|kp| kp.pubkey() == signer) {
                maybe_ix_tx_signers.push(kp);
            }
        }

        let maybe_tx = build_transaction(&client, &all_ixs, maybe_ix_tx_signers, address_lookup_table_account.clone());

        let size: usize = bincode::serialized_size(&maybe_tx).unwrap() as usize;

        //Add new ixs to current tx instructions if size below 1232, else create new tx and reset current tx instructions, and add new ixs to it
        if size > 1232 {
            println!("Instructions: {:?}", current_tx_ixs);
            let new_tx = build_transaction(&client, &current_tx_ixs, tx_signers, address_lookup_table_account.clone());
            transactions.push(new_tx);
            println!("Added new tx to transactions, with size {}", size);
            current_tx_ixs = vec![];
            current_tx_ixs.extend(buy_ixs);
            if index % 5 == 0 {
                println!("Adding tip ix to current tx instructions");
                let tip_ix = jito.get_tip_ix(dev_with_amount.keypair.pubkey()).await.unwrap();
                current_tx_ixs.push(tip_ix);
            }
        }else{
            println!("Adding new ixs to current tx instructions, current size: {}", size);
            current_tx_ixs.extend(buy_ixs);
        }
        println!("With {} instructions", current_tx_ixs.len());
        println!("Transaction size: {:?}", size);
    }

    if current_tx_ixs.len() > 0 {
        let mut unique_signers: HashSet<Pubkey> = HashSet::new();
        for ix in &current_tx_ixs {
            for acc in ix.accounts.iter().filter(|acc| acc.is_signer) {
                unique_signers.insert(acc.pubkey);
            }
        }

        let mut tx_signers: Vec<&Keypair> = Vec::new();

        for signer in unique_signers {
            if let Some(kp) = all_keypairs.iter().find(|kp| kp.pubkey() == signer) {
                tx_signers.push(kp);
            }else if signer == dev_with_amount.keypair.pubkey() {
                tx_signers.push(&dev_with_amount.keypair);
            }else if signer == admin.pubkey() {
                tx_signers.push(&admin);
            }
        }
        println!("Instructions: {:?}", current_tx_ixs.len());
        let tx = build_transaction(&client, &current_tx_ixs, tx_signers, address_lookup_table_account.clone());
        transactions.push(tx);
    }

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
                eprintln!("🚨 Transaction simulation error: {:?}", e);
            }
        }
    }
    /*

    TODO:

    -Test the build_transaction functions with the wallets

    -Play with the helper function and make it recursive
            - Make it detect when we are in first bundle
            - Make it add a tip instruction to every 5th transaction
            - Make it split instructions into transactions correctly, and maximize instructions per transactino respecting size limit

    ---> Goal is to make the helper function work correctly, and that it passes with_stimulate true only for the first 5 transactions

    -Submit multiple bundles and await for the first bundle

    ---> Goal is to ensure that the first bundle passes, second bundles

     */
}

/*

- Send sol to addresses ONCE - DONE

TOP UP INSTRUCTIONS
let admin_to_dev_ix = transfer_ix(&admin_keypair.pubkey(), &dev_with_amount.keypair.pubkey(), dev_with_amount.amount);
let admin_to_keypair_ixs: Vec<Instruction> = others_with_amount.iter().map(|other| transfer_ix(&admin_keypair.pubkey(), &other.keypair.pubkey(), other.amount)).collect();
let jito_tip_ix = jito.get_tip_ix(admin_keypair.pubkey()).await.unwrap();

let mut instructions: Vec<Instruction> = vec![admin_to_dev_ix];
instructions.extend(admin_to_keypair_ixs);
instructions.push(jito_tip_ix);

let tx = build_transaction(&client, &instructions, vec![&admin_keypair], address_lookup_table_account.clone());

//TODO: Add ATA instructions to the LUT
let _ = jito.one_tx_bundle(tx).await.unwrap();

    let mut all_ata: Vec<Pubkey> = Vec::new();
    for keypair in other_buyers.iter() {
        let ata = pumpfun_client.get_ata(&keypair.pubkey(), &mint_keypair.pubkey());
        all_ata.push(ata);
    }
    let dev_ata = pumpfun_client.get_ata(&dev_with_amount.keypair.pubkey(), &mint_keypair.pubkey());
    all_ata.push(dev_ata);  
    let _ = extend_lut(&client, &admin_keypair, lut_pubkey, &all_ata);

*/