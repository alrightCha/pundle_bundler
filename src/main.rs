mod jito;
mod pumpfun;
mod solana;
mod config;
mod handlers;
mod params;

use solana::refund::refund_keypairs;
use std::sync::Arc;
use config::setup_https_config;
use config::{DEFAULT_HOST, HTTPS_PORT};

use axum::routing::{get, post};
use axum::Router;
use solana::utils::load_keypair;
use solana_sdk::signer::Signer;
use tower_http::cors::CorsLayer;
use tower::ServiceBuilder;

use dotenv::dotenv;
use std::net::Ipv4Addr;
use std::env;
use std::str::FromStr;

use std::net::SocketAddr;

use handlers::{
    health_check, 
    HandlerManager
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    refund_keypairs().await;
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

    let handler_manager = Arc::new(HandlerManager::new(admin_keypair));

    //setup app
    let app = Router::new()
        .route("/", get(health_check))
        .route("/get-bundle", post({
            let handler_manager = Arc::clone(&handler_manager);
            move |payload| {
                let handler_manager = Arc::clone(&handler_manager);
                async move { handler_manager.get_bundle_wallets(payload).await }
            }
        }))
        .route("/post-bundle", post({
            let handler_manager = Arc::clone(&handler_manager);
            move |payload| {
                let handler_manager = Arc::clone(&handler_manager);
                async move { handler_manager.handle_post_bundle(payload).await }
            }
        }))
        .route("/pool-info", post({
            let handler_manager = Arc::clone(&handler_manager);
            move |payload| {
                let handler_manager = Arc::clone(&handler_manager);
                async move { handler_manager.get_pool_information(payload).await }
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