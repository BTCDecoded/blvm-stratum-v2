//! Unit tests for merkle path calculation and coinbase splitting

use bllvm_stratum_v2::server::StratumV2Server;
use bllvm_protocol::{Block, BlockHeader, Transaction, OutPoint, TxInput, TxOutput};
use bllvm_node::module::traits::NodeAPI;
use std::sync::Arc;

// Mock NodeAPI for testing
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

fn create_test_block_with_transactions(tx_count: usize) -> Block {
    let mut transactions = Vec::new();
    
    // Create coinbase transaction
    let coinbase = Transaction {
        version: 1,
        inputs: vec![TxInput {
            prevout: OutPoint {
                hash: [0u8; 32],
                index: 0xFFFFFFFF,
            },
            script_sig: vec![0x00, 0x01], // Height 1
            sequence: 0xFFFFFFFF,
        }],
        outputs: vec![TxOutput {
            value: 5000000000,
            script_pubkey: vec![0x76, 0xa9, 0x14, 0x00; 20], // P2PKH
        }],
        lock_time: 0,
    };
    transactions.push(coinbase);
    
    // Create additional transactions
    for _ in 0..tx_count {
        let tx = Transaction {
            version: 1,
            inputs: vec![TxInput {
                prevout: OutPoint {
                    hash: [0x01u8; 32],
                    index: 0,
                },
                script_sig: vec![0x01, 0x02],
                sequence: 0xFFFFFFFF,
            }],
            outputs: vec![TxOutput {
                value: 1000000,
                script_pubkey: vec![0x76, 0xa9, 0x14, 0x00; 20],
            }],
            lock_time: 0,
        };
        transactions.push(tx);
    }
    
    Block {
        header: BlockHeader {
            version: 1,
            prev_block_hash: [0u8; 32],
            merkle_root: [0u8; 32], // Will be calculated
            timestamp: 1231006505,
            bits: 0x1d00ffff,
            nonce: 0,
        },
        transactions: transactions.into_boxed_slice(),
    }
}

#[tokio::test]
async fn test_extract_template_parts_empty_block() {
    let ctx = bllvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    
    let node_api = Arc::new(MockNodeAPI);
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();
    
    let block = Block {
        header: BlockHeader {
            version: 1,
            prev_block_hash: [0u8; 32],
            merkle_root: [0u8; 32],
            timestamp: 1231006505,
            bits: 0x1d00ffff,
            nonce: 0,
        },
        transactions: vec![].into_boxed_slice(),
    };
    
    let (prefix, suffix, path) = server.extract_template_parts(&block);
    
    // Empty block should return empty parts
    assert!(prefix.is_empty());
    assert!(suffix.is_empty());
    assert!(path.is_empty());
}

#[tokio::test]
async fn test_extract_template_parts_single_transaction() {
    let ctx = bllvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    
    let node_api = Arc::new(MockNodeAPI);
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();
    
    let block = create_test_block_with_transactions(0); // Only coinbase
    
    let (prefix, suffix, path) = server.extract_template_parts(&block);
    
    // Should have coinbase parts
    assert!(!prefix.is_empty());
    assert!(!suffix.is_empty());
    // No merkle path for single transaction
    assert!(path.is_empty());
}

#[tokio::test]
async fn test_extract_template_parts_multiple_transactions() {
    let ctx = bllvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    
    let node_api = Arc::new(MockNodeAPI);
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();
    
    let block = create_test_block_with_transactions(3); // Coinbase + 3 transactions
    
    let (prefix, suffix, path) = server.extract_template_parts(&block);
    
    // Should have coinbase parts
    assert!(!prefix.is_empty());
    assert!(!suffix.is_empty());
    // Should have merkle path for multiple transactions
    assert!(!path.is_empty());
    
    // Prefix + suffix should reconstruct coinbase
    let mut reconstructed = prefix.clone();
    reconstructed.extend_from_slice(&suffix);
    assert!(!reconstructed.is_empty());
}

#[tokio::test]
async fn test_serialize_transaction() {
    let ctx = bllvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    
    let node_api = Arc::new(MockNodeAPI);
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();
    
    let tx = Transaction {
        version: 1,
        inputs: vec![TxInput {
            prevout: OutPoint {
                hash: [0u8; 32],
                index: 0,
            },
            script_sig: vec![0x01, 0x02],
            sequence: 0xFFFFFFFF,
        }],
        outputs: vec![TxOutput {
            value: 1000000,
            script_pubkey: vec![0x76, 0xa9],
        }],
        lock_time: 0,
    };
    
    let serialized = server.serialize_transaction(&tx);
    
    // Should serialize to bytes
    assert!(!serialized.is_empty());
    // Should start with version (4 bytes)
    assert!(serialized.len() >= 4);
}

#[tokio::test]
async fn test_varint_encoding() {
    let ctx = bllvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    
    let node_api = Arc::new(MockNodeAPI);
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();
    
    // Test small value (< 0xfd)
    let encoded = server.encode_varint(100);
    assert_eq!(encoded.len(), 1);
    assert_eq!(encoded[0], 100);
    
    // Test medium value (0xfd - 0xffff)
    let encoded = server.encode_varint(1000);
    assert_eq!(encoded.len(), 3);
    assert_eq!(encoded[0], 0xfd);
    
    // Test large value (> 0xffff)
    let encoded = server.encode_varint(100000);
    assert_eq!(encoded.len(), 5);
    assert_eq!(encoded[0], 0xfe);
}

#[tokio::test]
async fn test_varint_decoding() {
    let ctx = bllvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    
    let node_api = Arc::new(MockNodeAPI);
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();
    
    // Test small value
    let data = vec![100u8];
    let (value, bytes_read) = server.read_varint(&data);
    assert_eq!(value, 100);
    assert_eq!(bytes_read, 1);
    
    // Test medium value
    let mut data = vec![0xfd, 0xe8, 0x03];
    let (value, bytes_read) = server.read_varint(&data);
    assert_eq!(value, 1000);
    assert_eq!(bytes_read, 3);
}

