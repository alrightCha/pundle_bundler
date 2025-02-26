mod jito;
mod pumpfun;
mod solana;
mod config;
mod handlers;
mod params;
mod jupiter;

use std::sync::Arc;
use config::setup_https_config;
use config::{DEFAULT_HOST, HTTPS_PORT};

use axum::routing::{get, post};
use axum::Router;
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

use handlers::{
    health_check, 
    HandlerManager
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    //seup mode from env 
    let mode = env::var("MODE").unwrap_or_else(|_| "DEBUG".to_string());
    //setup ip address from env 
    let ip_address = Ipv4Addr::from_str(
        &env::var("HOST_IPV4").unwrap_or_else(|_| DEFAULT_HOST.to_string())
    )?;
    
    let addr = SocketAddr::from((ip_address, HTTPS_PORT));

    //Load admin keypair 
    let admin_keypair_path = env::var("ADMIN_KEYPAIR").unwrap();
    let admin_keypair = load_keypair(&admin_keypair_path).unwrap();
    
    println!("Admin keypair loaded: {}", admin_keypair.pubkey());

    refund_keypairs("EzYBEUw6FL9h6hpT1fM91TuX9DsTwA7ASn1Gs9viBsbr".to_string(), admin_keypair.pubkey().to_string(), "".to_string()).await;
 

    let handler_manager = Arc::new(Mutex::new(HandlerManager::new(admin_keypair)));
    //Storing LUT to access it across handlers 
    let pubkey_to_lut:  Arc<Mutex<HashMap<String, Pubkey>>> = Arc::new(Mutex::new(HashMap::new()));

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
    match mode.as_str() {
        "PRODUCTION" => {
            let config = setup_https_config().await;
            axum_server::bind_rustls(addr, config)
                .serve(app.into_make_service())
                .await?;
        }
        _ => {
            let listener = tokio::net::TcpListener::bind(addr).await?;
            axum::serve(listener, app.into_make_service())
                .await?;
        }
    }

    Ok(())
}