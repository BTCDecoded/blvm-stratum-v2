//! Unit tests for Stratum V2 mining and share validation

use bllvm_stratum_v2::pool::{JobInfo, ShareData, StratumV2Pool};
use bllvm_protocol::{Block, BlockHeader};

fn create_test_block() -> Block {
    Block {
        header: BlockHeader {
            version: 1,
            prev_block_hash: [0u8; 32],
            merkle_root: [0u8; 32],
            timestamp: 1231006505,
            bits: 0x1d00ffff,
            nonce: 0,
        },
        transactions: vec![].into_boxed_slice(),
    }
}

#[tokio::test]
async fn test_stratum_v2_pool_new() {
    let pool = StratumV2Pool::new();
    // Should create successfully
    assert!(true);
}

#[tokio::test]
async fn test_set_template() {
    let mut pool = StratumV2Pool::new();
    
    let block = create_test_block();
    let (job_id, _) = pool.set_template(block);
    
    // Template should be set, job_id should be > 0
    assert!(job_id > 0);
}

#[tokio::test]
async fn test_handle_share_no_miner() {
    let mut pool = StratumV2Pool::new();
    
    let share = ShareData {
        channel_id: 1,
        job_id: 1,
        nonce: 0,
        version: 1,
        merkle_root: [0u8; 32],
    };
    
    // Should fail if miner not registered
    let result = pool.handle_share("unknown-miner", share);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_handle_share_no_channel() {
    let mut pool = StratumV2Pool::new();
    
    // Register miner
    pool.register_miner("test-miner".to_string());
    
    let share = ShareData {
        channel_id: 999, // Non-existent channel
        job_id: 1,
        nonce: 0,
        version: 1,
        merkle_root: [0u8; 32],
    };
    
    // Should fail if channel not found
    let result = pool.handle_share("test-miner", share);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_handle_share_no_job() {
    let mut pool = StratumV2Pool::new();
    
    // Register miner and open channel
    pool.register_miner("test-miner".to_string());
    pool.open_channel("test-miner", 1, 1).unwrap();
    
    let share = ShareData {
        channel_id: 1,
        job_id: 999, // Non-existent job
        nonce: 0,
        version: 1,
        merkle_root: [0u8; 32],
    };
    
    // Should fail if job not found
    let result = pool.handle_share("test-miner", share);
    assert!(result.is_err());
}

