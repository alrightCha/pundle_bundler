use super::bonding_curve::BondingCurve;

pub fn get_splits(dev_buy: u64, amount: u64) -> Vec<u64> {
    let mut bonding_curve = BondingCurve::new();
    let _ = bonding_curve.get_buy_price(dev_buy).unwrap(); //Register dev buy to alter bonding curve account 
    let amount_to_buy = bonding_curve.get_buy_price(amount).unwrap(); // Total amount eligible to other wallets 
    
    const TOTAL_SUPPLY: u64 = 1_000_000_000;
    const MAX_WALLET_PERCENTAGE: f64 = 0.02; // 2%
    const MAX_PER_WALLET: u64 = (TOTAL_SUPPLY as f64 * MAX_WALLET_PERCENTAGE) as u64;

    let number_of_wallets = amount_to_buy / MAX_PER_WALLET;
    let mut remaining_amount = amount;
    let mut splits = Vec::new();

    // If only 1 wallet needed, return the full amount
    if number_of_wallets <= 1 {
        splits.push(amount);
        return splits;
    }

    // Generate random splits with natural deviations
    use rand::Rng;
    let mut rng = rand::thread_rng();

    // Keep track of total allocated amount
    let mut total_allocated = 0;

    for i in 0..number_of_wallets {
        let is_last = i == number_of_wallets - 1;
        
        if is_last {
            // Last wallet gets whatever is needed to reach exact amount
            let final_amount = amount - total_allocated;
            splits.push(final_amount);
        } else {
            // Calculate base amount per wallet
            let base_amount = remaining_amount / (number_of_wallets - i);
            
            // Add random deviation between -5% to +5%
            let deviation = (base_amount as f64 * (rng.gen_range(-5.0..5.0) / 100.0)) as u64;
            let split_amount = base_amount.saturating_add(deviation);
            
            // Ensure we don't exceed remaining amount and leave enough for other wallets
            let min_remaining = (number_of_wallets - i - 1) * (base_amount / 2); // Ensure minimum for remaining wallets
            let max_this_split = remaining_amount.saturating_sub(min_remaining);
            let split_amount = std::cmp::min(split_amount, max_this_split);
            
            splits.push(split_amount);
            remaining_amount = remaining_amount.saturating_sub(split_amount);
            total_allocated += split_amount;
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
    fn test_small_amount_single_split() {
        let dev_buy = 1000;
        let amount = 10_000; // Small amount that should result in single wallet
        let splits = get_splits(dev_buy, amount);
        
        assert_eq!(splits.len(), 1);
        assert_eq!(splits[0], amount);
    }

    #[test]
    fn test_large_amount_multiple_splits() {
        let dev_buy = 1000000000;
        let amount = 2000000000; // Large amount that should result in multiple wallets
        let splits = get_splits(dev_buy, amount);
        
        println!("Splits: {:?}", splits);
        let total = splits.iter().sum::<u64>();
        println!("Total: {:?}", total);
        assert_eq!(total, amount);
    }

    #[test]
    fn test_splits_distribution() {
        let dev_buy = 1000;
        let amount = 50_000_000;
        let splits = get_splits(dev_buy, amount);
        
        // Check that splits are somewhat evenly distributed
        if splits.len() > 1 {
            let avg = amount / splits.len() as u64;
            for split in splits.iter() {
                // Verify that no split deviates more than 10% from average
                let deviation = ((*split as f64 - avg as f64) / avg as f64).abs();
                assert!(deviation <= 0.10);
            }
        }
    }

    #[test]
    fn test_edge_case_max_wallet() {
        let dev_buy = 1000;
        let max_per_wallet = (1_000_000_000_u64 as f64 * 0.02) as u64;
        let splits = get_splits(dev_buy, max_per_wallet);
        
        // Should result in exactly one split
        assert_eq!(splits.len(), 1);
        assert_eq!(splits[0], max_per_wallet);
    }

    #[test]
    fn test_edge_case_zero_amount() {
        let dev_buy = 1000;
        let splits = get_splits(dev_buy, 0);
        
        assert_eq!(splits.len(), 1);
        assert_eq!(splits[0], 0);
    }
}

