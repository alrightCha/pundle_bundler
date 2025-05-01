mod config;
mod handlers;
mod jito;
mod jupiter;
mod params;
mod pumpfun;
mod solana;
mod warmup;

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
    
    let dev = Keypair::from_base58_string("giPQn8DXaLVYb6QM6WjWLL1ZY9bmoGnaEUreRFTq5AaPrPeVnEV3hLuzBGMBJkJce8jZG456q71n2SJFVW3jgpR"); //0.1559
    let kp1 = Keypair::from_base58_string("5UFQPVBAYeViB84MXtqHwPP2kJWnANQ175v76n6cupFMkratDQnQahbC4eaqRwJ3Lq5DSFpKtzwnDSNydLEBHkdU"); //0.129
    let kp2 = Keypair::from_base58_string("3Qc3JUbm4yoC66y82SbeUiEh2jnZNyUy7L7Hs1EsN7LPyUbEKi56sYnrGxDE9brvi7wCAvJL1aacnxuoc8Z3poLF"); //0.177
    let kp3 = Keypair::from_base58_string("3NQmT5LRvw1ih25t99Ybhe2DgQp6C5tGDC3ZX2My6jA2TJ3LTH5XPCxeQCPS7BxZKVfuyvvhAhpvGznhGgcpKnoc"); 
    let kp4 = Keypair::from_base58_string("4MRF4XJEACUKGmPXZJvu3YB5bZKQAuu1fchSCdYo7WCh3XGEvVBxhYeqfRaSi3s9pv6GKx638Npeqrr86Xku5qvN"); 
    let kp5 = Keypair::from_base58_string("5MwAax9brEZdg4CaEFpvfQZ6hS7UqxGxih5THA8iGSXdKL6BYyNrL1fztTUR5N65g2RhgJHhe4AyTxfTJws5hYTB"); 
    let kp6 = Keypair::from_base58_string("4avWj8CEZQVZhNGuMHs8vYgrSYm3nBE9a7XSqpmiH6ZwScrAUPYEuBgFe77gfZhioajgtWexZXKVaep94r56bNAW"); 
    let kp7 = Keypair::from_base58_string("5CkoGxoEcYHavpQ4ewjVJJNH91dXr1Bo6jwBACJQ1hW5dxVoKron7ohPo7yeFU9YJnqkXK3WJXR8nWXzRw6GdZE9"); 

    let dev_with_amount = KeypairWithAmount {
        keypair: dev, 
        amount: 300000000
    }; 

    let one = KeypairWithAmount {
        keypair: kp1, 
        amount: 270000000
    }; 

    let two = KeypairWithAmount {
        keypair: kp2, 
        amount: 280000000
    }; 

    let three = KeypairWithAmount {
        keypair: kp3, 
        amount: 280000000
    }; 

    let four = KeypairWithAmount {
        keypair: kp4, 
        amount: 280000000
    }; 

    let five = KeypairWithAmount {
        keypair: kp5, 
        amount: 280000000
    }; 

    let six = KeypairWithAmount {
        keypair: kp6, 
        amount: 280000000
    }; 

    let seven = KeypairWithAmount {
        keypair: kp7, 
        amount: 280000000
    }; 

    let with_amounts: Vec<KeypairWithAmount> = vec![one, two, three, four, five, six, seven]; 

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