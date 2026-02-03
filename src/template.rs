//! Block template generation for Stratum V2

use crate::error::StratumV2Error;
use bllvm_node::module::traits::NodeAPI;
use bllvm_protocol::{Block, BlockHeader, Hash, Transaction};
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
    pub async fn generate_template(&self) -> Result<Block, StratumV2Error> {
        debug!("Generating block template via NodeAPI");

        // Get block template from node (uses formally verified consensus function)
        let template = self
            .node_api
            .get_block_template(
                vec!["segwit".to_string()], // Standard segwit rules
                None,                        // Default coinbase script
                None,                        // Default coinbase address
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

}

