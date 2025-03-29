use crate::jupiter::swap::swap_ixs;
use crate::params::{
    BumpRequest, BumpResponse, GetPoolInformationRequest, LutInit, LutRecord, LutResponse,
    PoolInformation, RecursivePayRequest, SellAllRequest, SellResponse, UniqueSellRequest,
    WithdrawAllSolRequest,
};
use crate::pumpfun::pump::PumpFun;
use crate::pumpfun::utils::get_splits;
use crate::solana::bump::Bump;
use crate::solana::recursive_pay::recursive_pay;
use anchor_spl::associated_token::get_associated_token_address;
use axum::Json;
use pumpfun_cpi::instruction::Create;
use solana_client::rpc_client::RpcClient;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::system_instruction;
use solana_sdk::transaction::Transaction;
use solana_sdk::transaction::VersionedTransaction;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::spawn;
use tokio::sync::Mutex;

//Params needed for the handlers
use crate::params::{
    BundleWallet, GetBundleWalletsRequest, GetBundleWalletsResponse, KeypairWithAmount,
    PostBundleRequest, PostBundleResponse, Wallet,
};

//My crates
use crate::config::{JITO_TIP_AMOUNT, MAX_RETRIES, RPC_URL, TOKEN_AMOUNT_MULTIPLIER};
use crate::jito::bundle::process_bundle;
use crate::jito::jito::JitoBundle;
use crate::solana::grind::grind;
use crate::solana::helpers::sell_all_txs;
use crate::solana::utils::{create_keypair, get_keypairs_for_pubkey, load_keypair};
use std::collections::HashMap;

//TODO : Sell all , Sell unique, Sell bulk
pub async fn health_check() -> &'static str {
    "Pundle, working"
}

pub struct HandlerManager {
    admin_kp: Keypair,
}

impl HandlerManager {
    pub fn new(admin_kp: Keypair) -> Self {
        //setup Jito Client
        Self { admin_kp }
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
    pub async fn handle_post_bundle(
        &mut self,
        pubkey_to_lut: Arc<Mutex<HashMap<String, Pubkey>>>,
        Json(payload): Json<PostBundleRequest>,
    ) -> Json<PostBundleResponse> {
        println!(
            "Received payload with wallets buy amount: {:?}",
            payload.wallets_buy_amount
        );

        //Step 0: Initialize variables

        let requester_pubkey = payload.requester_pubkey.clone();
        //Creating mint keypair ending in pump
        let mint_pubkey = grind(requester_pubkey.clone()).unwrap();

        let dev_keypair = create_keypair(&requester_pubkey, &mint_pubkey).unwrap();

        let token_metadata = Create {
            _name: payload.name,
            _symbol: payload.symbol,
            _uri: payload.uri,
            _creator: dev_keypair.pubkey(),
        };

        let mint = load_keypair(&format!(
            "accounts/{}/{}/{}.json",
            requester_pubkey, mint_pubkey, mint_pubkey
        ))
        .unwrap();

        //Preparing keypairs and respective amounts in sol
        let dev_keypair_with_amount = KeypairWithAmount {
            keypair: dev_keypair,
            amount: payload.dev_buy_amount,
        };

        println!("Wallet buy amount: {:?}", payload.wallets_buy_amount);
        let wallets_buy_amount = get_splits(payload.dev_buy_amount, payload.wallets_buy_amount);

        //Get split length and break if more than 12
        println!("Wallets buy amount length: {:?}", wallets_buy_amount.len());
        println!("Wallets buy amount: {:?}", wallets_buy_amount);
        //TODO: Break down wallets buy amount into array of newly generated keypairs with amount of lamports for each keypair
        let keypairs_with_amount: Vec<KeypairWithAmount> = wallets_buy_amount
            .iter()
            .map(|amount| KeypairWithAmount {
                keypair: create_keypair(&requester_pubkey, &mint_pubkey).unwrap(),
                amount: *amount,
            })
            .collect();

        //STEP 1: Create and extend lut to spread solana across wallets

        let mut wallets: Vec<Wallet> = keypairs_with_amount
            .iter()
            .map(|keypair| Wallet {
                pubkey: keypair.keypair.pubkey().to_string(),
                secret_key: bs58::encode(keypair.keypair.to_bytes()).into_string(),
                is_dev: false,
                amount: keypair.amount,
            })
            .collect();

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
            match process_bundle(
                keypairs_with_amount,
                dev_keypair_with_amount,
                &mint,
                payload.requester_pubkey,
                token_metadata,
            )
            .await
            {
                Ok(lut) => {
                    println!("Inserting LUT for mint: {:?}", mint_pubkey);
                    println!("LUT: {:?}", lut);
                    pubkey_to_lut.lock().await.insert(mint_pubkey, lut);
                }
                Err(e) => {
                    eprintln!("Error processing bundle for mint {}: {}", mint_pubkey, e);
                }
            }
        });

