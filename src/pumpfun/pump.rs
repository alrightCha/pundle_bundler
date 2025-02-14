use crate::params::CreateTokenMetadata;
use anchor_client::{
    solana_client::nonblocking::rpc_client::RpcClient,
    solana_sdk::{
        pubkey::Pubkey,
        signature::Keypair,
        signer::Signer,
        instruction::Instruction,
    },
    Cluster
};
use anchor_spl::associated_token::{
    get_associated_token_address,
    spl_associated_token_account::instruction::create_associated_token_account,
};

use borsh::BorshDeserialize;
use pumpfun::instruction;
use serde::{Deserialize, Serialize};
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use std::sync::Arc;

use crate::pumpfun::bonding_curve::BondingCurve;

/// Configuration for priority fee compute unit parameters
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriorityFee {
    /// Maximum compute units that can be consumed by the transaction
    pub limit: Option<u32>,
    /// Price in micro-lamports per compute unit
    pub price: Option<u64>,
}

/// Main client for interacting with the Pump.fun program
pub struct PumpFun {
    /// RPC client for Solana network requests
    pub rpc: RpcClient,
    /// Keypair used to sign transactions
    pub payer: Arc<Keypair>,

    pub bonding_curve: BondingCurve,
}

impl PumpFun {
    /// Creates a new PumpFun client instance
    ///
    /// # Arguments
    ///
    /// * `cluster` - Solana cluster to connect to (e.g. devnet, mainnet-beta)
    /// * `payer` - Keypair used to sign and pay for transactions
    /// * `options` - Optional commitment config for transaction finality
    /// * `ws` - Whether to use websocket connection instead of HTTP
    ///
    /// # Returns
    ///
    /// Returns a new PumpFun client instance configured with the provided parameters
    pub fn new(
        cluster: Cluster,
        payer: Arc<Keypair>,
        ws: Option<bool>,
    ) -> Self {
        // Create Solana RPC Client with either WS or HTTP endpoint
        let url = if ws.unwrap_or(false) {
            cluster.ws_url()
        } else {
            cluster.url()
        };
        let rpc: RpcClient = RpcClient::new(url.to_string());

        let bonding_curve = BondingCurve::new();
        // Return configured PumpFun client
        Self {
            rpc,
            payer,
            bonding_curve,
        }
    }

    /// Creates a new token with metadata by uploading metadata to IPFS and initializing on-chain accounts
    ///
    /// # Arguments
    ///
    /// * `mint` - Keypair for the new token mint account that will be created
    /// * `metadata` - Token metadata including name, symbol, description and image file
    /// * `priority_fee` - Optional priority fee configuration for compute units
    ///
    /// # Returns
    ///
    /// Returns the transaction signature if successful, or a ClientError if the operation fails
    pub async fn create_instruction(
        &self,
        mint: &Keypair,
        metadata: CreateTokenMetadata,
    ) -> Result<Instruction, pumpfun::error::ClientError> {
        //Add to instruction the payer, and mint, add to signer the payer and mint as well !
        let create_ix: Instruction = pumpfun::instruction::create(
            &self.payer,
            mint,
            pumpfun::cpi::instruction::Create {
                _name: metadata.name,
                _symbol: metadata.ticker,
                _uri: metadata.uri,
            },
        );

        Ok(create_ix)
    }

    /// Buys tokens from a bonding curve by spending SOL
    ///
    /// # Arguments
    ///
    /// * `mint` - Public key of the token mint to buy
    /// * `amount_sol` - Amount of SOL to spend in lamports
    /// * `slippage_basis_points` - Optional maximum acceptable slippage in basis points (1 bp = 0.01%). Defaults to 500
    /// * `priority_fee` - Optional priority fee configuration for compute units
    ///
    /// # Returns
    ///
    /// Returns the list of instructions for the buy transaction
    pub async fn buy_ixs(
        &mut self,
        mint: &Pubkey,
        keypair: &Keypair,
        amount_sol: u64,
        slippage_basis_points: Option<u64>,
        priority_fee: Option<PriorityFee>,
    ) -> Result<Vec<Instruction>, pumpfun::error::ClientError> {
        // Get accounts and calculate buy amounts
        let global_account = self.get_global_account().await?;
         //Problem here. The bonding curve account is not found. 
     
        let buy_amount = self.bonding_curve.get_buy_price(amount_sol).unwrap();

        let buy_amount_with_slippage =
            pumpfun::utils::calculate_with_slippage_buy(amount_sol, slippage_basis_points.unwrap_or(500));

        let mut instructions: Vec<Instruction> = Vec::new();

        // Add priority fee if provided
        if let Some(fee) = priority_fee {
            if let Some(limit) = fee.limit {
                let limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(limit);
                instructions.push(limit_ix);
            }

            if let Some(price) = fee.price {
                let price_ix = ComputeBudgetInstruction::set_compute_unit_price(price);
                instructions.push(price_ix);
            }
        }

        // Add ata instruction or get acc if available
        let ata: Pubkey = get_associated_token_address(&keypair.pubkey(), mint);
        println!("ATA: {:?}", ata);
        if self.rpc.get_account(&ata).await.is_err() {
            println!("Passing create ATA instruction");
            let create_ata_ix = create_associated_token_account(
                &keypair.pubkey(),
                &keypair.pubkey(),
                mint,
                &pumpfun::constants::accounts::TOKEN_PROGRAM,
            );
            instructions.push(create_ata_ix);
        }

        // Create & add buy instruction to request
        instructions.push(pumpfun::instruction::buy(
            &keypair,
            mint,
            &global_account.fee_recipient,
            pumpfun::cpi::instruction::Buy {
                _amount: buy_amount,
                _max_sol_cost: buy_amount_with_slippage,
            },
        ));

        Ok(instructions)
    }

