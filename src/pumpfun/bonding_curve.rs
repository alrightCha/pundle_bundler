use pumpfun::error::ClientError;
use crate::config::LAMPORTS_PER_SOL;

pub struct BondingCurve {
    pub tokens_bought: u64,
    pub reserve_sol: u64,
}

impl BondingCurve {
    pub fn new() -> Self {
        let sol_in_lamports = 30 * LAMPORTS_PER_SOL;
        Self { tokens_bought: 0, reserve_sol: sol_in_lamports }
    }
    
    pub fn get_buy_price(&mut self, lamports_amount: u64) -> Result<u64, ClientError> {
        if self.tokens_bought >= 793100000 {
            return Ok(0);
        }

        let sol_amount = lamports_amount as f64 / LAMPORTS_PER_SOL as f64;
        
        let eligible_amount = 1073000191 - self.tokens_bought - 32190005730 / ((self.reserve_sol as f64 /LAMPORTS_PER_SOL as f64 + sol_amount) as u64);
        let until_now_bought = self.tokens_bought.clone();
        self.tokens_bought += eligible_amount;
        self.reserve_sol += lamports_amount;

        if eligible_amount + self.tokens_bought > 793100000 {
            self.tokens_bought = 793100000;
            self.reserve_sol = 85 * LAMPORTS_PER_SOL;
            return Ok(793100000 - until_now_bought);
        }

        Ok(eligible_amount) //Conver to decimals 6 for spl tokens
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
            90000000,     // 2 SOL
            1000000000,     // 5 SOL
            1000000000,    // 10 SOL
        ];

        for amount in test_amounts {
            match curve.get_buy_price(amount) {
                Ok(tokens_out) => {
                    let num_digits = tokens_out.to_string().len();
                    println!(
                        "Buy {} SOL -> Get ({}) tokens with {} digits", 
                        amount as f64 / LAMPORTS_PER_SOL as f64,
                        tokens_out,
                        num_digits
                    );
                },
                Err(e) => println!("Error for {} SOL: {:?}", amount as f64 / LAMPORTS_PER_SOL as f64, e),
            }
        }
    }
}

