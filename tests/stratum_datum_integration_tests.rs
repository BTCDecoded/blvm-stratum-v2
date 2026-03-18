//! E2E integration tests for Stratum V2 + DATUM coordination
//!
//! Tests the flow when both modules are loaded:
//! - BlockTemplateGenerator queries DATUM get_coinbase_payout and passes to get_block_template
//! - handle_submit_shares calls DATUM submit_pow when a valid block is found

use blvm_stratum_v2::{
    messages::{self, *},
    pool::StratumV2Pool,
    protocol::TlvEncoder,
    server::StratumV2Server,
    template::BlockTemplateGenerator,
};
use blvm_node::module::traits::NodeAPI;
use blvm_protocol::{Block, BlockHeader, Hash, OutPoint, Transaction, TransactionInput, TransactionOutput};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// Mock NodeAPI that simulates DATUM + node for Stratum V2 integration tests
struct MockDatumNodeAPI {
    /// Track call_module invocations: (method, params_len)
    call_module_invocations: Arc<RwLock<Vec<(String, usize)>>>,
    /// DATUM get_coinbase_payout response (when Some, call_module returns it)
    datum_payout_response: Option<Vec<u8>>,
    /// Block template to return from get_block_template
    block_template: blvm_protocol::mining::BlockTemplate,
}

impl MockDatumNodeAPI {
    fn new_with_datum_payout(script_hex: &str) -> Self {
        let payout_json = serde_json::json!({
            "outputs": [{"script": script_hex, "value": 5000000000i64}],
            "primary_tag": "pool",
            "unique_id": "test-1"
        });
        Self {
            call_module_invocations: Arc::new(RwLock::new(Vec::new())),
            datum_payout_response: Some(serde_json::to_vec(&payout_json).unwrap()),
            block_template: create_test_block_template(),
        }
    }

    fn new_without_datum() -> Self {
        Self {
            call_module_invocations: Arc::new(RwLock::new(Vec::new())),
            datum_payout_response: None,
            block_template: create_test_block_template(),
        }
    }

    async fn get_call_invocations(&self) -> Vec<(String, usize)> {
        self.call_module_invocations.read().await.clone()
    }
}

fn create_test_block_template() -> blvm_protocol::mining::BlockTemplate {
    let coinbase = Transaction {
        version: 1,
        inputs: vec![TransactionInput {
            prevout: OutPoint {
                hash: [0u8; 32],
                index: 0xFFFFFFFF,
            },
            script_sig: vec![0x51, 0x00],
            sequence: 0xFFFFFFFF,
        }].into(),
        outputs: vec![TransactionOutput {
            value: 5000000000,
            script_pubkey: vec![0x51, 0x00],
        }].into(),
        lock_time: 0,
    };
    blvm_protocol::mining::BlockTemplate {
        header: BlockHeader {
            version: 1,
            prev_block_hash: [0u8; 32],
            merkle_root: [0u8; 32],
            timestamp: 1700000000,
            bits: 0x1d00ffff,
            nonce: 0,
        },
        coinbase_tx: coinbase,
        transactions: vec![],
        target: 0x00000000ffff00000000000000000000u128,
        height: 100,
        timestamp: 1700000000,
    }
}

