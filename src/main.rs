mod config;
mod handlers;
mod jito;
mod jupiter;
mod params;
mod pumpfun;
mod solana;

use std::sync::Arc;
use tokio::sync::RwLock;
use axum::routing::{get, post};
use axum::Router;
use config::PORT;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;

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
    dotenv().ok();
    //seup mode from env
    let mode = env::var("MODE").unwrap_or_else(|_| "DEBUG".to_string());
    let ip_address =
        Ipv4Addr::from_str(&env::var("HOST_IPV4").unwrap_or_else(|_| "127.0.0.1".to_string()))?;

    let addr = SocketAddr::from((ip_address, PORT));

    //Storing LUT to access it across handlers
    let pubkey_to_lut: SharedLut = Arc::new(RwLock::new(HashMap::new()));
    
    //setup app
    let app = Router::new()
        .route("/", get(health_check))
        .route(
            "/post-bundle",
            post({
                let lut = Arc::clone(&pubkey_to_lut);
                async move |payload| {
                    handle_post_bundle(lut, payload).await
                }
            }),
        )
        .route(
            "/lut",
            post({
                let lut = Arc::clone(&pubkey_to_lut);
                async move |payload| {
                    setup_lut_record(lut, payload).await
                }
            }),
        )
        .route(
            "/pool-info",
            post(async move |payload| get_pool_information(payload).await),
        )
        .route(
            "/confirm",
            post(async move |payload| complete_bundle(payload).await),
        )
        .route(
            "/sell",
            post(async move |payload| sell_for_keypair(payload).await),
        )
        .route(
            "/sell-all",
            post({
                let lut = Arc::clone(&pubkey_to_lut);
                async move |payload| {
                    sell_all_leftover_tokens(lut, payload).await
                }
            }),
        )
        .route(
            "/withdraw",
            post(async move |payload| withdraw_all_sol(payload).await),
        )
        .route(
            "/free-pay",
            post(async move |payload| pay_recursive(payload).await),
        )
        .route(
            "/price",
            post(async move |payload| get_price(payload).await),
        )
        .layer(ServiceBuilder::new().layer(CorsLayer::permissive()));

    println!("Starting Server [{}] at: {}", mode, addr);

    //setup https config
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}