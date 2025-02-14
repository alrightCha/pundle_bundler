use axum::Json;
use solana_sdk::transaction::VersionedTransaction;
use std::sync::Arc;
use solana_sdk::address_lookup_table::AddressLookupTableAccount;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signer::Signer;  
use solana_sdk::signature::Keypair;
use solana_sdk::instruction::Instruction;
use solana_client::rpc_client::RpcClient;
use anchor_client::Cluster;
use solana_sdk::rent::Rent;
use crate::params::CreateTokenMetadata;
use tokio::time::Duration;
use std::collections::HashMap;

//Params needed for the handlers 
use crate::params::{
    PostBundleRequest, 
    PostBundleResponse, 
    GetBundleWalletsRequest, 
    GetBundleWalletsResponse, 
    BundleWallet
};

//My crates 
use crate::jito::jito::JitoBundle;
use crate::solana::grind::grind;
use crate::solana::lut::{create_lut, extend_lut, verify_lut_ready};
use crate::solana::utils::{create_keypair, transfer_ix, build_transaction, load_keypair};
use crate::config::{MAX_RETRIES, JITO_TIP_AMOUNT, RPC_URL, FEE_AMOUNT, BUFFER_AMOUNT};
use crate::pumpfun::pump::PumpFun;
use crate::solana::helper::pack_instructions;

//TODO : Sell all , Sell unique, Sell bulk

pub async fn health_check() -> &'static str {
    "Pundle, working"
}


#[derive(Debug)]
struct KeypairWithAmount {
    pub keypair: Keypair,
    pub amount: u64,
}


