use crate::config::RPC_URL;
use crate::pumpfun::pump::PumpFun;
use anchor_spl::associated_token::{
    get_associated_token_address,
    spl_associated_token_account::instruction::create_associated_token_account,
};
use jito_sdk_rust::JitoJsonRpcSDK;
use rand::Rng;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::commitment_config::CommitmentLevel;
use solana_sdk::hash::Hash;
use solana_sdk::instruction::Instruction;
use solana_sdk::signer::Signer;
use solana_sdk::system_instruction;
use solana_sdk::transaction::Transaction;
use solana_sdk::{pubkey::Pubkey, signature::Keypair};
use std::str::FromStr;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

pub struct Bump {
    pub client: RpcClient,
    pub jito: JitoJsonRpcSDK,
    pub pump_cli: PumpFun,
    pub mint: Pubkey,
    pub main_bump: Keypair,
}

struct OneBump {
    pub keypair: Keypair,
    pub lamports: u64,
}

impl OneBump {
    pub fn new(keypair: Keypair, lamports: u64) -> Self {
        Self { keypair, lamports }
    }
}

impl Bump {

    pub fn new(mint_address: String) -> Self {
        let client = RpcClient::new(RPC_URL);
        let jito_sdk = JitoJsonRpcSDK::new("https://mainnet.block-engine.jito.wtf/api/v1", None);
        let mint = Pubkey::from_str(&mint_address).unwrap();
        let new_kp = Keypair::new();

        let payer: Arc<Keypair> = Arc::new(new_kp.insecure_clone());
        let pumpfun = PumpFun::new(payer);

        Self {
            client,
            jito: jito_sdk,
            pump_cli: pumpfun,
            mint,
            main_bump: new_kp,
        }
    }

    pub fn get_funding_pubkey(&self) -> Pubkey {
        self.main_bump.pubkey()
    }

    pub async fn bump(&mut self) {
        print!("Bump Wallet: {:?}", self.main_bump.secret());
        loop {
            //Make buy
            let balance: u64 = self.client.get_balance(&self.main_bump.pubkey()).unwrap();
            //Stop the loop if the balance of main wallet got low
            if balance < 1_000_000 {
                //transfer remaining to admin wallet
                break;
            }

            let mut all_bump_ixs: Vec<Instruction> = Vec::new();

            let new_wallets: Vec<Keypair> = self.generate_new_wallets();

            let new_bumps: Vec<OneBump> = self.get_splits(&new_wallets);

            let fund_bumps_ixs: Vec<Instruction> = self.get_fund_ixs(&new_bumps);
            let buy_ixs: Vec<Instruction> = self.get_buy_ixs(&new_bumps).await;

            all_bump_ixs.extend(fund_bumps_ixs);
            all_bump_ixs.extend(buy_ixs);

            let mut signing_kps: Vec<&Keypair> = vec![&self.main_bump];
            signing_kps.extend(new_wallets.iter());

            let _ = self
                .send_tx(all_bump_ixs, signing_kps.clone())
                .await;

            sleep(Duration::from_secs(5));

            let sell_ixs: Vec<Instruction> = self.get_sell_ixs(&new_wallets).await;

            let mut signers: Vec<&Keypair> = Vec::new();

            for keypair in new_wallets.iter() {
                signers.push(keypair);
            }

            let _ = self.send_tx(sell_ixs,  signers.clone()).await;

            let withdaw_ixs: Vec<Instruction> = self.get_withdraw_ixs(&new_wallets);

            let _ = self.send_tx(withdaw_ixs, signers);
        }
    }

    async fn get_buy_ixs(&mut self, bumps: &Vec<OneBump>) -> Vec<Instruction> {
        let mut ixs: Vec<Instruction> = Vec::new();
        for wallet in bumps.iter() {
            let buy_ixs = self
                .pump_cli
                .buy_ixs(&self.mint, &wallet.keypair, wallet.lamports, None, false)
                .await
                .unwrap();
            ixs.extend(buy_ixs);
        }
        ixs
    }

    async fn get_sell_ixs(&self, bump_wallets: &Vec<Keypair>) -> Vec<Instruction> {
        let mut ixs: Vec<Instruction> = Vec::new();
        for wallet in bump_wallets.iter() {
            let sell_ixs = self.pump_cli.sell_all_ix(&self.mint, wallet).await.unwrap();
            ixs.extend(sell_ixs);
        }
        ixs
    }

    fn get_fund_ixs(&self, bump_wallets: &Vec<OneBump>) -> Vec<Instruction> {
        let mut fund_ixs: Vec<Instruction> = Vec::new();
        for bump in bump_wallets {
            let fund_ix = system_instruction::transfer(
                &self.main_bump.pubkey(),
                &bump.keypair.pubkey(),
                bump.lamports,
            );
            fund_ixs.push(fund_ix);
        }
        fund_ixs
    }

    fn get_withdraw_ixs(&mut self, bump_wallets: &Vec<Keypair>) -> Vec<Instruction> {
        let mut reset_ixs: Vec<Instruction> = Vec::new();

        for keypair in bump_wallets {
            let balance = self.client.get_balance(&keypair.pubkey()).unwrap();
            if balance > 200_000 {
                let ix: Instruction = system_instruction::transfer(
                    &keypair.pubkey(),
                    &self.main_bump.pubkey(),
                    balance - 200_000,
                );
                reset_ixs.push(ix);
            }
        }

        reset_ixs
    }

    //Split funds ixs
    fn get_splits(&self, bump_wallets: &Vec<Keypair>) -> Vec<OneBump> {
        let mut splits: Vec<u64> = self.split_million();
        let mut bumps: Vec<OneBump> = Vec::new();
        for keypair in bump_wallets {
            let amount = splits.pop();
            if let Some(amount) = amount {
                let one_bump = OneBump::new(keypair.insecure_clone(), amount);
                bumps.push(one_bump);
            }
        }
        bumps
    }

    //Split 1M lamports into 2 random amounts that sum to 1M
    fn split_million(&self) -> Vec<u64> {
        let mut rng = rand::thread_rng();
        let amount1: u64 = rng.gen_range(100_000..900_000);
        let amount2: u64 = 1_000_000 - amount1;
        vec![amount1, amount2]
    }

    //Make 2 new wallets
    fn generate_new_wallets(&self) -> Vec<Keypair> {
        let kp1: Keypair = Keypair::new();
        let kp2: Keypair = Keypair::new();
        vec![kp1, kp2]
    }

    pub async fn send_tx(
        &self,
        all_bump_ixs: Vec<Instruction>,
        signers: Vec<&Keypair>,
    ) {
        let blockhash: Hash = self.client.get_latest_blockhash().unwrap();

        let tx = Transaction::new_signed_with_payer(
            &all_bump_ixs,
            Some(&self.main_bump.pubkey()),
            &signers,
            blockhash,
        );

        let config = RpcSendTransactionConfig {
            skip_preflight: true,
            preflight_commitment: Some(CommitmentLevel::Confirmed),
            encoding: None,
            max_retries: None,
            min_context_slot: None,
        };

        let signature = self
            .client
            .send_transaction_with_config(&tx, config)
            .unwrap();

        self.client
            .confirm_transaction_with_commitment(&signature, CommitmentConfig::confirmed())
            .unwrap();
    }
}
