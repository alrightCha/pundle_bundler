use std::str::FromStr;

use jupiter_swap_api_client::{
    quote::QuoteRequest, swap::SwapRequest, transaction_config::TransactionConfig,
    JupiterSwapApiClient,
};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::instruction::Instruction;


pub async fn get_swap_instruction(pubkey: Pubkey, input_mint: Pubkey, amount: u64, slippage_bps: Option<u16>) -> Instruction {
    let native_mint: Pubkey = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
    let jupiter_swap_api_client = JupiterSwapApiClient::new("https://quote-api.jup.ag/v6".to_string());

    //Quote resquest 
    let quote_request = QuoteRequest {
        amount: amount,
        input_mint: input_mint,
        output_mint: native_mint,
        slippage_bps: slippage_bps.unwrap_or(50),
        ..QuoteRequest::default()
    };

    // GET /quote
    let quote_response = jupiter_swap_api_client.quote(&quote_request).await.unwrap();


    let swap_instructions = jupiter_swap_api_client
        .swap_instructions(&SwapRequest {
            user_public_key: pubkey,
            quote_response,
            config: TransactionConfig::default(),
        })
        .await
        .unwrap().swap_instruction;

    swap_instructions
}