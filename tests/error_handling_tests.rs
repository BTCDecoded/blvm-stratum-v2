//! Tests for error handling and edge cases

use bllvm_stratum_v2::{
    messages::*,
    pool::{ShareData, StratumV2Pool},
    server::StratumV2Server,
    error::StratumV2Error,
};
use bllvm_protocol::{Block, BlockHeader};
use bllvm_node::module::traits::NodeAPI;
use std::sync::Arc;

// Minimal mock NodeAPI
struct MockNodeAPI;

#[async_trait::async_trait]
impl NodeAPI for MockNodeAPI {
    async fn get_block(&self, _: &bllvm_protocol::Hash) -> Result<Option<bllvm_protocol::Block>, bllvm_node::module::traits::ModuleError> { Ok(None) }
    async fn get_block_header(&self, _: &bllvm_protocol::Hash) -> Result<Option<bllvm_protocol::BlockHeader>, bllvm_node::module::traits::ModuleError> { Ok(None) }
    async fn get_transaction(&self, _: &bllvm_protocol::Hash) -> Result<Option<bllvm_protocol::Transaction>, bllvm_node::module::traits::ModuleError> { Ok(None) }
    async fn has_transaction(&self, _: &bllvm_protocol::Hash) -> Result<bool, bllvm_node::module::traits::ModuleError> { Ok(false) }
    async fn get_chain_tip(&self) -> Result<bllvm_protocol::Hash, bllvm_node::module::traits::ModuleError> { Ok([0u8; 32]) }
    async fn get_block_height(&self) -> Result<u64, bllvm_node::module::traits::ModuleError> { Ok(100) }
    async fn get_utxo(&self, _: &bllvm_protocol::OutPoint) -> Result<Option<bllvm_protocol::UTXO>, bllvm_node::module::traits::ModuleError> { Ok(None) }
    async fn subscribe_events(&self, _: Vec<bllvm_node::module::traits::EventType>) -> Result<tokio::sync::mpsc::Receiver<bllvm_node::module::ipc::protocol::ModuleMessage>, bllvm_node::module::traits::ModuleError> {
        let (_tx, rx) = tokio::sync::mpsc::channel(100);
        Ok(rx)
    }
    async fn get_mempool_transactions(&self) -> Result<Vec<bllvm_protocol::Hash>, bllvm_node::module::traits::ModuleError> { Ok(Vec::new()) }
    async fn get_mempool_transaction(&self, _: &bllvm_protocol::Hash) -> Result<Option<bllvm_protocol::Transaction>, bllvm_node::module::traits::ModuleError> { Ok(None) }
    async fn get_mempool_size(&self) -> Result<bllvm_node::module::traits::MempoolSize, bllvm_node::module::traits::ModuleError> {
        Ok(bllvm_node::module::traits::MempoolSize { count: 0, size_bytes: 0 })
    }
    async fn get_network_stats(&self) -> Result<bllvm_node::module::traits::NetworkStats, bllvm_node::module::traits::ModuleError> {
        Ok(bllvm_node::module::traits::NetworkStats { connected_peers: 0, bytes_sent: 0, bytes_received: 0 })
    }
    async fn get_network_peers(&self) -> Result<Vec<bllvm_node::module::traits::PeerInfo>, bllvm_node::module::traits::ModuleError> { Ok(Vec::new()) }
    async fn get_chain_info(&self) -> Result<bllvm_node::module::traits::ChainInfo, bllvm_node::module::traits::ModuleError> {
        Ok(bllvm_node::module::traits::ChainInfo { tip: [0u8; 32], height: 100, difficulty: 1.0 })
    }
    async fn get_block_by_height(&self, _: u64) -> Result<Option<bllvm_protocol::Block>, bllvm_node::module::traits::ModuleError> { Ok(None) }
    async fn get_lightning_node_url(&self) -> Result<Option<String>, bllvm_node::module::traits::ModuleError> { Ok(None) }
    async fn get_lightning_info(&self) -> Result<Option<bllvm_node::module::traits::LightningInfo>, bllvm_node::module::traits::ModuleError> { Ok(None) }
    async fn get_payment_state(&self, _: &str) -> Result<Option<bllvm_node::module::traits::PaymentState>, bllvm_node::module::traits::ModuleError> { Ok(None) }
    async fn check_transaction_in_mempool(&self, _: &bllvm_protocol::Hash) -> Result<bool, bllvm_node::module::traits::ModuleError> { Ok(false) }
    async fn get_fee_estimate(&self, _: u32) -> Result<u64, bllvm_node::module::traits::ModuleError> { Ok(1) }
    async fn read_file(&self, _: String) -> Result<Vec<u8>, bllvm_node::module::traits::ModuleError> { Ok(Vec::new()) }
    async fn write_file(&self, _: String, _: Vec<u8>) -> Result<(), bllvm_node::module::traits::ModuleError> { Ok(()) }
    async fn delete_file(&self, _: String) -> Result<(), bllvm_node::module::traits::ModuleError> { Ok(()) }
    async fn list_directory(&self, _: String) -> Result<Vec<String>, bllvm_node::module::traits::ModuleError> { Ok(Vec::new()) }
    async fn create_directory(&self, _: String) -> Result<(), bllvm_node::module::traits::ModuleError> { Ok(()) }
    async fn get_file_metadata(&self, _: String) -> Result<bllvm_node::module::ipc::protocol::FileMetadata, bllvm_node::module::traits::ModuleError> {
        Ok(bllvm_node::module::ipc::protocol::FileMetadata { size: 0, modified: 0, is_dir: false })
    }
    async fn storage_open_tree(&self, _: String) -> Result<String, bllvm_node::module::traits::ModuleError> { Ok("test".to_string()) }
    async fn storage_insert(&self, _: String, _: Vec<u8>, _: Vec<u8>) -> Result<(), bllvm_node::module::traits::ModuleError> { Ok(()) }
    async fn storage_get(&self, _: String, _: Vec<u8>) -> Result<Option<Vec<u8>>, bllvm_node::module::traits::ModuleError> { Ok(None) }
    async fn storage_remove(&self, _: String, _: Vec<u8>) -> Result<(), bllvm_node::module::traits::ModuleError> { Ok(()) }
    async fn storage_contains_key(&self, _: String, _: Vec<u8>) -> Result<bool, bllvm_node::module::traits::ModuleError> { Ok(false) }
    async fn storage_iter(&self, _: String) -> Result<Vec<(Vec<u8>, Vec<u8>)>, bllvm_node::module::traits::ModuleError> { Ok(Vec::new()) }
    async fn storage_transaction(&self, _: String, _: Vec<bllvm_node::module::ipc::protocol::StorageOperation>) -> Result<(), bllvm_node::module::traits::ModuleError> { Ok(()) }
    async fn register_rpc_endpoint(&self, _: String, _: String) -> Result<(), bllvm_node::module::traits::ModuleError> { Ok(()) }
    async fn unregister_rpc_endpoint(&self, _: &str) -> Result<(), bllvm_node::module::traits::ModuleError> { Ok(()) }
    async fn register_timer(&self, _: u64, _: Arc<dyn bllvm_node::module::timers::manager::TimerCallback>) -> Result<bllvm_node::module::timers::manager::TimerId, bllvm_node::module::traits::ModuleError> { Ok(0) }
    async fn cancel_timer(&self, _: bllvm_node::module::timers::manager::TimerId) -> Result<(), bllvm_node::module::traits::ModuleError> { Ok(()) }
    async fn schedule_task(&self, _: u64, _: Arc<dyn bllvm_node::module::timers::manager::TaskCallback>) -> Result<bllvm_node::module::timers::manager::TaskId, bllvm_node::module::traits::ModuleError> { Ok(0) }
    async fn report_metric(&self, _: bllvm_node::module::metrics::manager::Metric) -> Result<(), bllvm_node::module::traits::ModuleError> { Ok(()) }
    async fn get_module_metrics(&self, _: &str) -> Result<Vec<bllvm_node::module::metrics::manager::Metric>, bllvm_node::module::traits::ModuleError> { Ok(Vec::new()) }
    async fn initialize_module(&self, _: &str, _: bllvm_node::module::traits::ModuleManifest) -> Result<(), bllvm_node::module::traits::ModuleError> { Ok(()) }
    async fn discover_modules(&self) -> Result<Vec<bllvm_node::module::traits::ModuleInfo>, bllvm_node::module::traits::ModuleError> { Ok(Vec::new()) }
    async fn get_module_info(&self, _: &str) -> Result<Option<bllvm_node::module::traits::ModuleInfo>, bllvm_node::module::traits::ModuleError> { Ok(None) }
    async fn is_module_available(&self, _: &str) -> Result<bool, bllvm_node::module::traits::ModuleError> { Ok(false) }
    async fn publish_event(&self, _: bllvm_node::module::traits::EventType, _: bllvm_node::module::traits::EventPayload) -> Result<(), bllvm_node::module::traits::ModuleError> { Ok(()) }
    async fn call_module(&self, _: Option<&str>, _: &str, _: Vec<u8>) -> Result<Vec<u8>, bllvm_node::module::traits::ModuleError> { Ok(Vec::new()) }
    async fn register_module_api(&self, _: Vec<String>, _: u32) -> Result<(), bllvm_node::module::traits::ModuleError> { Ok(()) }
    async fn unregister_module_api(&self) -> Result<(), bllvm_node::module::traits::ModuleError> { Ok(()) }
    async fn get_module_health(&self, _: &str) -> Result<Option<bllvm_node::module::process::monitor::ModuleHealth>, bllvm_node::module::traits::ModuleError> { Ok(None) }
    async fn get_all_module_health(&self) -> Result<Vec<(String, bllvm_node::module::process::monitor::ModuleHealth)>, bllvm_node::module::traits::ModuleError> { Ok(Vec::new()) }
    async fn report_module_health(&self, _: bllvm_node::module::process::monitor::ModuleHealth) -> Result<(), bllvm_node::module::traits::ModuleError> { Ok(()) }
    async fn send_mesh_packet_to_module(&self, _: &str, _: Vec<u8>, _: String) -> Result<(), bllvm_node::module::traits::ModuleError> { Ok(()) }
    async fn send_mesh_packet_to_peer(&self, _: String, _: Vec<u8>) -> Result<(), bllvm_node::module::traits::ModuleError> { Ok(()) }
    async fn send_stratum_v2_message_to_peer(&self, _: String, _: Vec<u8>) -> Result<(), bllvm_node::module::traits::ModuleError> { Ok(()) }
    async fn get_node_public_key(&self) -> Result<Option<Vec<u8>>, bllvm_node::module::traits::ModuleError> { Ok(None) }
    async fn get_event_publisher(&self) -> Result<Option<Arc<bllvm_node::node::event_publisher::EventPublisher>>, bllvm_node::module::traits::ModuleError> { Ok(None) }
    async fn get_block_template(&self, _: Vec<String>, _: Option<Vec<u8>>, _: Option<String>) -> Result<bllvm_node::module::traits::BlockTemplate, bllvm_node::module::traits::ModuleError> {
        Err(bllvm_node::module::traits::ModuleError::OperationError("Not implemented".to_string()))
    }
    async fn submit_block(&self, _: bllvm_protocol::Block) -> Result<bllvm_node::module::traits::SubmitBlockResult, bllvm_node::module::traits::ModuleError> {
        Ok(bllvm_node::module::traits::SubmitBlockResult::Accepted)
    }
}