       /// Sells tokens back to the bonding curve in exchange for SOL
    ///
    /// # Arguments
    ///
    /// * `mint` - Public key of the token mint to sell
    /// * `amount_token` - Optional amount of tokens to sell in base units. If None, sells entire balance
    /// * `slippage_basis_points` - Optional maximum acceptable slippage in basis points (1 bp = 0.01%). Defaults to 500
    /// * `priority_fee` - Optional priority fee configuration for compute units
    ///
    /// # Returns
    ///
    /// Returns the sell transaction request builder
    pub async fn sell_ix(
        &self,
        mint: &Pubkey,
        keypair: &Keypair,
        amount_token: Option<u64>,
        slippage_basis_points: Option<u64>,
        priority_fee: Option<PriorityFee>,
    ) -> Result<Vec<Instruction>, pumpfun::error::ClientError> {
        // Get accounts and calculate sell amounts
        let ata: Pubkey = get_associated_token_address(&keypair.pubkey(), mint);
        let balance = self.rpc.get_token_account_balance(&ata).await?;
        let balance_u64: u64 = balance.amount.parse::<u64>().unwrap();
        let amount = amount_token.unwrap_or(balance_u64);
        let global_account = self.get_global_account().await?;
        let bonding_curve_account = self.get_bonding_curve_account(mint).await?;
        let min_sol_output = bonding_curve_account
            .get_sell_price(amount, global_account.fee_basis_points)
            .map_err(pumpfun::error::ClientError::BondingCurveError)?;
        let min_sol_output = pumpfun::utils::calculate_with_slippage_sell(
            min_sol_output,
            slippage_basis_points.unwrap_or(500),
        );

        let mut instructions: Vec<Instruction> = Vec::new();

        // Add priority fee if provided
        if let Some(fee) = priority_fee {
            if let Some(limit) = fee.limit {
                let limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(limit);
                instructions.push(limit_ix);
            }

            if let Some(price) = fee.price {
                let price_ix = ComputeBudgetInstruction::set_compute_unit_price(price);
                instructions.push(price_ix);
            }
        }

        // Add sell instruction
        let sell_ix = instruction::sell(
            &keypair,
            mint,
            &global_account.fee_recipient,
            pumpfun::cpi::instruction::Sell {
                _amount: amount,
                _min_sol_output: min_sol_output,
            },
        );

        instructions.push(sell_ix);

        Ok(instructions)
    }

    /// Gets the global state account data containing program-wide configuration
    ///
    /// # Returns
    ///
    /// Returns the deserialized GlobalAccount if successful, or a ClientError if the operation fails
    pub async fn get_global_account(&self) -> Result<pumpfun::accounts::GlobalAccount, pumpfun::error::ClientError> {
        let global: Pubkey = Self::get_global_pda();

        let account = self
            .rpc
            .get_account(&global)
            .await
            .map_err(pumpfun::error::ClientError::SolanaClientError)?;

        pumpfun::accounts::GlobalAccount::try_from_slice(&account.data)
            .map_err(pumpfun::error::ClientError::BorshError)
    }


    /// Gets the Program Derived Address (PDA) for the global state account
    ///
    /// # Returns
    ///
    /// Returns the PDA public key derived from the GLOBAL_SEED
    pub fn get_global_pda() -> Pubkey {
        let seeds: &[&[u8]; 1] = &[pumpfun::constants::seeds::GLOBAL_SEED];
        let program_id: &Pubkey = &pumpfun::cpi::ID;
        Pubkey::find_program_address(seeds, program_id).0
    }


    /// Gets the Program Derived Address (PDA) for a token's bonding curve account
    ///
    /// # Arguments
    ///
    /// * `mint` - Public key of the token mint
    ///
    /// # Returns
    ///
    /// Returns Some(PDA) if derivation succeeds, or None if it fails
    pub fn get_bonding_curve_pda(mint: &Pubkey) -> Option<Pubkey> {
        let seeds: &[&[u8]; 2] = &[pumpfun::constants::seeds::BONDING_CURVE_SEED, mint.as_ref()];
        let program_id: &Pubkey = &pumpfun::cpi::ID;
        let pda: Option<(Pubkey, u8)> = Pubkey::try_find_program_address(seeds, program_id);
        pda.map(|pubkey| pubkey.0)
    }


    /// Gets a token's bonding curve account data containing pricing parameters
    ///
    /// # Arguments
    ///
    /// * `mint` - Public key of the token mint
    ///
    /// # Returns
    ///
    /// Returns the deserialized BondingCurveAccount if successful, or a ClientError if the operation fails
    pub async fn get_bonding_curve_account(
        &self,
        mint: &Pubkey,
    ) -> Result<pumpfun::accounts::BondingCurveAccount, pumpfun::error::ClientError> {
        let bonding_curve_pda =
            Self::get_bonding_curve_pda(mint).ok_or(pumpfun::error::ClientError::BondingCurveNotFound)?;

        let account = self
            .rpc
            .get_account(&bonding_curve_pda)
            .await
            .map_err(pumpfun::error::ClientError::SolanaClientError)?;

        pumpfun::accounts::BondingCurveAccount::try_from_slice(&account.data)
            .map_err(pumpfun::error::ClientError::BorshError)
    }
}
