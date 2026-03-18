//! Block template generation for Stratum V2

use crate::error::StratumV2Error;
use blvm_node::module::traits::NodeAPI;
use blvm_protocol::{Block, BlockHeader, Hash, Transaction};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Block template generator
pub struct BlockTemplateGenerator {
    /// Node API for querying node state
    node_api: Arc<dyn NodeAPI>,
}

impl BlockTemplateGenerator {
    /// Create a new block template generator
    pub fn new(node_api: Arc<dyn NodeAPI>) -> Self {
        Self { node_api }
    }

    /// Generate a block template
    ///
    /// Uses NodeAPI get_block_template() which leverages the formally verified
    /// consensus function for template generation.
    /// When DATUM module is loaded (pool mode), queries coinbase payout and uses
    /// the first output's script as the payout address.
    pub async fn generate_template(&self) -> Result<Block, StratumV2Error> {
        debug!("Generating block template via NodeAPI");

        // Try DATUM coinbase coordination when pool module is loaded
        let (coinbase_script, coinbase_address) = self.get_coinbase_from_datum().await;

        // Get block template from node (uses formally verified consensus function)
        let template = self
            .node_api
            .get_block_template(
                vec!["segwit".to_string()], // Standard segwit rules
                coinbase_script,
                coinbase_address,
            )
            .await
            .map_err(|e| StratumV2Error::TemplateError(format!("Failed to get block template: {}", e)))?;

        info!(
            "Got block template: height={}, {} transactions",
            template.height,
            template.transactions.len()
        );

        // Convert BlockTemplate to Block format
        // BlockTemplate has coinbase separate, Block has all transactions together
        let mut all_transactions = vec![template.coinbase_tx];
        all_transactions.extend(template.transactions);

        let block = Block {
            header: template.header,
            transactions: all_transactions.into_boxed_slice(),
        };

        info!(
            "Converted template to block: height={}, prev_hash={:x?}, {} transactions, merkle_root={:x?}",
            template.height,
            &block.header.prev_block_hash[..8],
            block.transactions.len(),
            &block.header.merkle_root[..8]
        );

        Ok(block)
    }

    /// Query DATUM for coinbase payout when pool module is loaded.
    /// Returns (script, address) for get_block_template; address uses "hex:" prefix for raw script bytes.
    async fn get_coinbase_from_datum(&self) -> (Option<Vec<u8>>, Option<String>) {
        if self.node_api.is_module_available("datum").await.ok() != Some(true) {
            return (None, None);
        }
        let response = match self
            .node_api
            .call_module(None, "get_coinbase_payout", vec![])
            .await
        {
            Ok(r) => r,
            Err(_) => return (None, None),
        };

        let json: serde_json::Value = match serde_json::from_slice(&response) {
            Ok(j) => j,
            Err(e) => {
                debug!("DATUM coinbase response parse error: {}", e);
                return (None, None);
            }
        };

        let outputs = match json.get("outputs").and_then(|o| o.as_array()) {
            Some(outs) if !outs.is_empty() => outs,
            _ => return (None, None),
        };

        // Use first output's script as coinbase address (consensus supports single output)
        let first = match outputs.first().and_then(|o| o.get("script")) {
            Some(s) => s,
            None => return (None, None),
        };

        let script_hex = match first.as_str() {
            Some(h) => h.to_string(),
            None => return (None, None),
        };

        let script = match hex::decode(&script_hex) {
            Ok(s) => s,
            Err(_) => return (None, None),
        };

        info!(
            "Using DATUM coinbase payout: {} bytes from pool",
            script.len()
        );
        (None, Some(format!("hex:{}", script_hex)))
    }
}

