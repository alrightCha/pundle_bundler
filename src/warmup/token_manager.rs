use super::spls::{get_aggregators, JUP};
use crate::{
    config::{JITO_TIP_AMOUNT, MAX_RETRIES, RPC_URL},
    jito::jito::JitoBundle,
    jupiter::swap::{shadow_swap, sol_for_tokens, swap_ixs},
    params::KeypairWithAmount,
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

pub struct TokenManager {
    jup: Pubkey,
    wallet_to_amount: HashMap<Pubkey, u64>,
    pubkey_to_keypair: HashMap<Pubkey, Keypair>,
    hop_to_pubkey: HashMap<Pubkey, Pubkey>,
    admin: Keypair,
    client: RpcClient,
    shadow_ixs: Vec<Instruction>,
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

        let shadow_ixs: Vec<Instruction> = Vec::new();
        let last_funding = Pubkey::default();

        Self {
            jup,
            pubkey_to_keypair,
            wallet_to_amount,
            hop_to_pubkey,
            admin,
            client,
            shadow_ixs,
            last_funding,
        }
    }

    //1ST CALL
    //generates hop keypairs, collects total swap amount, maps each new keypair to associated buyer keypair, returns swap total usdc amount for admin wallet
    pub async fn init_alloc_ixs(&mut self, wallets: &Vec<KeypairWithAmount>) {
        let mut total: u64 = 0;
        for wallet in wallets.iter() {
            if let Ok(amount) = sol_for_tokens(self.jup, wallet.amount).await {
                total += wallet.amount;
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
        }
        let swap_ixs = swap_ixs(&self.admin, self.jup, Some(total), Some(500), false)
            .await
            .unwrap();
        //Reset our shadow vector and extend it
        self.shadow_ixs = Vec::new();
        self.shadow_ixs.extend(swap_ixs);
        self.hop_alloc_ixs().await;
    }

    //2ND CALL
    //Return :
    //Admin jup token account pubkey
    //Jup address
    //hop pubkeys
    //Jup aggregators
    pub fn get_pubkeys_for_lut(&self) -> Vec<Pubkey> {
        let mut pubkeys: Vec<Pubkey> = Vec::new();
        pubkeys.push(self.jup.clone());
        let admin_jup_ata = get_associated_token_address(&self.admin.pubkey(), &self.jup);
        pubkeys.push(admin_jup_ata);
        for (pubkey, _) in self.pubkey_to_keypair.iter() {
            pubkeys.push(pubkey.clone());
        }
        let aggregators = get_aggregators();
        pubkeys.extend(aggregators);
        pubkeys
    }

    //3RD CALL -> GET IXS
    //Distributes tokens to respective token accounts, then cleans up token accounts and unwraps wsol to sol with jup cleanup and gasless txs
    async fn hop_alloc_ixs(&mut self) {
        let mut discrete_swaps_ixs: Vec<Instruction> = Vec::new();
        for (pubkey, amount) in self.wallet_to_amount.iter() {
            let swap_ixs = shadow_swap(
                &self.client,
                &self.admin,
                self.jup,
                *pubkey,
                Some(500),
                *amount,
            )
            .await
            .unwrap();
            discrete_swaps_ixs.extend(swap_ixs);
            self.last_funding = pubkey.clone();
        }
        self.shadow_ixs.extend(discrete_swaps_ixs);
    }

    pub async fn shadow_bundle(&self, lut: &AddressLookupTableAccount) -> bool {
        let priority_fee_amount = 500_000; // 0.0005 SOL
        let jito = JitoBundle::new(MAX_RETRIES, JITO_TIP_AMOUNT);

        let tip_ix = jito.get_tip_ix(self.admin.pubkey(), None).await.unwrap();
        let fee_ix = ComputeBudgetInstruction::set_compute_unit_price(priority_fee_amount);

        let mut bundle_txs: Vec<VersionedTransaction> = Vec::new();
        let mut tx_ixs: Vec<Instruction> = Vec::new();
        tx_ixs.push(fee_ix.clone());
        
        let mut pushed = false; 
        for ix in self.shadow_ixs.iter() {
            let mut maybe_ixs: Vec<Instruction> = tx_ixs.clone();
            maybe_ixs.push(ix.clone());
            println!("#1");
            self.print_signers(&maybe_ixs);
            let maybe_tx =
                build_transaction(&self.client, &maybe_ixs, vec![], lut.clone(), &self.admin);
            let size: usize = bincode::serialized_size(&maybe_tx).unwrap() as usize;
            println!("Maybe instruction count: {:?}", maybe_ixs.len()); 
            println!("Maybe size is: {:?}", size); 
            if size >= 1232 {
                println!("#2");
                self.print_signers(&tx_ixs);
                let tx = build_transaction(&self.client, &tx_ixs, vec![], lut.clone(), &self.admin);
                bundle_txs.push(tx);
                tx_ixs = vec![fee_ix.clone(), ix.clone()];
                if bundle_txs.len() % 5 == 4 && !pushed {
                    tx_ixs.push(tip_ix.clone());
                    pushed = true; 
                }else{
                    pushed = false; 
                }
            } else {
                tx_ixs.push(ix.clone());
            }
        }

        if tx_ixs.len() > 1 {
            tx_ixs.push(tip_ix);
            println!("#3");
            self.print_signers(&tx_ixs);
            let last_tx =
                build_transaction(&self.client, &tx_ixs, vec![], lut.clone(), &self.admin);
            bundle_txs.push(last_tx);
        }

        let chunks: Vec<_> = bundle_txs.chunks(5).collect();

        for chunk in chunks {
            let chunk_vec = chunk.to_vec();
            let _ = jito
                .process_bundle(chunk_vec, Pubkey::default(), None)
                .await;
        }

        //If final funding balance is higher than 0, we have successfully funded all hop keypairs
        let mut retries = 0;
        while retries < 3 {
            let res = self.client.get_balance(&self.last_funding).unwrap();
            let final_balance = res > 0;
            if !final_balance {
                println!("Final balance not reached, retrying..");
                sleep(Duration::from_secs(5));
                retries += 1;
            } else {
                println!("Final balance has been reached");
                return true;
            }
        }
        false
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
        let unwrap_wsol = close_account(&SplID, &ata, to, &signer.pubkey(), &[&to]).unwrap();

        let instructions: Vec<Instruction> = vec![fee_ix, unwrap_wsol];
        let blockhash = self.client.get_latest_blockhash().unwrap();

        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&signer.pubkey()),
            &[&signer],
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

    fn print_signers(&self, ixs: &Vec<Instruction>) {
        for ix in ixs {
            for acc in ix.accounts.iter().filter(|acc| acc.is_signer) {
                println!("Signer needed: {}", acc.pubkey.to_string());
            }
        }
    }
}
