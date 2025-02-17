use axum::Json;
use solana_sdk::transaction::VersionedTransaction;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signer::Signer;  
use solana_sdk::signature::Keypair;
use solana_sdk::instruction::Instruction;
use solana_client::rpc_client::RpcClient;
use crate::params::{CreateTokenMetadata, GetPoolInformationRequest, PoolInformation, SellAllRequest, UniqueSellRequest, SellResponse};
use crate::pumpfun::utils::get_splits;
use crate::pumpfun::pump::PumpFun;
use crate::solana::refund::refund_keypairs;
use tokio::spawn;

//Params needed for the handlers 
use crate::params::{
    PostBundleRequest, 
    PostBundleResponse, 
    GetBundleWalletsRequest, 
    GetBundleWalletsResponse, 
    BundleWallet,
    Wallet,
    KeypairWithAmount
};

//My crates 
use crate::jito::jito::JitoBundle;
use crate::solana::grind::grind;
use crate::solana::utils::{create_keypair, build_transaction, load_keypair, get_keypairs_for_pubkey};
use crate::config::{JITO_TIP_AMOUNT, MAX_RETRIES, RPC_URL, TOKEN_AMOUNT_MULTIPLIER};
use crate::solana::helper::pack_instructions;
use crate::jito::bundle::process_bundle;
use std::collections::HashMap;
use solana_sdk::address_lookup_table::AddressLookupTableAccount;


//TODO : Sell all , Sell unique, Sell bulk
pub async fn health_check() -> &'static str {
    "Pundle, working"
}


pub struct HandlerManager{
    jito: JitoBundle,
    admin_kp: Keypair,
}

impl HandlerManager {
    pub fn new(admin_kp: Keypair) -> Self {
        //setup Jito Client 
        let client = RpcClient::new(RPC_URL);
        let jito = JitoBundle::new(client, MAX_RETRIES, JITO_TIP_AMOUNT);
        Self {  jito, admin_kp }
    }

    //Receive request to create a bundle
    // - Requester pubkey
    // - metadata for token
    // - amount of SOL to buy
    // - Dev buy amount of SOL 
    // - wallet count 
    // -> Generates wallets, 
    // -> Funds them, 
    // -> create lut, 
    // -> adds addresses to lut, 
    // -> bundle launch token, 
    // -> make sure it is complete, 
    // -> close lut, 
    // -> map requester to keypairs, 
    // -> return array of public keys
    pub async fn handle_post_bundle(&mut self,
        pubkey_to_lut: Arc<Mutex<HashMap<String, AddressLookupTableAccount>>>,
        Json(payload): Json<PostBundleRequest>,
    ) -> Json<PostBundleResponse> {
        println!("Received payload with wallets buy amount: {:?}", payload.wallets_buy_amount);

        //Step 0: Initialize variables 

        let requester_pubkey = payload.requester_pubkey.clone();  
        //Creating mint keypair ending in pump 
        let mint_pubkey = grind(requester_pubkey.clone()).unwrap();
        
        let dev_keypair = create_keypair(&requester_pubkey).unwrap();
        
        let token_metadata : CreateTokenMetadata = CreateTokenMetadata {
            name: payload.name,
            ticker: payload.symbol,
            uri: payload.uri
        };

        let mint = load_keypair(&format!("accounts/{}/{}.json", requester_pubkey, mint_pubkey)).unwrap();

        //Preparing keypairs and respective amounts in sol 
        let dev_keypair_with_amount = KeypairWithAmount { keypair: dev_keypair, amount: payload.dev_buy_amount };
        
        let wallets_buy_amount = get_splits(payload.dev_buy_amount, payload.wallets_buy_amount);
        println!("Wallets buy amount: {:?}", wallets_buy_amount);
        //TODO: Break down wallets buy amount into array of newly generated keypairs with amount of lamports for each keypair 
        let keypairs_with_amount: Vec<KeypairWithAmount> = wallets_buy_amount
            .iter()
            .map(|amount| KeypairWithAmount { keypair: create_keypair(&requester_pubkey)
            .unwrap(), amount: *amount })
            .collect();
        
        //STEP 1: Create and extend lut to spread solana across wallets 

        let mut wallets: Vec<Wallet> = keypairs_with_amount.iter().map(|keypair| Wallet {
            pubkey: keypair.keypair.pubkey().to_string(),
            secret_key: bs58::encode(keypair.keypair.to_bytes()).into_string(),
            is_dev: false,
            amount: keypair.amount,
        }).collect();

        wallets.push(Wallet {
            pubkey: dev_keypair_with_amount.keypair.pubkey().to_string(),
            secret_key: bs58::encode(dev_keypair_with_amount.keypair.to_bytes()).into_string(),
            is_dev: true,
            amount: dev_keypair_with_amount.amount,
        });

        let response = PostBundleResponse {
            pubkey: requester_pubkey,
            mint: mint.pubkey().to_string(),
            due_amount: payload.wallets_buy_amount + payload.dev_buy_amount,
            wallets,
        };
    
        // Spawn background processing of bundle in a separate task
        spawn(async move {
            match process_bundle(keypairs_with_amount, dev_keypair_with_amount, mint, payload.requester_pubkey, token_metadata).await {
                Ok(lut) => {
                    pubkey_to_lut.lock().await.insert(mint_pubkey, lut);
                },
                Err(e) => {
                    eprintln!("Error processing bundle for mint {}: {}", mint_pubkey, e);
                }
            }
        });

        println!("Response: {:?}", response);
        Json(response)
    }


