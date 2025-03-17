use crate::jito::jito::JitoBundle;
use crate::pumpfun::pump::PumpFun;
use crate::jupiter::swap::swap_ixs;
use crate::solana::utils::{get_ata_balance, build_transaction};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    address_lookup_table::state::AddressLookupTable,
    address_lookup_table::AddressLookupTableAccount,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::VersionedTransaction,
};
use std::collections::{HashMap, HashSet};
use crate::config::{MAX_RETRIES, JITO_TIP_AMOUNT, RPC_URL};

pub async fn sell_all_txs(admin_keypair: Keypair,all_keypairs: Vec<&Keypair>, mint_pubkey: &Pubkey, lut_pubkey: Pubkey, pumpfun_client: PumpFun, client: RpcClient) -> Vec<VersionedTransaction> {
    let jito_client = RpcClient::new(RPC_URL);
    let jito = JitoBundle::new(jito_client, MAX_RETRIES, JITO_TIP_AMOUNT);
    let raw_account = client.get_account(&lut_pubkey).unwrap();
    let address_lookup_table = AddressLookupTable::deserialize(&raw_account.data).unwrap();

    let address_lookup_table_account = AddressLookupTableAccount {
        key: lut_pubkey,
        addresses: address_lookup_table.addresses.to_vec(),
    };

    print!("LUT: {:?}", address_lookup_table_account);
    
    let mut signers_map: HashMap<Pubkey, &Keypair> = HashMap::new();


    for keypair in all_keypairs.iter() {
        signers_map.insert(keypair.pubkey(), keypair);
    }

    let addresses: Vec<Pubkey> = all_keypairs
        .iter()
        .map(|keypair| keypair.pubkey())
        .collect();

    print!("Buying addresses: {:?}", addresses);

    //BUILD INSTRUCTIONS

    let mut transactions: Vec<VersionedTransaction> = Vec::new();

    let mut current_tx_ixs: Vec<Instruction> = Vec::new();


    let tip_ix = jito
        .get_tip_ix(admin_keypair.pubkey())
        .await
        .unwrap();


    current_tx_ixs.push(tip_ix);

    let token_bonded = pumpfun_client
        .get_pool_information(mint_pubkey)
        .await
        .unwrap()
        .is_bonding_curve_complete;

    for (index, keypair) in all_keypairs.iter().enumerate() {
        
        let balance = client.get_balance(&keypair.pubkey()).unwrap();
        if balance < 1_900_000 {
            println!(
                "Keypair {} has insufficient balance. Skipping sell.",
                keypair.pubkey()
            );
            continue;
        }

        let new_ixs = match token_bonded {
            true => {
                let amount = get_ata_balance(&client, &keypair, &mint_pubkey).await;
                let swap_ixs = swap_ixs(&keypair, *mint_pubkey, amount, None)
                    .await
                    .unwrap();
                swap_ixs
            }
            false => {
                let pump_ixs = pumpfun_client
                    .sell_all_ix(&mint_pubkey, &keypair)
                    .await
                    .unwrap();
                pump_ixs
            }
        };
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

        for ix in &new_ixs {
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

        let maybe_tx = build_transaction(
            &client,
            &all_ixs,
            maybe_ix_tx_signers,
            address_lookup_table_account.clone(),
            None,
        );

        let size: usize = bincode::serialized_size(&maybe_tx).unwrap() as usize;

        //Add new ixs to current tx instructions if size below 1232, else create new tx and reset current tx instructions, and add new ixs to it
        if size > 1232 {
            let new_tx = build_transaction(
                &client,
                &current_tx_ixs,
                tx_signers,
                address_lookup_table_account.clone(),
                None,
            );
            transactions.push(new_tx);
            println!("Added new tx to transactions, with size {}", size);
            current_tx_ixs = vec![];
            current_tx_ixs.extend(new_ixs);
            if index % 5 == 0 {
                println!("Adding tip ix to current tx instructions");
                let tip_ix = jito
                    .get_tip_ix(admin_keypair.pubkey())
                    .await
                    .unwrap();
                current_tx_ixs.push(tip_ix);
            }
        } else {
            println!(
                "Adding new ixs to current tx instructions, current size: {}",
                size
            );
            current_tx_ixs.extend(new_ixs);
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
            }
        }
        transactions.push(build_transaction(
            &client,
            &current_tx_ixs,
            tx_signers,
            address_lookup_table_account.clone(),
            None,
        ));
    }

    transactions
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
