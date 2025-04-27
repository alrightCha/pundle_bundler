use crate::pumpfun::pump::PumpFun;
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose, Engine as _};
use jito_sdk_rust::JitoJsonRpcSDK;
use serde_json::{json, Value};
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    system_instruction,
    transaction::{Transaction, VersionedTransaction},
};
use std::str::FromStr;
use tokio::time::{sleep, Duration};

use super::utils::check_final_bundle_status;

pub struct JitoBundle {
    jito_sdk: JitoJsonRpcSDK,
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
    pub fn new(max_retries: u32, jito_tip_amount: u64) -> Self {
        let jito_sdk = JitoJsonRpcSDK::new("https://mainnet.block-engine.jito.wtf/api/v1", None);
        Self {
            jito_sdk,
            max_retries,
            jito_tip_amount,
        }
    }

    pub async fn get_tip_ix(
        &self,
        deployer_pubkey: Pubkey,
        tip_account: Option<Pubkey>,
    ) -> Result<Instruction> {
        let jito_tip_account: Pubkey = match tip_account {
            Some(tip_account) => tip_account,
            None => self.get_tip_account().await,
        };

        let jito_tip_ix =
            system_instruction::transfer(&deployer_pubkey, &jito_tip_account, self.jito_tip_amount);
        Ok(jito_tip_ix)
    }

    pub async fn get_custom_tip_ix(
        &self,
        deployer_pubkey: Pubkey,
        tip_account: Pubkey,
        amount: u64,
    ) -> Result<Instruction> {
        let jito_tip_ix = system_instruction::transfer(&deployer_pubkey, &tip_account, amount);
        Ok(jito_tip_ix)
    }

    pub async fn get_tip_account(&self) -> Pubkey {
        let random_tip_account = self.jito_sdk.get_random_tip_account().await.unwrap();
        Pubkey::from_str(&random_tip_account).unwrap()
    }

    pub async fn one_tx_sell(&self, transaction: Transaction) -> Result<String> {
        // Serialize the full transaction
        let serialized_tx = general_purpose::STANDARD.encode(bincode::serialize(&transaction)?);

        // Send transaction using Jito SDK
        println!("Sending transaction...");
        let params = json!([
            serialized_tx,
            {
                "encoding": "base64"
            }
        ]);

        let response = self.jito_sdk.send_txn(Some(params), true).await?;

        // Extract signature from response
        let signature = response["result"]
            .as_str()
            .ok_or_else(|| anyhow!("Failed to get signature from response"))?;
        println!("Transaction sent with signature: {}", signature);

        Ok(signature.to_string())
    }

    pub async fn submit_bundle(
        &self,
        transactions: Vec<VersionedTransaction>,
        mint: Pubkey,
        pumpfun_client: Option<&PumpFun>,
    ) -> Result<()> {
        let res = self
            .process_bundle(transactions.clone(), mint, pumpfun_client)
            .await;
        if res.is_err() {
            println!("Error processing bundle. Resubmitting...");
            if let Some(pumpfun_client) = pumpfun_client {
                let pool_info = pumpfun_client.get_pool_information(&mint).await;

                match pool_info {
                    Ok(_) => {
                        println!("Bonding curve found. Resubmitting bundle...");
                        return Ok(());
                    }
                    Err(e) => {
                        println!(
                            "Error getting pool information. Resubmitting bundle...: {:?}",
                            e
                        );
                        return Err(anyhow!(
                            "Error getting pool information. Cannot resubmit bundle."
                        ));
                    }
                }
            }
        }
        res
    }

    //Processes a bundle and returns the bundle UUID when confirmed
    pub async fn process_bundle(
        &self,
        transactions: Vec<VersionedTransaction>,
        mint: Pubkey,
        pumpfun_client: Option<&PumpFun>,
    ) -> Result<()> {
        // Serialize each transaction and encode it using bs58

        //TODO: Check if this step is necessary
        let serialized_txs: Vec<String> = transactions
            .into_iter()
            .map(|tx| general_purpose::STANDARD.encode(bincode::serialize(&tx).unwrap()))
            .collect();

            let bundle = json!([
                serialized_txs,
                {
                    "encoding": "base64"
                }
            ]);

        // UUID for the bundle
        let uuid = None;

        let response = self.jito_sdk.send_bundle(Some(bundle), uuid).await?;

        self.validate_bundle(pumpfun_client, mint, response).await
    }

    pub async fn validate_bundle(&self, pumpfun_client: Option<&PumpFun>, mint: Pubkey, response: Value) -> Result<()> {
        // Extract bundle UUID from response
        let bundle_uuid = response["result"]
            .as_str()
            .ok_or_else(|| anyhow!("Failed to get bundle UUID from response"))?;
        println!("Bundle sent with UUID: {}", bundle_uuid);

        let retry_delay = Duration::from_secs(2);

        let mut is_pending = false;

        for attempt in 1..=self.max_retries {
            println!(
                "Checking bundle status (attempt {}/{})",
                attempt, self.max_retries
            );

            let status_response = self.jito_sdk.get_in_flight_bundle_statuses(vec![bundle_uuid.to_string()]).await?;

            if let Some(result) = status_response.get("result") {
                if let Some(value) = result.get("value") {
                    if let Some(statuses) = value.as_array() {
                        if let Some(bundle_status) = statuses.get(0) {
                            if let Some(status) = bundle_status.get("status") {
                                match status.as_str() {
                                    Some("Landed") => {
                                        println!(
                                            "Bundle landed on-chain. Checking final status..."
                                        );
                                        return check_final_bundle_status(
                                            &self.jito_sdk,
                                            bundle_uuid,
                                        )
                                        .await;
                                    }
                                    Some("Pending") => {
                                        println!("Bundle is pending. Waiting...");
                                        if !is_pending {
                                            is_pending = true;
                                        }
                                    }
                                    Some("Invalid") => {
                                        //Look for bonding curve if available or not
                                        //If not available, return error and resubmit bundle
                                        println!("Bundle is invalid. Waiting...");
                                        if is_pending {
                                            match pumpfun_client {
                                                Some(pumpfun_client) => {
                                                    let bonding_curve = pumpfun_client
                                                        .get_pool_information(&mint)
                                                        .await;
                                                    if bonding_curve.is_ok() {
                                                        println!("Bonding curve found. Resubmitting bundle...");
                                                        //No need to resubmit bundle, end here
                                                        return Ok(());
                                                    } else {
                                                        println!("No bonding curve found. Resubmitting bundle...");
                                                        return Err(anyhow!("No bonding curve found. Cannot resubmit bundle."));
                                                    }
                                                }
                                                None => {
                                                    //Invalid and not a token launch, resubmit bundle
                                                    println!("Invalid and not a token launch. Resubmitting bundle...");
                                                    return Err(anyhow!("Invalid and not a token launch. Cannot resubmit bundle."));
                                                }
                                            }
                                        }
                                    }
                                    Some(status) => {
                                        println!(
                                            "Unexpected bundle status: {}. Waiting...",
                                            status
                                        );
                                    }
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
        Err(anyhow!(
            "Failed to confirm bundle status after {} attempts",
            self.max_retries
        ))
    }
}
