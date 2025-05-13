use crate::config::{JITO_TIP_AMOUNT, MAX_RETRIES};
use crate::jito::jito::JitoBundle;
use crate::jupiter::swap::sol_for_tokens;
use crate::pumpfun::pump::PumpFun;
use crate::pumpfun::swap::PumpSwap;
use crate::solana::utils::{build_transaction, test_transactions};
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
use std::str::FromStr;
use std::sync::Arc;

use super::utils::get_admin_keypair;

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

    let mut tips_count = 0;
    let jito = JitoBundle::new(MAX_RETRIES, JITO_TIP_AMOUNT);

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

    let token_bonded = pumpfun_client
        .get_pool_information(mint_pubkey)
        .await
        .unwrap()
        .is_bonding_curve_complete;

    let mut ixs: Vec<Instruction> = Vec::new();

    let mut all_signers: Vec<Keypair> = Vec::new();
    all_signers.push(admin_keypair.insecure_clone());

    for keypair in all_keypairs.iter() {
        all_signers.push(keypair.insecure_clone());

        let new_ixs: Option<Vec<Instruction>> = match token_bonded {
            true => {
                let swap_engine = PumpSwap::new();
                let sell_ixs = swap_engine
                    .sell_ixs(*mint_pubkey, keypair.pubkey(), None, Some(keypair.insecure_clone()))
                    .await;
                Some(sell_ixs)
            }
            false => {
                let pump_ixs = pumpfun_client.sell_all_ix(&mint_pubkey, &keypair).await;
                match pump_ixs {
                    Ok(ixs) => Some(ixs),
                    Err(_) => None,
                }
            }
        };

        if let Some(new_ixs) = new_ixs {
            println!(
                "Passing sell ixs {:?} for {:?}",
                new_ixs.len(),
                keypair.pubkey().to_string()
            );
            ixs.extend(new_ixs);
        }
    }

    let mut transactions: Vec<VersionedTransaction> = Vec::new();

    let mut current_tx_ixs: Vec<Instruction> = Vec::new();
    let priority_fee_ix = ComputeBudgetInstruction::set_compute_unit_price(200_000);
    current_tx_ixs.push(priority_fee_ix);

    for ix in ixs {
        let mut maybe_ixs: Vec<Instruction> = Vec::new();
        for cix in current_tx_ixs.iter() {
            maybe_ixs.push(cix.clone());
        }
        maybe_ixs.push(ix.clone());
        let maybe_signers = get_tx_signers(&maybe_ixs, &all_signers);
        let maybe_tx = build_transaction(
            &client,
            &maybe_ixs,
            maybe_signers.iter().collect(),
            address_lookup_table_account.clone(),
            &admin_keypair,
        );
        let size: usize = bincode::serialized_size(&maybe_tx).unwrap() as usize;

        if size < 1232 {
            current_tx_ixs.push(ix);
        } else {
            let mut new_ixs: Vec<Instruction> = Vec::new();
            let priority_fee_ix = ComputeBudgetInstruction::set_compute_unit_price(200_000);
            new_ixs.push(priority_fee_ix);
            new_ixs.push(ix);
            if transactions.len() % 5 == 4 {
                let mut maybe_with_tip: Vec<Instruction> = Vec::new();
                for ix in current_tx_ixs.iter() {
                    maybe_with_tip.push(ix.clone());
                }
                let tip_ix = jito.get_tip_ix(admin_keypair.pubkey(), None).await.unwrap();
                maybe_with_tip.push(tip_ix.clone());
                let size: usize = bincode::serialized_size(&maybe_tx).unwrap() as usize;

                if size > 1232 {
                    let revert_ix = current_tx_ixs.pop();
                    if let Some(revert_ix) = revert_ix {
                        new_ixs.push(revert_ix);
                    }
                    current_tx_ixs.push(tip_ix);
                    tips_count += 1;
                }
            }
            let tx_signers = get_tx_signers(&current_tx_ixs, &all_signers);
            let tx = build_transaction(
                &client,
                &current_tx_ixs,
                tx_signers.iter().collect(),
                address_lookup_table_account.clone(),
                &admin_keypair,
            );
            transactions.push(tx);
            current_tx_ixs = new_ixs;
        }
    }

    if current_tx_ixs.len() > 0 {
        let tip_ix = jito.get_tip_ix(admin_keypair.pubkey(), None).await.unwrap();
        let mut maybe_ixs: Vec<Instruction> = Vec::new();
        for ix in current_tx_ixs.iter() {
            maybe_ixs.push(ix.clone());
        }
        maybe_ixs.push(tip_ix.clone());
        let signers = get_tx_signers(&maybe_ixs, &all_signers);

        let one_tx = build_transaction(
            &client,
            &maybe_ixs,
            signers.iter().collect(),
            address_lookup_table_account.clone(),
            &admin_keypair,
        );
        let size: usize = bincode::serialized_size(&one_tx).unwrap() as usize;

        if size > 1232 {
            let signers = get_tx_signers(&current_tx_ixs, &all_signers);
            let first_tx = build_transaction(
                &client,
                &current_tx_ixs,
                signers.iter().collect(),
                address_lookup_table_account.clone(),
                &admin_keypair,
            );
            let tip_tx = build_transaction(
                &client,
                &vec![tip_ix],
                vec![&admin_keypair.insecure_clone()],
                address_lookup_table_account.clone(),
                &admin_keypair,
            );

            if transactions.len() % 5 < 4 {
                //Add 2 txs
                transactions.push(first_tx);
                transactions.push(tip_tx);
                tips_count += 1;
            } else {
                //Add 3 txs
                transactions.push(tip_tx.clone());
                tips_count += 1;
                transactions.push(first_tx);
                transactions.push(tip_tx);
                tips_count += 1;
            }
        } else {
            transactions.push(one_tx);
            tips_count += 1;
        }
    }

    print!(
        "Sending {:?} sell transactions with {:?} tip instructions",
        transactions.len(),
        tips_count
    );
    test_transactions(&client, &transactions).await;
    transactions
}

