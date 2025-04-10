mod config;
mod handlers;
mod jito;
mod jupiter;
mod params;
mod pumpfun;
mod solana;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use config::PORT;
use params::Price;
use solana::helpers::get_sol_amount;
use solana::lut;
use solana::refund::refund_keypairs;
use solana::utils::load_keypair;
use solana_sdk::signer::Signer;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;

use dotenv::dotenv;
use std::env;
use std::net::Ipv4Addr;
use std::str::FromStr;

use handlers::{health_check, HandlerManager};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    //seup mode from env
    let mode = env::var("MODE").unwrap_or_else(|_| "DEBUG".to_string());
    let ip_address =
        Ipv4Addr::from_str(&env::var("HOST_IPV4").unwrap_or_else(|_| "127.0.0.1".to_string()))?;

    let addr = SocketAddr::from((ip_address, PORT));

    //Load admin keypair
    let admin_keypair_path = env::var("ADMIN_KEYPAIR").unwrap();
    let admin_keypair = load_keypair(&admin_keypair_path).unwrap();

    println!("Admin keypair loaded: {}", admin_keypair.pubkey());

    //refund_keypairs("EzYBEUw6FL9h6hpT1fM91TuX9DsTwA7ASn1Gs9viBsbr".to_string(), admin_keypair.pubkey().to_string(), "".to_string()).await;

    let handler_manager = Arc::new(Mutex::new(HandlerManager::new(admin_keypair)));
    //Storing LUT to access it across handlers
    let pubkey_to_lut: Arc<Mutex<HashMap<String, Pubkey>>> = Arc::new(Mutex::new(HashMap::new()));
    //setup app
    let app = Router::new()
        .route("/", get(health_check))
        .route(
            "/bump",
            post({
                let handler_manager = Arc::clone(&handler_manager);
                move |payload| {
                    let handler_manager = Arc::clone(&handler_manager);
                    async move { handler_manager.lock().await.bump_token(payload).await }
                }
            }),
        )
        .route(
            "/post-bundle",
            post({
                let handler_manager = Arc::clone(&handler_manager);
                let pubkey_to_lut = Arc::clone(&pubkey_to_lut);
                move |payload| {
                    let handler_manager = Arc::clone(&handler_manager);
                    let pubkey_to_lut = Arc::clone(&pubkey_to_lut);
                    async move {
                        handler_manager
                            .lock()
                            .await
                            .handle_post_bundle(pubkey_to_lut, payload)
                            .await
                    }
                }
            }),
        )
        .route(
            "/lut",
            post({
                let handler_manager = Arc::clone(&handler_manager);
                let pubkey_to_lut = Arc::clone(&pubkey_to_lut);
                move |payload| {
                    let handler_manager = Arc::clone(&handler_manager);
                    let pubkey_to_lut = Arc::clone(&pubkey_to_lut);
                    async move {
                        handler_manager
                            .lock()
                            .await
                            .setup_lut_record(pubkey_to_lut, payload)
                            .await
                    }
                }
            }),
        )
        .route(
            "/pool-info",
            post({
                let handler_manager = Arc::clone(&handler_manager);
                move |payload| {
                    let handler_manager = Arc::clone(&handler_manager);
                    async move {
                        handler_manager
                            .lock()
                            .await
                            .get_pool_information(payload)
                            .await
                    }
                }
            }),
        )
        .route(
            "/sell",
            post({
                let handler_manager = Arc::clone(&handler_manager);
                move |payload| {
                    let handler_manager = Arc::clone(&handler_manager);
                    async move { handler_manager.lock().await.sell_for_keypair(payload).await }
                }
            }),
        )
        .route(
            "/sell-all",
            post({
                let handler_manager = Arc::clone(&handler_manager);
                let pubkey_to_lut = Arc::clone(&pubkey_to_lut);
                move |payload| {
                    let handler_manager = Arc::clone(&handler_manager);
                    let pubkey_to_lut = Arc::clone(&pubkey_to_lut);
                    async move {
                        handler_manager
                            .lock()
                            .await
                            .sell_all_leftover_tokens(pubkey_to_lut, payload)
                            .await
                    }
                }
            }),
        )
        .route(
            "/withdraw",
            post({
                let handler_manager = Arc::clone(&handler_manager);
                move |payload| {
                    let handler_manager = Arc::clone(&handler_manager);
                    async move { handler_manager.lock().await.withdraw_all_sol(payload).await }
                }
            }),
        )
        .route(
            "/free-pay",
            post({
                let handler_manager = Arc::clone(&handler_manager);
                move |payload| {
                    let handler_manager = Arc::clone(&handler_manager);
                    async move { handler_manager.lock().await.pay_recursive(payload).await }
                }
            }),
        )
        .route(
            "/price",
            post({
                let handler_manager = Arc::clone(&handler_manager);
                move |payload| {
                    let handler_manager = Arc::clone(&handler_manager);
                    async move { handler_manager.lock().await.get_price(payload).await }
                }
            }),
        )
        .layer(ServiceBuilder::new().layer(CorsLayer::permissive()));

    println!("Starting Server [{}] at: {}", mode, addr);

    //setup https config
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}
