use super::spls::init_mints;
use crate::{
    config::{JITO_TIP_AMOUNT, MAX_RETRIES, RPC_URL},
    jito::jito::JitoBundle,
    params::KeypairWithAmount,
    pumpfun::swap::PumpSwap,
    solana::utils::{build_transaction, get_admin_keypair, store_secret},
};
use anchor_spl::token::spl_token::instruction::close_account;
use anchor_spl::{
    associated_token::get_associated_token_address,
    token::spl_token::{native_mint::ID, ID as SplID},
};
use solana_client::rpc_client::RpcClient;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::transaction::VersionedTransaction;
use solana_sdk::{address_lookup_table::AddressLookupTableAccount, transaction::Transaction};
use solana_sdk::{instruction::Instruction, pubkey::Pubkey, signature::Keypair, signer::Signer};
use std::{collections::HashMap, str::FromStr, thread::sleep, time::Duration};

/**
 * generate new hop keypair for each buying keypair
 * collect total usdc buying amount and map individual usdc to each hop keypair
 * execute swaps for given usdc amount and set hop keypair as recipient
 * transfer total sol balance from hop keypair to buying keypair
 */

/**
 * STEP 1: GET SWAP IX FROM ADMIN WALLET TO JUP ATA
 * STEP 2: GET LUT PUBKEYS TO EXTEND TO LUT
 * STEP 3: EXTEND LUT WITH PUBKEYS
 * STEP 4: COLLECT SIGNING KEYPAIRS
 * STEP 5: COLLECT REST IXS TO COMPLETE SHADOW SWAP
 * STEP 6: BUILD JITO BUNDLES WITH IXS & SIGNERES
 * STEP 7: COMPLETE SWAPS IN BUNDLES
 * STEP 8: TRANSFER IXS THROUGH CUSTOM PROGRAM
 * STEP 9: (OPTIONAL): WARMUP BUYING WALLETS
 */

struct MintWithAmount {
    pub mint: Pubkey,
    pub amount: u64,
}

pub struct TokenManager {
    swap_provider: PumpSwap,
    tokens: Vec<Pubkey>,
    wallet_to_mint_with_amount: HashMap<Pubkey, MintWithAmount>,
    pubkey_to_keypair: HashMap<Pubkey, Keypair>,
    admin: Keypair,
    client: RpcClient,
}

impl TokenManager {
    pub fn new() -> Self {
        let swap_provider = PumpSwap::new();
        let tokens: Vec<Pubkey> = init_mints();
        let pubkey_to_keypair: HashMap<Pubkey, Keypair> = HashMap::new();
        let wallet_to_mint_with_amount: HashMap<Pubkey, MintWithAmount> = HashMap::new();
        let admin = get_admin_keypair();
        let client = RpcClient::new(RPC_URL);

        Self {
            swap_provider,
            tokens,
            pubkey_to_keypair,
            wallet_to_mint_with_amount,
            admin,
            client,
        }
    }

    //TODO: Implement retry here
    pub async fn shadow_bundle(&mut self, lut: &AddressLookupTableAccount) {
        for (_, swap_info) in self.wallet_to_mint_with_amount.iter() {
            let ixs = self
                .swap_provider
                .buy_ixs(swap_info.mint, swap_info.amount, None)
                .await;
            self.build_send_bundle(ixs, &lut.clone()).await;
        }

        sleep(Duration::from_secs(20));
        for (wallet, swap_info) in self.wallet_to_mint_with_amount.iter() {
            if let Some(keypair) = self.pubkey_to_keypair.get(wallet) {
                let ixs = self
                    .swap_provider
                    .sell_ixs(swap_info.mint, keypair.pubkey(), None, None)
                    .await;
                self.build_send_bundle(ixs, &lut.clone()).await;
            }
        }

        //unwrap WSOL for buyers
        for (_, keypair) in self.pubkey_to_keypair.iter() {
            self.close_tx(keypair).await;
        }
        self.close_admin_wsol();
    }

    pub fn get_lut_extension(&self) -> Vec<Pubkey> {
        let mut all_pubkeys: Vec<Pubkey> = Vec::new();

        for (pubkey, _) in self.pubkey_to_keypair.iter() {
            if let Some(mint) = self.wallet_to_mint_with_amount.get(pubkey) {
                all_pubkeys.push(*pubkey);
                all_pubkeys.push(mint.mint);
                let admin_ata = get_associated_token_address(&self.admin.pubkey(), &mint.mint);
                all_pubkeys.push(admin_ata);
                let recip_wsol_ata = get_associated_token_address(&pubkey, &ID);
                all_pubkeys.push(recip_wsol_ata);
            }
        }
        all_pubkeys
    }

