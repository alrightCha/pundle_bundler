use serde::{Deserialize, Serialize};
use solana_sdk::signature::Keypair;
//Has requester public key, token metadata, dev buy amount and wallets buy amount
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostBundleRequest {
    pub requester_pubkey: String,
    pub name: String, 
    pub symbol: String,
    pub uri: String,
    pub dev_buy_amount: u64,
    pub wallets_buy_amount: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[derive(Debug)]
pub struct Wallet {
    pub pubkey: String, 
    pub secret_key: String,
    pub is_dev: bool,
    pub amount: u64
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[derive(Debug)]
pub struct PostBundleResponse {
    pub pubkey: String,
    pub mint: String,
    pub due_amount: u64,
    pub wallets: Vec<Wallet>
}

#[derive(Deserialize)]
pub struct GetBundleWalletsRequest {
    pub requester_pubkey: String,
}

#[derive(Serialize)]
pub struct BundleWallet {
    pub pubkey: String,
    pub secret_key: String,
}

#[derive(Serialize)]
pub struct GetBundleWalletsResponse {
    pub keypairs: Vec<BundleWallet>,
}

#[derive(Deserialize)]
pub struct UniqueSellRequest {
    pub pubkey: String, 
    pub mint: String,
    pub wallet: String,
    pub amount: u64,
}

#[derive(Deserialize)]
pub struct SellAllRequest {
    pub owner_pubkey: String,
    pub token_mint: String,
    pub slippage_bps: u16,
    pub with_admin_transfer: bool,
}

#[derive(Serialize)]
pub struct SellResponse {
    pub success: bool,
}

#[derive(Deserialize)]
pub struct CreateTokenMetadata {
    pub name: String,
    pub ticker: String,
    pub uri: String,
}

#[derive(Deserialize)]
#[derive(Debug)]
pub struct GetPoolInformationRequest {
    pub mint: String,
}


#[derive(Debug)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolInformation {
    pub current_mc: u64,
    pub sell_price: u64,
    pub is_bonding_curve_complete: bool,
    pub reserve_sol: u64,
    pub reserve_token: u64,
}


#[derive(Debug)]
pub struct KeypairWithAmount {
    pub keypair: Keypair,
    pub amount: u64,
}