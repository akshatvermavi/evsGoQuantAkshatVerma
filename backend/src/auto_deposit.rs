use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PriorityLevel {
    Low,
    Medium,
    High,
}

pub struct AutoDepositCalculator;

impl AutoDepositCalculator {
    pub fn estimate_fee_per_trade(priority: PriorityLevel) -> u64 {
        // Very rough constants for demonstration; a production system would fetch
        // recent fee parameters from the Solana RPC and add a safety margin.
        match priority {
            PriorityLevel::Low => 5_000,      // lamports
            PriorityLevel::Medium => 10_000,  // lamports
            PriorityLevel::High => 25_000,    // lamports
        }
    }

    pub fn compute_deposit_for_trades(num_trades: u64, priority: PriorityLevel) -> Result<u64> {
        let per_trade = Self::estimate_fee_per_trade(priority);
        num_trades
            .checked_mul(per_trade)
            .ok_or_else(|| anyhow::anyhow!("fee calculation overflow"))
    }
}
