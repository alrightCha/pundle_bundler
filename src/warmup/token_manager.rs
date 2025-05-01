use super::spls::init_mints;
use crate::{
    config::RPC_URL,
    jupiter::swap::{shadow_swap, swap_ixs},
    params::KeypairWithAmount,
    solana::utils::{build_transaction, get_admin_keypair},
};
use anchor_spl::associated_token::get_associated_token_address;
use solana_client::rpc_client::RpcClient;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::transaction::Transaction;
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
};
use std::collections::HashMap;

pub struct TokenManager {
    mints: Vec<Pubkey>,
    mint_to_wallet: HashMap<Pubkey, Keypair>,
    admin: Keypair,
    client: RpcClient,
}

impl TokenManager {
    pub fn new() -> Self {
        let mints: Vec<Pubkey> = init_mints();
        let mint_to_wallet: HashMap<Pubkey, Keypair> = HashMap::new();
        let admin = get_admin_keypair();
        let client = RpcClient::new(RPC_URL);

        Self {
            mints,
            mint_to_wallet,
            admin,
            client,
        }
    }

    //Distributes tokens to respective token accounts, then cleans up token accounts and unwraps wsol to sol with jup cleanup and gasless txs
    pub async fn discrete_distribute(&mut self) {
        for (_, mint) in self.mints.iter().enumerate() {
            let wallet = self.get_wallet_for_mint(mint.clone());
            if let Some(wallet) = wallet {
                //Get instructions for swap, then cleanup with gas less tx from jito
                println!(
                    "Attempting to shadow swap {:?} to wallet {:?}",
                    mint.to_string(),
                    wallet.pubkey().to_string()
                );
                let ixs = shadow_swap(&self.client, &self.admin, *mint, wallet.pubkey(), Some(500))
                    .await
                    .unwrap();
                self.send_tx(ixs, Some(wallet)).await;
            }
        }
        self.cleanup();
    }

    //sets each wallet to the designated mint & returns the buying instructions for each token
    pub async fn swap_buys(&mut self, wallets: &Vec<KeypairWithAmount>) {
        for (index, wallet) in wallets.iter().enumerate() {
            let mint = self.mints[index];
            self.set_mint_for_wallet(wallet.keypair.insecure_clone(), mint.clone());
            let swap_ixs = swap_ixs(&self.admin, mint, Some(wallet.amount), Some(500), false)
                .await
                .unwrap();

            let sig = self.send_tx(swap_ixs, None).await;
            println!("Swapped {:?} with confirmation. Sig: {:?}", mint, sig);
        }
    }

    fn get_wallet_for_mint(&self, mint: Pubkey) -> Option<Keypair> {
        let wallet = self.mint_to_wallet.get(&mint);
        if let Some(wallet) = wallet {
            Some(wallet.insecure_clone())
        } else {
            None
        }
    }

    fn set_mint_for_wallet(&mut self, wallet: Keypair, mint: Pubkey) {
        self.mint_to_wallet.insert(mint, wallet);
    }

    fn reset_map(&mut self) {
        self.mint_to_wallet = HashMap::new();
    }

    async fn cleanup(&mut self) {
        let mut all_instructions = Vec::new();
        let mut current_bundle = Vec::new();

        for (index, mint) in self.mints.iter().enumerate() {
            let ata = get_associated_token_address(&self.admin.pubkey(), mint);
            if !self.client.get_account(&ata).is_err() {
                let ixs = swap_ixs(&self.admin, *mint, None, Some(200), true).await.unwrap();
                current_bundle.extend(ixs);
                
                // When we have 2 mints worth of instructions or this is the last mint
                if current_bundle.len() >= 2 || index == self.mints.len() - 1 {
                    all_instructions.push(current_bundle.clone());
                    current_bundle.clear();
                }
            }
        }

        // Send each bundle of instructions
        for bundle in all_instructions {
            self.send_tx(bundle, None).await;
        }

        self.reset_map();
    }

    async fn send_tx(&self, ixs: Vec<Instruction>, signer: Option<Keypair>) {
        let max_retries = 3;
        let mut retry_count = 0;

        loop {
            let priority_fee_amount = 500_000; // 0.0005 SOL
            let fee_ix = ComputeBudgetInstruction::set_compute_unit_price(priority_fee_amount);
            let mut instructions: Vec<Instruction> = vec![fee_ix];
            instructions.extend(ixs.clone());

            let blockhash = self.client.get_latest_blockhash().unwrap();

            let mut signers: Vec<Keypair> = vec![self.admin.insecure_clone()];

            if let Some(ref signer) = signer {
                signers.push(signer.insecure_clone());
            }

            let tx_signers: Vec<&Keypair> = signers.iter().collect();

            let transaction = Transaction::new_signed_with_payer(
                &instructions,
                Some(&self.admin.pubkey()),
                &tx_signers,
                blockhash,
            );

            match self.client.send_and_confirm_transaction(&transaction) {
                Ok(sig) => println!("Sent distirbute tx with sig {:?}", sig.to_string()),
                Err(e) => {
                    retry_count += 1;
                    if retry_count >= max_retries {
                        println!(
                            "Failed to send transaction after {} retries: {}",
                            max_retries, e
                        );
                    }
                    println!(
                        "Transaction failed, retrying ({}/{}): {}",
                        retry_count, max_retries, e
                    );
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    continue;
                }
            };
        }
    }
}
