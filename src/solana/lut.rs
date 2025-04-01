use solana_sdk::{
    address_lookup_table::instruction::{create_lookup_table, extend_lookup_table, close_lookup_table, deactivate_lookup_table}, 
    hash::Hash, 
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
    transaction::Transaction,
};
use std::{thread, time::Duration};
use solana_client::rpc_client::RpcClient;
use solana_client::client_error::ClientError;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_sdk::commitment_config::CommitmentLevel;
use std::result::Result::Ok;
use anyhow::Result;
use solana_sdk::compute_budget::ComputeBudgetInstruction;

use crate::solana::utils::get_slot_and_blockhash;

fn send_transaction(
    client: &RpcClient, 
    recent_blockhash: Hash, 
    signer: &Keypair, 
    ixes: &[Instruction]
) -> Result<Signature, solana_client::client_error::ClientError> {
    let tx = Transaction::new_signed_with_payer(
        ixes,
        Some(&signer.pubkey()),
        &[&signer],
        recent_blockhash,
    );
    
    let signature = client.send_and_confirm_transaction(&tx).unwrap();

    Ok(signature)
}

//Require signing and sending transaction and returning pda and signature
pub fn create_lut(client: &RpcClient, payer: &Keypair) -> Result<(Pubkey, Signature), Box<dyn std::error::Error>> {
    // Fetch the latest slot and blockhash just before creating the transaction
    let (slot, blockhash) = get_slot_and_blockhash(&client)?;

    // Add priority fee instruction
    let priority_fee_ix = ComputeBudgetInstruction::set_compute_unit_price(50_000);

    // Create the lookup table instruction
    let (ix, pda) = create_lookup_table(
        payer.pubkey(),
        payer.pubkey(),
        slot,
    );

    // Create and sign the transaction with both instructions
    let tx = Transaction::new_signed_with_payer(
        &[priority_fee_ix, ix],
        Some(&payer.pubkey()),
        &[&payer],
        blockhash,
    );

    // Send the transaction and wait for confirmation with specific commitment
    let signature = client.send_and_confirm_transaction_with_spinner_and_config(
        &tx,
        CommitmentConfig::finalized(),  // Use finalized commitment
        RpcSendTransactionConfig {
            skip_preflight: true,  // Skip preflight to avoid false negatives
            preflight_commitment: Some(CommitmentLevel::Processed),
            ..RpcSendTransactionConfig::default()
        },
    )?;

    println!("Transaction confirmed: {:?}", signature);
    println!("View transaction on Solscan: https://solscan.io/tx/{}", signature);

    Ok((pda, signature))
}


//add addresses to pre-existing lut
pub fn extend_lut_size(
    client: &RpcClient,
    authority: &Keypair,
    lut_pubkey: Pubkey,
    addresses: &Vec<Pubkey>,
) -> Result<usize, ClientError> {
    let (_, blockhash) = get_slot_and_blockhash(&client).unwrap();

    // Add priority fee instruction
    let priority_fee_ix = ComputeBudgetInstruction::set_compute_unit_price(50_000);

    let ix = extend_lookup_table(
        lut_pubkey,
        authority.pubkey(),
        Some(authority.pubkey()),
        addresses.to_vec(),
    );

    let tx = Transaction::new_signed_with_payer(
        &[priority_fee_ix, ix],
        Some(&authority.pubkey()),
        &[&authority],
        blockhash,
    );

    let size: usize = bincode::serialized_size(&tx).unwrap() as usize;
    println!("Size of transaction: {}", size);

    Ok(size)
}


//add addresses to pre-existing lut
pub fn extend_lut(
    client: &RpcClient,
    authority: &Keypair,
    lut_pubkey: Pubkey,
    addresses: &Vec<Pubkey>,
) -> Result<Signature, ClientError> {
    let (_, blockhash) = get_slot_and_blockhash(&client).unwrap();

    // Add priority fee instruction
    let priority_fee_ix = ComputeBudgetInstruction::set_compute_unit_price(50_000);

    let ix = extend_lookup_table(
        lut_pubkey,
        authority.pubkey(),
        Some(authority.pubkey()),
        addresses.to_vec(),
    );

    let tx = Transaction::new_signed_with_payer(
        &[priority_fee_ix, ix],
        Some(&authority.pubkey()),
        &[&authority],
        blockhash,
    );

    // Send and wait for finalized confirmation
    let signature = client.send_and_confirm_transaction_with_spinner_and_config(
        &tx,
        CommitmentConfig::finalized(),  // Use finalized commitment
        RpcSendTransactionConfig {
            skip_preflight: true,  // Skip preflight to avoid false negatives
            preflight_commitment: Some(CommitmentLevel::Finalized),
            ..RpcSendTransactionConfig::default()
        },
    )?;

    println!("LUT extension confirmed: {:?}", signature);
    println!("View transaction on Solscan: https://solscan.io/tx/{}", signature);

    Ok(signature)
}

// Add this helper function to verify LUT is ready
pub fn verify_lut_ready(
    client: &RpcClient, 
    lut_pubkey: &Pubkey
) -> Result<bool, ClientError> {
    match client.get_account_with_commitment(
        lut_pubkey,
        CommitmentConfig::finalized()
    ) {
        Ok(response) => {
            if let Some(_account) = response.value {
                Ok(true)
            } else {
                Ok(false)
            }
        },
        Err(e) => Err(e)
    }
}

fn deactivate_lut(client: &RpcClient, authority: &Keypair, lut_pubkey: Pubkey) -> Result<()> {

    let (_, blockhash) = get_slot_and_blockhash(&client).unwrap();

    let ix = deactivate_lookup_table(lut_pubkey, authority.pubkey());

    let signature = send_transaction(client, blockhash, authority, &[ix])?;

       // Confirm transaction
    let confirmation = client.confirm_transaction_with_spinner(
    &signature,
&client.get_latest_blockhash()?,
    CommitmentConfig::finalized(),
    ).unwrap();

    Ok(confirmation)
}

pub fn close_lut(client: &RpcClient, authority: &Keypair, lut_pubkey: Pubkey){

    let _ = deactivate_lut(client, authority, lut_pubkey).unwrap();

     // Step 2: Wait for the deactivation cooldown (512 slots)
     let initial_slot = client.get_slot().unwrap();
     let cooldown_slots = 520; // Solana's deactivation cooldown period
 
     loop {
         let current_slot = client.get_slot().unwrap();
         if current_slot >= initial_slot + cooldown_slots {
             break;
         }
 
         // Wait for a short period before checking again
         thread::sleep(Duration::from_secs(1));
     }

    let (_, blockhash) = get_slot_and_blockhash(&client).unwrap();

    let authority_pubkey = authority.pubkey();

    let ix = close_lookup_table(lut_pubkey, authority_pubkey, authority_pubkey);

    let tx = send_transaction(&client, blockhash, authority,&[ix]).unwrap();

    client.confirm_transaction(&tx).unwrap();
}