use crate::jupiter::swap::pumpswap_pool_id;
use crate::pumpfun::utils::{PUMP_AMM_PROGRAM, PUMP_GLOBAL};
use crate::solana::utils::get_ata_balance;
use crate::{config::RPC_URL, solana::utils::get_admin_keypair};
use anchor_client::anchor_lang::InstructionData;
use anchor_spl::associated_token::{
    get_associated_token_address,
    spl_associated_token_account::instruction::create_associated_token_account,
};

use anchor_spl::token::spl_token::instruction::sync_native;
use anchor_spl::token::spl_token::{native_mint::ID, ID as SplID};
use pumpswap_cpi::{
    instruction::{Buy, Sell},
    ID as SwapID,
};
use solana_client::rpc_client::RpcClient;
use solana_sdk::system_instruction;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
};

use super::utils::{
    ASSOCIATED_TOKEN_PROGRAM, PUMPFUN_EVENT_AUTH, PUMPFUN_FEE_ACC, SYSTEM_PROGRAM, TOKEN_PROGRAM,
};

struct PoolInfo {
    pool_id: Pubkey,
    pool_base: Pubkey,
    pool_quote: Pubkey,
    fee_recipient_ata: Pubkey,
    amount_out: u64,
}

pub struct PumpSwap {
    pub client: RpcClient,
    pub admin: Keypair,
}

impl PumpSwap {
    pub fn new() -> Self {
        let client = RpcClient::new(RPC_URL.to_string());
        let admin = get_admin_keypair();

        Self { client, admin }
    }

    pub fn wrap_admin_sol(&self, total_amount: u64) -> Vec<Instruction> {
        let mut ixs: Vec<Instruction> = Vec::new();
        let ata = get_associated_token_address(&self.admin.pubkey(), &ID);
        if self.client.get_account(&ata).is_err() {
            let create_ata_ix = create_associated_token_account(
                &self.admin.pubkey(),
                &self.admin.pubkey(),
                &ID,
                &SplID,
            );
            ixs.push(create_ata_ix);
        }
        let transfer_ix = system_instruction::transfer(&self.admin.pubkey(), &ata, total_amount);
        let sync_ix = sync_native(&SplID, &ata).unwrap();
        ixs.push(transfer_ix);
        ixs.push(sync_ix);
        ixs
    }

    //TODO: Maybe last buy doesn't pass because total wsol holding is not enough. Double-check with some tests
    pub async fn buy_ixs(
        &self,
        mint: Pubkey,
        amount: u64,
        slippage_bps: Option<u64>,
    ) -> Vec<Instruction> {
        let mut ixs: Vec<Instruction> = Vec::new();

        let base_ata: Pubkey = get_associated_token_address(&self.admin.pubkey(), &ID);
        let ata: Pubkey = get_associated_token_address(&self.admin.pubkey(), &mint); // mint ata for admin

        let buy_amount_with_slippage =
            Self::calculate_with_slippage(amount, slippage_bps.unwrap_or(400));

        let swap_info = self
            .get_swap_info(&mint, &buy_amount_with_slippage, true)
            .await;

        if let Some(swap_info) = swap_info {
            println!(
                "Amount in solana : {:?}. Willing to buy {:?}",
                amount, swap_info.amount_out
            );

            let data = Buy {
                _base_amount_out: swap_info.amount_out,
                _max_quote_amount_in: amount,
            };

            if let Err(err) = self.client.get_account(&ata) {
                println!(
                    "Received error when trying to find ata for mint on admin keypair: {:?}",
                    err
                );
                let create_ata_ix = create_associated_token_account(
                    &self.admin.pubkey(),
                    &self.admin.pubkey(),
                    &mint,
                    &SplID,
                );
                ixs.push(create_ata_ix);
            }

            let buy_ix = Instruction::new_with_bytes(
                SwapID,
                &data.data(),
                vec![
                    AccountMeta::new_readonly(swap_info.pool_id, false), // Pool id
                    AccountMeta::new(self.admin.pubkey(), true),         // ADMIN as signer
                    AccountMeta::new_readonly(PUMP_GLOBAL, false),       //GLOBAL
                    AccountMeta::new_readonly(mint, false),              //MINT
                    AccountMeta::new_readonly(ID, false),                //WSOL
                    AccountMeta::new(ata, false),                        //MINT ADMIN ATA
                    AccountMeta::new(base_ata, false),                   //WSOL ADMIN ATA
                    AccountMeta::new(swap_info.pool_base, false),
                    AccountMeta::new(swap_info.pool_quote, false),
                    AccountMeta::new_readonly(PUMPFUN_FEE_ACC, false),
                    AccountMeta::new(swap_info.fee_recipient_ata, false),
                    AccountMeta::new_readonly(TOKEN_PROGRAM, false),
                    AccountMeta::new_readonly(TOKEN_PROGRAM, false),
                    AccountMeta::new_readonly(SYSTEM_PROGRAM, false),
                    AccountMeta::new_readonly(ASSOCIATED_TOKEN_PROGRAM, false),
                    AccountMeta::new_readonly(PUMPFUN_EVENT_AUTH, false),
                    AccountMeta::new_readonly(PUMP_AMM_PROGRAM, false),
                ],
            );
            ixs.push(buy_ix);
        }
        ixs
    }