pub fn get_tx_signers(ixs: &Vec<Instruction>, all_keypairs: &Vec<Keypair>) -> Vec<Keypair> {
    let mut maybe_ix_unique_signers: HashSet<Pubkey> = HashSet::new();

    for ix in ixs {
        for acc in ix.accounts.iter().filter(|acc| acc.is_signer) {
            maybe_ix_unique_signers.insert(acc.pubkey);
        }
    }

    let mut all_ixs_signers: Vec<Keypair> = Vec::new();

    for signer in maybe_ix_unique_signers {
        if let Some(kp) = all_keypairs.iter().find(|kp| kp.pubkey() == signer) {
            all_ixs_signers.push(kp.insecure_clone());
        }
    }
    all_ixs_signers
}

pub async fn get_sol_amount(amount: u64, mint: String) -> u64 {
    let admin_kp = get_admin_keypair();
    let payer: Arc<Keypair> = Arc::new(admin_kp);

    let pumpfun_client = PumpFun::new(payer);

    let mint_pubkey = Pubkey::from_str(&mint).unwrap();

    let pool_info = pumpfun_client
        .get_pool_information(&mint_pubkey)
        .await
        .unwrap();

    let mut amount_sol: u64 = 0;

    if pool_info.is_bonding_curve_complete {
        let amount = sol_for_tokens(mint_pubkey, amount).await;
        match amount {
            Ok(amount_recv) => amount_sol = amount_recv,
            Err(_) => println!("Token account balance not found"),
        }
    } else {
        let price = pool_info.sell_price;
        let sol_amount = (price * amount) / 100_000; //Returns amount in lamports 100_000 because sell_price is per 100 000 tokens
        amount_sol = sol_amount;
    }
    amount_sol
}