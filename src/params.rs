use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
//Has requester public key, token metadata, dev buy amount and wallets buy amount
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostBundleRequest {
    pub requester_pubkey: String,
    pub vanity: String,
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub dev_buy_amount: u64,
    pub wallets_buy_amount: u64,
    pub with_delay: bool,
    pub split_percent: f64,
    pub fee: u64,
    pub jito_tip: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[derive(Debug)]
pub struct Wallet {
    pub pubkey: String,
    pub secret_key: String,
    pub is_dev: bool,
    pub amount: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[derive(Debug)]
pub struct PostBundleResponse {
    pub pubkey: String,
    pub mint: String,
    pub due_amount: u64,
    pub wallets: Vec<Wallet>,
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
pub struct CollectFeesRequest {
    pub requester: String, 
    pub mint: String,
    pub dev: String,
}

#[derive(Serialize)] 
pub struct CollectedFeesResponse {
    pub amount: u64 
}

#[derive(Deserialize)]
pub struct SellAllRequest {
    pub pubkey: String,
    pub mint: String,
    pub admin: bool,
}

#[derive(Serialize)]
pub struct SellResponse {
    pub success: bool,
}

#[derive(Deserialize)]
pub struct WithdrawAllSolRequest {
    pub pubkey: String,
    pub mint: String,
}

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone)]
pub struct CreateTokenMetadata {
    pub name: String,
    pub ticker: String,
    pub uri: String,
    pub creator: Pubkey,
}

#[derive(Deserialize, Debug)]
pub struct GetPoolInformationRequest {
    pub mint: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolInformation {
    pub current_mc: u64,
    pub sell_price: u64,
    pub is_bonding_curve_complete: bool,
    pub reserve_sol: u64,
    pub reserve_token: u64,
}

#[derive(Deserialize)]
pub struct BumpRequest {
    pub mint_address: String,
    pub lamports: u64,
}

#[derive(Serialize)]
pub struct BumpResponse {
    pub success: bool,
}

#[derive(Debug)]
pub struct KeypairWithAmount {
    pub keypair: Keypair,
    pub amount: u64,
}

#[derive(Deserialize)]
pub struct RecursivePayRequest {
    pub pubkey: String,
    pub mint: String,
    pub lamports: u64,
}

#[derive(Serialize)]
pub struct RecursivePayResponse {
    pub signatures: Vec<String>,
}

#[derive(Deserialize, Debug)]
pub struct LutRecord {
    pub mint: String,
    pub lut: String,
}

#[derive(Deserialize, Debug)]
pub struct LutInit {
    pub luts: Vec<LutRecord>,
}

#[derive(Serialize)]
pub struct LutResponse {
    pub confirmed: bool,
}

#[derive(Deserialize, Debug)]
pub struct Price {
    pub token_amount: u64,
    pub mint: String,
}

#[derive(Serialize)]
pub struct PriceResponse {
    pub price: u64,
}

#[derive(Deserialize, Debug)]
pub struct CompleteRequest {
    pub mint: String,
}

#[derive(Serialize)]
pub struct CompleteResponse {
    pub confirmed: bool,
}
