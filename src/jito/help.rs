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

//Sum up for FIRST BUNDLE : - can add tip ix for all transactions
//TX 1 : 675 as base (create tx + dev buy ixs) followed by 146 bytes per buy ixs -> create + dev buy + 3 more buys
//TX 2:  389 as base (buy ixs) followed by 146 bytes per buy ixs -> 5 more ixs (total, 6 buys)
//Tx 3: Same as tx2 -> 6 buys
//Tx 4: Same as tx3 -> 6 buys
//Tx 5: Same as tx4 + tip-ix (49 bytes) -> 6 buys + tip ix

//TOTAL: 28 buys + create + tip ix

pub struct BundleTransactions {
    admin_keypair: Keypair,
    dev_keypair: Keypair,
    mint_keypair: Keypair,
    client: RpcClient,
    pumpfun_client: PumpFun,
    jito: JitoBundle,
    address_lookup_table_account: AddressLookupTableAccount,
    keypairs_to_treat: Vec<KeypairWithAmount>,
    jito_tip_account: Pubkey,
}

impl BundleTransactions {
    pub fn new(
        dev_keypair: Keypair,
        mint_keypair: &Keypair,
        address_lookup_table_account: AddressLookupTableAccount,
        others_with_amount: Vec<KeypairWithAmount>,
        jito_tip_account: Pubkey,
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

        let dev: Keypair = admin_keypair.insecure_clone();
        let payer: Arc<Keypair> = Arc::new(dev);

        let pumpfun_client = PumpFun::new(payer);

        let keypairs_to_treat: Vec<KeypairWithAmount> = others_with_amount;
        let mint_keypair: Keypair = mint_keypair.insecure_clone();

        println!(
            "Creating BundleTransactions Class with lut: {:?}",
            address_lookup_table_account.key.to_string()
        );
        println!(
            "Accounts in lut : {:?}",
            address_lookup_table_account.addresses
        );

        Self {
            admin_keypair,
            dev_keypair,
            mint_keypair,
            client,
            pumpfun_client,
            jito,
            address_lookup_table_account,
            keypairs_to_treat,
            jito_tip_account,
        }
    }
    //Separate logic of checking txs size into separate function
    //When adding ixs, remove actual keypairwithamount from keypairs to treat
    pub async fn collect_first_bundle_txs(
        &mut self,
        dev_amount: u64,
        token_metadata: Create,
    ) -> Vec<VersionedTransaction> {
        let rent = Rent::default();
        let rent_exempt_min = rent.minimum_balance(0);

        let for_many: u64 = BUFFER_AMOUNT * std::cmp::max(self.keypairs_to_treat.len(), 10) as u64;
        let to_sub_for_dev: u64 = rent_exempt_min + FEE_AMOUNT + JITO_TIP_AMOUNT + for_many;

        let final_dev_buy_amount = dev_amount - to_sub_for_dev;

        let mint_pubkey: Pubkey = self.mint_keypair.pubkey();

        let mut transactions: Vec<VersionedTransaction> = Vec::new();

        let jito_tip_ix = self.get_tip_ix().await;
        let priority_fee_ix = self.get_priority_fee_ix(2_000_000);
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

        let mut tx_ixs: Vec<Instruction> = vec![priority_fee_ix.clone(), mint_ix];

        tx_ixs.extend(dev_ix);
        let mut last_index = 2;
        for (index, keypair) in self.keypairs_to_treat.iter().enumerate() {
            let buy_ixs: Vec<Instruction> = self
                .pumpfun_client
                .buy_ixs(&mint_pubkey, &keypair.keypair, keypair.amount, None, true)
                .await
                .unwrap();

            //Treating first tx with mint instruction
            if index < 3 && transactions.len() == 0 {
                tx_ixs.extend(buy_ixs);
            } else {
                //Push first transaction
                if transactions.len() == 0 {
                    let first_tx: VersionedTransaction = self.get_tx(&tx_ixs);
                    transactions.push(first_tx);
                    tx_ixs = vec![priority_fee_ix.clone()];
                }
                //Treating other txs
                if (index - last_index) <= 6 {
                    //If we are still within 6 buy instructions for the given tx
                    tx_ixs.extend(buy_ixs);
                    break; //Added just for testing to see second bundle where in block difference
                } else {
                    //Treating last transaction
                    if transactions.len() == 4 {
                        tx_ixs.push(jito_tip_ix.clone());
                    }
                    let tx = self.get_tx(&tx_ixs);
                    transactions.push(tx);

                    //Breaking up early, there are still some keypairs untreated
                    if transactions.len() == 5 {
                        return transactions;
                    }
                    tx_ixs = vec![priority_fee_ix.clone()];
                    tx_ixs.extend(buy_ixs);
                    last_index = index - 1;
                }
            }
        }

        if tx_ixs.len() > 1 {
            tx_ixs.push(jito_tip_ix.clone());
            // has more than just the priority fee instruction
            let final_tx = self.get_tx(&tx_ixs);
            transactions.push(final_tx);
        }

        test_transactions(&self.client, &transactions).await;
        transactions
    }

