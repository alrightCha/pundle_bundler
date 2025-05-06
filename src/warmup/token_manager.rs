use super::spls::init_mints;
use crate::{
    config::{JITO_TIP_AMOUNT, MAX_RETRIES, RPC_URL},
    jito::jito::JitoBundle,
    params::KeypairWithAmount,
    pumpfun::swap::PumpSwap,
    solana::utils::{get_admin_keypair, store_secret},
};
use anchor_spl::token::spl_token::instruction::close_account;
use anchor_spl::{
    associated_token::get_associated_token_address,
    token::spl_token::{native_mint::ID, ID as SplID},
};
use solana_client::rpc_client::RpcClient;
use solana_sdk::address_lookup_table::state::AddressLookupTable;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::message::v0::Message;
use solana_sdk::message::VersionedMessage;
use solana_sdk::transaction::VersionedTransaction;
use solana_sdk::{address_lookup_table::AddressLookupTableAccount, transaction::Transaction};
use solana_sdk::{instruction::Instruction, pubkey::Pubkey, signature::Keypair, signer::Signer};
use std::collections::HashMap;

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
    hop_to_pubkey: HashMap<Pubkey, Pubkey>,
    admin: Keypair,
    client: RpcClient,
    last_funding: Pubkey,
}

impl TokenManager {
    pub fn new() -> Self {
        let swap_provider = PumpSwap::new();
        let tokens: Vec<Pubkey> = init_mints();
        let pubkey_to_keypair: HashMap<Pubkey, Keypair> = HashMap::new();
        let wallet_to_mint_with_amount: HashMap<Pubkey, MintWithAmount> = HashMap::new();
        let hop_to_pubkey: HashMap<Pubkey, Pubkey> = HashMap::new();
        let admin = get_admin_keypair();
        let client = RpcClient::new(RPC_URL);
        let last_funding = Pubkey::default();

        Self {
            swap_provider,
            tokens,
            pubkey_to_keypair,
            wallet_to_mint_with_amount,
            hop_to_pubkey,
            admin,
            client,
            last_funding,
        }
    }

    //TODO: Implement retry here
    pub async fn shadow_bundle(&mut self, wallets: &Vec<KeypairWithAmount>) {
        //Setup hashmaps and wrap total sol needed by admin
        self.init_alloc_ixs(wallets).await;

        //Buy memecoins with admin wallet
        let priority_fee_amount = 500_000; // 0.0005 SOL
        let fee_ix = ComputeBudgetInstruction::set_compute_unit_price(priority_fee_amount);
        //Make admin swaps to tokens
        for (_, swap_info) in self.wallet_to_mint_with_amount.iter() {
            let mut ixs: Vec<Instruction> = Vec::new();
            ixs.push(fee_ix.clone());
            let buy_ixs = self
                .swap_provider
                .buy_ixs(swap_info.mint, swap_info.amount, None)
                .await;
            ixs.extend(buy_ixs);

            let blockhash = self.client.get_latest_blockhash().unwrap();

            let transaction = Transaction::new_signed_with_payer(
                &ixs,
                Some(&self.admin.pubkey()),
                &[&self.admin.insecure_clone()],
                blockhash,
            );

            let sig = self
                .client
                .send_and_confirm_transaction(&transaction)
                .unwrap();
            println!(
                "Bought {:?} for {:?} SOL with admin wallet",
                swap_info.mint, swap_info.amount
            );
            println!("Confirmation TX: {:?}", sig);
        }

        //Sell memecoin, trasnfer WSOL to hop pubkey, unwrap by setting buyer keypair as recipient
        for (_, keypair) in self.pubkey_to_keypair.iter() {
            if let Some(buyer_pubkey) = self.hop_to_pubkey.get(&keypair.pubkey()) {
                if let Some(mint) = self.wallet_to_mint_with_amount.get(&keypair.pubkey()) {
                    let mut ixs: Vec<Instruction> = vec![fee_ix.clone()];
                    let sell_ixs = self
                        .swap_provider
                        .sell_ixs(mint.mint, keypair.pubkey())
                        .await;
                    ixs.extend(sell_ixs);
                    let blockhash = self.client.get_latest_blockhash().unwrap();

                    let transaction = Transaction::new_signed_with_payer(
                        &ixs,
                        Some(&self.admin.pubkey()),
                        &[&self.admin.insecure_clone()],
                        blockhash,
                    );

                    let sig = self
                        .client
                        .send_and_confirm_transaction(&transaction)
                        .unwrap();
                    println!(
                        "Swapped back token {:?} from admin wallet to hop wallet {:?}",
                        mint.mint.to_string(),
                        keypair.pubkey().to_string()
                    );
                    println!("Tx Signature confirmation: {:?}", sig);
                    self.send_tx(keypair, buyer_pubkey).await;
                }
            }
        }
    }

    //1ST CALL
    //generates hop keypairs, collects total swap amount, maps each new keypair to associated buyer keypair, returns swap total usdc amount for admin wallet
    async fn init_alloc_ixs(&mut self, wallets: &Vec<KeypairWithAmount>) {
        for (index, mint) in self.tokens.iter().enumerate() {
            let buying_wallet = wallets.get(index);
            if let Some(buying_wallet) = buying_wallet {
                let hop_keypair = Keypair::new();
                store_secret("hop_keypairs.txt", &hop_keypair);
                println!("New keypair: {:?}", hop_keypair.secret()); 
                //Insert hop wallet to actual recipient wallet that is stored in DB for buys
                self.hop_to_pubkey
                    .insert(hop_keypair.pubkey(), buying_wallet.keypair.pubkey());

                let mint_with_amount = MintWithAmount {
                    mint: *mint,
                    amount: buying_wallet.amount,
                };

                //Map hop pubkey to mint with amount
                self.wallet_to_mint_with_amount
                    .insert(hop_keypair.pubkey(), mint_with_amount);

                //Map hop pubkey to keypair
                self.pubkey_to_keypair
                    .insert(hop_keypair.pubkey(), hop_keypair);
            }
        }
        let mut total: u64 = wallets.iter().map(|wallet| wallet.amount.clone()).sum();
        total = total * 120 / 100;
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

    async fn send_tx(&self, signer: &Keypair, to: &Pubkey) {
        let priority_fee_amount = 500_000; // 0.0005 SOL

        let fee_ix = ComputeBudgetInstruction::set_compute_unit_price(priority_fee_amount);

        let ata: Pubkey = get_associated_token_address(&signer.pubkey(), &ID);
        let unwrap_wsol = close_account(
            &SplID,
            &ata,
            to,
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

    fn build_transaction_multi_luts(
        &self,
        ixes: Vec<Instruction>,
        luts: Vec<Pubkey>,
    ) -> VersionedTransaction {
        let blockhash = self.client.get_latest_blockhash().unwrap();

        let mut all_luts: Vec<AddressLookupTableAccount> = Vec::new();

        for lut in luts {
            let raw_account = self.client.get_account(&lut).unwrap();
            let address_lookup_table = AddressLookupTable::deserialize(&raw_account.data).unwrap();

            let address_lookup_table_account = AddressLookupTableAccount {
                key: lut,
                addresses: address_lookup_table.addresses.to_vec(),
            };
            all_luts.push(address_lookup_table_account);
        }

        let message =
            Message::try_compile(&self.admin.pubkey(), &ixes, &all_luts, blockhash).unwrap();

        // Compile the message with the payer's public key

        let versioned_message = VersionedMessage::V0(message);

        // Create the transaction with all keypairs as signers
        VersionedTransaction::try_new(versioned_message, &vec![&self.admin.insecure_clone()])
            .unwrap()
    }
}
