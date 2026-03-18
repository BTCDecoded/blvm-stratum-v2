//! Tests for error handling and edge cases

mod common;

use blvm_stratum_v2::{
    messages::*,
    pool::{ShareData, StratumV2Pool},
    server::StratumV2Server,
    error::StratumV2Error,
};
use blvm_protocol::{Block, BlockHeader};
use common::MockNodeAPI;
use std::sync::Arc;

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
                .as_secs(),
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
                .as_secs(),
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
    let ctx = blvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    
    let node_api = Arc::new(MockNodeAPI::default());
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
                .as_secs(),
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