    //1ST CALL
    //generates hop keypairs, collects total swap amount, maps each new keypair to associated buyer keypair, returns swap total usdc amount for admin wallet
    pub async fn init_alloc_ixs(&mut self, wallets: &Vec<KeypairWithAmount>) {
        for (index, mint) in self.tokens.iter().enumerate() {
            let buying_wallet = wallets.get(index);
            if let Some(buying_wallet) = buying_wallet {
                //Insert hop wallet to actual recipient wallet that is stored in DB for buys
                let mint_with_amount = MintWithAmount {
                    mint: *mint,
                    amount: buying_wallet.amount,
                };

                //Map hop pubkey to mint with amount
                self.wallet_to_mint_with_amount
                    .insert(buying_wallet.keypair.pubkey(), mint_with_amount);

                //Map hop pubkey to keypair
                self.pubkey_to_keypair.insert(
                    buying_wallet.keypair.pubkey(),
                    buying_wallet.keypair.insecure_clone(),
                );
            }
        }

        let total: u64 = wallets.iter().map(|wallet| wallet.amount.clone()).sum();
        let priority_fee_amount = 500_000; // 0.0005 SOL
        let fee_ix = ComputeBudgetInstruction::set_compute_unit_price(priority_fee_amount);
        let funding_ix = self.swap_provider.wrap_admin_sol(total);

        let mut instructions: Vec<Instruction> = vec![fee_ix];
        instructions.extend(funding_ix);
        let blockhash = self.client.get_latest_blockhash().unwrap();

        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.admin.pubkey()),
            &vec![&self.admin.insecure_clone()],
            blockhash,
        );

        let sig = self
            .client
            .send_and_confirm_transaction(&transaction)
            .unwrap();
        println!(
            "wrapped total SOL amount for admin wallet with sig: {:?}",
            sig
        );
    }

    async fn close_tx(&self, signer: &Keypair) {
        let priority_fee_amount = 500_000; // 0.0005 SOL

        let fee_ix = ComputeBudgetInstruction::set_compute_unit_price(priority_fee_amount);

        let ata: Pubkey = get_associated_token_address(&signer.pubkey(), &ID);
        let unwrap_wsol = close_account(
            &SplID,
            &ata,
            &signer.pubkey(),
            &signer.pubkey(),
            &[&signer.pubkey(), &self.admin.pubkey()],
        )
        .unwrap();

        let instructions: Vec<Instruction> = vec![fee_ix, unwrap_wsol];
        let blockhash = self.client.get_latest_blockhash().unwrap();

        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.admin.pubkey()),
            &[&signer, &self.admin.insecure_clone()],
            blockhash,
        );

        match self.client.send_and_confirm_transaction(&transaction) {
            Ok(sig) => {
                println!("Sent distirbute tx with sig {:?}", sig.to_string());
            }
            Err(e) => {
                println!(
                    "Failed to send transaction to buying wallet for hop wallet: {:?}",
                    signer.pubkey().to_string()
                );
            }
        };
    }

    fn get_usdc_balance(&self, buy: bool) -> bool {
        let usdc = Pubkey::from_str(USDC).unwrap();
        let first_ata = get_associated_token_address(&self.admin.pubkey(), &usdc);
        let balance = self.client.get_token_account_balance(&first_ata).unwrap();
        if let Some(ui_amount) = balance.ui_amount {
            let balance_not_null = ui_amount > 0.0;
            return buy == balance_not_null;
        }
        false
    }

    async fn build_send_bundle(&self, ixs: Vec<Instruction>, lut: &AddressLookupTableAccount) {
        let mut new_ixs: Vec<Instruction> = Vec::new();
        let priority_fee_ix = ComputeBudgetInstruction::set_compute_unit_price(200_000);
        new_ixs.push(priority_fee_ix);
        new_ixs.extend(ixs.clone());
        loop {
            let tx = build_transaction(
                &self.client,
                &new_ixs,
                vec![&self.admin.insecure_clone()],
                lut.clone(),
                &self.admin,
            );

            let sig = self.client.send_and_confirm_transaction(&tx);

            if let Ok(sig) = sig {
                println!("Transaction successful: {:?}", sig);
                break;
            } else {
                println!("Not successful, retrying...");
                sleep(Duration::from_secs(2));
            }
        }
    }

    fn close_admin_wsol(&self) {
        let admin_wsol_ata = get_associated_token_address(&self.admin.pubkey(), &ID);
        //Close admin ATA
        let close_ix = close_account(
            &SplID,
            &admin_wsol_ata,
            &self.admin.pubkey(),
            &self.admin.pubkey(),
            &vec![&self.admin.pubkey()],
        )
        .unwrap();

        let blockhash = self.client.get_latest_blockhash().unwrap();

        let tx = Transaction::new_signed_with_payer(
            &vec![close_ix],
            Some(&self.admin.pubkey()),
            &vec![&self.admin.insecure_clone()],
            blockhash,
        );

        let sig = self.client.send_and_confirm_transaction(&tx);
        if let Ok(sig) = sig {
            println!(
                "Closed ATA and refunded SOL balance for admin with sig confirm: {:?}",
                sig
            );
        }
    }
}
