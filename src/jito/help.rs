use super::jito::JitoBundle;
use crate::config::{
    BUFFER_AMOUNT, FEE_AMOUNT, JITO_TIP_AMOUNT, MAX_BUYERS_FIRST_BUNDLE, MAX_RETRIES, RPC_URL,
};
use crate::params::KeypairWithAmount;
use crate::pumpfun::pump::PumpFun;
use crate::solana::utils::{build_transaction, test_transactions};
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
use std::sync::Arc;

//Sum up for FIRST BUNDLE : - can add tip ix for all transactions
//44 bytes for fees

//base with create -> 703 bytes

//Base without create -> 482 bytes

//One full buy ixs -> 146 bytes

//49 bytes for tip to jito ix

//703 + 3 * 146 + 44 = 1185 bytes -> first tx remains unchanged

//second tx: 44 + first buy (482 bytes) + 4 buy ixs (582 bytes) = 1110 bytes (has space for jito tip) -> 5 buy ixs per tx

//TOTAL: create + dev buy + 3 buys + 4 * 5 buys + tip = create + 24 buys + tip ix
//TOTAL: 24 buys + create + tip ix

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
    with_delay: bool,
    priority_fee: u64,
    jito_fee: u64,
}

impl BundleTransactions {
    pub fn new(
        admin_keypair: Keypair,
        dev_keypair: Keypair,
        mint_keypair: &Keypair,
        address_lookup_table_account: AddressLookupTableAccount,
        others_with_amount: Vec<KeypairWithAmount>,
        jito_tip_account: Pubkey,
        with_delay: bool,
        priority_fee: u64,
        jito_fee: u64,
    ) -> Self {
        let client: RpcClient = RpcClient::new(RPC_URL);

        let jito: JitoBundle = JitoBundle::new(MAX_RETRIES, JITO_TIP_AMOUNT);

        let dev: Keypair = dev_keypair.insecure_clone();
        let payer: Arc<Keypair> = Arc::new(dev);

        let pumpfun_client: PumpFun = PumpFun::new(payer);

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

        println!("ADMIN: {:?}", admin_keypair.pubkey());

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
            with_delay,
            jito_fee,
            priority_fee,
        }
    }
    //Separate logic of checking txs size into separate function
    //When adding ixs, remove actual keypairwithamount from keypairs to treat
    pub async fn collect_first_bundle_txs(
        &mut self,
        token_metadata: Create,
    ) -> Vec<Vec<Instruction>> {
        let mut all_ixs: Vec<Vec<Instruction>> = Vec::new();

        let rent = Rent::default();
        let rent_exempt_min = rent.minimum_balance(0);

        let for_many: u64 = BUFFER_AMOUNT * std::cmp::max(self.keypairs_to_treat.len(), 10) as u64;
        let to_sub_for_dev: u64 = rent_exempt_min + FEE_AMOUNT + JITO_TIP_AMOUNT + for_many;

        let dev_balance = self.client.get_balance(&self.dev_keypair.pubkey()).unwrap();
        let final_dev_buy_amount = dev_balance - to_sub_for_dev;

        let mint_pubkey: Pubkey = self.mint_keypair.pubkey();

        let jito_tip_ix = self.get_tip_ix(None).await;
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

        let mut first_tx_ixs: Vec<Instruction> = vec![priority_fee_ix.clone(), mint_ix];
        first_tx_ixs.extend(dev_ix);

        if self.with_delay {
            let dev_jito_tip = self
                .jito
                .get_tip_ix(self.dev_keypair.pubkey(), Some(self.jito_tip_account))
                .await
                .unwrap();
            first_tx_ixs.push(dev_jito_tip);
        }

        all_ixs.push(first_tx_ixs);

        //Break early and return first tx if with delay
        if self.with_delay {
            return all_ixs;
        }

        let mut tx_ixs: Vec<Instruction> = vec![priority_fee_ix.clone()];

        for (index, buyer) in self.keypairs_to_treat.iter().enumerate() {
            println!(
                "INDEX: {:?} FOR PUBKEY: {:?}",
                index,
                buyer.keypair.pubkey().to_string()
            );

            let balance = self.client.get_balance(&buyer.keypair.pubkey()).unwrap();
            let buy_with = balance - 10000000; // total balance - 0.01 sol

            let buy_ixs = self
                .pumpfun_client
                .buy_ixs(&mint_pubkey, &buyer.keypair, buy_with, None, true)
                .await
                .unwrap();

            tx_ixs.extend(buy_ixs);

            if index == MAX_BUYERS_FIRST_BUNDLE || index == self.keypairs_to_treat.len() - 1 {
                // last item or 23rd item of list
                println!("Adding tip here");
                tx_ixs.push(jito_tip_ix.clone());
                all_ixs.push(tx_ixs);
                tx_ixs = Vec::new();
                break;
            }

            // Every 5 buyers, create new transaction
            if (index + 1) % 5 == 0 {
                all_ixs.push(tx_ixs);
                tx_ixs = vec![priority_fee_ix.clone()];
            }
        }

        if tx_ixs.len() > 1 && all_ixs.len() < 5 {
            println!("Added tip ix");
            tx_ixs.push(jito_tip_ix);
            all_ixs.push(tx_ixs);
        }
        all_ixs
    }

    pub async fn collect_rest_txs(&mut self) -> Vec<Vec<Instruction>> {
        let mut transactions: Vec<Vec<Instruction>> = Vec::new();

        let mint_pubkey: Pubkey = self.mint_keypair.pubkey(); // Split off the first 27 buyers since they have been treated

        let priority_fee_ix = self.get_priority_fee_ix(self.priority_fee);
        let jito_tip_ix = self.get_tip_ix(Some(self.jito_fee)).await;

        //Adding all instructions into array to treat
        let mut tx_ixs: Vec<Instruction> = vec![priority_fee_ix.clone()];

        for (index, keypair) in self
            .keypairs_to_treat
            .iter()
            .skip(if self.with_delay {
                0
            } else {
                MAX_BUYERS_FIRST_BUNDLE
            })
            .enumerate()
        {
            let balance = self.client.get_balance(&keypair.keypair.pubkey()).unwrap();
            let buy_amount = balance - 10000000;
            let buy_ixs: Vec<Instruction> = self
                .pumpfun_client
                .buy_ixs(&mint_pubkey, &keypair.keypair, buy_amount, Some(800), true)
                .await
                .unwrap();

            tx_ixs.extend(buy_ixs);

            // Check if we're on the 5th transaction (index 4) and have 4 buyers
            if transactions.len() + 1 % 5 == 0 && (index + 1) % 4 == 0 {
                println!("Adding tip here");
                tx_ixs.push(jito_tip_ix.clone());
                transactions.push(tx_ixs.clone());
                tx_ixs = vec![priority_fee_ix.clone()];
            }
            // Check if we're on the last buyer
            else if (!self.with_delay
                && index == self.keypairs_to_treat.len() - MAX_BUYERS_FIRST_BUNDLE - 1)
                || (self.with_delay && index == self.keypairs_to_treat.len() - 1)
            {
                println!("Adding tip here");
                tx_ixs.push(jito_tip_ix.clone());

                transactions.push(tx_ixs.clone());
            }
            // Normal case: every 5 buyers
            else if (index + 1) % 5 == 0 {
                transactions.push(tx_ixs);
                tx_ixs = vec![priority_fee_ix.clone()];
            }
        }
        transactions
    }

    pub fn has_delayed_bundle(&mut self) -> bool {
        self.keypairs_to_treat.len() >= MAX_BUYERS_FIRST_BUNDLE + 1 || self.with_delay
        // In total we can get 23 buys + dev buy for first bundle
    }

    async fn get_tip_ix(&self, fee: Option<u64>) -> Instruction {
        if let Some(fee) = fee {
            let tip_ix = self
                .jito
                .get_custom_tip_ix(self.admin_keypair.pubkey(), self.jito_tip_account, fee)
                .await
                .unwrap();
            tip_ix
        } else {
            let tip_ix = self
                .jito
                .get_tip_ix(self.admin_keypair.pubkey(), Some(self.jito_tip_account))
                .await
                .unwrap();
            tip_ix
        }
    }

    fn get_priority_fee_ix(&self, fee: u64) -> Instruction {
        ComputeBudgetInstruction::set_compute_unit_price(fee)
    }
}
