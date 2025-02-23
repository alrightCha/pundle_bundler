use solana_sdk::signer::Signer;
use solana_sdk::{
    instruction::Instruction,
    transaction::VersionedTransaction,
    address_lookup_table::AddressLookupTableAccount
};
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use crate::params::InstructionWithSigners;
use solana_client::rpc_client::RpcClient;
use std::collections::HashSet;
use crate::solana::utils::build_transaction;

fn get_priority_fee_ix() -> Instruction {
    let priority_fee_amount = 2_000_000; // 0.000007 SOL
    // Create priority fee instruction
    let set_compute_unit_price_ix = ComputeBudgetInstruction::set_compute_unit_price(priority_fee_amount);
    set_compute_unit_price_ix
}

pub fn pack_instructions(
    instructions: Vec<InstructionWithSigners>,
    client: &RpcClient,
    lut: &AddressLookupTableAccount,
    max_tx_size: usize,
) -> Vec<VersionedTransaction> {

    let mut batches = Vec::new();
    let mut current_batch = Vec::new();
    let mut seen_pubkeys = HashSet::new();
    let mut current_signers = Vec::new();

    for ix_with_signers in instructions {
        // Create temp version for size checking
        let mut temp_instructions = Vec::new();
        let tip_ix = get_priority_fee_ix();
        temp_instructions.push(&tip_ix);
        let mut temp_signers = current_signers.clone();
        let mut temp_seen = seen_pubkeys.clone();
        
        // Add new instructions
        temp_instructions.extend(current_batch.iter().flat_map(|iw: &InstructionWithSigners| iw.instructions.iter()));
        temp_instructions.extend(ix_with_signers.instructions.iter());

        // Add new signers
        for signer in &ix_with_signers.signers {
            let pubkey = signer.pubkey();
            if !temp_seen.contains(&pubkey) {
                temp_signers.push(*signer);
                temp_seen.insert(pubkey);
            }
        }

        // Build test transaction
        let tx = build_transaction(
            client,
            &temp_instructions.into_iter().map(|i| i.clone()).collect::<Vec<Instruction>>(),
            temp_signers.clone(),
            lut.clone()
        );

        if bincode::serialized_size(&tx).unwrap() as usize <= max_tx_size {
            // Keep the added instruction
            current_batch.push(ix_with_signers);
            current_signers = temp_signers;
            seen_pubkeys = temp_seen;
        } else {
            // Commit current batch if not empty
            if !current_batch.is_empty() {
                batches.push((current_batch, current_signers));
                current_batch = Vec::new();
                seen_pubkeys.clear();
                current_signers = Vec::new();
            }

            // Check if single instruction fits
            let tx = build_transaction(
                client,
                &ix_with_signers.instructions,
                ix_with_signers.signers.iter().copied().collect(),
                lut.clone()
            );
            
            if bincode::serialized_size(&tx).unwrap() as usize > max_tx_size {
                panic!("Single instruction exceeds size limit");
            }
            
            // Add to new batch
            current_batch.push(ix_with_signers);
            for signer in &current_batch.last().unwrap().signers {
                let pubkey = signer.pubkey();
                if !seen_pubkeys.contains(&pubkey) {
                    current_signers.push(*signer);
                    seen_pubkeys.insert(pubkey);
                }
            }
        }
    }

    // Add final batch
    if !current_batch.is_empty() {
        batches.push((current_batch, current_signers));
    }

    // Convert to transactions
    batches.into_iter().map(|(batch, signers)| {
        let instructions: Vec<Instruction> = batch.iter()
            .flat_map(|iw| iw.instructions.iter())
            .cloned()
            .collect();

        build_transaction(client, &instructions, signers, lut.clone())
    }).collect()
}