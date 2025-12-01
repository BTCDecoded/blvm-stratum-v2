//! Tests for Stratum V2 Pool

use bllvm_stratum_v2::messages::{
    OpenMiningChannelMessage, SetupConnectionMessage,
};
use bllvm_stratum_v2::pool::{MinerStats, StratumV2Pool};
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
async fn test_stratum_v2_pool_register_miner() {
    let mut pool = StratumV2Pool::new();
    
    pool.register_miner("test-miner".to_string());
    
    // Miner should be registered
    assert!(pool.miners.contains_key("test-miner"));
}

#[tokio::test]
async fn test_stratum_v2_pool_open_channel() {
    let mut pool = StratumV2Pool::new();
    
    // First register miner
    pool.register_miner("test-miner".to_string());
    
    // Then open channel
    let result = pool.open_channel("test-miner", 1, 1);
    assert!(result.is_ok());
    
    let target = result.unwrap();
    // Target should be non-zero
    assert_ne!(target, [0u8; 32]);
}

#[tokio::test]
async fn test_stratum_v2_pool_open_channel_no_miner() {
    let mut pool = StratumV2Pool::new();
    
    // Should fail if miner not registered
    let result = pool.open_channel("unknown-miner", 1, 1);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_stratum_v2_pool_set_template() {
    let mut pool = StratumV2Pool::new();
    let block = create_test_block();
    
    let (job_id, distributions) = pool.set_template(block);
    
    // Should return job_id and distributions
    assert!(job_id > 0);
    assert!(distributions.is_empty()); // No miners connected yet
}

#[tokio::test]
async fn test_miner_stats_default() {
    let stats = MinerStats::default();
    assert_eq!(stats.total_shares, 0);
    assert_eq!(stats.accepted_shares, 0);
    assert_eq!(stats.rejected_shares, 0);
    assert!(stats.last_share_time.is_none());
}

