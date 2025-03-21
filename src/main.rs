mod jito;
mod pumpfun;
mod solana;
mod config;
mod handlers;
mod params;
mod jupiter;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use config::PORT;
use solana::lut::{extend_lut, extend_lut_size};
use solana::refund::refund_keypairs;
use solana::utils::load_keypair;
use solana_sdk::signer::Signer;
use tower_http::cors::CorsLayer;
use tower::ServiceBuilder;

use dotenv::dotenv;
use std::net::Ipv4Addr;
use std::env;
use std::str::FromStr;

use std::net::SocketAddr;
use tokio::sync::Mutex;
use std::collections::HashMap;
use solana_sdk::pubkey::Pubkey;
use solana_client::rpc_client::RpcClient;
use crate::config::RPC_URL;
use handlers::{
    health_check, 
    HandlerManager
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    //seup mode from env 
    let mode = env::var("MODE").unwrap_or_else(|_| "DEBUG".to_string());
    let ip_address = Ipv4Addr::from_str(
        &env::var("HOST_IPV4").unwrap_or_else(|_| "127.0.0.1".to_string())
    )?;

    let addr = SocketAddr::from((ip_address, PORT));

    //Load admin keypair 
    let admin_keypair_path = env::var("ADMIN_KEYPAIR").unwrap();
    let admin_keypair = load_keypair(&admin_keypair_path).unwrap();
    let another_admin_keypair = admin_keypair.insecure_clone();
    let client = RpcClient::new(RPC_URL.to_string());

    println!("Admin keypair loaded: {}", admin_keypair.pubkey());

    refund_keypairs("EzYBEUw6FL9h6hpT1fM91TuX9DsTwA7ASn1Gs9viBsbr".to_string(), admin_keypair.pubkey().to_string(), "".to_string()).await;

    let handler_manager = Arc::new(Mutex::new(HandlerManager::new(admin_keypair)));
    //Storing LUT to access it across handlers 
    let pubkey_to_lut:  Arc<Mutex<HashMap<String, Pubkey>>> = Arc::new(Mutex::new(HashMap::new()));

    let key_1 = Pubkey::from_str("CytkYUNagWrfkebwpn3JijDADjetFGBUvvxiFEbgjCe").unwrap();
    let key_2 = Pubkey::from_str("7PFsqyG9Z6rXp4gxPHPP2xSNkCGDUmT6ouPhU1XGa8XC").unwrap();
    let key_3 = Pubkey::from_str("DPzJQHTUApDLmjjksYuf4hVTWvuvEhs7RYBWwa6n9FcJ").unwrap();
    let key_4 = Pubkey::from_str("6yjN7B9ewzG6wJrUNwPRg77CabBvEzUz8SsVDNaVjYkb").unwrap();
    let key_5 = Pubkey::from_str("VzLhCbScHJ9Qhvfy91hXMQAsdp8euRJQrg37VK4u62X").unwrap();
    let key_6 = Pubkey::from_str("E2aZ8rPaEaTStWbVsmWPRHtqPUfFRTqjftw775jDoZQF").unwrap();

    let lut_pubkey = Pubkey::from_str("DwB2ERydDWT8mEFzDpBkJKNjzdjJ7ZVGVPLYLwJu9iKu").unwrap();

    let addresses = vec![key_1, key_2];
    let _ = extend_lut_size(&client, &another_admin_keypair, lut_pubkey, &addresses);

    let other_addresses = vec![key_1, key_2, key_3, key_4];
    let _ = extend_lut_size(&client, &another_admin_keypair, lut_pubkey, &other_addresses);
    //setup app
    let app = Router::new()
        .route("/", get(health_check))
        .route("/get-bundle", post({
            let handler_manager = Arc::clone(&handler_manager);
            move |payload| {
                let handler_manager = Arc::clone(&handler_manager);
                async move { handler_manager.lock().await.get_bundle_wallets(payload).await }
            }
        }))
        .route("/post-bundle", post({
            let handler_manager = Arc::clone(&handler_manager);
            let pubkey_to_lut = Arc::clone(&pubkey_to_lut);
            move |payload| {
                let handler_manager = Arc::clone(&handler_manager);
                let pubkey_to_lut = Arc::clone(&pubkey_to_lut);
                async move { handler_manager.lock().await.handle_post_bundle(pubkey_to_lut, payload).await }
            }
        }))
        .route("/pool-info", post({
            let handler_manager = Arc::clone(&handler_manager);
            move |payload| {
                let handler_manager = Arc::clone(&handler_manager);
                async move { handler_manager.lock().await.get_pool_information(payload).await }
            }
        }))
        .route("/sell", post({
            let handler_manager = Arc::clone(&handler_manager);
            move |payload| {
                let handler_manager = Arc::clone(&handler_manager);
                async move { handler_manager.lock().await.sell_for_keypair(payload).await }
            }
        }))
        .route("/sell-all", post({
            let handler_manager = Arc::clone(&handler_manager);
            let pubkey_to_lut = Arc::clone(&pubkey_to_lut);
            move |payload| {
                let handler_manager = Arc::clone(&handler_manager);
                let pubkey_to_lut = Arc::clone(&pubkey_to_lut);
                async move { handler_manager.lock().await.sell_all_leftover_tokens(pubkey_to_lut, payload).await }
            }
        }))
        .route("/withdraw", post({
            let handler_manager = Arc::clone(&handler_manager);
            move |payload| {
                let handler_manager = Arc::clone(&handler_manager);
                async move { handler_manager.lock().await.withdraw_all_sol(payload).await }
            }
        }))
        .route("/free-pay", post({
            let handler_manager = Arc::clone(&handler_manager);
            move |payload| {
                let handler_manager = Arc::clone(&handler_manager);
                async move { handler_manager.lock().await.pay_recursive(payload).await }
            }
        }))
        .layer(ServiceBuilder::new().layer(CorsLayer::permissive()));

    println!("Starting Server [{}] at: {}", mode, addr);

    //setup https config
    let listener = tokio::net::TcpListener::bind(addr).await?;
            axum::serve(listener, app.into_make_service())
                .await?;

    Ok(())
}