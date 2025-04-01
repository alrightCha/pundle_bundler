use super::jito::JitoBundle;
use crate::config::{BUFFER_AMOUNT, FEE_AMOUNT};
use crate::config::{JITO_TIP_AMOUNT, MAX_RETRIES, RPC_URL};
use crate::params::KeypairWithAmount;
use crate::pumpfun::pump::PumpFun;
use crate::solana::utils::{build_transaction, load_keypair, test_transactions};
use dotenv::dotenv;
use pumpfun_cpi::instruction::Create;
use solana_client::rpc_client::RpcClient;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::{
    address_lookup_table::AddressLookupTableAccount,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    sysvar::rent::Rent,
    transaction::VersionedTransaction,
};
use std::collections::HashSet;
use std::env;
use std::sync::Arc;
/*
keypairs_with_amount: Vec<KeypairWithAmount>,
dev_keypair_with_amount: KeypairWithAmount,
mint: Keypair,
requester_pubkey: String,
token_metadata: CreateTokenMetadata
*/

pub struct BundleTransactions {
    admin_keypair: Keypair,
    dev_keypair: Keypair,
    mint_keypair: Keypair,
    client: RpcClient,
    pumpfun_client: PumpFun,
    jito: JitoBundle,
    address_lookup_table_account: AddressLookupTableAccount,
    keypairs_to_treat: Vec<KeypairWithAmount>,
    treated_keypairs: Pubkey,
}

const MAX_TX_SIZE: usize = 1232;