    //Receive request to get a bundle
    // - Returns keypairs involved in the bundle 
    pub async fn get_bundle_wallets(&self,
        Json(payload): Json<GetBundleWalletsRequest>,
    ) -> Json<GetBundleWalletsResponse> {
        let requester_pubkey = payload.requester_pubkey;
        
        // Get keypairs using the existing utility function
        let keypairs = match get_keypairs_for_pubkey(&requester_pubkey) {
            Ok(kps) => kps,
            Err(e) => {
                eprintln!("Error getting keypairs: {}", e);
                Vec::new() // Return empty vector if there's an error
            }
        };

        // Convert keypairs to BundleWallet format
        let bundle_wallets: Vec<BundleWallet> = keypairs.iter().map(|kp| {
            BundleWallet {
                pubkey: kp.pubkey().to_string(),
                secret_key: bs58::encode(kp.to_bytes()).into_string(),
            }
        }).collect();

        Json(GetBundleWalletsResponse {
            keypairs: bundle_wallets,
        })
    }

    //Get pool information for given token 
    pub async fn get_pool_information(&self,
        Json(payload): Json<GetPoolInformationRequest>,
    ) -> Json<PoolInformation> {
        println!("Received payload: {:?}", payload);
        let mint = Pubkey::from_str(&payload.mint).unwrap();
        let loaded_admin_kp = Keypair::from_bytes(&self.admin_kp.to_bytes()).unwrap();
        let payer: Arc<Keypair> = Arc::new(loaded_admin_kp);
        
        let pumpfun_client = PumpFun::new(payer);

        let pool_information = pumpfun_client.get_pool_information(&mint).await.unwrap();

        Json(pool_information)
    }

    pub async fn sell_for_keypair(&self, 
        pubkey_to_lut: Arc<Mutex<HashMap<String, AddressLookupTableAccount>>>,
        Json(payload): Json<UniqueSellRequest>,
    ) -> Json<SellResponse> {
        let requester: String = payload.pubkey;
        let mint: String = payload.mint;
        let amount: u64 = payload.amount * TOKEN_AMOUNT_MULTIPLIER;
        let wallet: String = payload.wallet;
        let mint_pubkey: Pubkey = Pubkey::from_str(&mint).unwrap();
        
        let keypair = load_keypair(&format!("accounts/{}/{}.json", requester, wallet)).unwrap();
        
        let client = RpcClient::new(RPC_URL);

        let payer: Arc<Keypair> = Arc::new(keypair.insecure_clone());

        let pumpfun_client = PumpFun::new(payer);
        
        let sell_ixs = pumpfun_client.sell_ix(&mint_pubkey, &keypair, Some(amount), None, None).await.unwrap();

        let lut = pubkey_to_lut.lock().await;
        let lut_account = lut.get(&mint).ok_or("LUT not found for this mint").unwrap();
        let tx = build_transaction(&client, &sell_ixs, vec![&keypair], lut_account.clone());

        let _ = self.jito.one_tx_bundle(tx).await.unwrap();

        Json(SellResponse {
            success: true,
        })
    }

    //This function sells all leftover tokens for a given mint and deployer 
    pub async fn sell_all_leftover_tokens(&self,
        pubkey_to_lut: Arc<Mutex<HashMap<String, AddressLookupTableAccount>>>,
        Json(payload): Json<SellAllRequest>,
    ) -> Json<SellResponse> {
        let with_admin_transfer: bool = payload.with_admin_transfer;
        let requester: String = payload.owner_pubkey;
        let mint: String = payload.token_mint;
        let mint_pubkey: Pubkey = Pubkey::from_str(&mint).unwrap();

        //Initializing pumpfun client & rpc client 
        let client = RpcClient::new(RPC_URL);

        let loaded_admin_kp = Keypair::from_bytes(&self.admin_kp.to_bytes()).unwrap();
        let payer: Arc<Keypair> = Arc::new(loaded_admin_kp);

        let pumpfun_client = PumpFun::new(payer);

        // Get keypairs using the existing utility function and filter out the mint keypair
        let keypairs = match get_keypairs_for_pubkey(&requester) {
            Ok(kps) => kps.into_iter()
                         .filter(|kp| kp.pubkey() != mint_pubkey)
                         .collect(),
            Err(e) => {
                eprintln!("Error getting keypairs: {}", e);
                Vec::new() // Return empty vector if there's an error
            }
        };

        let mut instructions: Vec<Instruction> = Vec::new();

        for keypair in keypairs {
            let sell_ixs = pumpfun_client.sell_all_ix(&mint_pubkey, &keypair).await.unwrap();
            instructions.extend(sell_ixs);
        }

        let unlocked_lut = pubkey_to_lut.lock().await;
        let lut_account = unlocked_lut.get(&mint).ok_or("LUT not found for this mint").unwrap();
        let packed_ixs = pack_instructions(instructions, lut_account);

        let txs: Vec<VersionedTransaction> = packed_ixs.iter().map(|ix| build_transaction(&client, &ix.instructions, vec![&self.admin_kp], lut_account.clone())).collect();

        let _ = self.jito.submit_bundle(txs).await.unwrap();

        if with_admin_transfer {
            refund_keypairs(requester, mint).await;
        }

        Json(SellResponse {
            success: true,
        })
    }

    //Receive request to sell a token 
    // Must check if the amount is valid, and if the user has paid for the bundle concerning this keypair
    // - Requester pubkey
    // - token address
    // - amount of tokens to sell 

    //Receive request to sell all tokens 
    // Must check if the amount is valid, and if the user has paid for the bundle concerning this keypair
    // - Requester pubkey
    // - token address
    // - amount of tokens to sell 

    //Receive request to sell unique tokens 
    // Must check if the amount is valid, and if the user has paid for the bundle concerning this keypair
    // - Requester pubkey
    // - token address
    // - amount of tokens to sell 

    //Receive request to sell in bulk 
}