#[async_trait::async_trait]
impl NodeAPI for MockDatumNodeAPI {
    async fn get_block(&self, _: &Hash) -> Result<Option<Block>, blvm_node::module::traits::ModuleError> {
        Ok(None)
    }
    async fn get_block_header(&self, _: &Hash) -> Result<Option<BlockHeader>, blvm_node::module::traits::ModuleError> {
        Ok(None)
    }
    async fn get_transaction(&self, _: &Hash) -> Result<Option<Transaction>, blvm_node::module::traits::ModuleError> {
        Ok(None)
    }
    async fn has_transaction(&self, _: &Hash) -> Result<bool, blvm_node::module::traits::ModuleError> {
        Ok(false)
    }
    async fn get_chain_tip(&self) -> Result<Hash, blvm_node::module::traits::ModuleError> {
        Ok([0u8; 32])
    }
    async fn get_block_height(&self) -> Result<u64, blvm_node::module::traits::ModuleError> {
        Ok(100)
    }
    async fn get_utxo(&self, _: &OutPoint) -> Result<Option<blvm_protocol::UTXO>, blvm_node::module::traits::ModuleError> {
        Ok(None)
    }
    async fn subscribe_events(
        &self,
        _: Vec<blvm_node::module::traits::EventType>,
    ) -> Result<
        tokio::sync::mpsc::Receiver<blvm_node::module::ipc::protocol::ModuleMessage>,
        blvm_node::module::traits::ModuleError,
    > {
        let (_tx, rx) = tokio::sync::mpsc::channel(100);
        Ok(rx)
    }
    async fn get_mempool_transactions(&self) -> Result<Vec<Hash>, blvm_node::module::traits::ModuleError> {
        Ok(Vec::new())
    }
    async fn get_mempool_transaction(&self, _: &Hash) -> Result<Option<Transaction>, blvm_node::module::traits::ModuleError> {
        Ok(None)
    }
    async fn get_mempool_size(&self) -> Result<blvm_node::module::traits::MempoolSize, blvm_node::module::traits::ModuleError> {
        Ok(blvm_node::module::traits::MempoolSize {
            transaction_count: 0,
            size_bytes: 0,
            total_fee_sats: 0,
        })
    }
    async fn get_network_stats(&self) -> Result<blvm_node::module::traits::NetworkStats, blvm_node::module::traits::ModuleError> {
        Ok(blvm_node::module::traits::NetworkStats {
            peer_count: 0,
            hash_rate: 0.0,
            bytes_sent: 0,
            bytes_received: 0,
        })
    }
    async fn get_network_peers(&self) -> Result<Vec<blvm_node::module::traits::PeerInfo>, blvm_node::module::traits::ModuleError> {
        Ok(Vec::new())
    }
    async fn get_chain_info(&self) -> Result<blvm_node::module::traits::ChainInfo, blvm_node::module::traits::ModuleError> {
        Ok(blvm_node::module::traits::ChainInfo {
            tip_hash: [0u8; 32],
            height: 100,
            difficulty: 1,
            chain_work: 0,
            is_synced: true,
        })
    }
    async fn get_block_by_height(&self, _: u64) -> Result<Option<Block>, blvm_node::module::traits::ModuleError> {
        Ok(None)
    }
    async fn get_lightning_node_url(&self) -> Result<Option<String>, blvm_node::module::traits::ModuleError> {
        Ok(None)
    }
    async fn get_lightning_info(&self) -> Result<Option<blvm_node::module::traits::LightningInfo>, blvm_node::module::traits::ModuleError> {
        Ok(None)
    }
    async fn get_payment_state(&self, _: &str) -> Result<Option<blvm_node::module::traits::PaymentState>, blvm_node::module::traits::ModuleError> {
        Ok(None)
    }
    async fn check_transaction_in_mempool(&self, _: &Hash) -> Result<bool, blvm_node::module::traits::ModuleError> {
        Ok(false)
    }
    async fn get_fee_estimate(&self, _: u32) -> Result<u64, blvm_node::module::traits::ModuleError> {
        Ok(1)
    }
    async fn read_file(&self, _: String) -> Result<Vec<u8>, blvm_node::module::traits::ModuleError> {
        Ok(Vec::new())
    }
    async fn write_file(&self, _: String, _: Vec<u8>) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn delete_file(&self, _: String) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn list_directory(&self, _: String) -> Result<Vec<String>, blvm_node::module::traits::ModuleError> {
        Ok(Vec::new())
    }
    async fn create_directory(&self, _: String) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn get_file_metadata(&self, _: String) -> Result<blvm_node::module::ipc::protocol::FileMetadata, blvm_node::module::traits::ModuleError> {
        Ok(blvm_node::module::ipc::protocol::FileMetadata {
            path: String::new(),
            size: 0,
            is_file: false,
            is_directory: false,
            modified: None,
            created: None,
        })
    }
    async fn get_all_metrics(&self) -> Result<std::collections::HashMap<String, Vec<blvm_node::module::metrics::manager::Metric>>, blvm_node::module::traits::ModuleError> {
        Ok(std::collections::HashMap::new())
    }
    async fn register_rpc_endpoint(&self, _: String, _: String) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn unregister_rpc_endpoint(&self, _: &str) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn register_timer(&self, _: u64, _: Arc<dyn blvm_node::module::timers::manager::TimerCallback>) -> Result<blvm_node::module::timers::manager::TimerId, blvm_node::module::traits::ModuleError> {
        Ok(0)
    }
    async fn cancel_timer(&self, _: blvm_node::module::timers::manager::TimerId) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn schedule_task(&self, _: u64, _: Arc<dyn blvm_node::module::timers::manager::TaskCallback>) -> Result<blvm_node::module::timers::manager::TaskId, blvm_node::module::traits::ModuleError> {
        Ok(0)
    }
    async fn report_metric(&self, _: blvm_node::module::metrics::manager::Metric) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn get_module_metrics(&self, _: &str) -> Result<Vec<blvm_node::module::metrics::manager::Metric>, blvm_node::module::traits::ModuleError> {
        Ok(Vec::new())
    }
    async fn initialize_module(&self, _: String, _: std::path::PathBuf, _: std::path::PathBuf) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn discover_modules(&self) -> Result<Vec<blvm_node::module::traits::ModuleInfo>, blvm_node::module::traits::ModuleError> {
        Ok(Vec::new())
    }
    async fn get_module_info(&self, _: &str) -> Result<Option<blvm_node::module::traits::ModuleInfo>, blvm_node::module::traits::ModuleError> {
        Ok(None)
    }
    async fn is_module_available(&self, id: &str) -> Result<bool, blvm_node::module::traits::ModuleError> {
        Ok(id == "datum" && self.datum_payout_response.is_some())
    }
    async fn publish_event(&self, _: blvm_node::module::traits::EventType, _: blvm_node::module::ipc::protocol::EventPayload) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn call_module(&self, _: Option<&str>, method: &str, params: Vec<u8>) -> Result<Vec<u8>, blvm_node::module::traits::ModuleError> {
        self.call_module_invocations.write().await.push((method.to_string(), params.len()));
        if method == "get_coinbase_payout" {
            if let Some(ref resp) = self.datum_payout_response {
                return Ok(resp.clone());
            }
        }
        if method == "submit_pow" {
            return Ok(serde_json::to_vec(&serde_json::json!({ "accepted": true })).unwrap());
        }
        Ok(Vec::new())
    }
    async fn register_module_api(&self, _: Arc<dyn blvm_node::module::inter_module::api::ModuleAPI>) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn unregister_module_api(&self) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn get_module_health(&self, _: &str) -> Result<Option<blvm_node::module::process::monitor::ModuleHealth>, blvm_node::module::traits::ModuleError> {
        Ok(None)
    }
    async fn get_all_module_health(&self) -> Result<Vec<(String, blvm_node::module::process::monitor::ModuleHealth)>, blvm_node::module::traits::ModuleError> {
        Ok(Vec::new())
    }
    async fn report_module_health(&self, _: blvm_node::module::process::monitor::ModuleHealth) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn send_mesh_packet_to_module(&self, _: &str, _: Vec<u8>, _: String) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn send_mesh_packet_to_peer(&self, _: String, _: Vec<u8>) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn send_stratum_v2_message_to_peer(&self, _: String, _: Vec<u8>) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn get_block_template(&self, _: Vec<String>, _: Option<Vec<u8>>, _: Option<String>) -> Result<blvm_protocol::mining::BlockTemplate, blvm_node::module::traits::ModuleError> {
        Ok(self.block_template.clone())
    }
    async fn submit_block(&self, _: Block) -> Result<blvm_node::module::traits::SubmitBlockResult, blvm_node::module::traits::ModuleError> {
        Ok(blvm_node::module::traits::SubmitBlockResult::Accepted)
    }
}

