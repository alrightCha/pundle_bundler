use super::jito::JitoBundle;
use crate::config::{BUFFER_AMOUNT, FEE_AMOUNT};
use crate::config::{JITO_TIP_AMOUNT, MAX_RETRIES, RPC_URL};
use crate::params::KeypairWithAmount;
use crate::pumpfun::pump::PumpFun;
use crate::solana::utils::{build_transaction, test_transactions};
use pumpfun_cpi::instruction::Create;
use solana_client::rpc_client::RpcClient;
use solana_sdk::account::Account;
use solana_sdk::address_lookup_table::state::AddressLookupTable;
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
}

impl BundleTransactions {
    pub fn new(
        admin_keypair: Keypair,
        dev_keypair: Keypair,
        mint_keypair: &Keypair,
        address_lookup_table_account: AddressLookupTableAccount,
        others_with_amount: Vec<KeypairWithAmount>,
        jito_tip_account: Pubkey,
    ) -> Self {
        let client: RpcClient = RpcClient::new(RPC_URL);

        let jito_rpc: RpcClient = RpcClient::new_with_commitment(
            RPC_URL.to_string(),
            solana_sdk::commitment_config::CommitmentConfig::confirmed(),
        );

        let jito: JitoBundle = JitoBundle::new(jito_rpc, MAX_RETRIES, JITO_TIP_AMOUNT);

        let dev: Keypair = admin_keypair.insecure_clone();
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

        let mut first_tx_ixs: Vec<Instruction> = vec![priority_fee_ix.clone(), mint_ix];
        first_tx_ixs.extend(dev_ix);

        let first_tx_chunk = self.keypairs_to_treat.get(..3).unwrap_or_default();

        for buyer in first_tx_chunk {
            let buy_ixs = self
                .pumpfun_client
                .buy_ixs(&mint_pubkey, &buyer.keypair, buyer.amount, None, true)
                .await
                .unwrap();

            first_tx_ixs.extend(buy_ixs);
        }

        let first_tx: VersionedTransaction = self.get_tx(&first_tx_ixs);
        transactions.push(first_tx);

        let mut tx_ixs: Vec<Instruction> = vec![priority_fee_ix.clone()];

        for (index, buyer) in self.keypairs_to_treat.iter().skip(3).enumerate() {
            if index > 19 {
                // break at 23rd keypair. can treat total of 24 buyers 19 + 3 = 22, starting at 0 gives 23 keypairs.
                break;
            }

            let buy_ixs = self
                .pumpfun_client
                .buy_ixs(&mint_pubkey, &buyer.keypair, buyer.amount, None, true)
                .await
                .unwrap();

            tx_ixs.extend(buy_ixs);

            if index == 19 || index == self.keypairs_to_treat.len() - 1 {
                // last item or 23rd item of list
                tx_ixs.push(jito_tip_ix.clone());
            }

            // Every 5 buyers, create new transaction
            if (index + 1) % 5 == 0 {
                let new_tx = self.get_tx(&tx_ixs);
                transactions.push(new_tx);
                tx_ixs = vec![priority_fee_ix.clone()];
            }
        }

        test_transactions(&self.client, &transactions).await;
        transactions
    }

    pub async fn collect_rest_txs(&mut self) -> Vec<VersionedTransaction> {
        let mut transactions: Vec<VersionedTransaction> = Vec::new();

        let mint_pubkey: Pubkey = self.mint_keypair.pubkey(); // Split off the first 27 buyers since they have been treated

        let priority_fee_ix = self.get_priority_fee_ix(300_000);
        let jito_tip_ix = self.get_tip_ix().await;

        //Adding all instructions into array to treat
        let mut tx_ixs: Vec<Instruction> = vec![priority_fee_ix.clone()];

        for (index, keypair) in self.keypairs_to_treat.iter().skip(22).enumerate() {
            let buy_ixs: Vec<Instruction> = self
                .pumpfun_client
                .buy_ixs(&mint_pubkey, &keypair.keypair, keypair.amount, None, false)
                .await
                .unwrap();

            tx_ixs.extend(buy_ixs);

            // Check if we're on the 5th transaction (index 4) and have 4 buyers
            if transactions.len() == 4 && (index + 1) % 4 == 0 {
                tx_ixs.push(jito_tip_ix.clone());
                let new_tx = self.get_tx(&tx_ixs);
                transactions.push(new_tx);
                tx_ixs = vec![priority_fee_ix.clone()];
            }
            // Check if we're on the last buyer
            else if index == self.keypairs_to_treat.len() - 23 {
                tx_ixs.push(jito_tip_ix.clone());
                let new_tx = self.get_tx(&tx_ixs);
                transactions.push(new_tx);
            }
            // Normal case: every 5 buyers
            else if (index + 1) % 5 == 0 {
                let new_tx = self.get_tx(&tx_ixs);
                transactions.push(new_tx);
                tx_ixs = vec![priority_fee_ix.clone()];
            }
        }
        transactions
    }

    pub fn has_delayed_bundle(&mut self) -> bool {
        self.keypairs_to_treat.len() > 23 // In total we can get 23 buys + dev buy for first bundle
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