impl BundleTransactions {
    pub fn new(
        dev_keypair: Keypair,
        mint_keypair: &Keypair,
        address_lookup_table_account: AddressLookupTableAccount,
        others_with_amount: Vec<KeypairWithAmount>,
    ) -> Self {
        dotenv().ok();
        //Load admin keypair
        let admin_keypair_path = env::var("ADMIN_KEYPAIR").unwrap();
        let admin_keypair = load_keypair(&admin_keypair_path).unwrap();

        let client = RpcClient::new(RPC_URL);

        let jito_rpc = RpcClient::new_with_commitment(
            RPC_URL.to_string(),
            solana_sdk::commitment_config::CommitmentConfig::confirmed(),
        );

        let jito = JitoBundle::new(jito_rpc, MAX_RETRIES, JITO_TIP_AMOUNT);

        let dev: Keypair = dev_keypair.insecure_clone();
        let payer: Arc<Keypair> = Arc::new(dev);

        let pumpfun_client = PumpFun::new(payer);

        let keypairs_to_treat: Vec<KeypairWithAmount> = others_with_amount;
        let treated_keypairs: Pubkey = Pubkey::default();
        let mint_keypair: Keypair = mint_keypair.insecure_clone();

        println!("Creating BundleTransactions Class with lut: {:?}", address_lookup_table_account.key.to_string());
        println!("Accounts in lut : {:?}", address_lookup_table_account.addresses);

        Self {
            admin_keypair,
            dev_keypair,
            mint_keypair,
            client,
            pumpfun_client,
            jito,
            address_lookup_table_account,
            keypairs_to_treat,
            treated_keypairs,
        }
    }
    //Separate logic of checking txs size into separate function
    //When adding ixs, remove actual keypairwithamount from keypairs to treat
    pub async fn collect_first_bundle_txs(
        &mut self,
        dev_amount: u64,
        token_metadata: Create,
    ) -> Vec<VersionedTransaction> {
        let mut tip_ix_count = 0;
        let rent = Rent::default();
        let rent_exempt_min = rent.minimum_balance(0);

        let for_many: u64 = BUFFER_AMOUNT * std::cmp::max(self.keypairs_to_treat.len(), 10) as u64;
        let to_sub_for_dev: u64 = rent_exempt_min + FEE_AMOUNT + JITO_TIP_AMOUNT + for_many;

        let final_dev_buy_amount = dev_amount - to_sub_for_dev;

        let mut transactions: Vec<VersionedTransaction> = Vec::new();

        let mut all_ixs: Vec<(Pubkey, Instruction)> = Vec::new();

        let mint_ix = self
            .pumpfun_client
            .create_instruction(&self.mint_keypair, token_metadata);

        let dev_ix = self
            .pumpfun_client
            .buy_ixs(
                &self.mint_keypair.pubkey(),
                &self.dev_keypair,
                final_dev_buy_amount,
                None,
                true,
            )
            .await
            .unwrap();

        all_ixs.push((self.mint_keypair.pubkey(), mint_ix));
        for ix in dev_ix {
            all_ixs.push((self.dev_keypair.pubkey(), ix));
        }

        let mint_pubkey: Pubkey = self.mint_keypair.pubkey();

        for keypair in self.keypairs_to_treat.iter() {
            let buy_ixs: Vec<Instruction> = self
                .pumpfun_client
                .buy_ixs(&mint_pubkey, &keypair.keypair, keypair.amount, None, true)
                .await
                .unwrap();
            for ix in buy_ixs {
                all_ixs.push((keypair.keypair.pubkey(), ix));
            }
        }

        let mut current_tx_ixs: Vec<Instruction> = Vec::new();
        let priority_fee_ix = ComputeBudgetInstruction::set_compute_unit_price(2_000_000);
        current_tx_ixs.push(priority_fee_ix);
        let mut should_break: bool = false;

        for (index, (_, ix)) in all_ixs.iter().enumerate() {
            let to_add = vec![ix.clone()];
            let possible: bool = self.is_allowed(&current_tx_ixs, &to_add).await;
            if possible {
                current_tx_ixs.push(ix.clone());
            } else {
                if transactions.len() == 1 {
                    let tip_ix = self
                        .jito
                        .get_tip_ix(self.dev_keypair.pubkey())
                        .await
                        .unwrap();
                    let to_add = vec![tip_ix.clone()];
                    let can_add: bool = self.is_allowed(&current_tx_ixs, &to_add).await;
                    if !can_add {
                        current_tx_ixs.pop();
                        current_tx_ixs.push(tip_ix);
                        tip_ix_count += 1;
                        self.treated_keypairs = all_ixs[index - 1].0;
                        should_break = true;
                    }
                }
                let signers = self.get_tx_signers(&current_tx_ixs);
                let tx = build_transaction(
                    &self.client,
                    &current_tx_ixs,
                    signers.iter().collect(),
                    self.address_lookup_table_account.clone(),
                    &self.dev_keypair,
                );
                transactions.push(tx);

                if !should_break {
                    current_tx_ixs = Vec::new();
                    let priority_fee_ix =
                        ComputeBudgetInstruction::set_compute_unit_price(2_000_000);
                    current_tx_ixs.push(priority_fee_ix);
                    current_tx_ixs.push(ix.clone());
                }
            }

            if should_break {
                break;
            }
        }

        //We did not break, meaning that there are still untreated instructions and we did not reach 5 transactions for the bundle
        if !should_break {
            let tip_ix = self
                .jito
                .get_tip_ix(self.dev_keypair.pubkey())
                .await
                .unwrap();
            let in_vec = vec![tip_ix.clone()];
            let can_add = self.is_allowed(&current_tx_ixs, &in_vec).await;
            if can_add {
                current_tx_ixs.push(tip_ix);
                let last_tx_signers = self.get_tx_signers(&current_tx_ixs);
                let tx = build_transaction(
                    &self.client,
                    &current_tx_ixs,
                    last_tx_signers.iter().collect(),
                    self.address_lookup_table_account.clone(),
                    &self.dev_keypair,
                );
                transactions.push(tx);
                tip_ix_count += 1;
            } else {
                if transactions.len() == 4 {
                    current_tx_ixs.pop();
                    current_tx_ixs.push(tip_ix);
                    let last_tx_signers = self.get_tx_signers(&current_tx_ixs);
                    let tx = build_transaction(
                        &self.client,
                        &current_tx_ixs,
                        last_tx_signers.iter().collect(),
                        self.address_lookup_table_account.clone(),
                        &self.dev_keypair,
                    );
                    transactions.push(tx);
                    tip_ix_count += 1;
                    self.treated_keypairs = all_ixs[self.keypairs_to_treat.len() - 1].0;
                // removed last instruction only.
                } else {
                    let tx_signers = self.get_tx_signers(&current_tx_ixs);
                    let before_last_tx = build_transaction(
                        &self.client,
                        &current_tx_ixs,
                        tx_signers.iter().collect(),
                        self.address_lookup_table_account.clone(),
                        &self.dev_keypair,
                    );
                    let tip_ix = self
                        .jito
                        .get_tip_ix(self.dev_keypair.pubkey())
                        .await
                        .unwrap();
                    let in_vec = vec![tip_ix];
                    let signer = vec![&self.dev_keypair];
                    let last_tx = build_transaction(
                        &self.client,
                        &in_vec,
                        signer,
                        self.address_lookup_table_account.clone(),
                        &self.dev_keypair,
                    );
                    transactions.push(before_last_tx);
                    transactions.push(last_tx);
                    tip_ix_count += 1;
                }
            }
        }
        println!(
            "Adding {:?} transactions in the first bundle with {:?} tip instructions",
            transactions.len(),
            tip_ix_count
        );
        test_transactions(&self.client, &transactions).await;
        transactions
    }

