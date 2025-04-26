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
use params::SellAllRequest;
use solana::utils::get_admin_keypair;
use solana_sdk::signer::Signer;
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

use crate::solana::refund::refund_keypairs;
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

    let admin_keypair = get_admin_keypair();

    //Storing LUT to access it across handlers
    let pubkey_to_lut: Arc<Mutex<HashMap<String, Pubkey>>> = Arc::new(Mutex::new(HashMap::new()));
    let new_lut = Pubkey::from_str("hEcKEgCXAEgr56SatwCxQtzGsXKdXWkuhRZ4aW1jQvb").unwrap();

    let sell_all: SellAllRequest = SellAllRequest {
        pubkey: "FGSccTymvdCUJj5tFw7JFLAQamrBW4LLK3HT8f3p9YJc".to_string(),
        mint: "yVsfpmgCDbMHNnhithdfnHDRFookNiVLzUv5jrJAWPp".to_string(),
        admin: true,
    };
    pubkey_to_lut.lock().await.insert(
        "yVsfpmgCDbMHNnhithdfnHDRFookNiVLzUv5jrJAWPp".to_string(),
        new_lut,
    );

    let lut_pub = Arc::clone(&pubkey_to_lut); 
    let _ = sell_all_leftover_tokens(lut_pub, axum::Json(sell_all)).await;
    //setup app
    let app = Router::new()
        .route("/", get(health_check))
        .route(
            "/post-bundle",
            post({
                let pubkey_to_lut = Arc::clone(&pubkey_to_lut);
                async move |payload| handle_post_bundle(pubkey_to_lut, payload).await
            }),
        )
        .route(
            "/lut",
            post({
                let pubkey_to_lut = Arc::clone(&pubkey_to_lut);
                async move |payload| setup_lut_record(pubkey_to_lut, payload).await
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
                let pubkey_to_lut = Arc::clone(&pubkey_to_lut);
                async move |payload| sell_all_leftover_tokens(pubkey_to_lut, payload).await
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
