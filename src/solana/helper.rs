use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    address_lookup_table::AddressLookupTableAccount
};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub struct PackedTransaction {
    pub instructions: Vec<Instruction>,
    pub signers: HashSet<Pubkey>, 
    pub accounts: HashMap<Pubkey, bool>, // Tracks non-LUT accounts and their writable status       // Tracks required signers for the transaction
    estimated_size: usize,
}

/// Packs instructions into transactions while respecting Solana's limits with LUT
pub fn pack_instructions(
    instructions: Vec<Instruction>,
    lut: &AddressLookupTableAccount,
) -> Vec<PackedTransaction> {
    const MAX_ACCOUNTS_PER_TX: usize = 256;
    const MAX_TX_SIZE: usize = 1100; // 1232 is the max size of a transaction, but using less to be safe
    const BASE_TX_SIZE: usize = 64 + 3; // Signature (1 signer) + message header

    let lut_addresses: HashSet<Pubkey> = lut.addresses.iter().cloned().collect();
    let mut packed_txs = Vec::new();
    let mut current_tx = PackedTransaction {
        instructions: Vec::new(),
        accounts: HashMap::new(),
        signers: HashSet::new(),
        estimated_size: BASE_TX_SIZE,
    };

    for ix in instructions {
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
                current_tx = PackedTransaction {
                    instructions: Vec::new(),
                    accounts: HashMap::new(),
                    signers: HashSet::new(),
                    estimated_size: BASE_TX_SIZE,
                };
            }
            // Re-evaluate adding the instruction to a new transaction
        }

        // Add the instruction and update accounts
        current_tx.instructions.push(ix);
        current_tx.estimated_size += additional_size + ix_size;
        for (pubkey, is_writable) in new_accounts {
            current_tx.accounts.insert(pubkey, is_writable);
        }
    }

    if !current_tx.instructions.is_empty() {
        packed_txs.push(current_tx);
    }

    packed_txs
}