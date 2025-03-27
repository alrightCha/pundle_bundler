use crate::config::{JITO_TIP_AMOUNT, MAX_RETRIES, RPC_URL};
use crate::jito::jito::JitoBundle;
use crate::jupiter::swap::swap_ixs;
use crate::pumpfun::pump::PumpFun;
use crate::solana::utils::{build_transaction, get_ata_balance, test_transactions};
use solana_client::rpc_client::RpcClient;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::{
    address_lookup_table::state::AddressLookupTable,
    address_lookup_table::AddressLookupTableAccount,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::VersionedTransaction,
};
use std::collections::{HashMap, HashSet};

pub async fn sell_all_txs(
    admin_keypair: Keypair,
    all_keypairs: Vec<&Keypair>,
    mint_pubkey: &Pubkey,
    lut_pubkey: Pubkey,
    pumpfun_client: PumpFun,
    client: RpcClient,
) -> Vec<VersionedTransaction> {
    println!("Selling all txs...");
    println!(
        "All Keypairs: {:?}",
        all_keypairs
            .iter()
            .map(|kp| kp.pubkey())
            .collect::<Vec<Pubkey>>()
    );

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
    let priority_fee_ix = ComputeBudgetInstruction::set_compute_unit_price(500_000);

    current_tx_ixs.push(priority_fee_ix);

    let mut added_tip_ix: bool = false;

    let token_bonded = pumpfun_client
        .get_pool_information(mint_pubkey)
        .await
        .unwrap()
        .is_bonding_curve_complete;

    for keypair in all_keypairs.iter() {
        let balance = client.get_balance(&keypair.pubkey()).unwrap();

        if balance < 1_000_000 {
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
                    .await;
                match pump_ixs {
                    Ok(ixs) => ixs, 
                    Err(error) => {
                        println!("Error occurred: {:?}", error.to_string());
                        print!("Error finding instructions for keypair {:?}", keypair.pubkey().to_string());
                        let empty: Vec<Instruction> = Vec::new();
                        empty
                    }
                }
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
            if signer == admin_keypair.pubkey() {
                tx_signers.push(&admin_keypair);
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
            if signer == admin_keypair.pubkey() {
                maybe_ix_tx_signers.push(&admin_keypair);
            }
        }

        let maybe_tx = build_transaction(
            &client,
            &all_ixs,
            maybe_ix_tx_signers,
            address_lookup_table_account.clone(),
            &admin_keypair,
        );

        let size: usize = bincode::serialized_size(&maybe_tx).unwrap() as usize;

        //Add new ixs to current tx instructions if size below 1232, else create new tx and reset current tx instructions, and add new ixs to it
        if size > 1232 {
            let new_tx = build_transaction(
                &client,
                &current_tx_ixs,
                tx_signers,
                address_lookup_table_account.clone(),
                &admin_keypair,
            );
            transactions.push(new_tx);
            println!("Added new tx to transactions, with size {}", size);
            current_tx_ixs = vec![];
            let priority_fee_ix = ComputeBudgetInstruction::set_compute_unit_price(500_000);
            current_tx_ixs.push(priority_fee_ix);
            current_tx_ixs.extend(new_ixs);
            if !added_tip_ix && transactions.len() % 5 == 4 {
                println!("Adding tip ix to current tx instructions");
                let tip_ix = jito.get_tip_ix(admin_keypair.pubkey()).await.unwrap();
                current_tx_ixs.push(tip_ix);
                added_tip_ix = true;
            }
            if transactions.len() % 5 == 0 {
                added_tip_ix = false;
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
        let mut maybe_last_ixs_with_tip: Vec<Instruction> = Vec::new();

        for ix in &current_tx_ixs {
            maybe_last_ixs_with_tip.push(ix.clone());
        }

        //If the current transactions are below 5, meaning this tx will be the last one in the batch, add a tip instruction to it
        if !added_tip_ix {
            println!("Adding tip ix to current tx instructions");
            let tip_ix = jito
                .get_tip_ix(admin_keypair.pubkey())
                .await
                .unwrap();

            maybe_last_ixs_with_tip.push(tip_ix);
        }

        let mut unique_signers: HashSet<Pubkey> = HashSet::new();
        for ix in &current_tx_ixs {
            for acc in ix.accounts.iter().filter(|acc| acc.is_signer) {
                unique_signers.insert(acc.pubkey);
            }
        }

        let mut tx_signers: Vec<&Keypair> = Vec::new();
        for signer in unique_signers {
            if let Some(kp) = all_keypairs.iter().find(|kp| kp.pubkey() == signer) {
                println!("Signer required to sell instruction: {:?}", kp.pubkey());
                tx_signers.push(kp);
            }
            if signer == admin_keypair.pubkey() {
                tx_signers.push(&admin_keypair);
            }
        }
         //Checking if last transaction is too big, if so, split it into two transactions with a tip instruction in between

        let mut maybe_last_tx_signers: Vec<&Keypair> = Vec::new();

        maybe_last_tx_signers.extend(tx_signers.clone());

        //Means that maybe last tx has different signers than normal, and we add the dev keypair
        if !added_tip_ix {
            maybe_last_tx_signers.push(&admin_keypair);
        }

        let maybe_last_tx = build_transaction(
            &client,
            &maybe_last_ixs_with_tip,
            maybe_last_tx_signers,
            address_lookup_table_account.clone(),
            &admin_keypair,
        );

        let tx_size: usize = bincode::serialized_size(&maybe_last_tx).unwrap() as usize;

        if tx_size > 1232 {
            println!("Last transaction is too big, splitting it into two transactions with a tip instruction in between");
            let before_last_tx = build_transaction(
                &client,
                &current_tx_ixs,
                tx_signers.clone(),
                address_lookup_table_account.clone(),
                &admin_keypair,
            );

            let tip_ix = jito
                .get_tip_ix(admin_keypair.pubkey())
                .await
                .unwrap();

            let ixs: Vec<Instruction> = vec![tip_ix];
            let last_signer: Vec<&Keypair> = vec![&admin_keypair];
            let tip_tx = build_transaction(
                &client,
                &ixs,
                last_signer,
                address_lookup_table_account.clone(),
                &admin_keypair,
            );
            //If we have 4 transactions, meaning this tx will be the last one in the batch, add a tip instruction to it
            if transactions.len() % 5 == 4 {
                //Case where no room for full TX so we only add tip tx to the pre final batch and have one last tx
                transactions.push(tip_tx);
                transactions.push(maybe_last_tx);
            } else {
                //Case where we have room for split txs within same last batch
                transactions.push(before_last_tx);
                transactions.push(tip_tx);
            }
        } else {
            //Case where we added a tip instruction to the last transaction
            println!("Last transaction is not too big, adding it to transactions");
            transactions.push(maybe_last_tx);
        }
    }
    
    test_transactions(&client, &transactions).await;
    transactions
}
