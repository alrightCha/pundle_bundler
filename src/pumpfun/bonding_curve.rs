//! Bonding curve account for the Pump.fun Solana Program
//!
//! This module contains the definition for the bonding curve account.
//!
//! # Bonding Curve Account
//!
//! The bonding curve account is used to manage token pricing and liquidity.
//!
//! # Fields
//!
//! - `discriminator`: Unique identifier for the bonding curve
//! - `virtual_token_reserves`: Virtual token reserves used for price calculations
//! - `virtual_sol_reserves`: Virtual SOL reserves used for price calculations
//! - `real_token_reserves`: Actual token reserves available for trading
//! - `real_sol_reserves`: Actual SOL reserves available for trading
//! - `token_total_supply`: Total supply of tokens
//! - `complete`: Whether the bonding curve is complete/finalized
//!
//! # Methods
//!
//! - `new`: Creates a new bonding curve instance
//! - `get_buy_price`: Calculates the amount of tokens received for a given SOL amount
//! - `get_sell_price`: Calculates the amount of SOL received for selling tokens
//! - `get_market_cap_sol`: Calculates the current market cap in SOL
//! - `get_final_market_cap_sol`: Calculates the final market cap in SOL after all tokens are sold
//! - `get_buy_out_price`: Calculates the price to buy out all remaining tokens

use borsh::{BorshDeserialize, BorshSerialize};

/// Represents a bonding curve for token pricing and liquidity management
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct BondingCurveAccount {
    /// Unique identifier for the bonding curve
    pub discriminator: u64,
    /// Virtual token reserves used for price calculations
    pub virtual_token_reserves: u64,
    /// Virtual SOL reserves used for price calculations
    pub virtual_sol_reserves: u64,
    /// Total supply of tokens
    pub token_total_supply: u64,
    /// Whether the bonding curve is complete/finalized
    pub complete: bool,
}

impl BondingCurveAccount {
    /// Creates a new bonding curve instance
    ///
    /// # Arguments
    /// * `discriminator` - Unique identifier for the curve
    /// * `virtual_token_reserves` - Virtual token reserves for price calculations
    /// * `virtual_sol_reserves` - Virtual SOL reserves for price calculations
    /// * `real_token_reserves` - Actual token reserves available
    /// * `real_sol_reserves` - Actual SOL reserves available
    /// * `token_total_supply` - Total supply of tokens
    /// * `complete` - Whether the curve is complete

    pub fn default() -> Self {
        Self {
            discriminator: 6,
            virtual_token_reserves: 1_073_000_191,
            virtual_sol_reserves: 30_000_000_000,
            token_total_supply: 1_073_000_191,
            complete: false,
        }
    }


    /*
        let initSol = 30_000_000_000n
        let initSupply = 1_073_000_191n

     let untilNowBoughtSupply = 1_073_000_191n - 32_190_005_730n / (30_000_000_000n + untilNowBoughtSol.reduce((a, b) => a + b))
     let s = initSupply - untilNowBoughtSupply - (virtualSol * virtualTokenReserves) / virtualSol + amount + 1n;
    
    let virtualSol = 30_000_000_000
    // Calculate the product of virtual reserves
    // Calculate the new virtual sol reserves after the purchase
    // Calculate the new virtual token reserves after the purchase
    // Calculate the amount of tokens to be purchased

    return s;
    */
    /// Calculates the amount of tokens received for a given SOL amount
    ///
    /// # Arguments
    /// * `amount` - Amount of SOL to spend
    ///
    /// # Returns
    /// * `Ok(u64)` - Amount of tokens that would be received
    /// * `Err(&str)` - Error message if curve is complete
    /// 
    /// TODO: Make sure to cover the fees before passing the sol buy amount 
    pub fn get_buy_price(&mut self, sol_buy_amount: u64) -> Result<u64, &'static str> {
        if self.complete {
            return Err("Curve is complete");
        }

        if sol_buy_amount == 0 {
            return Ok(0);
        }

        // Calculate the product of virtual reserves using u128 to avoid overflow
        let n: u128 = (self.virtual_sol_reserves as u128) * (self.virtual_token_reserves as u128);

        // Calculate the new virtual sol reserves after the purchase
        let i: u128 = (self.virtual_sol_reserves as u128) + (sol_buy_amount as u128);

