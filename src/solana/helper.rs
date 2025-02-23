use solana_sdk::{
    address_lookup_table::AddressLookupTableAccount, compute_budget::ComputeBudgetInstruction, instruction::Instruction, pubkey::Pubkey, signer::Signer, transaction::VersionedTransaction
};
use crate::params::InstructionWithSigners;
use solana_client::rpc_client::RpcClient;
use crate::solana::utils::build_transaction;

fn get_priority_fee_ix() -> Instruction {
    let priority_fee_amount = 2_000_000; // 0.000007 SOL
    // Create priority fee instruction
    let set_compute_unit_price_ix = ComputeBudgetInstruction::set_compute_unit_price(priority_fee_amount);
    set_compute_unit_price_ix
}

/// Packs instructions into transactions while respecting Solana's limits with LUT
pub fn pack_instructions(
    instructions: Vec<InstructionWithSigners>,
    client: &RpcClient,
    lut: &AddressLookupTableAccount,
) -> Vec<VersionedTransaction> {
    const MAX_TX_SIZE: usize = 1230;
    let mut packed_txs: Vec<VersionedTransaction> = Vec::new();
    let mut new_tx_ixs: InstructionWithSigners = InstructionWithSigners {
        instructions: Vec::new(),
        signers: Vec::new(),
    };

    let fee_ix = get_priority_fee_ix();
    let mut seen: Vec<Pubkey> = Vec::new();
    new_tx_ixs.instructions.push(fee_ix);

    for ix in instructions {
        let signers = ix.signers;
        let instructions = ix.instructions;
        let mut mock_ixs = new_tx_ixs;
        mock_ixs.instructions.extend(instructions.clone());
        
        // Add new signers that haven't been seen
        for signer in signers.iter() {
            if !seen.contains(&signer.pubkey()) {
                seen.push(signer.pubkey());
                mock_ixs.signers.push(signer);
            }
        }

        let tx = build_transaction(&client, &mock_ixs.instructions, mock_ixs.signers.clone(), lut.clone());
        let size: usize = bincode::serialized_size(&tx).unwrap() as usize;

        if size < MAX_TX_SIZE {
            // If size is ok, update new_tx_ixs with the mock version
            new_tx_ixs = mock_ixs;
        } else {
            // If size would be too large, pack current tx and start a new one
            if !instructions.is_empty() {
                let tx = build_transaction(&client, &instructions, signers.clone(), lut.clone());
                packed_txs.push(tx);
            }

            // Start new transaction with current instruction
            new_tx_ixs = InstructionWithSigners {
                instructions: vec![get_priority_fee_ix()],
                signers: Vec::new(),
            };
            seen = Vec::new();

            // Add current instruction to new transaction
            new_tx_ixs.instructions.extend(instructions);
            for signer in signers.clone().iter() {
                seen.push(signer.pubkey());
                new_tx_ixs.signers.push(signer);
            }
        }
    }

    // Pack any remaining instructions into a final transaction
    if !new_tx_ixs.instructions.is_empty() {
        let tx = build_transaction(&client, &new_tx_ixs.instructions, new_tx_ixs.signers.clone(), lut.clone());
        packed_txs.push(tx);
    }

    packed_txs
}