//! Integration tests for Stratum V2 protocol flow

use bllvm_stratum_v2::{
    messages::*,
    pool::{ShareData, StratumV2Pool},
    protocol::{TlvDecoder, TlvEncoder},
    server::StratumV2Server,
};
use bllvm_protocol::{Block, BlockHeader, Transaction, OutPoint, TxInput, TxOutput};
use bllvm_node::module::traits::NodeAPI;
use std::sync::Arc;
use tokio::sync::RwLock;

// Mock NodeAPI for testing
struct MockNodeAPI {
    submitted_blocks: Arc<RwLock<Vec<Block>>>,
}

impl MockNodeAPI {
    fn new() -> Self {
        Self {
            submitted_blocks: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

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
    async fn submit_block(&self, block: bllvm_protocol::Block) -> Result<bllvm_node::module::traits::SubmitBlockResult, bllvm_node::module::traits::ModuleError> {
        self.submitted_blocks.write().await.push(block);
        Ok(bllvm_node::module::traits::SubmitBlockResult::Accepted)
    }
}

fn create_test_block() -> Block {
    Block {
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
        transactions: vec![Transaction {
            version: 1,
            inputs: vec![TxInput {
                prevout: OutPoint {
                    hash: [0u8; 32],
                    index: 0xFFFFFFFF,
                },
                script_sig: vec![0x00, 0x01],
                sequence: 0xFFFFFFFF,
            }],
            outputs: vec![TxOutput {
                value: 5000000000,
                script_pubkey: vec![0x76, 0xa9, 0x14],
            }],
            lock_time: 0,
        }].into_boxed_slice(),
    }
}

#[tokio::test]
async fn test_setup_connection_flow() {
    let ctx = bllvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    
    let node_api = Arc::new(MockNodeAPI::new());
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();
    
    // Create Setup Connection message
    let setup_msg = SetupConnectionMessage {
        protocol_version: 2,
        endpoint: "test-miner".to_string(),
        capabilities: vec!["mining".to_string()],
    };
    
    // Handle message
    let response = server.handle_setup_connection(setup_msg, "test-miner").await;
    
    assert!(response.is_ok());
    let success = response.unwrap();
    assert_eq!(success.supported_versions, vec![2]);
    assert!(success.capabilities.contains(&"mining".to_string()));
}

#[tokio::test]
async fn test_setup_connection_invalid_version() {
    let ctx = bllvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    
    let node_api = Arc::new(MockNodeAPI::new());
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();
    
    // Create Setup Connection message with invalid version
    let setup_msg = SetupConnectionMessage {
        protocol_version: 1, // Invalid version
        endpoint: "test-miner".to_string(),
        capabilities: vec!["mining".to_string()],
    };
    
    // Handle message - should fail
    let response = server.handle_setup_connection(setup_msg, "test-miner").await;
    assert!(response.is_err());
}

#[tokio::test]
async fn test_open_channel_flow() {
    let ctx = bllvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    
    let node_api = Arc::new(MockNodeAPI::new());
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();
    
    // First setup connection
    let setup_msg = SetupConnectionMessage {
        protocol_version: 2,
        endpoint: "test-miner".to_string(),
        capabilities: vec!["mining".to_string()],
    };
    server.handle_setup_connection(setup_msg, "test-miner").await.unwrap();
    
    // Then open channel
    let open_msg = OpenMiningChannelMessage {
        channel_id: 1,
        request_id: 1,
        min_difficulty: 1,
    };
    
    let response = server.handle_open_channel("test-miner", open_msg).await;
    assert!(response.is_ok());
    let success = response.unwrap();
    assert_eq!(success.channel_id, 1);
    assert_eq!(success.request_id, 1);
    assert_eq!(success.max_jobs, 10);
}

#[tokio::test]
async fn test_open_channel_without_setup() {
    let ctx = bllvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    
    let node_api = Arc::new(MockNodeAPI::new());
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();
    
    // Try to open channel without setup
    let open_msg = OpenMiningChannelMessage {
        channel_id: 1,
        request_id: 1,
        min_difficulty: 1,
    };
    
    let response = server.handle_open_channel("unknown-miner", open_msg).await;
    assert!(response.is_err());
}

#[tokio::test]
async fn test_submit_shares_flow() {
    let ctx = bllvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    
    let node_api = Arc::new(MockNodeAPI::new());
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();
    
    // Setup connection
    let setup_msg = SetupConnectionMessage {
        protocol_version: 2,
        endpoint: "test-miner".to_string(),
        capabilities: vec!["mining".to_string()],
    };
    server.handle_setup_connection(setup_msg, "test-miner").await.unwrap();
    
    // Open channel
    let open_msg = OpenMiningChannelMessage {
        channel_id: 1,
        request_id: 1,
        min_difficulty: 1,
    };
    server.handle_open_channel("test-miner", open_msg).await.unwrap();
    
    // Set template to create a job
    let block = create_test_block();
    let pool = server.get_pool().await;
    pool.write().await.set_template(block);
    
    // Submit shares
    let submit_msg = SubmitSharesMessage {
        channel_id: 1,
        shares: vec![ShareData {
            channel_id: 1,
            job_id: 1,
            nonce: 0,
            version: 1,
            merkle_root: [0u8; 32],
        }],
    };
    
    let response = server.handle_submit_shares("test-miner", submit_msg).await;
    assert!(response.is_ok());
    let success = response.unwrap();
    assert_eq!(success.channel_id, 1);
    assert_eq!(success.last_job_id, 1);
}

#[tokio::test]
async fn test_message_encoding_decoding() {
    // Test full message encoding/decoding flow
    let setup_msg = SetupConnectionMessage {
        protocol_version: 2,
        endpoint: "test-miner".to_string(),
        capabilities: vec!["mining".to_string()],
    };
    
    // Encode message
    let payload = setup_msg.to_bytes().unwrap();
    let mut encoder = TlvEncoder::new();
    let encoded = encoder.encode(setup_msg.message_type(), &payload).unwrap();
    
    // Decode message
    let (tag, decoded_payload) = TlvDecoder::decode_raw(&encoded[4..]).unwrap(); // Skip length prefix
    assert_eq!(tag, message_types::SETUP_CONNECTION);
    
    let decoded_msg = SetupConnectionMessage::from_bytes(&decoded_payload).unwrap();
    assert_eq!(decoded_msg.protocol_version, 2);
    assert_eq!(decoded_msg.endpoint, "test-miner");
}

#[tokio::test]
async fn test_error_responses() {
    let ctx = bllvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    
    let node_api = Arc::new(MockNodeAPI::new());
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();
    
    // Test invalid protocol version
    let setup_msg = SetupConnectionMessage {
        protocol_version: 1, // Invalid
        endpoint: "test-miner".to_string(),
        capabilities: vec!["mining".to_string()],
    };
    
    let data = setup_msg.to_bytes().unwrap();
    let mut encoder = TlvEncoder::new();
    let encoded = encoder.encode(setup_msg.message_type(), &data).unwrap();
    
    // Handle message - should return error response
    let response = server.handle_message(encoded[4..].to_vec(), "test-miner".to_string()).await;
    assert!(response.is_ok());
    
    // Decode response
    let (tag, payload) = TlvDecoder::decode_raw(&response.unwrap()[4..]).unwrap();
    assert_eq!(tag, message_types::SETUP_CONNECTION_ERROR);
    
    let error_msg = SetupConnectionErrorMessage::from_bytes(&payload).unwrap();
    assert_eq!(error_msg.error_code, 1);
}

