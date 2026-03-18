//! Stratum V2 ModuleAPI for inter-module communication
//!
//! Allows other modules (e.g. blvm-merge-mining) to register merge-mining channels,
//! get templates, and submit shares via call_module.

use blvm_node::module::inter_module::api::ModuleAPI;
use blvm_node::module::traits::ModuleError;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::pool::StratumV2Pool;

/// API methods exposed to other modules
pub const API_METHOD_REGISTER_MERGE_MINING_CHANNEL: &str = "register_merge_mining_channel";
pub const API_METHOD_GET_MERGE_MINING_TEMPLATE: &str = "get_merge_mining_template";
pub const API_METHOD_SUBMIT_MERGE_MINING_SHARE: &str = "submit_merge_mining_share";
pub const API_METHOD_GET_BLOCK_MERGE_MINING_REWARDS: &str = "get_block_merge_mining_rewards";
/// Operational RPC methods (get_template, get_connected_miners, get_pool_stats)
pub const API_METHOD_GET_TEMPLATE: &str = "get_template";
pub const API_METHOD_GET_CONNECTED_MINERS: &str = "get_connected_miners";
pub const API_METHOD_GET_POOL_STATS: &str = "get_pool_stats";

/// Stratum V2 ModuleAPI implementation for merge-mining integration
pub struct StratumV2ModuleAPI {
    pool: Arc<RwLock<StratumV2Pool>>,
}

impl StratumV2ModuleAPI {
    pub fn new(pool: Arc<RwLock<StratumV2Pool>>) -> Self {
        Self { pool }
    }

    /// Method names for register_module_api
    pub fn method_names() -> Vec<String> {
        vec![
            API_METHOD_REGISTER_MERGE_MINING_CHANNEL.to_string(),
            API_METHOD_GET_MERGE_MINING_TEMPLATE.to_string(),
            API_METHOD_SUBMIT_MERGE_MINING_SHARE.to_string(),
            API_METHOD_GET_BLOCK_MERGE_MINING_REWARDS.to_string(),
            API_METHOD_GET_TEMPLATE.to_string(),
            API_METHOD_GET_CONNECTED_MINERS.to_string(),
            API_METHOD_GET_POOL_STATS.to_string(),
        ]
    }
}

#[async_trait]
impl ModuleAPI for StratumV2ModuleAPI {
    async fn handle_request(
        &self,
        method: &str,
        params: &[u8],
        _caller_module_id: &str,
    ) -> Result<Vec<u8>, ModuleError> {
        let params_str = std::str::from_utf8(params).unwrap_or("");
        let result: Result<serde_json::Value, String> = match method {
            API_METHOD_REGISTER_MERGE_MINING_CHANNEL => {
                let req: serde_json::Value = serde_json::from_str(params_str)
                    .map_err(|e| e.to_string())?;
                let chain_id = req.get("chain_id").and_then(|v| v.as_str())
                    .ok_or_else(|| "missing chain_id".to_string())?;
                let min_difficulty = req.get("min_difficulty").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
                // TODO: Call pool.open_channel for merge-mining endpoint
                Ok(serde_json::json!({ "ok": true, "chain_id": chain_id, "min_difficulty": min_difficulty }))
            }
            API_METHOD_GET_MERGE_MINING_TEMPLATE => {
                let _pool = self.pool.read().await;
                // TODO: Return current template for merge-mining
                Ok(serde_json::json!({ "template": null }))
            }
            API_METHOD_SUBMIT_MERGE_MINING_SHARE => {
                let _req: serde_json::Value = serde_json::from_str(params_str)
                    .map_err(|e| e.to_string())?;
                // TODO: Validate and record share
                Ok(serde_json::json!({ "ok": true }))
            }
            API_METHOD_GET_BLOCK_MERGE_MINING_REWARDS => {
                let _req: serde_json::Value = serde_json::from_str(params_str)
                    .unwrap_or(serde_json::json!({}));
                // Stub: returns empty rewards. Full impl would parse coinbase for aux chain headers
                // and match shares to determine per-chain rewards.
                Ok(serde_json::json!({ "rewards": [] }))
            }
            API_METHOD_GET_TEMPLATE => {
                let pool = self.pool.read().await;
                let template = pool.current_template();
                let result = template.map(|b| {
                    serde_json::json!({
                        "prev_hash": hex::encode(b.header.prev_block_hash),
                        "merkle_root": hex::encode(b.header.merkle_root),
                        "timestamp": b.header.timestamp,
                        "bits": b.header.bits,
                        "tx_count": b.transactions.len()
                    })
                });
                Ok(serde_json::json!({ "template": result }))
            }
            API_METHOD_GET_CONNECTED_MINERS => {
                let pool = self.pool.read().await;
                let miners: Vec<&str> = pool.miners.keys().map(|s| s.as_str()).collect();
                Ok(serde_json::json!({ "miners": miners }))
            }
            API_METHOD_GET_POOL_STATS => {
                let pool = self.pool.read().await;
                let miner_count = pool.miners.len();
                let (total_shares, accepted_shares, rejected_shares) = pool.miners.values()
                    .fold((0u64, 0u64, 0u64), |(t, a, r), m| {
                        (t + m.stats.total_shares, a + m.stats.accepted_shares, r + m.stats.rejected_shares)
                    });
                Ok(serde_json::json!({
                    "miner_count": miner_count,
                    "total_shares": total_shares,
                    "accepted_shares": accepted_shares,
                    "rejected_shares": rejected_shares
                }))
            }
            _ => Err(format!("Unknown method: {}", method)),
        };

        result
            .map(|v| serde_json::to_vec(&v).unwrap_or_default())
            .map_err(ModuleError::OperationError)
    }

    fn list_methods(&self) -> Vec<String> {
        Self::method_names()
    }

    fn api_version(&self) -> u32 {
        1
    }
}