    pub async fn collect_rest_txs(&mut self) -> Vec<VersionedTransaction> {
        let new_dev = self.admin_keypair.insecure_clone();
        let new_payer: Arc<Keypair> = Arc::new(new_dev);
        let new_pump = PumpFun::new(new_payer);
        self.pumpfun_client = new_pump;

        let mut txs: Vec<VersionedTransaction> = Vec::new();

        let mint_pubkey: Pubkey = self.mint_keypair.pubkey();

        let rest_keypairs: Vec<KeypairWithAmount> = self.keypairs_to_treat.split_off(27); // Split off the first 27 buyers since they have been treated

        //Adding all instructions into array to treat
        let mut all_ixs: Vec<Instruction> = Vec::new();

        for keypair in rest_keypairs {
            let buy_ixs: Vec<Instruction> = self
                .pumpfun_client
                .buy_ixs(&mint_pubkey, &keypair.keypair, keypair.amount, None, false)
                .await
                .unwrap();

            all_ixs.extend(buy_ixs);
        }

        let priority_fee_ix = self.get_priority_fee_ix(300_000);
        let jito_tip_ix = self.get_tip_ix().await;

        let mut tx_ixs: Vec<Instruction> = vec![priority_fee_ix.clone()];

        //Each tx instructions should have 6 buys (2 per buy) + 1 priority fee ix = 13 instructions. if last tx, has 14
        let limit = 13;

        for ix in all_ixs {
            if tx_ixs.len() < limit {
                tx_ixs.push(ix);
            } else {
                if txs.len() % 5 == 4 {
                    tx_ixs.push(jito_tip_ix.clone());
                }
                // has more than just the priority fee instruction
                let new_tx = self.get_tx(&tx_ixs);
                txs.push(new_tx);
                tx_ixs = vec![priority_fee_ix.clone()];
            }
        }

        if tx_ixs.len() > 1 {
            //higher than just the priority fee ix
            tx_ixs.push(jito_tip_ix);
            let final_tx = self.get_tx(&tx_ixs);
            txs.push(final_tx);
        }

        txs
    }

    pub fn has_delayed_bundle(&self) -> bool {
        self.keypairs_to_treat.len() > 26
    }

    fn get_tx(&self, ixs: &Vec<Instruction>) -> VersionedTransaction {
        let signers = self.get_signers(&ixs);
        let tx = build_transaction(
            &self.client,
            &ixs,
            signers.iter().collect(),
            self.address_lookup_table_account.clone(),
            &self.admin_keypair,
        );
        tx
    }

    fn get_signers(&self, ixs: &Vec<Instruction>) -> Vec<Keypair> {
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

    async fn get_tip_ix(&self) -> Instruction {
        let tip_ix = self
            .jito
            .get_tip_ix(self.admin_keypair.pubkey(), Some(self.jito_tip_account))
            .await
            .unwrap();
        tip_ix
    }

    fn get_priority_fee_ix(&self, fee: u64) -> Instruction {
        ComputeBudgetInstruction::set_compute_unit_price(fee)
    }
}
