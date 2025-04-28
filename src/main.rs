mod config;
mod handlers;
mod jito;
mod jupiter;
mod params;
mod pumpfun;
mod solana;

use std::sync::Arc;
use jito::bundle::process_bundle;
use params::KeypairWithAmount;
use solana_sdk::native_token::LAMPORTS_PER_SOL;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use tokio::sync::RwLock;
use axum::routing::{get, post};
use axum::Router;
use config::{JITO_TIP_AMOUNT, PORT};
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use pumpfun_cpi::instruction::Create;

use dotenv::dotenv;
use std::env;
use std::net::Ipv4Addr;
use std::str::FromStr;

use handlers::{
    complete_bundle, get_pool_information, get_price, handle_post_bundle, health_check,
    pay_recursive, sell_all_leftover_tokens, sell_for_keypair, setup_lut_record, withdraw_all_sol,
};

use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::net::SocketAddr;

pub type SharedLut = Arc<RwLock<HashMap<String, Pubkey>>>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = Keypair::from_base58_string("5hRpWaBJ2dAYw6VmHF8y3Bt97iFBxVpnPiRSTwHPLvWCPSmgBZnYaGhPCSMUecyNhjkwdsFcHco6NAcxPGaxJxNC"); //0.1559
    let kp1 = Keypair::from_base58_string("5kCGq9qfrqykXvwwTvZ4hpCKKeZZnaAjsUxUympY9NHeo58kFq9n5rEiePErWZiL31cH7LF6QLuwqPgQLcmsdp1h"); //0.129
    let kp2 = Keypair::from_base58_string("3uUc9JDvwfTxqFJeFKRJ6Kb2vLAEhgCeiTXhkjzEWr2xStYefgqxACJcRztYcz6bTjGvJk16nVLq11p9128VPTXn"); //0.177


    let dev_with_amount = KeypairWithAmount {
        keypair: dev, 
        amount: 14 * LAMPORTS_PER_SOL / 100
    }; 

    let one = KeypairWithAmount {
        keypair: kp1, 
        amount: 12 * LAMPORTS_PER_SOL / 100 
    }; 

    let two = KeypairWithAmount {
        keypair: kp2, 
        amount: 16 * LAMPORTS_PER_SOL / 100 
    }; 

    let with_amounts: Vec<KeypairWithAmount> = vec![one, two]; 

    let mint = Keypair::new(); 

    let priority_fee = 700000; 
    
    let token = Create {
        _name: "x".to_string(), 
        _symbol: "x".to_string(), 
        _uri: "https://ipfs.io/ipfs/QmRKmENhmN8Lq5GT4A3XcbJHM3Yzjiks4KBp5fyxgNqspY".to_string(), 
        _creator: dev_with_amount.keypair.pubkey()

    }; 

    let bundle = process_bundle(with_amounts, dev_with_amount, &mint, token, priority_fee, JITO_TIP_AMOUNT, false).await; 

    Ok(())
}
