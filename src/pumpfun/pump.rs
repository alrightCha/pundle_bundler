use crate::config::{ADMIN_PUBKEY, TOKEN_AMOUNT_MULTIPLIER};
use anchor_client::anchor_lang::InstructionData;
use anchor_client::{
    solana_client::nonblocking::rpc_client::RpcClient,
    solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        signature::Keypair,
        signer::Signer,
        system_instruction::transfer,
    },
};
use anchor_spl::associated_token::{
    get_associated_token_address,
    spl_associated_token_account::instruction::create_associated_token_account,
};
use pumpfun_cpi::instruction::{Buy, Create, Sell};

use crate::config::RPC_URL;
use crate::params::PoolInformation;
use crate::pumpfun::bonding_curve::BondingCurveAccount;
use borsh::BorshDeserialize;
use serde::{Deserialize, Serialize};
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use std::{str::FromStr, sync::Arc};

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

    pub bonding_curve: BondingCurveAccount,
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
    pub fn new(payer: Arc<Keypair>) -> Self {
        // Create Solana RPC Client with either WS or HTTP endpoint
        let rpc: RpcClient = RpcClient::new(RPC_URL.to_string());

        let bonding_curve = BondingCurveAccount::default();
        // Return configured PumpFun client
        Self {
            rpc,
            payer,
            bonding_curve,
        }
    }

    pub async fn is_token_live(&self, mint: &Pubkey) -> bool {
        let bonding_curve_account = self.get_bonding_curve_account(mint).await;
        match bonding_curve_account {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    //Gets or creates an associated token account for a keypair TODO: Use this serialized to all accounts and pass it to the lut
    pub fn get_ata(&self, pubkey: &Pubkey, mint: &Pubkey) -> Pubkey {
        let ata: Pubkey = get_associated_token_address(&pubkey, mint);
        ata
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
    pub fn create_instruction(&self, mint: &Keypair, args: Create) -> Instruction {
        let bonding_curve: Pubkey = self.get_bonding_curve_pda(&mint.pubkey()).unwrap();
        Instruction::new_with_bytes(
            pumpfun::constants::accounts::PUMPFUN,
            &args.data(),
            vec![
                AccountMeta::new(mint.pubkey(), true),
                AccountMeta::new(Self::get_mint_authority_pda(), false),
                AccountMeta::new(bonding_curve, false),
                AccountMeta::new(
                    get_associated_token_address(&bonding_curve, &mint.pubkey()),
                    false,
                ),
                AccountMeta::new_readonly(self.get_global_pda(), false),
                AccountMeta::new_readonly(pumpfun::constants::accounts::MPL_TOKEN_METADATA, false),
                AccountMeta::new(Self::get_metadata_pda(&mint.pubkey()), false),
                AccountMeta::new(self.payer.pubkey(), true),
                AccountMeta::new_readonly(pumpfun::constants::accounts::SYSTEM_PROGRAM, false),
                AccountMeta::new_readonly(pumpfun::constants::accounts::TOKEN_PROGRAM, false),
                AccountMeta::new_readonly(
                    pumpfun::constants::accounts::ASSOCIATED_TOKEN_PROGRAM,
                    false,
                ),
                AccountMeta::new_readonly(pumpfun::constants::accounts::RENT, false),
                AccountMeta::new_readonly(pumpfun::constants::accounts::EVENT_AUTHORITY, false),
                AccountMeta::new_readonly(pumpfun::constants::accounts::PUMPFUN, false),
            ],
        )
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
        with_stimulate: bool,
    ) -> Result<Vec<Instruction>, pumpfun::error::ClientError> {
        // Get accounts and calculate buy amounts
        let global_account = self.get_global_account().await.unwrap();

        let buy_amount_with_slippage =
            Self::calculate_with_slippage(amount_sol, slippage_basis_points.unwrap_or(500));

        let buy_amount = match with_stimulate {
            true => self
                .bonding_curve
                .get_buy_price(buy_amount_with_slippage)
                .unwrap(),
            false => {
                let bonding_curve_account = self.get_bonding_curve_account(mint).await?;
                bonding_curve_account
                    .get_buy_price(buy_amount_with_slippage)
                    .unwrap()
            }
        };

        let mut instructions: Vec<Instruction> = Vec::new();

        // Add ata instruction or get acc if available
        let ata: Pubkey = get_associated_token_address(&keypair.pubkey(), mint);
        println!("PUBKEY: {:?} ATA: {:?}", keypair.pubkey().to_string(), ata);

        if self.rpc.get_account(&ata).await.is_err() {
            let create_ata_ix = create_associated_token_account(
                &keypair.pubkey(),
                &keypair.pubkey(),
                mint,
                &pumpfun::constants::accounts::TOKEN_PROGRAM,
            );
            instructions.push(create_ata_ix);
        }

        let bonding_curve: Pubkey = self.get_bonding_curve_pda(mint).unwrap();

        let args = Buy {
            _amount: buy_amount,
            _max_sol_cost: amount_sol,
        };
        let ix = Instruction::new_with_bytes(
            pumpfun::constants::accounts::PUMPFUN,
            &args.data(),
            vec![
                AccountMeta::new_readonly(self.get_global_pda(), false),
                AccountMeta::new(global_account.fee_recipient, false),
                AccountMeta::new_readonly(*mint, false),
                AccountMeta::new(bonding_curve, false),
                AccountMeta::new(get_associated_token_address(&bonding_curve, mint), false),
                AccountMeta::new(get_associated_token_address(&keypair.pubkey(), mint), false),
                AccountMeta::new(keypair.pubkey(), true),
                AccountMeta::new_readonly(pumpfun::constants::accounts::SYSTEM_PROGRAM, false),
                AccountMeta::new_readonly(pumpfun::constants::accounts::TOKEN_PROGRAM, false),
                AccountMeta::new_readonly(pumpfun::constants::accounts::RENT, false),
                AccountMeta::new_readonly(pumpfun::constants::accounts::EVENT_AUTHORITY, false),
                AccountMeta::new_readonly(pumpfun::constants::accounts::PUMPFUN, false),
            ],
        );
        instructions.push(ix);

        Ok(instructions)
    }

    /// Get all addresses to push to lut to optimize it for interacting with pumpfun instructions
    ///
    /// # Arguments
    ///
    /// * `mint` - Public key of the token mint to sell
    ///
    /// # Returns vec of public keys to extend
    /// # Note
    /// Should not have well known accounts and system programs should not be in lut
    /// From Solana's documentation: The addresses of programs that are invoked in the transaction (i.e., the program IDs of each instruction) must be present in the static account keys and cannot be loaded through an address table lookup. So any program ID that's part of an instruction's program_id field must be present in the static list, not via LUT.
    /// Returns the sell transaction request builder
    ///
    pub async fn get_addresse_for_lut(&self, mint: &Pubkey) -> Vec<Pubkey> {
        //AUTHORITY & FEE RECIPIENT
        let global_fee_recipient: Pubkey = self.get_global_account().await.unwrap().fee_recipient;
        let auth: Pubkey = pumpfun::constants::accounts::EVENT_AUTHORITY;

        //let pump: Pubkey = pumpfun::constants::accounts::PUMPFUN; // Should add ?

        //PDAS
        let mint_authority_pda: Pubkey = Self::get_mint_authority_pda();
        let metadata_pda: Pubkey = Self::get_metadata_pda(&mint);
        let global_pda: Pubkey = self.get_global_pda();
        let bonding_curve: Pubkey = self.get_bonding_curve_pda(mint).unwrap();

        //ATA
        let ata: Pubkey = get_associated_token_address(&bonding_curve, &mint);

        vec![
            global_fee_recipient,
            global_pda,
            bonding_curve,
            auth,
            mint_authority_pda,
            ata,
            metadata_pda,
        ]
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
        println!("Balance: {:?}", balance_u64);
        println!("Amount token: {:?}", amount_token);
        // If amount_token is greater than balance, use balance instead
        let amount = match amount_token {
            Some(requested_amount) => {
                if requested_amount > balance_u64 {
                    balance_u64
                } else {
                    requested_amount
                }
            }
            None => balance_u64,
        };

        let global_account = self.get_global_account().await?;
        let bonding_curve_account = self.get_bonding_curve_account(mint).await?;

        let min_sol_output = bonding_curve_account
            .get_sell_price(amount, global_account.fee_basis_points)
            .map_err(pumpfun::error::ClientError::BondingCurveError)?;

        let min_sol_output =
            Self::calculate_with_slippage(min_sol_output, slippage_basis_points.unwrap_or(500));

        let mut instructions: Vec<Instruction> = Vec::new();

        let priority_fee_ix = ComputeBudgetInstruction::set_compute_unit_price(2_000_000);
        instructions.push(priority_fee_ix);

        let bonding_curve: Pubkey = self.get_bonding_curve_pda(mint).unwrap();

        let args = Sell {
            _amount: amount,
            _min_sol_output: min_sol_output,
        };

        let sell_ix = Instruction::new_with_bytes(
            pumpfun::constants::accounts::PUMPFUN,
            &args.data(),
            vec![
                AccountMeta::new_readonly(self.get_global_pda(), false),
                AccountMeta::new(global_account.fee_recipient, false),
                AccountMeta::new_readonly(*mint, false),
                AccountMeta::new(bonding_curve, false),
                AccountMeta::new(get_associated_token_address(&bonding_curve, mint), false),
                AccountMeta::new(get_associated_token_address(&keypair.pubkey(), mint), false),
                AccountMeta::new(keypair.pubkey(), true),
                AccountMeta::new_readonly(pumpfun::constants::accounts::SYSTEM_PROGRAM, false),
                AccountMeta::new_readonly(
                    pumpfun::constants::accounts::ASSOCIATED_TOKEN_PROGRAM,
                    false,
                ),
                AccountMeta::new_readonly(pumpfun::constants::accounts::TOKEN_PROGRAM, false),
                AccountMeta::new_readonly(pumpfun::constants::accounts::EVENT_AUTHORITY, false),
                AccountMeta::new_readonly(pumpfun::constants::accounts::PUMPFUN, false),
            ],
        );

        let tax_amount = min_sol_output * 5 / 100; // Calculate 1% of min_sol_output

        let tax_ix = transfer(
            &keypair.pubkey(),
            &Pubkey::from_str(ADMIN_PUBKEY).unwrap(), // Send to admin public key
            tax_amount,
        );

        instructions.push(sell_ix);
        instructions.push(tax_ix);

        Ok(instructions)
    }

    //Sells all tokens in the keypair
    pub async fn sell_all_ix(
        &self,
        mint: &Pubkey,
        keypair: &Keypair,
    ) -> Result<Vec<Instruction>, pumpfun::error::ClientError> {
        let ata: Pubkey = get_associated_token_address(&keypair.pubkey(), &mint);
        let balance = self.rpc.get_token_account_balance(&ata).await?;
        let balance_u64: u64 = balance.amount.parse::<u64>().unwrap();

        if balance_u64 < 1000 {
            return Err(pumpfun::error::ClientError::InsufficientFunds);
        }

        let global_account = self.get_global_account().await?;
        let bonding_curve_account = self.get_bonding_curve_account(mint).await?;

        let min_sol = bonding_curve_account
            .get_sell_price(balance_u64, global_account.fee_basis_points)
            .map_err(pumpfun::error::ClientError::BondingCurveError)?;

        // 500 basis points = 5% slippage
        // Setting slippage to 10000 (100%) means you'll accept any price above min_sol * 0,
        // which could result in getting much less SOL than expected
        let min_sol_output = Self::calculate_with_slippage(
            min_sol, 10000, // 10% slippage - you'll get at least 0% of min_sol
        );

        let mut instructions: Vec<Instruction> = Vec::new();

        let bonding_curve: Pubkey = self.get_bonding_curve_pda(mint).unwrap();

        let args = Sell {
            _amount: balance_u64,
            _min_sol_output: min_sol_output,
        };

        let sell_ix = Instruction::new_with_bytes(
            pumpfun::constants::accounts::PUMPFUN,
            &args.data(),
            vec![
                AccountMeta::new_readonly(self.get_global_pda(), false),
                AccountMeta::new(global_account.fee_recipient, false),
                AccountMeta::new_readonly(*mint, false),
                AccountMeta::new(bonding_curve, false),
                AccountMeta::new(get_associated_token_address(&bonding_curve, mint), false),
                AccountMeta::new(get_associated_token_address(&keypair.pubkey(), mint), false),
                AccountMeta::new(keypair.pubkey(), true),
                AccountMeta::new_readonly(pumpfun::constants::accounts::SYSTEM_PROGRAM, false),
                AccountMeta::new_readonly(
                    pumpfun::constants::accounts::ASSOCIATED_TOKEN_PROGRAM,
                    false,
                ),
                AccountMeta::new_readonly(pumpfun::constants::accounts::TOKEN_PROGRAM, false),
                AccountMeta::new_readonly(pumpfun::constants::accounts::EVENT_AUTHORITY, false),
                AccountMeta::new_readonly(pumpfun::constants::accounts::PUMPFUN, false),
            ],
        );

        let tax_amount = min_sol * 5 / 100; // Calculate 1% of min_sol_output
        let tax_ix = transfer(
            &keypair.pubkey(),
            &Pubkey::from_str(ADMIN_PUBKEY).unwrap(), // Send to admin public key
            tax_amount,
        );

        instructions.push(sell_ix);
        instructions.push(tax_ix);
        Ok(instructions)
    }

    /// Gets the Program Derived Address (PDA) for the mint authority
    ///
    /// # Returns
    ///
    /// Returns the PDA public key derived from the MINT_AUTHORITY_SEED
    fn get_mint_authority_pda() -> Pubkey {
        let seeds: &[&[u8]; 1] = &[pumpfun::constants::seeds::MINT_AUTHORITY_SEED];
        let program_id: &Pubkey = &pumpfun::cpi::ID;
        Pubkey::find_program_address(seeds, program_id).0
    }

    fn get_metadata_pda(mint: &Pubkey) -> Pubkey {
        let seeds: &[&[u8]; 3] = &[
            pumpfun::constants::seeds::METADATA_SEED,
            pumpfun::constants::accounts::MPL_TOKEN_METADATA.as_ref(),
            mint.as_ref(),
        ];
        let program_id: &Pubkey = &pumpfun::constants::accounts::MPL_TOKEN_METADATA;
        Pubkey::find_program_address(seeds, program_id).0
    }

    /// Gets the global state account data containing program-wide configuration
    ///
    /// # Returns
    ///
    /// Returns the deserialized GlobalAccount if successful, or a ClientError if the operation fails
    pub async fn get_global_account(
        &self,
    ) -> Result<pumpfun::accounts::GlobalAccount, pumpfun::error::ClientError> {
        let global: Pubkey = self.get_global_pda();

        let account = self
            .rpc
            .get_account(&global)
            .await
            .map_err(pumpfun::error::ClientError::SolanaClientError)?;

        solana_sdk::borsh1::try_from_slice_unchecked::<pumpfun::accounts::GlobalAccount>(
            &account.data,
        )
        .map_err(pumpfun::error::ClientError::BorshError)
    }

    /// Gets the Program Derived Address (PDA) for the global state account
    ///
    /// # Returns
    ///
    /// Returns the PDA public key derived from the GLOBAL_SEED
    pub fn get_global_pda(&self) -> Pubkey {
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
    pub fn get_bonding_curve_pda(&self, mint: &Pubkey) -> Option<Pubkey> {
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
        let bonding_curve_pda = self
            .get_bonding_curve_pda(mint)
            .ok_or(pumpfun::error::ClientError::BondingCurveNotFound)?;

        let account = self
            .rpc
            .get_account(&bonding_curve_pda)
            .await
            .map_err(pumpfun::error::ClientError::SolanaClientError)?;

        solana_sdk::borsh1::try_from_slice_unchecked::<pumpfun::accounts::BondingCurveAccount>(&account.data)
            .map_err(pumpfun::error::ClientError::BorshError)
    }

    pub async fn get_pool_information(
        &self,
        mint: &Pubkey,
    ) -> Result<PoolInformation, pumpfun::error::ClientError> {
        let bonding_curve_account = self.get_bonding_curve_account(mint).await?;
        let current_mc = bonding_curve_account.get_market_cap_sol();
        let sell_price = bonding_curve_account
            .get_sell_price(100000 * TOKEN_AMOUNT_MULTIPLIER, 500)
            .unwrap(); // price per 10k tokens
        let is_bonding_curve_complete = bonding_curve_account.complete;
        let reserve_sol = bonding_curve_account.real_sol_reserves;
        let reserve_token = bonding_curve_account.real_token_reserves;

        let pool_information = PoolInformation {
            current_mc,
            sell_price,
            is_bonding_curve_complete,
            reserve_sol,
            reserve_token,
        };
        Ok(pool_information)
    }

    fn calculate_with_slippage(amount: u64, basis_points: u64) -> u64 {
        let res = amount - (amount * basis_points) / 10000;
        res
    }
}
