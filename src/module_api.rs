//! Stratum V2 ModuleAPI for inter-module communication
//!
//! Allows other modules (e.g. blvm-merge-mining) to register merge-mining channels,
//! get templates, and submit shares via call_module.

use async_trait::async_trait;
use blvm_node::module::inter_module::api::ModuleAPI;
use blvm_node::module::traits::ModuleError;
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
        let params = if params.is_empty() {
            serde_json::json!({})
        } else {
            match serde_json::from_slice::<serde_json::Value>(params) {
                Ok(value) => value,
                Err(_) => {
                    return Err(ModuleError::OperationError(
                        "invalid merge-mining parameters".into(),
                    ));
                }
            }
        };
        let result: Result<serde_json::Value, String> = match method {
            API_METHOD_REGISTER_MERGE_MINING_CHANNEL => {
                let endpoint = params
                    .get("endpoint")
                    .and_then(|v| v.as_str())
                    .unwrap_or("merge")
                    .to_string();
                let channel_id = params
                    .get("channel_id")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1) as u32;
                let mut pool = self.pool.write().await;
                match pool.register_merge_channel(&endpoint, channel_id) {
                    Ok(()) => Ok(serde_json::json!({
                        "endpoint": endpoint,
                        "channel_id": channel_id
                    })),
                    Err(reason) => Err(reason),
                }
            }
            API_METHOD_GET_MERGE_MINING_TEMPLATE => {
                let pool = self.pool.read().await;
                Ok(match pool.merge_template() {
                    Some((prev_hash, merkle_root, timestamp, bits)) => serde_json::json!({
                        "prev_hash": hex::encode(prev_hash),
                        "merkle_root": hex::encode(merkle_root),
                        "timestamp": timestamp,
                        "bits": bits
                    }),
                    None => serde_json::json!({ "template": null }),
                })
            }
            API_METHOD_SUBMIT_MERGE_MINING_SHARE => 'submit: {
                let channel_id = params
                    .get("channel_id")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                let Some(hash_hex) = params.get("hash").and_then(|v| v.as_str()) else {
                    break 'submit Err("share hash is required".to_string());
                };
                let Ok(bytes) = hex::decode(hash_hex) else {
                    break 'submit Err("share hash is not hex".to_string());
                };
                if bytes.len() != 32 {
                    break 'submit Err("share hash must be 32 bytes".to_string());
                }
                let mut share_hash = [0u8; 32];
                share_hash.copy_from_slice(&bytes);
                let mut pool = self.pool.write().await;
                match pool.submit_merge_share(channel_id, share_hash) {
                    Ok(accepted) => Ok(serde_json::json!({ "accepted": accepted })),
                    Err(reason) => Err(reason),
                }
            }
            API_METHOD_GET_BLOCK_MERGE_MINING_REWARDS => {
                let hash_hex = params
                    .get("block_hash")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let rewards = if hash_hex.is_empty() {
                    Vec::new()
                } else {
                    let bytes = hex::decode(hash_hex).unwrap_or_default();
                    if bytes.len() != 32 {
                        Vec::new()
                    } else {
                        let mut block_hash = [0u8; 32];
                        block_hash.copy_from_slice(&bytes);
                        let pool = self.pool.read().await;
                        pool.merge_rewards(&block_hash)
                    }
                };
                Ok(serde_json::json!({ "rewards": rewards }))
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
                let (total_shares, accepted_shares, rejected_shares) =
                    pool.miners
                        .values()
                        .fold((0u64, 0u64, 0u64), |(t, a, r), m| {
                            (
                                t + m.stats.total_shares,
                                a + m.stats.accepted_shares,
                                r + m.stats.rejected_shares,
                            )
                        });
                Ok(serde_json::json!({
                    "miner_count": miner_count,
                    "total_shares": total_shares,
                    "accepted_shares": accepted_shares,
                    "rejected_shares": rejected_shares
                }))
            }
            _ => Err(format!("Unknown method: {method}")),
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
