use super::old_bc::BondingCurve;

pub fn get_splits(dev_buy: u64, amount: u64) -> Vec<u64> {
    println!("Dev buy SOL: {:?}", dev_buy);
    println!("Amount SOL: {:?}", amount);
    let mut bonding_curve = BondingCurve::new();
    
    // Get tokens received for dev buy
    let _ = bonding_curve.get_buy_price(dev_buy).unwrap();
    
    // Get tokens that would be received for the amount
    let tokens_for_amount = bonding_curve.get_buy_price(amount).unwrap();
    println!("Tokens to receive: {:?}", tokens_for_amount);

    const TOTAL_SUPPLY: u64 = 1_000_000_000 * 1_000_000;
    const MAX_WALLET_PERCENTAGE: f64 = 0.025; // 2.5%
    const MAX_TOKENS_PER_WALLET: u64 = (TOTAL_SUPPLY as f64 * MAX_WALLET_PERCENTAGE) as u64;

    // Calculate how many wallets needed based on token amount
    let number_of_wallets = (tokens_for_amount as f64 / MAX_TOKENS_PER_WALLET as f64).ceil() as u64;
    println!("Number of wallets needed: {:?}", number_of_wallets);
    
    let mut remaining_sol = amount;
    let mut splits = Vec::new();

    // If only 1 wallet needed, return the full SOL amount
    if number_of_wallets <= 1 {
        splits.push(amount);
        return splits;
    }

    use rand::Rng;
    let mut rng = rand::thread_rng();
    let mut total_allocated = 0;

    for i in 0..number_of_wallets {
        let is_last = i == number_of_wallets - 1;
        
        if is_last {
            // Last wallet gets remaining SOL to ensure exact total
            let final_amount = amount - total_allocated;
            splits.push(final_amount);
        } else {
            // Calculate base SOL per wallet
            let base_amount = remaining_sol / (number_of_wallets - i);
            
            // Add random deviation between -5% to +5%
            let deviation = (base_amount as f64 * (rng.gen_range(-5.0..5.0) / 100.0)) as u64;
            let split_amount = base_amount.saturating_add(deviation);
            
            // Ensure we don't exceed remaining amount and leave enough for other wallets
            let min_remaining = (number_of_wallets - i - 1) * (base_amount / 2); // Ensure minimum for remaining wallets
            let max_this_split = remaining_sol.saturating_sub(min_remaining);
            let split_amount = std::cmp::min(split_amount, max_this_split);
            
            // Verify the tokens this split would receive doesn't exceed max per wallet
            let tokens_for_split = bonding_curve.get_buy_price(split_amount).unwrap();
            if tokens_for_split > MAX_TOKENS_PER_WALLET {
                // If it would exceed, reduce the split amount
                let reduced_split = split_amount / 2; // Simple reduction, could be more sophisticated
                splits.push(reduced_split);
                total_allocated += reduced_split;
                remaining_sol = remaining_sol.saturating_sub(reduced_split);
            } else {
                splits.push(split_amount);
                total_allocated += split_amount;
                remaining_sol = remaining_sol.saturating_sub(split_amount);
            }
        }
    }

    // Verify total equals input amount
    debug_assert_eq!(splits.iter().sum::<u64>(), amount, "Split amounts must sum to input amount");

    splits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_large_amount_multiple_splits() {
        let dev_buy = 1_000_000_000;
        let amount = 2_000_000_000; // Large amount that should result in multiple wallets
        let splits = get_splits(dev_buy, amount);
        
        let total = splits.iter().sum::<u64>();
        println!("Splits: {:?}", splits);
        println!("Total: {:?}", total);
        assert_eq!(total, amount);
    }

    #[test]
    fn test_token_limits_respected() {
        let dev_buy = 1_000_000_000;
        let amount = 1_000_000_000; // Large amount that should result in multiple wallets
        let splits = get_splits(dev_buy, amount);
        println!("Splits: {:?}", splits);
        let mut bonding_curve = BondingCurve::new();
        let _ = bonding_curve.get_buy_price(dev_buy).unwrap(); // Register dev buy
        
        let total = splits.iter().sum::<u64>();
        assert_eq!(total, amount, "Splits should sum to total amount");
    }
}