#[tokio::test]
async fn test_template_generator_queries_datum_for_coinbase() {
    let script_hex = "76a9141234567890abcdef1234567890abcdef123456789088ac";
    let node_api = Arc::new(MockDatumNodeAPI::new_with_datum_payout(script_hex));
    let generator = BlockTemplateGenerator::new(node_api.clone());

    let block = generator.generate_template().await.unwrap();
    assert!(!block.transactions.is_empty());

    let invocations = node_api.get_call_invocations().await;
    assert!(
        invocations.iter().any(|(m, _)| m == "get_coinbase_payout"),
        "Expected get_coinbase_payout to be called: {:?}",
        invocations
    );
}

#[tokio::test]
async fn test_template_generator_works_without_datum() {
    let node_api = Arc::new(MockDatumNodeAPI::new_without_datum());
    let generator = BlockTemplateGenerator::new(node_api.clone());

    let block = generator.generate_template().await.unwrap();
    assert!(!block.transactions.is_empty());

    let invocations = node_api.get_call_invocations().await;
    assert!(
        invocations.is_empty(),
        "No call_module expected when DATUM not loaded: {:?}",
        invocations
    );
}

#[tokio::test]
async fn test_pool_accepts_mined_block_share() {
    use blvm_protocol::genesis;

    // Direct pool test: use regtest genesis (has valid nonce) + share
    let mut pool = StratumV2Pool::new();
    pool.register_miner("miner-1".to_string());
    pool.open_channel("miner-1", 1, 0).unwrap(); // min_difficulty=0 => 1000x easier target

    // Use mainnet genesis (bits 0x1d00ffff, valid nonce)
    let block = genesis::mainnet_genesis();

    let (job_id, _) = pool.set_template(block.clone());
    let share_data = blvm_stratum_v2::pool::ShareData {
        channel_id: 1,
        job_id,
        nonce: block.header.nonce as u32,
        version: block.header.version as i64,
        merkle_root: block.header.merkle_root,
    };
    let (is_valid_share, is_valid_block) = pool.handle_share("miner-1", share_data).unwrap();
    assert!(is_valid_share, "Share should be valid (channel target)");
    assert!(is_valid_block, "Share should be valid block (network target)");
}