        // Calculate the new virtual token reserves after the purchase
        let r: u128 = n / i + 1;

        // Calculate the amount of tokens to be purchased
        let s: u128 = (self.virtual_token_reserves as u128) - r;

        // Convert back to u64 and return the minimum of calculated tokens and real reserves
        let s_u64 = s as u64;
        
        let tokens_out = match s_u64 < self.virtual_token_reserves {
            true => s_u64,
            false => self.virtual_token_reserves
        };

        //TODO: Remove tokens bought from virtual reserves
        self.virtual_token_reserves -= tokens_out;
        //TODO: Add sol bought to virtual reserves
        self.virtual_sol_reserves += sol_buy_amount;

        let tokens_out_decimals = tokens_out * 1_000_000;
        Ok(tokens_out_decimals)
    }


    /// Given a desired token amount (expressed with 6‑decimals,
    /// i.e. the value that `get_buy_price` returns), compute how much
    /// SOL (lamports) must be supplied to the curve.
    ///
    /// *Returns* the lamports required **after** updating the virtual
    /// reserves, or an error if the curve is complete or the request
    /// cannot be satisfied.
    pub fn get_sol_for_tokens(
        &mut self,
        token_amount_decimals: u64,
    ) -> Result<u64, &'static str> {
        if self.complete {
            return Err("Curve is complete");
        }
        if token_amount_decimals == 0 {
            return Ok(0);
        }

        // Convert the 6‑decimal input back to whole “base‑unit” tokens
        // (the same units that `virtual_token_reserves` use).
        let desired_tokens: u64 = token_amount_decimals
            .checked_div(1_000_000)
            .ok_or("Decimal conversion under‑flow")?;

        if desired_tokens == 0 {
            return Err("Requested amount is below 1 token base‑unit");
        }
        if desired_tokens >= self.virtual_token_reserves {
            return Err("Not enough liquidity in virtual reserves");
        }

        // Algebraic inverse of the maths in `get_buy_price`
        //
        //   n = V_s * V_t
        //   r = V_t - desired_tokens          (resulting virtual‑token reserves)
        //   n / (V_s + ΔS) + 1  = r
        //   ΔS = V_s * V_t / (r - 1)  - V_s
        //
        // where V_s = self.virtual_sol_reserves
        //       V_t = self.virtual_token_reserves

        let v_s: u128 = self.virtual_sol_reserves as u128;
        let v_t: u128 = self.virtual_token_reserves as u128;
        let r: u128   = v_t - (desired_tokens as u128); // ≥ 1 guaranteed above

        // Guard against r == 1 (would divide by zero ⇒ requester asked for
        // practically the entire supply).
        if r <= 1 {
            return Err("Requested amount exhausts the pool");
        }

        let numerator: u128   = v_s * v_t;
        let denominator: u128 = r - 1;
        let sol_needed: u128  = numerator
            .checked_div(denominator)
            .ok_or("Math error: division by zero")?
            .saturating_sub(v_s);

        let sol_needed_u64: u64 = sol_needed
            .try_into()
            .map_err(|_| "Required SOL exceeds u64")?;

        // Mutate reserves just like `get_buy_price`
        self.virtual_sol_reserves = self
            .virtual_sol_reserves
            .checked_add(sol_needed_u64)
            .ok_or("SOL reserve overflow")?;

        self.virtual_token_reserves = self
            .virtual_token_reserves
            .checked_sub(desired_tokens)
            .ok_or("Token reserve under‑flow")?;

        Ok(sol_needed_u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_bonding_curve() -> BondingCurveAccount {
        BondingCurveAccount::default()
    }

    #[test]
    fn test_bonding_curve_account() {
        let amount = 300000000;
        // Multiply first by 300 could overflow u64 since 300000000 * 300 > u64::MAX
        // Instead divide first to reduce intermediate value
        let res = amount - (amount / 10000 * 300);
        println!("Resp: {:?}", res); 
        let mut bonding_curve: BondingCurveAccount = get_bonding_curve();

        let buys = [50000000];

        for buy in buys {
            let tokens_out = bonding_curve.get_buy_price(buy).unwrap();
            println!("Tokens out: {}", tokens_out);
        }
    }
}