use super::spls::JUP;
use crate::{
    config::{JITO_TIP_AMOUNT, MAX_RETRIES, RPC_URL},
    jito::jito::JitoBundle,
    jupiter::swap::{shadow_swap, swap_ixs, tokens_for_sol},
    params::KeypairWithAmount,
    solana::utils::{build_transaction, get_admin_keypair, store_secret},
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
use std::{collections::HashMap, str::FromStr};

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

pub struct TokenManager {
    jup: Pubkey,
    wallet_to_amount: HashMap<Pubkey, u64>,
    pubkey_to_keypair: HashMap<Pubkey, Keypair>,
    hop_to_pubkey: HashMap<Pubkey, Pubkey>,
    admin: Keypair,
    client: RpcClient,
    last_funding: Pubkey,
}

impl TokenManager {
    pub fn new() -> Self {
        let jup: Pubkey = Pubkey::from_str(JUP).unwrap();
        let pubkey_to_keypair: HashMap<Pubkey, Keypair> = HashMap::new();
        let wallet_to_amount: HashMap<Pubkey, u64> = HashMap::new();
        let hop_to_pubkey: HashMap<Pubkey, Pubkey> = HashMap::new();
        let admin = get_admin_keypair();
        let client = RpcClient::new(RPC_URL);
        let last_funding = Pubkey::default();

        Self {
            jup,
            pubkey_to_keypair,
            wallet_to_amount,
            hop_to_pubkey,
            admin,
            client,
            last_funding,
        }
    }

    //TODO: Implement retry here
    pub async fn shadow_bundle(
        &mut self,
        wallets: &Vec<KeypairWithAmount>,
        lut: &AddressLookupTableAccount,
    ) {
        let jito = JitoBundle::new(MAX_RETRIES, JITO_TIP_AMOUNT);
        let priority_fee_amount = 500_000; // 0.0005 SOL
        let tip_ix = jito.get_tip_ix(self.admin.pubkey(), None).await.unwrap();
        let fee_ix = ComputeBudgetInstruction::set_compute_unit_price(priority_fee_amount);

        let jito = JitoBundle::new(MAX_RETRIES, JITO_TIP_AMOUNT);

        let mut txs: Vec<VersionedTransaction> = Vec::new();

        let fund_ixs = self.init_alloc_ixs(wallets).await;

        let fund_tx = build_transaction(&self.client, &fund_ixs, vec![], lut.clone(), &self.admin);
        txs.push(fund_tx);

        let shadow_swaps: Vec<(Vec<Instruction>, Vec<Pubkey>)> = self.hop_alloc_ixs().await;

        let mut counter = 1;
        let swap_count = shadow_swaps.len();
        for (index, (ixs, luts)) in shadow_swaps.iter().enumerate() {
            let mut final_ixs: Vec<Instruction> = Vec::new();
            final_ixs.push(fee_ix.clone());
            final_ixs.extend(ixs.clone());
            if counter % 5 == 4 || swap_count < 5 && index == swap_count - 1 {
                final_ixs.push(tip_ix.clone());
            }
            let tx = self.build_transaction_multi_luts(final_ixs, luts.clone());
            txs.push(tx);
            counter += 1;
        }

        let chunks: Vec<_> = txs.chunks(5).collect();

        for chunk in chunks {
            let chunk_vec = chunk.to_vec();
            let _ = jito
                .process_bundle(chunk_vec.clone(), Pubkey::default(), None)
                .await;
        }
    }

    //4TH CALL: DISTIRBUTE SOL FROM HOP WALLETS TO BUYING WALLETS
    pub async fn final_distribute(&mut self) {
        for (pubkey, keypair) in self.pubkey_to_keypair.iter() {
            let buying_pubkey = self.hop_to_pubkey.get(&*pubkey);
            if let Some(buying_pubkey) = buying_pubkey {
                self.send_tx(keypair, buying_pubkey).await;
            }
        }
        self.reset_maps();
    }

    //1ST CALL
    //generates hop keypairs, collects total swap amount, maps each new keypair to associated buyer keypair, returns swap total usdc amount for admin wallet
    async fn init_alloc_ixs(&mut self, wallets: &Vec<KeypairWithAmount>) -> Vec<Instruction> {
        let mut total: u64 = wallets.iter().map(|wallet| wallet.amount.clone()).sum(); 
        total = total * 97 / 100; 
        let total_tokens = tokens_for_sol(self.jup, total.clone()).await.unwrap_or(0); 
        for wallet in wallets.iter() {
            let amount = wallet.amount * total_tokens / total; 
            let new_kp = Keypair::new();
            store_secret("hops.txt", &new_kp);
            self.hop_to_pubkey
                .insert(new_kp.pubkey(), wallet.keypair.pubkey());
            println!(
                "Amount in JUP: {} for wallet: {}",
                amount,
                new_kp.pubkey().to_string()
            );
            self.handle_wallet(amount, new_kp);
        }
        let swap_ixs = swap_ixs(&self.admin, self.jup, Some(total), None, false)
            .await
            .unwrap();

        swap_ixs
    }

    //3RD CALL -> GET IXS
    //Distributes tokens to respective token accounts, then cleans up token accounts and unwraps wsol to sol with jup cleanup and gasless txs
    async fn hop_alloc_ixs(&mut self) -> Vec<(Vec<Instruction>, Vec<Pubkey>)> {
        let mut discrete_swaps_txs: Vec<(Vec<Instruction>, Vec<Pubkey>)> = Vec::new();
        for (pubkey, amount) in self.wallet_to_amount.iter() {
            let swap_ixs = shadow_swap(
                &self.client,
                &self.admin,
                self.jup,
                *pubkey,
                None,
                *amount,
            )
            .await
            .unwrap();
            discrete_swaps_txs.push(swap_ixs);
            self.last_funding = pubkey.clone();
        }
        discrete_swaps_txs
    }

    fn handle_wallet(&mut self, amount: u64, wallet: Keypair) {
        // Need to use wallet.pubkey() as the key since Keypair doesn't implement Hash/Eq
        self.wallet_to_amount.insert(wallet.pubkey(), amount);
        self.pubkey_to_keypair.insert(wallet.pubkey(), wallet);
    }

    fn reset_maps(&mut self) {
        self.wallet_to_amount = HashMap::new();
        self.pubkey_to_keypair = HashMap::new();
        self.hop_to_pubkey = HashMap::new();
    }

    async fn send_tx(&self, signer: &Keypair, to: &Pubkey) {
        let priority_fee_amount = 500_000; // 0.0005 SOL

        let fee_ix = ComputeBudgetInstruction::set_compute_unit_price(priority_fee_amount);

        let ata: Pubkey = get_associated_token_address(&signer.pubkey(), &ID);
        let unwrap_wsol =
            close_account(&SplID, &ata, to, &signer.pubkey(), &[&signer.pubkey(), &self.admin.pubkey()]).unwrap();

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