#[tokio::test]
async fn test_submit_shares_calls_datum_submit_pow_on_valid_block() {
    use blvm_protocol::genesis;

    let ctx = blvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };

    let node_api = Arc::new(MockDatumNodeAPI::new_with_datum_payout("76a91400"));
    let server = StratumV2Server::new(&ctx, node_api.clone()).await.unwrap();

    // Use mainnet genesis (bits 0x1d00ffff, valid nonce)
    let block = genesis::mainnet_genesis();

    let pool_handle = server.get_pool();
    let mut pool = pool_handle.write().await;
    pool.register_miner("miner-1".to_string());
    pool.open_channel("miner-1", 1, 0).unwrap(); // min_difficulty=0 => 1000x easier target
    let pool_template = pool.set_template(block.clone());
    drop(pool);

    let share_data = messages::ShareData {
        channel_id: 1,
        job_id: pool_template.0,
        nonce: block.header.nonce as u32,
        version: block.header.version as i64,
        merkle_root: block.header.merkle_root,
    };

    let msg = SubmitSharesMessage {
        channel_id: 1,
        shares: vec![share_data],
    };

    let msg_bytes = msg.to_bytes().unwrap();
    let mut encoder = TlvEncoder::new();
    let encoded = encoder.encode(msg.message_type(), &msg_bytes).unwrap();
    let msg_result = server.handle_message(encoded[4..].to_vec(), "miner-1".to_string()).await;
    assert!(msg_result.is_ok(), "handle_message failed: {:?}", msg_result.err());

    let invocations = node_api.get_call_invocations().await;
    let submit_pow_calls: Vec<_> = invocations.iter().filter(|(m, _)| m == "submit_pow").collect();
    assert!(
        !submit_pow_calls.is_empty(),
        "Expected submit_pow to be called on valid block: {:?}",
        invocations
    );
}

fn create_test_block() -> Block {
    Block {
        header: BlockHeader {
            version: 1,
            prev_block_hash: [0u8; 32],
            merkle_root: [0u8; 32],
            timestamp: 1700000000,
            bits: 0x1d00ffff,
            nonce: 0,
        },
        transactions: vec![Transaction {
            version: 1,
            inputs: vec![TransactionInput {
                prevout: OutPoint {
                    hash: [0u8; 32],
                    index: 0xFFFFFFFF,
                },
                script_sig: vec![0x51, 0x00],
                sequence: 0xFFFFFFFF,
            }].into(),
            outputs: vec![TransactionOutput {
                value: 5000000000,
                script_pubkey: vec![0x51, 0x00],
            }].into(),
            lock_time: 0,
        }]
        .into_boxed_slice(),
    }
}
