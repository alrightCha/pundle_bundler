use std::str::FromStr;
use jup_ag::{QuoteConfig, SwapRequest};
use solana_sdk::{pubkey::Pubkey, signature::{Keypair, Signer}, system_instruction::transfer};
use spl_token::amount_to_ui_amount;
use solana_sdk::instruction::Instruction;
use crate::config::ADMIN_PUBKEY;


pub async fn swap_ixs(keypair: &Keypair, base_mint: Pubkey, amount: u64, slippage_bps: Option<u64>) -> Result<Vec<Instruction>, Box<dyn std::error::Error>> {
    
    let sol = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
    let slippage_bps = slippage_bps.unwrap_or(100);
    let only_direct_routes = true; //Might need to change this 

    let quotes = jup_ag::quote(
        base_mint,
        sol,
        amount,
        QuoteConfig {
            only_direct_routes,
            slippage_bps: Some(slippage_bps),
            ..QuoteConfig::default()
        },
    )
    .await?;

    let route = quotes.route_plan[0]
        .swap_info
        .label
        .clone()
        .unwrap_or_else(|| "Unknown DEX".to_string());

    println!(
        "Quote: {} SOL for {} mSOL via {} (worst case with slippage: {}). Impact: {:.2}%",
        amount_to_ui_amount(quotes.in_amount, 9),
        amount_to_ui_amount(quotes.out_amount, 9),
        route,
        amount_to_ui_amount(quotes.other_amount_threshold, 9),
        quotes.price_impact_pct * 100.
    );

    let request: SwapRequest = SwapRequest::new(keypair.pubkey(), quotes.clone());

    let swap_instructions = jup_ag::swap_instructions(request).await?;

    let mut instructions = Vec::new();

    instructions.extend(swap_instructions.setup_instructions);
    instructions.push(swap_instructions.swap_instruction);

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
    Ok(instructions)
}

pub async fn sol_for_tokens(base_mint: Pubkey, amount: u64) -> Result<u64, Box<dyn std::error::Error>> {
    let sol = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
    let only_direct_routes = true; 
    let slippage_bps = 100;
    let quotes = jup_ag::quote(
        base_mint,
        sol,
        amount,
        QuoteConfig {
            only_direct_routes,
            slippage_bps: Some(slippage_bps),
            ..QuoteConfig::default()
        },
    )
    .await?;

    let amount_sol = quotes.out_amount; 
    Ok(amount_sol)
}