pub struct HandlerManager{
    jito: JitoBundle,
    admin_kp: Keypair
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
    pub async fn handle_post_bundle(&self,
        Json(payload): Json<PostBundleRequest>,
    ) -> Json<PostBundleResponse> {
        let client = RpcClient::new(RPC_URL);

        //Step 0: Initialize variables 

        let requester_pubkey = payload.requester_pubkey.clone();  

        //Creating mint keypair ending in pump 
        let mint_pubkey = grind(requester_pubkey.clone()).unwrap();
        
        let dev_keypair = create_keypair(&requester_pubkey).unwrap();
        
        let dev_keypair_path = format!("accounts/{}/{}.json", requester_pubkey, dev_keypair.pubkey());

        let loaded_dev_keypair = load_keypair(&dev_keypair_path).unwrap();

        let payer: Arc<Keypair> = Arc::new(loaded_dev_keypair);

        let token_metadata : CreateTokenMetadata = CreateTokenMetadata {
            name: payload.name,
            ticker: payload.symbol,
            uri: payload.uri
        };
        
        let mut pumpfun_client = PumpFun::new(
            Cluster::Mainnet,
            payer,
            Some(false)
        );

        let mint = load_keypair(&format!("accounts/{}/{}.json", requester_pubkey, mint_pubkey)).unwrap();

        //Preparing keypairs and respective amounts in sol 
        let dev_keypair_with_amount = KeypairWithAmount { keypair: dev_keypair, amount: payload.dev_buy_amount };
        
        //TODO: Break down wallets buy amount into array of newly generated keypairs with amount of lamports for each keypair 
        let keypairs_with_amount: Vec<KeypairWithAmount> = payload.wallets_buy_amount
            .iter()
            .map(|amount| KeypairWithAmount { keypair: create_keypair(&requester_pubkey)
            .unwrap(), amount: *amount })
            .collect();
        
        //STEP 1: Create and extend lut to spread solana across wallets 

        println!("Creating lut with admin public key:  {}", self.admin_kp.pubkey());


        let lut: (solana_sdk::pubkey::Pubkey, solana_sdk::signature::Signature) = create_lut(&client, &self.admin_kp).unwrap();
        //Addresses to extend with lut 
        let mut addresses: Vec<Pubkey> = keypairs_with_amount.iter().map(|keypair| keypair.keypair.pubkey()).collect();
        let without_dev_addresses = addresses.clone();
        //Add dev address to addresses 
        addresses.push(dev_keypair_with_amount.keypair.pubkey());

        let mut retries = 5;
        while retries > 0 {
            if verify_lut_ready(&client, &lut.0).unwrap() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
            retries -= 1;
        }

        if retries == 0 {
            print!("LUT not ready after maximum retries");
        }

        //Extend lut with addresses 
        let extended_lut = extend_lut(&client, &self.admin_kp, lut.0, &addresses).unwrap();


        println!("LUT extended with addresses: {:?}", extended_lut);
        //STEP 2: Transfer funds needed from admin to dev + keypairs in a bundle 

        println!("Amount of lamports to transfer to dev: {}", dev_keypair_with_amount.amount);
        let admin_to_dev_ix = transfer_ix(&self.admin_kp.pubkey(), &dev_keypair_with_amount.keypair.pubkey(), dev_keypair_with_amount.amount);
        let admin_to_keypair_ixs: Vec<Instruction> = keypairs_with_amount.iter().map(|keypair| transfer_ix(&self.admin_kp.pubkey(), &keypair.keypair.pubkey(), keypair.amount)).collect();
        let jito_tip_ix = self.jito.get_tip_ix(self.admin_kp.pubkey()).await.unwrap();

        //Instructions to send sol from admin to dev + keypairs 
        let mut instructions = admin_to_keypair_ixs;
        instructions.extend([admin_to_dev_ix, jito_tip_ix]);
        
        println!("LUT address: {:?}", lut.0);
        println!("Addresses: {:?}", addresses);

        let final_lut = AddressLookupTableAccount {
            key: lut.0,
            addresses: addresses.to_vec(),
        };

        let tx = build_transaction(&client, &instructions, vec![&self.admin_kp], final_lut);
        
        println!("Transaction built");
        //let signature = client.send_and_confirm_transaction_with_spinner(&tx).unwrap();

        //Sending transaction to fund wallets from admin. 
        //TODO: Check if this is complete. might require tip instruction, signature to tx, and confirmation that bundle is complete
        let _ = self.jito.one_tx_bundle(tx).await.unwrap();

        //Close lut - TODO add as side job to diminish waiting time for the user 
        //close_lut(&client, &self.admin_kp, lut.0);

        
        //Step 4: Create and extend lut for the bundle 

        let bundle_lut = create_lut(&client, &dev_keypair_with_amount.keypair).unwrap();
        let _ = extend_lut(&client, &dev_keypair_with_amount.keypair, bundle_lut.0, &without_dev_addresses).unwrap(); 

         // Verify LUT is ready before using
        let mut retries = 5;
        while retries > 0 {
            if verify_lut_ready(&client, &bundle_lut.0).unwrap(){
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
            retries -= 1;
        }

        if retries == 0 {
           println!("LUT not ready after maximum retries");
        }

        let final_bundle_lut = AddressLookupTableAccount {
            key: bundle_lut.0,
            addresses: without_dev_addresses.to_vec(),
        };

        //Step 5: Prepare mint instruction and buy instructions as well as tip instruction 

        let mut instructions: Vec<Instruction> = Vec::new();
        println!("Mint keypair: {:?}", mint.pubkey());

        let mint_ix = pumpfun_client.create_instruction(&mint, token_metadata).await.unwrap();

        instructions.push(mint_ix);

        //calculating the max amount of lamports to buy with 
        let rent = Rent::default();
        let rent_exempt_min = rent.minimum_balance(0);

        let to_subtract: u64 = rent_exempt_min + FEE_AMOUNT + BUFFER_AMOUNT;
       
        let to_sub_for_dev: u64 = to_subtract.clone() + JITO_TIP_AMOUNT;

        let dev_buy_ixs = pumpfun_client.buy_ixs(
            &mint.pubkey(),
            &dev_keypair_with_amount.keypair, 
            dev_keypair_with_amount.amount - to_sub_for_dev, 
            None, 
            None)
            .await
            .unwrap();

        instructions.extend(dev_buy_ixs);

        for keypair in keypairs_with_amount.iter() {
            let mint_pubkey: &Pubkey = &mint.pubkey();

            let buy_ixs = pumpfun_client.buy_ixs(
                mint_pubkey,
                &keypair.keypair, 
                keypair.amount - to_subtract, 
                None, 
                None)
                .await
                .unwrap();

            instructions.extend(buy_ixs);
        }

        //Step 6: Prepare tip instruction 

        let jito_tip_ix = self.jito.get_tip_ix(dev_keypair_with_amount.keypair.pubkey()).await.unwrap();
        instructions.push(jito_tip_ix);
        //Step 7: Bundle instructions into transactions
        
        let packed_txs = pack_instructions(instructions, &final_bundle_lut);
        println!("Packed transactions: {:?}", packed_txs.len());
        println!("Packed transactions. Needed keypairs for: {:?}", packed_txs[0].signers);
        println!("Packed transactions. Needed accounts for: {:?}", packed_txs[0].accounts);
        let mut transactions: Vec<VersionedTransaction> = Vec::new();
        // Create a map of pubkey to keypair for all possible signers
        let mut signers_map: HashMap<Pubkey, &Keypair> = HashMap::new();

        signers_map.insert(dev_keypair_with_amount.keypair.pubkey(), &dev_keypair_with_amount.keypair);

        for keypair in &keypairs_with_amount {
            signers_map.insert(keypair.keypair.pubkey(), &keypair.keypair);
        }
        
        signers_map.insert(mint.pubkey(), &mint);

        // Process each packed transaction
        for packed_tx in packed_txs {
            // Collect required signers' keypairs
            let mut signers = Vec::new();
            for pubkey in &packed_tx.signers {
                if let Some(kp) = signers_map.get(pubkey) {
                    print!("Adding for pubkey: {:?}", pubkey);
                    signers.push(*kp);
                } else {
                    panic!("Missing keypair for signer {}", pubkey);
                }
            }
            println!("Signers: {:?}", signers.iter().map(|kp| kp.pubkey()).collect::<Vec<Pubkey>>());
            // Build the transaction with the collected signers
            let tx = build_transaction(
                &client,
                &packed_tx.instructions,
                signers,
                final_bundle_lut.clone(),
            );
            transactions.push(tx);
        }

         // Send the bundle....
         let _ = self.jito.submit_bundle(transactions).await.unwrap();
    
        Json(PostBundleResponse {
            public_keys: keypairs_with_amount.iter().map(|keypair| keypair.keypair.pubkey().to_string()).collect(),
            dev_wallet: dev_keypair_with_amount.keypair.pubkey().to_string(),
            mint_pubkey: mint_pubkey,
        })
    }


    //Receive request to get a bundle
    // - Returns keypairs involved in the bundle 
    pub async fn get_bundle_wallets(&self,
        Json(payload): Json<GetBundleWalletsRequest>,
    ) -> Json<GetBundleWalletsResponse> {
        // TODO: Implement your bundle retrieval logic here
        // This is a placeholder response
        Json(GetBundleWalletsResponse {
            keypairs: vec![BundleWallet {
                pubkey: "sample_pubkey".to_string(),
                secret_key: "sample_secret_key".to_string(),
            }],
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