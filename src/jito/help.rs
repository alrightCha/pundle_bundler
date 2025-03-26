use super::jito::JitoBundle;
use crate::config::{BUFFER_AMOUNT, FEE_AMOUNT, MAX_TX_PER_BUNDLE};
use crate::config::{JITO_TIP_AMOUNT, MAX_RETRIES, RPC_URL};
use crate::params::KeypairWithAmount;
use crate::pumpfun::pump::PumpFun;
use crate::solana::utils::{build_transaction, test_transactions};
use pumpfun_cpi::instruction::Create;
use solana_client::rpc_client::RpcClient;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::{
    address_lookup_table::state::AddressLookupTable,
    address_lookup_table::AddressLookupTableAccount,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    sysvar::rent::Rent,
    transaction::VersionedTransaction,
};
use std::collections::HashSet;
use std::sync::Arc;

/*
keypairs_with_amount: Vec<KeypairWithAmount>,
dev_keypair_with_amount: KeypairWithAmount,
mint: Keypair,
requester_pubkey: String,
token_metadata: CreateTokenMetadata
*/

pub async fn build_bundle_txs(
    dev_with_amount: KeypairWithAmount,
    mint_keypair: &Keypair,
    others_with_amount: Vec<KeypairWithAmount>,
    lut_pubkey: Pubkey,
    mint_pubkey: Pubkey,
    token_metadata: Create,
    admin_keypair: &Keypair,
) -> Vec<VersionedTransaction> {
    println!("Building bundle txs...");
    let client = RpcClient::new(RPC_URL);

    let jito_rpc = RpcClient::new_with_commitment(
        RPC_URL.to_string(),
        solana_sdk::commitment_config::CommitmentConfig::confirmed(),
    );

    let jito = JitoBundle::new(jito_rpc, MAX_RETRIES, JITO_TIP_AMOUNT);
    let dev: Keypair = dev_with_amount.keypair.insecure_clone();
    let payer: Arc<Keypair> = Arc::new(dev);

    let mut pumpfun_client = PumpFun::new(payer);

    let raw_account = client.get_account(&lut_pubkey).unwrap();
    let address_lookup_table = AddressLookupTable::deserialize(&raw_account.data).unwrap();

    let address_lookup_table_account = AddressLookupTableAccount {
        key: lut_pubkey,
        addresses: address_lookup_table.addresses.to_vec(),
    };

    //BUILD INSTRUCTIONS

    //calculating the max amount of lamports to buy with
    let rent = Rent::default();
    let rent_exempt_min = rent.minimum_balance(0);

    let to_sub_for_dev: u64 = rent_exempt_min + FEE_AMOUNT + JITO_TIP_AMOUNT + BUFFER_AMOUNT;

    let final_dev_buy_amount = dev_with_amount.amount - to_sub_for_dev;

    let mint_ix = pumpfun_client.create_instruction(&mint_keypair, token_metadata);

    let dev_ix = pumpfun_client
        .buy_ixs(
            &mint_pubkey,
            &dev_with_amount.keypair,
            final_dev_buy_amount,
            None,
            true,
        )
        .await
        .unwrap();

    //TODO: add to params
    let mint_pubkey: &Pubkey = &mint_keypair.pubkey();

    let mut transactions: Vec<VersionedTransaction> = Vec::new();

    let mut current_tx_ixs: Vec<Instruction> = Vec::new();
    current_tx_ixs.push(mint_ix);
    current_tx_ixs.extend(dev_ix);
    let priority_fee_ix = ComputeBudgetInstruction::set_compute_unit_price(2_000_000);
    current_tx_ixs.push(priority_fee_ix);

    let mut added_tip_ix: bool = false;

    for keypair in others_with_amount.iter() {
        let balance = client.get_balance(&keypair.keypair.pubkey()).unwrap();
        if balance < keypair.amount {
            println!(
                "Keypair {} has insufficient balance. Skipping buy.",
                keypair.keypair.pubkey()
            );
            continue;
        }

        //Return buy instructions
        let new_ixs = pumpfun_client
            .buy_ixs(
                mint_pubkey,
                &keypair.keypair,
                balance,
                None,
                transactions.len() < MAX_TX_PER_BUNDLE,
            )
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
            if let Some(kp) = others_with_amount
                .iter()
                .find(|kp| kp.keypair.pubkey() == signer)
            {
                tx_signers.push(&kp.keypair);
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
            if let Some(kp) = others_with_amount
                .iter()
                .find(|kp| kp.keypair.pubkey() == signer)
            {
                maybe_ix_tx_signers.push(&kp.keypair);
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
            let priority_fee_ix = ComputeBudgetInstruction::set_compute_unit_price(1_000_000);
            current_tx_ixs.push(priority_fee_ix);
            let new_ixs = pumpfun_client
                .buy_ixs(
                    mint_pubkey,
                    &keypair.keypair,
                    balance,
                    None,
                    transactions.len() < MAX_TX_PER_BUNDLE,
                )
                .await
                .unwrap();
            current_tx_ixs.extend(new_ixs);
            if !added_tip_ix && transactions.len() % 5 == 4 {
                println!("Adding tip ix to current tx instructions");
                let tip_ix = jito
                    .get_tip_ix(dev_with_amount.keypair.pubkey())
                    .await
                    .unwrap();
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
                .get_tip_ix(dev_with_amount.keypair.pubkey())
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
            if let Some(kp) = others_with_amount
                .iter()
                .find(|kp| kp.keypair.pubkey() == signer)
            {
                tx_signers.push(&kp.keypair);
            }
        }

        //Checking if last transaction is too big, if so, split it into two transactions with a tip instruction in between

        let mut maybe_last_tx_signers: Vec<&Keypair> = Vec::new();

        maybe_last_tx_signers.extend(tx_signers.clone());

        //Means that maybe last tx has different signers than normal, and we add the dev keypair
        if !added_tip_ix {
            maybe_last_tx_signers.push(&dev_with_amount.keypair);
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
                .get_tip_ix(dev_with_amount.keypair.pubkey())
                .await
                .unwrap();

            let ixs: Vec<Instruction> = vec![tip_ix];
            let last_signer: Vec<&Keypair> = vec![&dev_with_amount.keypair];
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