    pub fn has_delayed_bundle(&self) -> bool {
        self.treated_keypairs != Pubkey::default()
    }

    pub async fn collect_rest_txs(&mut self) -> Vec<VersionedTransaction> {
        let new_dev = self.admin_keypair.insecure_clone();
        let new_payer: Arc<Keypair> = Arc::new(new_dev);
        let new_pump = PumpFun::new(new_payer);
        self.pumpfun_client = new_pump;

        let mut tip_ix_count = 0;

        let mut txs: Vec<VersionedTransaction> = Vec::new();

        let mint_pubkey: Pubkey = self.mint_keypair.pubkey();

        let mut all_ixs: Vec<Instruction> = Vec::new();

        let mut reached: bool = false;
        for keypair in self.keypairs_to_treat.iter() {
            if !reached {
                if keypair.keypair.pubkey() == self.treated_keypairs {
                    reached = true;
                } else {
                    println!(
                        "Continuing for pubkey: {:?}",
                        keypair.keypair.pubkey().to_string()
                    );
                    continue;
                }
            }
            if reached {
                let buy_ixs: Vec<Instruction> = self
                    .pumpfun_client
                    .buy_ixs(
                        &mint_pubkey,
                        &keypair.keypair,
                        keypair.amount,
                        Some(1000),
                        false,
                    )
                    .await
                    .unwrap();
                println!(
                    "Passing buy ixs {:?} for {:?}",
                    buy_ixs.len(),
                    keypair.keypair.pubkey().to_string()
                );
                for ix in buy_ixs {
                    all_ixs.push(ix);
                }
            }
        }

        let mut current_ixs: Vec<Instruction> = Vec::new();
        print!("Keypair to reach: {:?}", self.treated_keypairs.to_string());
        for ix in all_ixs {
            let ixs = vec![ix.clone()];
            let can = self.is_allowed(&current_ixs, &ixs).await;
            if can {
                //Can, so adding latest ix to current ixs
                current_ixs.push(ix.clone());
            } else {
                let mut new_ixs: Vec<Instruction> = Vec::new();
                //Need to add tip instruction
                if txs.len() % 5 < 4 {
                    let tip_ix = self
                        .jito
                        .get_tip_ix(self.admin_keypair.pubkey())
                        .await
                        .unwrap();
                    let in_vec = vec![tip_ix.clone()];
                    let can_add = self.is_allowed(&current_ixs, &in_vec).await;
                    if !can_add {
                        let last_ix = current_ixs.pop();
                        if let Some(last_ix) = last_ix {
                            new_ixs.push(last_ix);
                        }
                        current_ixs.push(tip_ix);
                        tip_ix_count += 1;
                    }
                }
                let tx_signers = self.get_tx_signers(&current_ixs);
                let tx = build_transaction(
                    &self.client,
                    &current_ixs,
                    tx_signers.iter().collect(),
                    self.address_lookup_table_account.clone(),
                    &self.admin_keypair,
                );
                //Adding new tx, creating empty ixs and adding latest ix
                txs.push(tx);
            }
        }

        if current_ixs.len() > 0 {
            let tip_ix = self
                .jito
                .get_tip_ix(self.admin_keypair.pubkey())
                .await
                .unwrap();
            let in_vec = vec![tip_ix.clone()];
            let can_add_tip_ix = self.is_allowed(&current_ixs, &in_vec).await;
            if can_add_tip_ix {
                current_ixs.push(tip_ix);
                tip_ix_count += 1;
                let signers = self.get_tx_signers(&current_ixs);
                let last_tx = build_transaction(
                    &self.client,
                    &current_ixs,
                    signers.iter().collect(),
                    self.address_lookup_table_account.clone(),
                    &self.admin_keypair,
                );
                txs.push(last_tx);
            } else {
                if current_ixs.len() % 5 < 4 {
                    //Add both
                    let signers = self.get_tx_signers(&current_ixs);
                    let tx = build_transaction(
                        &self.client,
                        &current_ixs,
                        signers.iter().collect(),
                        self.address_lookup_table_account.clone(),
                        &self.admin_keypair,
                    );
                    txs.push(tx);
                    let tip_ix = self
                        .jito
                        .get_tip_ix(self.admin_keypair.pubkey())
                        .await
                        .unwrap();
                    let in_vec = vec![tip_ix.clone()];
                    let tx = build_transaction(
                        &self.client,
                        &in_vec,
                        vec![&self.admin_keypair],
                        self.address_lookup_table_account.clone(),
                        &self.admin_keypair,
                    );
                    txs.push(tx);
                    tip_ix_count += 1;
                } else {
                    //Add tip ix and add last tx with tip ix as last unique tx
                    let tip_ix = self
                        .jito
                        .get_tip_ix(self.admin_keypair.pubkey())
                        .await
                        .unwrap();
                    let in_vec = vec![tip_ix.clone()];
                    let tx = build_transaction(
                        &self.client,
                        &in_vec,
                        vec![&self.admin_keypair.insecure_clone()],
                        self.address_lookup_table_account.clone(),
                        &self.admin_keypair,
                    );
                    txs.push(tx);
                    tip_ix_count += 1;
                    let signers = self.get_tx_signers(&current_ixs);
                    let last_tx = build_transaction(
                        &self.client,
                        &current_ixs,
                        signers.iter().collect(),
                        self.address_lookup_table_account.clone(),
                        &self.admin_keypair,
                    );
                    txs.push(last_tx);
                    let last_tx = build_transaction(
                        &self.client,
                        &in_vec,
                        vec![&self.admin_keypair.insecure_clone()],
                        self.address_lookup_table_account.clone(),
                        &self.admin_keypair,
                    );
                    txs.push(last_tx);
                    tip_ix_count += 1;
                }
            }
        }
        print!(
            "Sending {:?} transactions as late bundles with {:?} tip instructions",
            txs.len(),
            tip_ix_count
        );
        txs
    }

