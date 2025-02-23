use anyhow::{Result, anyhow};
use jito_sdk_rust::JitoJsonRpcSDK;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::Instruction, 
    pubkey::Pubkey, 
    system_instruction, 
    transaction::VersionedTransaction,
    commitment_config::CommitmentConfig,
};
use std::str::FromStr;
use serde_json::json;use base64::{Engine as _, engine::general_purpose};
use tokio::time::{sleep, Duration};
use super::utils::check_final_bundle_status;


pub struct JitoBundle {
    jito_sdk: JitoJsonRpcSDK,
    solana_rpc: RpcClient,
    max_retries: u32,
    jito_tip_amount: u64,
}

//should divide the transactions into bundles 
//Get tip instruction
//Send bundles one by one 
//Retry until all bundles are confirmed
//Return the bundle UUIDs
//Check if is busy with a bundle 

impl JitoBundle {
    pub fn new(solana_rpc: RpcClient, max_retries: u32, jito_tip_amount: u64) -> Self {
        let jito_sdk = JitoJsonRpcSDK::new("https://mainnet.block-engine.jito.wtf/api/v1", None);
        Self {
            jito_sdk,
            solana_rpc,
            max_retries,
            jito_tip_amount,
        }
    }

    pub async  fn get_tip_ix(&self, deployer_pubkey: Pubkey) -> Result<Instruction> {
        let random_tip_account = self.jito_sdk.get_random_tip_account().await?;
        let jito_tip_account = Pubkey::from_str(&random_tip_account)?;
        let jito_tip_ix = system_instruction::transfer(
            &deployer_pubkey,
            &jito_tip_account,
            self.jito_tip_amount,
        );
        Ok(jito_tip_ix)
    }

    //Requires that transaction is already signed 
    pub async fn one_tx_bundle(&self, transaction: VersionedTransaction) -> Result<()> {
        let serialized_tx = general_purpose::STANDARD.encode(bincode::serialize(&transaction)?);
        let serialized_tx_size = serialized_tx.len();
        println!("Transaction size in bytes: {}", serialized_tx_size);
        // Send transaction using Jito SDK
        println!("Sending transaction...");
        let params = json!({
            "tx": serialized_tx
        });
        let response = self.jito_sdk.send_txn(Some(params), true).await?;

            // Extract signature from response
        let signature = response["result"]
        .as_str()
        .ok_or_else(|| anyhow!("Failed to get signature from response"))?;
        println!("Transaction sent with signature: {}", signature);

        // Confirm transaction
        let confirmation = self.solana_rpc.confirm_transaction_with_spinner(
            &signature.parse()?,
            &self.solana_rpc.get_latest_blockhash()?,
            CommitmentConfig::finalized(),
        )?;
        println!("Transaction confirmed: {:?}", confirmation);

        println!("View transaction on Solscan: https://solscan.io/tx/{}", signature);
        Ok(())
    }


    //Processes a bundle and returns the bundle UUID when confirmed
    pub async fn submit_bundle(&self, transactions: Vec<VersionedTransaction>) -> Result<()> {
        // Serialize each transaction and encode it using bs58

        //TODO: Check if this step is necessary
        let serialized_txs: Vec<String> = transactions
        .into_iter()
        .map(|tx| bs58::encode(bincode::serialize(&tx).unwrap()).into_string())
        .collect();

        let bundle = json!(serialized_txs);
    
        // UUID for the bundle
        let uuid = None;
    
        // Send bundle using Jito SDK
         println!("Sending bundle with 1 transaction...");
         let response = self.jito_sdk.send_bundle(Some(bundle), uuid).await?;
     
         // Extract bundle UUID from response
         let bundle_uuid = response["result"]
             .as_str()
             .ok_or_else(|| anyhow!("Failed to get bundle UUID from response"))?;
         println!("Bundle sent with UUID: {}", bundle_uuid);
     
         let retry_delay = Duration::from_secs(5);
     
         for attempt in 1..=self.max_retries {
             println!("Checking bundle status (attempt {}/{})", attempt, self.max_retries);
     
             let status_response = self.jito_sdk.get_in_flight_bundle_statuses(vec![bundle_uuid.to_string()]).await?;
     
             if let Some(result) = status_response.get("result") {
                 if let Some(value) = result.get("value") {
                     if let Some(statuses) = value.as_array() {
                         if let Some(bundle_status) = statuses.get(0) {
                             if let Some(status) = bundle_status.get("status") {
                                 match status.as_str() {
                                     Some("Landed") => {
                                         println!("Bundle landed on-chain. Checking final status...");
                                         return check_final_bundle_status(&self.jito_sdk, bundle_uuid).await;
                                     },
                                     Some("Pending") => {
                                         println!("Bundle is pending. Waiting...");
                                     },
                                     Some(status) => {
                                         println!("Unexpected bundle status: {}. Waiting...", status);
                                     },
                                     None => {
                                         println!("Unable to parse bundle status. Waiting...");
                                     }
                                 }
                             } else {
                                 println!("Status field not found in bundle status. Waiting...");
                             }
                         } else {
                             println!("Bundle status not found. Waiting...");
                         }
                     } else {
                         println!("Unexpected value format. Waiting...");
                     }
                 } else {
                     println!("Value field not found in result. Waiting...");
    
                 }
             } else if let Some(error) = status_response.get("error") {
                 println!("Error checking bundle status: {:?}", error);
             } else {
                 println!("Unexpected response format. Waiting...");
             }
     
             if attempt < self.max_retries {
                 sleep(retry_delay).await;
             }
         }
     
         Err(anyhow!("Failed to confirm bundle status after {} attempts", self.max_retries))
     }
}