use serde::{Deserialize, Serialize};

//Has requester public key, token metadata, dev buy amount and wallets buy amount
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostBundleRequest {
    pub requester_pubkey: String,
    pub name: String, 
    pub symbol: String,
    pub uri: String,
    pub dev_buy_amount: u64,
    pub wallets_buy_amount: Vec<u64>,
}

#[derive(Serialize)]
pub struct PostBundleResponse {
    pub public_keys: Vec<String>,
    pub dev_wallet: String,
    pub mint_pubkey: String,
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
pub struct WalletSell {
    pub pubkey: String,
    pub amount: f64
}

#[derive(Deserialize)]
pub struct UniqueSellRequest {
    pub owner_pubkey: String, 
    pub token_mint: String,
    pub wallet_sell: WalletSell,
    pub slippage_bps: u16,
}

#[derive(Deserialize)]
pub struct BulkSellRequest {
    pub owner_pubkey: String,
    pub token_mint: String, 
    pub slippage_bps: Vec<u16>,
    pub wallet_sells: Vec<WalletSell>,
}

#[derive(Deserialize)]
pub struct SellAllRequest {
    pub owner_pubkey: String,
    pub token_mint: String,
    pub slippage_bps: u16,
}

#[derive(Deserialize)]
pub struct CreateTokenMetadata {
    pub name: String,
    pub ticker: String,
    pub uri: String,
}