    pub fn get_tx_signers(&self, ixs: &Vec<Instruction>) -> Vec<Keypair> {
        let mut maybe_ix_unique_signers: HashSet<Pubkey> = HashSet::new();

        for ix in ixs {
            for acc in ix.accounts.iter().filter(|acc| acc.is_signer) {
                maybe_ix_unique_signers.insert(acc.pubkey);
            }
        }

        let mut all_ixs_signers: Vec<Keypair> = Vec::new();

        for signer in maybe_ix_unique_signers {
            if let Some(kp) = self
                .keypairs_to_treat
                .iter()
                .find(|kp| kp.keypair.pubkey() == signer)
            {
                all_ixs_signers.push(kp.keypair.insecure_clone());
            } else if signer == self.dev_keypair.pubkey() {
                all_ixs_signers.push(self.dev_keypair.insecure_clone());
            } else if signer == self.mint_keypair.pubkey() {
                all_ixs_signers.push(self.mint_keypair.insecure_clone());
            } else if signer == self.admin_keypair.pubkey() {
                all_ixs_signers.push(self.admin_keypair.insecure_clone());
            }
        }
        all_ixs_signers
    }

    pub async fn is_allowed(&self, ixs: &Vec<Instruction>, to_add: &Vec<Instruction>) -> bool {
        let mut all_ixs: Vec<Instruction> = Vec::new();
        for ix in ixs {
            all_ixs.push(ix.clone());
        }
        for ix in to_add {
            all_ixs.push(ix.clone());
        }

        let all_ixs_signers: Vec<Keypair> = self.get_tx_signers(&all_ixs);

        let tx = build_transaction(
            &self.client,
            &all_ixs,
            all_ixs_signers.iter().collect(),
            self.address_lookup_table_account.clone(),
            &self.admin_keypair,
        );

        let size: usize = bincode::serialized_size(&tx).unwrap() as usize;
        size <= MAX_TX_SIZE
    }
}
