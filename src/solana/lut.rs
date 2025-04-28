use crate::solana::utils::get_slot_and_blockhash;
use anyhow::Result;
use solana_client::client_error::ClientError;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::commitment_config::CommitmentLevel;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::{
    address_lookup_table::instruction::{
        close_lookup_table, create_lookup_table, deactivate_lookup_table, extend_lookup_table,
    },
    hash::Hash,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
    transaction::Transaction,
};
use std::result::Result::Ok;
use std::thread::sleep;
use std::{thread, time::Duration};

fn send_transaction(
    client: &RpcClient,
    recent_blockhash: Hash,
    signer: &Keypair,
    ixes: &[Instruction],
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
/// Creates a new Address Lookup Table (LUT) and extends it with the provided addresses
///
/// # Arguments
/// * `client` - RPC client to interact with Solana
/// * `payer` - Keypair that will pay for and sign transactions
/// * `addresses` - Vector of addresses to add to the LUT
///
/// # Returns
/// * `Result<Pubkey>` - The public key of the created LUT
pub async fn create_lut(
    client: &RpcClient,
    payer: &Keypair,
    addresses: &Vec<Pubkey>,
) -> Result<Pubkey> {
    // Get current slot and blockhash
    let (slot, blockhash) = get_slot_and_blockhash(&client).unwrap();

    // Create instructions
    let priority_fee_ix = ComputeBudgetInstruction::set_compute_unit_price(50_000);
    let (create_ix, lut_pubkey) = create_lookup_table(payer.pubkey(), payer.pubkey(), slot);

    let mut extend_ixs = extend_lut(payer, lut_pubkey, addresses);

    let rest = extend_ixs.split_off(1);

    let mut first: Vec<Instruction> = vec![priority_fee_ix.clone(), create_ix];

    first.extend(extend_ixs); //With split off is only 1 ix

    // First transaction creates the LUT
    let create_tx =
        Transaction::new_signed_with_payer(&first, Some(&payer.pubkey()), &[payer], blockhash);

    let size: usize = bincode::serialized_size(&create_tx).unwrap() as usize;

    println!("Create lut tx size: {:?}", size);

    client
        .send_and_confirm_transaction_with_spinner_and_config(
            &create_tx,
            CommitmentConfig::finalized(),
            RpcSendTransactionConfig {
                skip_preflight: true,
                preflight_commitment: Some(CommitmentLevel::Processed),
                ..RpcSendTransactionConfig::default()
            },
        )
        .unwrap();

    // Send extend transactions
    for extend_ix in rest {
        let extend_tx = Transaction::new_signed_with_payer(
            &[priority_fee_ix.clone(), extend_ix],
            Some(&payer.pubkey()),
            &[payer],
            blockhash,
        );

        client
            .send_and_confirm_transaction_with_spinner_and_config(
                &extend_tx,
                CommitmentConfig::finalized(),
                RpcSendTransactionConfig {
                    skip_preflight: true,
                    preflight_commitment: Some(CommitmentLevel::Processed),
                    ..RpcSendTransactionConfig::default()
                },
            )
            .unwrap();
    }

    Ok(lut_pubkey)
}

//add addresses to pre-existing lut
fn extend_lut(
    authority: &Keypair,
    lut_pubkey: Pubkey,
    addresses: &Vec<Pubkey>,
) -> Vec<Instruction> {
    println!("Addresses count: {:?}", addresses.len());
    let mut instructions: Vec<Instruction> = Vec::new();

    let chunks: Vec<_> = addresses.chunks(27).collect();

    for chunk in chunks {
        let ix = extend_lookup_table(
            lut_pubkey,
            authority.pubkey(),
            Some(authority.pubkey()),
            chunk.to_vec(),
        );
        instructions.push(ix);
    }

    instructions
}

// Add this helper function to verify LUT is ready
pub fn verify_lut_ready(client: &RpcClient, lut_pubkey: &Pubkey) -> Result<bool, ClientError> {
    match client.get_account_with_commitment(lut_pubkey, CommitmentConfig::finalized()) {
        Ok(response) => {
            if let Some(_account) = response.value {
                Ok(true)
            } else {
                Ok(false)
            }
        }
        Err(e) => Err(e),
    }
}

fn deactivate_lut(client: &RpcClient, authority: &Keypair, lut_pubkey: Pubkey) -> Result<()> {
    let (_, blockhash) = get_slot_and_blockhash(&client).unwrap();

    let ix = deactivate_lookup_table(lut_pubkey, authority.pubkey());

    let signature = send_transaction(client, blockhash, authority, &[ix])?;

    // Confirm transaction
    let confirmation = client
        .confirm_transaction_with_spinner(
            &signature,
            &client.get_latest_blockhash()?,
            CommitmentConfig::processed(),
        )
        .unwrap();

    Ok(confirmation)
}

pub fn close_lut(client: &RpcClient, authority: &Keypair, lut_pubkey: Pubkey) {
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

    let extend_lut_blockheight: (Hash, u64) = client
        .get_latest_blockhash_with_commitment(CommitmentConfig::finalized())
        .unwrap();

    let tx = send_transaction(&client, blockhash, authority, &[ix]).unwrap();

    client.confirm_transaction(&tx).unwrap();

    loop {
        let last_hash: u64 = client
            .get_block_height_with_commitment(CommitmentConfig::finalized())
            .unwrap();
        if last_hash > extend_lut_blockheight.1 + 2 {
            break;
        } else {
            sleep(Duration::from_secs(1));
        }
    }
}
