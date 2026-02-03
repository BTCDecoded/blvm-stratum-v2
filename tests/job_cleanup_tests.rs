//! Unit tests for job cleanup and connection management

use bllvm_stratum_v2::pool::StratumV2Pool;
use bllvm_protocol::{Block, BlockHeader};
use std::time::{SystemTime, UNIX_EPOCH};

fn create_test_block() -> Block {
    Block {
        header: BlockHeader {
            version: 1,
            prev_block_hash: [0u8; 32],
            merkle_root: [0u8; 32],
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            bits: 0x1d00ffff,
            nonce: 0,
        },
        transactions: vec![].into_boxed_slice(),
    }
}

#[tokio::test]
async fn test_cleanup_old_jobs() {
    let mut pool = StratumV2Pool::new();
    
    // Register miner and open channel
    pool.register_miner("test-miner".to_string());
    pool.open_channel("test-miner", 1, 1).unwrap();
    
    // Set max_jobs to 3
    if let Some(miner) = pool.miners.get_mut("test-miner") {
        if let Some(channel) = miner.channels.get_mut(&1) {
            channel.max_jobs = 3;
        }
    }
    
    // Create 5 jobs
    for _ in 0..5 {
        let block = create_test_block();
        pool.set_template(block);
    }
    
    // Check that we have 5 jobs
    if let Some(miner) = pool.miners.get("test-miner") {
        if let Some(channel) = miner.channels.get(&1) {
            assert_eq!(channel.jobs.len(), 5);
        }
    }
    
    // Cleanup old jobs
    pool.cleanup_old_jobs();
    
    // Should only have 3 jobs left (max_jobs)
    if let Some(miner) = pool.miners.get("test-miner") {
        if let Some(channel) = miner.channels.get(&1) {
            assert_eq!(channel.jobs.len(), 3);
        }
    }
}

#[tokio::test]
async fn test_cleanup_disconnected_miners() {
    let mut pool = StratumV2Pool::new();
    
    // Register two miners
    pool.register_miner("active-miner".to_string());
    pool.register_miner("inactive-miner".to_string());
    
    // Open channels for both
    pool.open_channel("active-miner", 1, 1).unwrap();
    pool.open_channel("inactive-miner", 1, 1).unwrap();
    
    // Simulate recent activity for active miner
    if let Some(miner) = pool.miners.get_mut("active-miner") {
        miner.stats.last_share_time = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );
    }
    
    // Simulate old activity for inactive miner (10 minutes ago)
    if let Some(miner) = pool.miners.get_mut("inactive-miner") {
        miner.stats.last_share_time = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .saturating_sub(600) // 10 minutes ago
        );
    }
    
    // Cleanup with 5 minute timeout
    pool.cleanup_disconnected_miners(300);
    
    // Active miner should still be there
    assert!(pool.miners.contains_key("active-miner"));
    
    // Inactive miner should be removed
    assert!(!pool.miners.contains_key("inactive-miner"));
}

#[tokio::test]
async fn test_cleanup_miner_with_no_shares() {
    let mut pool = StratumV2Pool::new();
    
    // Register miner but don't open channel
    pool.register_miner("no-channel-miner".to_string());
    
    // Cleanup
    pool.cleanup_disconnected_miners(300);
    
    // Miner with no channels should be removed
    assert!(!pool.miners.contains_key("no-channel-miner"));
}

#[tokio::test]
async fn test_cleanup_preserves_active_miners() {
    let mut pool = StratumV2Pool::new();
    
    // Register and activate miner
    pool.register_miner("active".to_string());
    pool.open_channel("active", 1, 1).unwrap();
    
    if let Some(miner) = pool.miners.get_mut("active") {
        miner.stats.last_share_time = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );
    }
    
    // Cleanup
    pool.cleanup_disconnected_miners(300);
    
    // Active miner should still be there
    assert!(pool.miners.contains_key("active"));
}

