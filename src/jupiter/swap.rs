use crate::config::{ADMIN_PUBKEY, RPC_URL};
use crate::solana::utils::get_ata_balance;
use anchor_spl::associated_token::spl_associated_token_account::instruction::create_associated_token_account;
use anchor_spl::token::spl_token::instruction::close_account;
use anchor_spl::{
    associated_token::get_associated_token_address,
    token::spl_token::{native_mint::ID, ID as SplID},
};
use jup_ag::{QuoteConfig, SwapRequest};
use solana_client::rpc_client::RpcClient;
use solana_sdk::instruction::Instruction;

use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction::transfer,
};
use spl_token::amount_to_ui_amount;
use std::cmp::min;
use std::str::FromStr;

pub async fn swap_ixs(
    keypair: &Keypair,
    base_mint: Pubkey,
    amount: Option<u64>,
    slippage_bps: Option<u64>,
    direction: bool,
) -> Result<Vec<Instruction>, Box<dyn std::error::Error + Send + Sync>> {
    let client = RpcClient::new(RPC_URL);
    let sol = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
    let slippage_bps = slippage_bps.unwrap_or(100);
    let only_direct_routes = true; //Might need to change this

    let quotes = match direction {
        true => {
            let final_amount = match amount {
                Some(amount) => amount,
                None => {
                    let amount = get_ata_balance(&client, &keypair, &base_mint).await;
                    amount.unwrap()
                }
            };
            jup_ag::quote(
                base_mint,
                sol,
                final_amount,
                QuoteConfig {
                    only_direct_routes,
                    slippage_bps: Some(slippage_bps),
                    ..QuoteConfig::default()
                },
            )
            .await?
        }
        false => {
            let mut sol_amount = 0;
            if let Some(amount) = amount {
                sol_amount = amount;
            }
            jup_ag::quote(
                sol,
                base_mint,
                sol_amount,
                QuoteConfig {
                    only_direct_routes,
                    slippage_bps: Some(slippage_bps),
                    ..QuoteConfig::default()
                },
            )
            .await?
        }
    };

    let route = quotes.route_plan[0]
        .swap_info
        .label
        .clone()
        .unwrap_or_else(|| "Unknown DEX".to_string());

    println!(
        "Quote: {} SOL for {} JUP via {} (worst case with slippage: {}). Impact: {:.2}%",
        amount_to_ui_amount(quotes.in_amount, 9),
        amount_to_ui_amount(quotes.out_amount, 6),
        route,
        amount_to_ui_amount(quotes.other_amount_threshold, 6),
        quotes.price_impact_pct * 100.
    );

    let request: SwapRequest = SwapRequest::new(keypair.pubkey(), quotes.clone());
    let swap_instructions = jup_ag::swap_instructions(request).await?;

    let mut instructions = Vec::new();

    instructions.extend(swap_instructions.setup_instructions);
    instructions.push(swap_instructions.swap_instruction);

    if direction {
        if let Some(cleanup_instruction) = swap_instructions.cleanup_instruction {
            instructions.push(cleanup_instruction);
        }

        let tax_amount = quotes.out_amount / 100;

        let tax_ix = transfer(
            &keypair.pubkey(),
            &Pubkey::from_str(ADMIN_PUBKEY).unwrap(), // Send to admin public key
            tax_amount,
        );

        instructions.push(tax_ix);
    }

    Ok(instructions)
}

pub async fn shadow_swap(
    client: &RpcClient,
    keypair: &Keypair,
    mint: Pubkey,
    recipient: Pubkey,
    slippage_bps: Option<u64>,
    amount: u64
) -> Result<Vec<Instruction>, Box<dyn std::error::Error + Send + Sync>> {
    println!("Collecting shadow swap IX for {} with passed amount {}", recipient.to_string(), amount); 
    let mut instructions = Vec::new();
    let ata: Pubkey = get_associated_token_address(&recipient, &ID);

    if client.get_account(&ata).is_err() {
        let create_ata_ix =
            create_associated_token_account(&keypair.pubkey(), &recipient, &ID, &SplID);
        instructions.push(create_ata_ix);
    }

    let slippage_bps = slippage_bps.unwrap_or(100);
    let only_direct_routes = false; //Might need to change this

    let quotes = jup_ag::quote(
        mint,
        ID,
        amount,
        QuoteConfig {
            only_direct_routes,
            slippage_bps: Some(slippage_bps),
            ..QuoteConfig::default()
        },
    )
    .await?;

    let mut request: SwapRequest = SwapRequest::new(keypair.pubkey(), quotes.clone());

    println!(
        "Found ATA: {:?} for wallet address: {:?}",
        ata.to_string(),
        recipient.to_string()
    );

    request.destination_token_account = Some(ata);
    let swap_instructions = jup_ag::swap_instructions(request).await?;

    instructions.extend(swap_instructions.setup_instructions);
    instructions.push(swap_instructions.swap_instruction);

    let unwrap_wsol = close_account(&SplID, &ata, &recipient, &recipient, &[&recipient]).unwrap();
    instructions.push(unwrap_wsol);

    Ok(instructions)
}

pub async fn sol_for_tokens(
    base_mint: Pubkey,
    amount: u64,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    rate(base_mint, amount, true).await
}

pub async fn tokens_for_sol(
    base_mint: Pubkey,
    amount: u64,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let rate = rate(base_mint, amount, false).await; 
    if let Ok(rate) = rate{
        println!("Rate found for {} SOL: {} JUP.", amount, rate); 
    }
    rate
}

pub async fn rate(
    base_mint: Pubkey,
    amount: u64,
    direction_sol: bool,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let sol = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
    let only_direct_routes = true;
    let slippage_bps = 100;

    // Determine direction and decimal scaling
    let (input_mint, output_mint, amount_scaled) = if direction_sol {
        // base_mint → SOL (e.g., USDC → SOL)
        (base_mint, sol, amount * 1_000_000) // assume base is 6 decimals
    } else {
        // SOL → base_mint (e.g., SOL → USDC)
        (sol, base_mint, amount * 1_000_000_000) // SOL is 9 decimals
    };

    let quotes = jup_ag::quote(
        input_mint,
        output_mint,
        amount_scaled,
        QuoteConfig {
            only_direct_routes,
            slippage_bps: Some(slippage_bps),
            ..QuoteConfig::default()
        },
    )
    .await?;

    let out_amount = quotes.out_amount;
    println!(
        "Swap {:?} {} → {} = {:?}",
        amount,
        if direction_sol { "token" } else { "SOL" },
        if direction_sol { "SOL" } else { "token" },
        out_amount
    );

    Ok(out_amount)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    // USDC mint on mainnet (6 decimals)
    const USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

    #[tokio::test]
    async fn test_usdc_to_sol() {
        let usdc = Pubkey::from_str(USDC_MINT).unwrap();
        let amount = 150; // 1 USDC
        let out = rate(usdc, amount, true).await.unwrap();
        println!("1 USDC → ~{} lamports", out);
        assert!(out > 0);
    }

    #[tokio::test]
    async fn test_sol_to_usdc() {
        let usdc = Pubkey::from_str(USDC_MINT).unwrap();
        let amount = 1; // 1 SOL
        let out = rate(usdc, amount, false).await.unwrap();
        println!("1 SOL → ~{} usdc micro-units", out);
        assert!(out > 0);
    }
}