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
use solana_sdk::transaction::Transaction;
use solana_sdk::address_lookup_table::state::AddressLookupTable;
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::commitment_config::CommitmentLevel;
use std::collections::HashSet;
use std::time::Duration;

use crate::params::{
    CreateTokenMetadata,
    GetPoolInformationRequest, 
    PoolInformation, 
    SellAllRequest, 
    SellResponse, 
    UniqueSellRequest, 
    WithdrawAllSolRequest, 
    RecursivePayRequest, 
};
use crate::pumpfun::utils::get_splits;
use crate::pumpfun::pump::PumpFun;
use crate::solana::refund::refund_keypairs;
use crate::solana::recursive_pay::recursive_pay;
use crate::solana::utils::build_transaction;

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
use crate::solana::utils::{create_keypair, load_keypair, get_keypairs_for_pubkey};
use crate::config::{JITO_TIP_AMOUNT, MAX_RETRIES, RPC_URL, TOKEN_AMOUNT_MULTIPLIER};
use crate::jito::bundle::process_bundle;
use crate::solana::helper::pack_instructions;
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
        pubkey_to_lut: Arc<Mutex<HashMap<String, Pubkey>>>,
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
        
        println!("Wallet buy amount: {:?}", payload.wallets_buy_amount);
        let wallets_buy_amount = get_splits(payload.dev_buy_amount, payload.wallets_buy_amount);

        //Get split length and break if more than 12
        println!("Wallets buy amount length: {:?}", wallets_buy_amount.len());
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
                    println!("Inserting LUT for mint: {:?}", mint_pubkey);
                    println!("LUT: {:?}", lut);
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
        let mint = Pubkey::from_str(&payload.mint).unwrap();
        let loaded_admin_kp = Keypair::from_bytes(&self.admin_kp.to_bytes()).unwrap();
        let payer: Arc<Keypair> = Arc::new(loaded_admin_kp);
        
        let pumpfun_client = PumpFun::new(payer);

        let pool_information = pumpfun_client.get_pool_information(&mint).await.unwrap();

        Json(pool_information)
    }

    pub async fn sell_for_keypair(&self, 
        Json(payload): Json<UniqueSellRequest>,
    ) -> Json<SellResponse> {
        let requester: String = payload.pubkey;
        let mint: String = payload.mint;
        let amount: u64 = payload.amount * TOKEN_AMOUNT_MULTIPLIER;
        let wallet: String = payload.wallet;
        let mint_pubkey: Pubkey = Pubkey::from_str(&mint).unwrap();
        println!("Mint pubkey: {:?}", mint_pubkey);
        println!("Amount: {:?}", amount);
        println!("Wallet: {:?}", wallet);
        println!("Requester: {:?}", requester);
        let keypair = load_keypair(&format!("accounts/{}/{}.json", requester, wallet)).unwrap();
        
        let client = RpcClient::new(RPC_URL);

        let payer: Arc<Keypair> = Arc::new(keypair.insecure_clone());

        let pumpfun_client = PumpFun::new(payer);
        
        let sell_ixs = pumpfun_client.sell_ix(&mint_pubkey, &keypair, Some(amount), None, None).await.unwrap();

        let blockhash = client.get_latest_blockhash().unwrap();

        let tx = Transaction::new_signed_with_payer(
            &sell_ixs,
            Some(&keypair.pubkey()),
            &[&keypair],
            blockhash,
        );

        let config = RpcSendTransactionConfig {
            skip_preflight: true,
            preflight_commitment: Some(CommitmentLevel::Confirmed),
            encoding: None,
            max_retries: None,
            min_context_slot: None,
        };

        let signature = client.send_transaction_with_config(&tx, config).unwrap();
        client.confirm_transaction_with_commitment(&signature, CommitmentConfig::confirmed()).unwrap();

        println!("Signature: {:?}", signature);

        Json(SellResponse {
            success: true,
        })
    }

    //This function sells all leftover tokens for a given mint and deployer 
    pub async fn sell_all_leftover_tokens(&self,
        pubkey_to_lut: Arc<Mutex<HashMap<String, Pubkey>>>,
        Json(payload): Json<SellAllRequest>,
    ) -> Json<SellResponse> {
        let with_admin_transfer: bool = payload.admin;
        let requester: String = payload.pubkey;
        let mint: String = payload.mint;
        let mint_pubkey: Pubkey = Pubkey::from_str(&mint).unwrap();

        //Initializing pumpfun client & rpc client 
        let client = RpcClient::new(RPC_URL);

        let loaded_admin_kp = Keypair::from_bytes(&self.admin_kp.to_bytes()).unwrap();

        let payer: Arc<Keypair> = Arc::new(loaded_admin_kp.insecure_clone());

        let pumpfun_client = PumpFun::new(payer);

        // Get keypairs using the existing utility function and filter out the mint keypair
        let mut keypairs = match get_keypairs_for_pubkey(&requester) {
            Ok(kps) => kps,
            Err(e) => {
                eprintln!("Error getting keypairs: {}", e);
                Vec::new() // Return empty vector if there's an error
            }
        }; 

        let keypairs_no_mint: Vec<Keypair> = keypairs.iter().filter(|kp| kp.pubkey() != mint_pubkey).map(|kp| kp.insecure_clone()).collect();

        keypairs.push(loaded_admin_kp.insecure_clone());
        
        let mut instructions: Vec<Instruction> = Vec::new();

        for keypair in keypairs_no_mint.iter() {
            let sell_ixs = pumpfun_client.sell_all_ix(&mint_pubkey, &keypair).await.unwrap();
            // Clone the keypair and store it in the struct

            instructions.extend(sell_ixs);
        }

        let jito_tip_ix = self.jito.get_tip_ix(self.admin_kp.pubkey()).await.unwrap();

        instructions.push(jito_tip_ix);

        let unlocked_lut = pubkey_to_lut.lock().await;
        let lut_account_pubkey = unlocked_lut.get(&mint);

        let txs: Vec<VersionedTransaction> = match lut_account_pubkey {
            Some(lut_pubkey) => {
                println!("LUT pubkey FOUND ! : {:?}", lut_pubkey);
               let raw_account = client.get_account(&lut_pubkey).unwrap();
                let address_lookup_table = AddressLookupTable::deserialize(&raw_account.data).unwrap();
                let address_lookup_table_account = AddressLookupTableAccount {
                    key: *lut_pubkey,
                    addresses: address_lookup_table.addresses.to_vec(),
                };

                let packed_txs = pack_instructions(instructions, &address_lookup_table_account);
                let mut ready_txs: Vec<VersionedTransaction> = Vec::new();
                for tx in packed_txs {
                    let mut unique_signers: HashSet<Pubkey> = HashSet::new();
                    for ix in &tx.instructions {
                        for acc in ix.accounts.iter().filter(|acc| acc.is_signer) {
                            unique_signers.insert(acc.pubkey);
                        }
                    }

                    let mut tx_signers: Vec<&Keypair> = Vec::new();
                    for signer in unique_signers {
                        if let Some(kp) = keypairs.iter().find(|kp| kp.pubkey() == signer) {
                            tx_signers.push(kp);
                        }
                    }
                    
                    let tx = build_transaction(&client, &tx.instructions, tx_signers, address_lookup_table_account.clone());
                    ready_txs.push(tx);
                }
                ready_txs
            }
            None => {
                println!("LUT pubkey NOT FOUND !");
                return Json(SellResponse { success: false });
            }
        };

        let _ = self.jito.submit_bundle(txs, mint_pubkey, None).await.unwrap();

        if with_admin_transfer {
            tokio::time::sleep(Duration::from_secs(10)).await; //waiting for amounts to reach wallets 
            refund_keypairs(requester, self.admin_kp.pubkey().to_string(), mint).await;
        }

        Json(SellResponse {
            success: true,
        })
    }

    pub async fn withdraw_all_sol(&self, 
        Json(payload): Json<WithdrawAllSolRequest>,
    ) -> Json<SellResponse> {
        let requester: String = payload.pubkey;
        let mint: String = payload.mint;
        let to = Pubkey::from_str(&requester).unwrap();
        refund_keypairs(requester, to.to_string(), mint).await;
        Json(SellResponse {
            success: true,
        })
    }

    pub async fn pay_recursive(&self, 
        Json(payload): Json<RecursivePayRequest>,
    ) -> Json<SellResponse> {
        let requester: String = payload.pubkey;
        let mint: String = payload.mint;
        let lamports: u64 = payload.lamports;

        let result = recursive_pay(requester, mint, lamports).await;

        Json(SellResponse {
            success: result
        })
    }
}