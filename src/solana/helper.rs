use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    address_lookup_table::AddressLookupTableAccount,
    compute_budget::ComputeBudgetInstruction,
};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub struct PackedTransaction {
    pub instructions: Vec<Instruction>,
    pub signers: HashSet<Pubkey>, 
    pub accounts: HashMap<Pubkey, bool>, // Tracks non-LUT accounts and their writable status       // Tracks required signers for the transaction
    estimated_size: usize,
}

fn initialize_tx() -> PackedTransaction {
    const BASE_TX_SIZE: usize = 64 + 3; // Signature (1 signer) + message header
    const PRIORITY_FEE_IX_SIZE: usize = 9; // Size of priority fee instruction (1 + 0 accounts + 8 bytes data)

    let mut tx = PackedTransaction {
        instructions: Vec::new(),
        accounts: HashMap::new(),
        signers: HashSet::new(),
        estimated_size: BASE_TX_SIZE + PRIORITY_FEE_IX_SIZE,
    };

    let priority_fee_amount = 200_000; // 0.000007 SOL
    // Create priority fee instruction
    let set_compute_unit_price_ix = ComputeBudgetInstruction::set_compute_unit_price(priority_fee_amount);
    tx.instructions.push(set_compute_unit_price_ix);

    tx
}

/// Packs instructions into transactions while respecting Solana's limits with LUT
pub fn pack_instructions(
    mut instructions: Vec<Instruction>,
    tip_ix: Instruction,
    lut: &AddressLookupTableAccount,
) -> Vec<PackedTransaction> {
    const MAX_ACCOUNTS_PER_TX: usize = 256;
    const MAX_TX_SIZE: usize = 1100; // 1232 is the max size of a transaction, but using less to be safe

    let lut_addresses: HashSet<Pubkey> = lut.addresses.iter().cloned().collect();
    let mut packed_txs = Vec::new();
    let mut current_tx = initialize_tx();
    let mut i = 0;

    while i < instructions.len() {
        let ix = instructions[i].clone();
        let mut new_accounts = HashMap::new();
        let mut additional_size = 0;
        let mut new_signers = HashSet::new();

        // Check program ID
        if !lut_addresses.contains(&ix.program_id) {
            if !current_tx.accounts.contains_key(&ix.program_id) {
                additional_size += 32;
                new_accounts.insert(ix.program_id, false); // Program ID is readonly
            }
        }

        // Check instruction accounts
        for account in &ix.accounts {
            let pubkey = account.pubkey;
            println!("Account: {:?} is signer : {:?}", pubkey, account.is_signer);
            // Track if account is a signer
            if account.is_signer {
                new_signers.insert(pubkey);
            }

            if lut_addresses.contains(&pubkey) {
                continue;
            }

            let is_writable = account.is_writable;
            if let Some(current_writable) = current_tx.accounts.get(&pubkey) {
                if !current_writable && is_writable {
                    new_accounts.insert(pubkey, true);
                }
            } else {
                additional_size += 32;
                new_accounts.insert(pubkey, is_writable);
            }
        }

        // Calculate the number of new signers and additional signature size
        let existing_signers_count = current_tx.signers.len();
        current_tx.signers.extend(new_signers);
        let new_signers_count = current_tx.signers.len() - existing_signers_count;
        additional_size += new_signers_count * 64;

        // Calculate instruction size: program index + account indexes + data
        let ix_size = 1 + ix.accounts.len() + ix.data.len();
        let total_accounts = current_tx.accounts.len() + new_accounts.len();

        // Check if adding this instruction exceeds limits
        if current_tx.estimated_size + additional_size + ix_size > MAX_TX_SIZE ||
           total_accounts > MAX_ACCOUNTS_PER_TX {
            if !current_tx.instructions.is_empty() {
                packed_txs.push(current_tx);
                current_tx = initialize_tx();
            }
            // Re-evaluate adding the instruction to a new transaction
            continue;
        }

        // Add the instruction and update accounts
        current_tx.instructions.push(ix);
        current_tx.estimated_size += additional_size + ix_size;
        for (pubkey, is_writable) in new_accounts {
            current_tx.accounts.insert(pubkey, is_writable);
        }

        // Check if we need to add tip_ix (on transactions 5, 10, 15, etc.)
        if packed_txs.len() == 4 && !current_tx.instructions.is_empty() {
            // Calculate size requirements for tip_ix
            let mut tip_additional_size = 0;
            let mut tip_new_accounts = HashMap::new();
            let mut tip_new_signers = HashSet::new();

            // Check program ID for tip_ix
            if !lut_addresses.contains(&tip_ix.program_id) {
                if !current_tx.accounts.contains_key(&tip_ix.program_id) {
                    tip_additional_size += 32;
                    tip_new_accounts.insert(tip_ix.program_id, false);
                }
            }

            // Check accounts for tip_ix
            for account in &tip_ix.accounts {
                let pubkey = account.pubkey;
                if account.is_signer {
                    tip_new_signers.insert(pubkey);
                }

                if lut_addresses.contains(&pubkey) {
                    continue;
                }

                let is_writable = account.is_writable;
                if let Some(current_writable) = current_tx.accounts.get(&pubkey) {
                    if !current_writable && is_writable {
                        tip_new_accounts.insert(pubkey, true);
                    }
                } else {
                    tip_additional_size += 32;
                    tip_new_accounts.insert(pubkey, is_writable);
                }
            }

            let tip_ix_size = 1 + tip_ix.accounts.len() + tip_ix.data.len();
            let total_tip_accounts = current_tx.accounts.len() + tip_new_accounts.len();

            // If adding tip_ix would exceed limits, pop last instruction and try again
            if current_tx.estimated_size + tip_additional_size + tip_ix_size > MAX_TX_SIZE ||
               total_tip_accounts > MAX_ACCOUNTS_PER_TX {
                if let Some(last_ix) = current_tx.instructions.pop() {
                    // Reinsert the popped instruction at the beginning for next iteration
                    instructions.insert(i, last_ix);
                    i -= 1;
                }
            }

            // Add tip_ix
            current_tx.instructions.push(tip_ix.clone());
            current_tx.estimated_size += tip_additional_size + tip_ix_size;
            for (pubkey, is_writable) in tip_new_accounts {
                current_tx.accounts.insert(pubkey, is_writable);
            }
            current_tx.signers.extend(tip_new_signers);

            // Push the transaction with tip_ix
            packed_txs.push(current_tx);
            current_tx = initialize_tx();
        }

        i += 1;
    }

    if !current_tx.instructions.is_empty() {
        packed_txs.push(current_tx);
    }

    packed_txs
}