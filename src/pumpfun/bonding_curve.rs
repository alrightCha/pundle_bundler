use pumpfun::error::ClientError;

use crate::pumpfun::consts::{
    PROPORTION, 
    INITIAL_LAMPORTS_FOR_POOL, 
    TOTAL_TOKEN_SUPPLY, 
    INITIAL_TOKEN_SUPPLY
};

pub struct BondingCurve {
    pub reserve_token: u64,
    pub reserve_sol: u64,
    pub total_supply: u64
}

impl BondingCurve {
    pub fn new() -> Self {
        let total_supply = TOTAL_TOKEN_SUPPLY;
        let reserve_sol = INITIAL_LAMPORTS_FOR_POOL;
        let reserve_token = INITIAL_TOKEN_SUPPLY;
        Self { reserve_token, reserve_sol, total_supply }
    }
    
    pub fn get_buy_price(&mut self, amount: u64) -> Result<u64, ClientError> {
        let bought_amount = (self.total_supply as f64 - self.reserve_token as f64) / 1_000_000.0 / 1_000_000_000.0;
        let root_val = (PROPORTION as f64 * amount as f64 / 1_000_000_000.0 + bought_amount * bought_amount).sqrt();

        let amount_out_f64 = (root_val - bought_amount as f64) * 1_000_000.0 * 1_000_000_000.0;

        let amount_out = amount_out_f64.round() as u64;


        if amount_out > self.reserve_token {
            return Err(ClientError::RateLimitExceeded);
        }

        self.reserve_sol += amount;
        self.reserve_token -= amount_out;

        Ok(amount_out / 100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multiple_buys() {
        let mut curve = BondingCurve::new();
        
        // Test different buy amounts (in lamports)
        let test_amounts = vec![
            1_000_000_000,     // 1 SOL
            10_000_000_000,     // 2 SOL
            10_000_000_000,     // 5 SOL
            10_000_000_000,    // 10 SOL
        ];

        for amount in test_amounts {
            match curve.get_buy_price(amount) {
                Ok(tokens_out) => println!(
                    "Buy {} SOL -> Get {} tokens ({})", 
                    amount as f64 / 1_000_000_000.0,
                    tokens_out,
                    tokens_out as f64 / 1_000_000.0  // Show in millions
                ),
                Err(e) => println!("Error for {} SOL: {:?}", amount as f64 / 1_000_000_000.0, e),
            }
            
            // Print pool state after each trade
            println!(
                "Pool state - SOL: {} ({}), Tokens: {} ({})\n",
                curve.reserve_sol,
                curve.reserve_sol as f64 / 1_000_000_000.0,
                curve.reserve_token,
                curve.reserve_token as f64 / 1_000_000.0
            );
        }
    }
}

