use pumpfun::error::ClientError;

pub const INITIAL_LAMPORTS_FOR_POOL: u64 = 10_000_000; // 0.01SOL
pub const PROPORTION: u64 = 1280; //  800M token is sold on 500SOL ===> (500 * 2 / 800) = 1.25 ===> 800 : 1.25 = 640 ====> 640 * 2 = 1280
pub const TOTAL_TOKEN_SUPPLY: u64 = 1_000_000_000_000_000_000;
pub const INITIAL_TOKEN_SUPPLY: u64 = 800_000_000_000_000_000;

pub struct BondingCurve {
    pub reserve_token: u64,
    pub reserve_sol: u64,
    pub total_supply: u64,
}

impl BondingCurve {
    pub fn new() -> Self {
        let total_supply = TOTAL_TOKEN_SUPPLY;
        let reserve_sol = INITIAL_LAMPORTS_FOR_POOL;
        let reserve_token = INITIAL_TOKEN_SUPPLY;
        Self {
            reserve_token,
            reserve_sol,
            total_supply,
        }
    }

    pub fn get_buy_price(&mut self, amount: u64) -> Result<u64, ClientError> {
        let bought_amount =
            (self.total_supply as f64 - self.reserve_token as f64) / 1_000_000.0 / 1_000_000_000.0;
        let root_val = (PROPORTION as f64 * amount as f64 / 1_000_000_000.0
            + bought_amount * bought_amount)
            .sqrt();

        let amount_out_f64 = (root_val - bought_amount as f64) * 1_000_000.0 * 1_000_000_000.0;

        let amount_out = amount_out_f64.round() as u64;

        if amount_out > self.reserve_token {
            return Err(ClientError::RateLimitExceeded);
        }

        self.reserve_sol += amount;
        self.reserve_token -= amount_out;

        let amount_out_decimals = amount_out / 100;

        Ok(amount_out_decimals)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]

    fn test_bonding_curve_account() {
        let mut bonding_curve: BondingCurve = BondingCurve::new();

        let buys = [
            1000000000, 1000000000, 1052061251, 994215416, 1082293166, 981632881, 981632881,
            1069224135, 959735067, 959735067, 959735068, 959735068,
        ];

        for buy in buys {
            let tokens_out = bonding_curve.get_buy_price(buy).unwrap();
            println!("Tokens out: {}", tokens_out);
        }
    }
}
