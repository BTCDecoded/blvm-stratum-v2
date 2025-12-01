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
    /// This method:
    /// 1. Gets the current chain tip
    /// 2. Gets transactions from mempool
    /// 3. Creates coinbase transaction
    /// 4. Calculates merkle root
    /// 5. Builds block template
    pub async fn generate_template(&self) -> Result<Block, StratumV2Error> {
        debug!("Generating block template");

        // 1. Get chain tip
        let tip_hash = self
            .node_api
            .get_chain_tip()
            .await
            .map_err(|e| StratumV2Error::TemplateError(format!("Failed to get chain tip: {}", e)))?;

        let tip_block = self
            .node_api
            .get_block(&tip_hash)
            .await
            .map_err(|e| StratumV2Error::TemplateError(format!("Failed to get tip block: {}", e)))?
            .ok_or_else(|| StratumV2Error::TemplateError("Chain tip block not found".to_string()))?;

        let height = self
            .node_api
            .get_block_height()
            .await
            .map_err(|e| StratumV2Error::TemplateError(format!("Failed to get block height: {}", e)))?;

        // 2. Get transactions from mempool
        let mempool_tx_hashes = self
            .node_api
            .get_mempool_transactions()
            .await
            .map_err(|e| StratumV2Error::TemplateError(format!("Failed to get mempool transactions: {}", e)))?;

        debug!("Found {} transactions in mempool", mempool_tx_hashes.len());

        // Get actual transactions (limited to reasonable size)
        let mut transactions = Vec::new();
        let max_txs = 1000; // Reasonable limit
        for tx_hash in mempool_tx_hashes.iter().take(max_txs) {
            if let Ok(Some(tx)) = self.node_api.get_mempool_transaction(tx_hash).await {
                transactions.push(tx);
            }
        }

        info!("Selected {} transactions for block template", transactions.len());

        // 3. Create coinbase transaction
        let coinbase_tx = self.create_coinbase_transaction(height + 1, &transactions).await?;

        // 4. Build transaction list (coinbase first)
        let mut all_transactions = vec![coinbase_tx];
        all_transactions.extend(transactions);

        // 5. Calculate merkle root
        let merkle_root = self.calculate_merkle_root(&all_transactions)?;

        // 6. Get difficulty bits from tip block
        let bits = tip_block.header.bits;

        // 7. Get current timestamp
        let timestamp = current_timestamp();

        // 8. Build block template
        let template = Block {
            header: BlockHeader {
                version: 1,
                prev_block_hash: tip_hash,
                merkle_root,
                timestamp,
                bits,
                nonce: 0,
            },
            transactions: all_transactions.into_boxed_slice(),
        };

        info!(
            "Generated block template: height={}, prev_hash={:x?}, {} transactions, merkle_root={:x?}",
            height + 1,
            &tip_hash[..8],
            template.transactions.len(),
            &merkle_root[..8]
        );

        Ok(template)
    }

    /// Create coinbase transaction with subsidy + fees
    async fn create_coinbase_transaction(
        &self,
        height: u64,
        transactions: &[Transaction],
    ) -> Result<Transaction, StratumV2Error> {
        use bllvm_consensus::ConsensusProof;

        // Calculate block subsidy
        let consensus = ConsensusProof::new();
        let subsidy = consensus.get_block_subsidy(height);

        // Calculate total fees from transactions
        // For now, use a simple fee estimate
        // In production, would calculate actual fees
        let fee_estimate = self
            .node_api
            .get_fee_estimate(1)
            .await
            .unwrap_or(0);
        let estimated_fees = fee_estimate * transactions.len() as u64;

        let total_value = subsidy + estimated_fees;

        // Create coinbase transaction
        // Coinbase has empty inputs and outputs the subsidy + fees
        let coinbase = Transaction {
            version: 1,
            inputs: vec![].into_boxed_slice(),
            outputs: vec![bllvm_protocol::TransactionOutput {
                value: total_value,
                script_pubkey: vec![].into_boxed_slice(), // Empty script (miner can set)
            }]
            .into_boxed_slice(),
            lock_time: 0,
        };

        Ok(coinbase)
    }

    /// Calculate merkle root from transactions
    fn calculate_merkle_root(&self, transactions: &[Transaction]) -> Result<Hash, StratumV2Error> {
        use sha2::{Digest, Sha256};

        if transactions.is_empty() {
            return Err(StratumV2Error::TemplateError(
                "Cannot calculate merkle root for empty transaction list".to_string(),
            ));
        }

        // Serialize all transaction hashes
        let mut tx_hashes = Vec::new();
        for tx in transactions {
            let tx_bytes = bincode::serialize(tx)
                .map_err(|e| StratumV2Error::TemplateError(format!("Failed to serialize transaction: {}", e)))?;
            let hash = Sha256::digest(&tx_bytes);
            tx_hashes.push(hash);
        }

        // Build merkle tree
        let mut level = tx_hashes;
        while level.len() > 1 {
            let mut next_level = Vec::new();
            for i in (0..level.len()).step_by(2) {
                if i + 1 < level.len() {
                    // Pair of hashes
                    let mut combined = level[i].to_vec();
                    combined.extend_from_slice(&level[i + 1]);
                    let hash = Sha256::digest(&combined);
                    next_level.push(hash);
                } else {
                    // Odd one out, duplicate it
                    let mut combined = level[i].to_vec();
                    combined.extend_from_slice(&level[i]);
                    let hash = Sha256::digest(&combined);
                    next_level.push(hash);
                }
            }
            level = next_level;
        }

        // Double SHA256 of root
        let root_hash = Sha256::digest(&level[0]);
        let final_hash = Sha256::digest(&root_hash);

        let mut result = [0u8; 32];
        result.copy_from_slice(&final_hash);
        Ok(result)
    }
}

/// Get current timestamp (Unix epoch seconds)
fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