        println!("Response: {:?}", response);
        Json(response)
    }

    //Get pool information for given token
    pub async fn get_pool_information(
        &self,
        Json(payload): Json<GetPoolInformationRequest>,
    ) -> Json<PoolInformation> {
        let mint = Pubkey::from_str(&payload.mint).unwrap();
        let loaded_admin_kp = Keypair::from_bytes(&self.admin_kp.to_bytes()).unwrap();
        let payer: Arc<Keypair> = Arc::new(loaded_admin_kp);

        let pumpfun_client = PumpFun::new(payer);

        let pool_information = pumpfun_client.get_pool_information(&mint).await.unwrap();

        Json(pool_information)
    }

    pub async fn sell_for_keypair(
        &self,
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
        let keypair = load_keypair(&format!(
            "accounts/{}/{}/{}.json",
            requester, mint_pubkey, wallet
        ))
        .unwrap();

        let client = RpcClient::new(RPC_URL);

        let payer: Arc<Keypair> = Arc::new(keypair.insecure_clone());

        tokio::spawn(async move {
            let pumpfun_client = PumpFun::new(payer);
            let bonded = pumpfun_client
                .get_pool_information(&mint_pubkey)
                .await
                .unwrap();

            let sell_ixs: Vec<Instruction> = match bonded.is_bonding_curve_complete {
                true => {
                    let swap_ixs = swap_ixs(&keypair, mint_pubkey, amount, None).await.unwrap();
                    swap_ixs
                }
                false => {
                    let pump_ixs = pumpfun_client
                        .sell_ix(&mint_pubkey, &keypair, Some(amount), None, None)
                        .await
                        .unwrap();
                    pump_ixs
                }
            };

            let blockhash = client.get_latest_blockhash().unwrap();

            let jito = JitoBundle::new(client, MAX_RETRIES, JITO_TIP_AMOUNT);
            let tip_ix = jito.get_tip_ix(keypair.pubkey()).await.unwrap();

            let mut instructions: Vec<Instruction> = Vec::new();
            instructions.extend(sell_ixs);
            instructions.push(tip_ix);

            let mut transaction = Transaction::new_signed_with_payer(
                &instructions,
                Some(&keypair.pubkey()),
                &[&keypair],
                blockhash,
            );

            // Sign Transaction
            transaction.sign(&[&keypair], blockhash);

            let signature = jito.one_tx_sell(transaction).await.unwrap();
            println!("Signature: {:?}", signature);
        });

        Json(SellResponse { success: true })
    }

    //This function sells all leftover tokens for a given mint and deployer
    pub async fn sell_all_leftover_tokens(
        &self,
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
        let mut keypairs = match get_keypairs_for_pubkey(&requester, &mint) {
            Ok(kps) => kps,
            Err(e) => {
                eprintln!("Error getting keypairs: {}", e);
                Vec::new() // Return empty vector if there's an error
            }
        };

        //Check total balance across wallets
        let mut total_token_balance: f64 = 0.0;
        for keypair in keypairs.iter() {
            if keypair.pubkey() == mint_pubkey {
                continue;
            }

            let ata: Pubkey = get_associated_token_address(&keypair.pubkey(), &mint_pubkey);
            let balance = client.get_token_account_balance(&ata);
            match balance {
                Ok(ui_amount) => {
                    if let Some(ui_amount) = ui_amount.ui_amount {
                        total_token_balance += ui_amount
                    }
                }
                Err(_) => println!("Token account balance not found"),
            }
        }

        println!("Total token balance: {:?}", total_token_balance);

        if total_token_balance < 1000.0 && !with_admin_transfer {
            return Json(SellResponse { success: false });
        }

        let keypairs_no_mint: Vec<Keypair> = keypairs
            .iter()
            .filter(|kp| kp.pubkey() != mint_pubkey)
            .map(|kp| kp.insecure_clone())
            .collect();

        keypairs.push(loaded_admin_kp.insecure_clone());

        let unlocked_lut = pubkey_to_lut.lock().await;
        let lut_account_pubkey = unlocked_lut.get(&mint);

        let txs: Vec<VersionedTransaction> = match lut_account_pubkey {
            Some(lut_pubkey) => {
                let result = sell_all_txs(
                    loaded_admin_kp,
                    keypairs_no_mint.iter().map(|kp| kp).collect(),
                    &mint_pubkey,
                    *lut_pubkey,
                    pumpfun_client,
                    client,
                )
                .await;
                result
            }
            None => {
                println!("LUT pubkey NOT FOUND !");
                return Json(SellResponse { success: false });
            }
        };

        if total_token_balance > 1000.0 {
            let client = RpcClient::new(RPC_URL);
            let jito = JitoBundle::new(client, MAX_RETRIES, JITO_TIP_AMOUNT);
            let chunks: Vec<_> = txs.chunks(5).collect();
            for chunk in chunks {
                let chunk_vec = chunk.to_vec();
                let _ = jito.submit_bundle(chunk_vec, mint_pubkey, None).await;
            }
        }

        if with_admin_transfer {
            let res = recursive_pay(requester, mint, None, true).await;
            Json(SellResponse { success: res })
        } else {
            Json(SellResponse { success: true })
        }
    }

    pub async fn withdraw_all_sol(
        &self,
        Json(payload): Json<WithdrawAllSolRequest>,
    ) -> Json<SellResponse> {
        let requester: String = payload.pubkey;
        let mint: String = payload.mint;
        tokio::spawn(async move {
            let _ = recursive_pay(requester, mint, None, false).await;
        });
        Json(SellResponse { success: true })
    }

    pub async fn pay_recursive(
        &self,
        Json(payload): Json<RecursivePayRequest>,
    ) -> Json<SellResponse> {
        let requester: String = payload.pubkey;
        let mint: String = payload.mint;
        let lamports: u64 = payload.lamports;

        let result = recursive_pay(requester, mint, Some(lamports), true).await;

        Json(SellResponse { success: result })
    }

    pub async fn bump_token(&self, Json(payload): Json<BumpRequest>) -> Json<BumpResponse> {
        let mint_address = payload.mint_address;
        let lamports: u64 = payload.lamports;
        let mut bump: Bump = Bump::new(mint_address);

        let new_kp: Pubkey = bump.get_funding_pubkey();
        let clone_admin: Keypair = self.admin_kp.insecure_clone();
        let fund_ix = system_instruction::transfer(&self.admin_kp.pubkey(), &new_kp, lamports);

        tokio::spawn(async move {
            bump.send_tx(vec![fund_ix], vec![&clone_admin]).await;
            let _ = bump.bump();
        });

        Json(BumpResponse { success: true })
    }

    pub async fn setup_lut_record(
        &self,
        pubkey_to_lut: Arc<Mutex<HashMap<String, Pubkey>>>,
        Json(payload): Json<LutInit>,
    ) -> Json<LutResponse> {
        println!("Received init lut");
        println!("with records: {:?}", payload.luts.len());
        let luts: Vec<LutRecord> = payload.luts;
        let mut all_successful = true;

        for lut_record in luts {
            match Pubkey::from_str(&lut_record.lut) {
                Ok(lut_pubkey) => {
                    if pubkey_to_lut
                        .lock()
                        .await
                        .insert(lut_record.mint, lut_pubkey)
                        .is_none()
                    {
                        // Insert succeeded
                    } else {
                        // Key already existed, still counts as successful
                    }
                }
                Err(_) => {
                    all_successful = false;
                    break;
                }
            }
        }

        Json(LutResponse {
            confirmed: all_successful,
        })
    }
}