#[tokio::test]
async fn test_duplicate_channel_id() {
    let mut pool = StratumV2Pool::new();
    
    pool.register_miner("test-miner".to_string());
    
    // Open channel with ID 1
    let result1 = pool.open_channel("test-miner", 1, 1);
    assert!(result1.is_ok());
    
    // Try to open another channel with same ID - should succeed (overwrites)
    let result2 = pool.open_channel("test-miner", 1, 2);
    assert!(result2.is_ok());
    
    // Verify channel was updated
    let miner = pool.miners.get("test-miner").unwrap();
    let channel = miner.channels.get(&1).unwrap();
    assert_eq!(channel.min_difficulty, 2);
}

#[tokio::test]
async fn test_submit_share_invalid_job() {
    let mut pool = StratumV2Pool::new();
    
    pool.register_miner("test-miner".to_string());
    pool.open_channel("test-miner", 1, 1).unwrap();
    
    // Create a job
    let block = Block {
        header: BlockHeader {
            version: 1,
            prev_block_hash: [0u8; 32],
            merkle_root: [0u8; 32],
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            bits: 0x1d00ffff,
            nonce: 0,
        },
        transactions: vec![].into_boxed_slice(),
    };
    pool.set_template(block);
    
    // Submit share with invalid job ID
    let share = ShareData {
        channel_id: 1,
        job_id: 999, // Invalid job ID
        nonce: 0,
        version: 1,
        merkle_root: [0u8; 32],
    };
    
    let result = pool.handle_share("test-miner", share);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_submit_share_wrong_channel() {
    let mut pool = StratumV2Pool::new();
    
    pool.register_miner("test-miner".to_string());
    pool.open_channel("test-miner", 1, 1).unwrap();
    
    // Create a job
    let block = Block {
        header: BlockHeader {
            version: 1,
            prev_block_hash: [0u8; 32],
            merkle_root: [0u8; 32],
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            bits: 0x1d00ffff,
            nonce: 0,
        },
        transactions: vec![].into_boxed_slice(),
    };
    pool.set_template(block);
    
    // Submit share with wrong channel ID
    let share = ShareData {
        channel_id: 999, // Invalid channel
        job_id: 1,
        nonce: 0,
        version: 1,
        merkle_root: [0u8; 32],
    };
    
    let result = pool.handle_share("test-miner", share);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_empty_share_list() {
    let ctx = bllvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    
    let node_api = Arc::new(MockNodeAPI);
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();
    
    // Setup connection and channel
    let setup_msg = SetupConnectionMessage {
        protocol_version: 2,
        endpoint: "test-miner".to_string(),
        capabilities: vec!["mining".to_string()],
    };
    server.handle_setup_connection(setup_msg, "test-miner").await.unwrap();
    
    let open_msg = OpenMiningChannelMessage {
        channel_id: 1,
        request_id: 1,
        min_difficulty: 1,
    };
    server.handle_open_channel("test-miner", open_msg).await.unwrap();
    
    // Submit empty share list
    let submit_msg = SubmitSharesMessage {
        channel_id: 1,
        shares: vec![], // Empty
    };
    
    let result = server.handle_submit_shares("test-miner", submit_msg).await;
    // Should handle gracefully (may succeed with last_job_id = 0)
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_multiple_shares_same_job() {
    let mut pool = StratumV2Pool::new();
    
    pool.register_miner("test-miner".to_string());
    pool.open_channel("test-miner", 1, 1).unwrap();
    
    // Create a job
    let block = Block {
        header: BlockHeader {
            version: 1,
            prev_block_hash: [0u8; 32],
            merkle_root: [0u8; 32],
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            bits: 0x1d00ffff,
            nonce: 0,
        },
        transactions: vec![].into_boxed_slice(),
    };
    pool.set_template(block);
    
    // Submit multiple shares for same job
    for nonce in 0..5 {
        let share = ShareData {
            channel_id: 1,
            job_id: 1,
            nonce,
            version: 1,
            merkle_root: [0u8; 32],
        };
        let _ = pool.handle_share("test-miner", share);
    }
    
    // Verify all shares counted
    let miner = pool.miners.get("test-miner").unwrap();
    assert_eq!(miner.stats.total_shares, 5);
}

#[tokio::test]
async fn test_channel_target_calculation_edge_cases() {
    let pool = StratumV2Pool::new();
    
    // Test zero difficulty
    let result = pool.calculate_channel_target(0);
    assert!(result.is_ok());
    
    // Test very high difficulty
    let result = pool.calculate_channel_target(1000);
    assert!(result.is_ok());
    
    // Test normal difficulty
    let result = pool.calculate_channel_target(1);
    assert!(result.is_ok());
    let target = result.unwrap();
    assert_eq!(target.len(), 32);
}