    pub async fn sell_ixs(
        &self,
        mint: Pubkey,
        recipient: Pubkey,
        amount: Option<u64>,
        seller: Option<Keypair>,
    ) -> Vec<Instruction> {
        let mut ixs: Vec<Instruction> = Vec::new();
        let mut seller_pubkey = Pubkey::default();

        let (signer, signer_ata) = match seller {
            Some(seller_kp) => {
                seller_pubkey = seller_kp.pubkey();
                (
                    seller_kp.insecure_clone(),
                    get_associated_token_address(&seller_kp.pubkey(), &mint),
                )
            }
            None => (
                self.admin.insecure_clone(),
                get_associated_token_address(&self.admin.pubkey(), &mint),
            ),
        };

        let mut signer_balance = get_ata_balance(&self.client, &signer, &mint).await.unwrap();

        if let Some(amount) = amount {
            signer_balance = amount;
        }

        let swap_info = self.get_swap_info(&mint, &signer_balance, false).await;

        if let Some(swap_info) = swap_info {
            //Add create ATA for WSOL IX
            let ata: Pubkey = get_associated_token_address(&recipient, &ID);

            if let Err(_) = self.client.get_account(&ata) {
                let create_ata_ix =
                    create_associated_token_account(&signer.pubkey(), &recipient, &ID, &SplID);
                ixs.push(create_ata_ix);

                let min_received_with_slippage =
                    Self::calculate_with_slippage(swap_info.amount_out, 400);

                let sell_data = Sell {
                    _base_amount_in: signer_balance,
                    _min_quote_amount_out: min_received_with_slippage,
                };

                let sell_ix = Instruction::new_with_bytes(
                    SwapID,
                    &sell_data.data(),
                    vec![
                        AccountMeta::new_readonly(swap_info.pool_id, false), // Pool id
                        AccountMeta::new(signer.pubkey(), true),             // Signer
                        AccountMeta::new_readonly(PUMP_GLOBAL, false),       //GLOBAL
                        AccountMeta::new_readonly(mint, false),              //MINT
                        AccountMeta::new_readonly(ID, false),                //WSOL
                        AccountMeta::new(signer_ata, false),                 //MINT SIGNER ATA
                        AccountMeta::new(ata, false),                        //WSOL RECIPIENT ATA
                        AccountMeta::new(swap_info.pool_base, false),
                        AccountMeta::new(swap_info.pool_quote, false),
                        AccountMeta::new_readonly(PUMPFUN_FEE_ACC, false),
                        AccountMeta::new(swap_info.fee_recipient_ata, false),
                        AccountMeta::new_readonly(TOKEN_PROGRAM, false),
                        AccountMeta::new_readonly(TOKEN_PROGRAM, false),
                        AccountMeta::new_readonly(SYSTEM_PROGRAM, false),
                        AccountMeta::new_readonly(ASSOCIATED_TOKEN_PROGRAM, false),
                        AccountMeta::new_readonly(PUMPFUN_EVENT_AUTH, false),
                        AccountMeta::new_readonly(PUMP_AMM_PROGRAM, false),
                    ],
                );
                ixs.push(sell_ix);
            }

            //Add sell tax instruction if seller pubkey has been registered 
            let tax_amount = swap_info.amount_out * 5 / 100;

            if seller_pubkey != Pubkey::default() {
                let tax_ix = system_instruction::transfer(
                    &seller_pubkey,
                    &self.admin.pubkey(), // Send to admin public key
                    tax_amount,
                );
    
                ixs.push(tax_ix);
            }
        }

        ixs
    }

    fn calculate_with_slippage(amount: u64, basis_points: u64) -> u64 {
        let res = amount - (amount * basis_points) / 10000;
        res
    }

    async fn get_swap_info(&self, mint: &Pubkey, amount: &u64, buy: bool) -> Option<PoolInfo> {
        let pool_info = pumpswap_pool_id(&mint, *amount, buy).await;
        if let Some(pool_info) = pool_info {
            let pool_id = pool_info.0;
            let amount_out = pool_info.1;
            let pool_base = get_associated_token_address(&pool_info.0, &mint);
            let pool_quote = get_associated_token_address(&pool_info.0, &ID);
            let fee_recipient_ata = get_associated_token_address(&PUMPFUN_FEE_ACC, &ID);

            let result = PoolInfo {
                pool_id,
                pool_base,
                pool_quote,
                fee_recipient_ata,
                amount_out,
            };
            return Some(result);
        }

        None
    }
}
