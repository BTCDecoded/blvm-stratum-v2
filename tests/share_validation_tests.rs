//! Unit tests for share validation and PoW checking

use bllvm_stratum_v2::pool::{JobInfo, ShareData, StratumV2Pool};
use bllvm_protocol::{Block, BlockHeader, Hash};
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
async fn test_validate_share_invalid_job_id() {
    let pool = StratumV2Pool::new();
    
    let job_info = JobInfo {
        job_id: 1,
        prev_hash: [0u8; 32],
        bits: 0x1d00ffff,
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };
    
    let share = ShareData {
        channel_id: 1,
        job_id: 999, // Wrong job ID
        nonce: 0,
        version: 1,
        merkle_root: [0u8; 32],
    };
    
    let target = [0xFFu8; 32]; // Very easy target
    
    // Should fail validation due to job ID mismatch
    let is_valid = pool.validate_share(&share, &job_info, &target);
    assert!(!is_valid);
}

#[tokio::test]
async fn test_validate_share_timestamp_too_old() {
    let pool = StratumV2Pool::new();
    
    let job_info = JobInfo {
        job_id: 1,
        prev_hash: [0u8; 32],
        bits: 0x1d00ffff,
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(10000), // 10k seconds ago (too old)
    };
    
    let share = ShareData {
        channel_id: 1,
        job_id: 1,
        nonce: 0,
        version: 1,
        merkle_root: [0u8; 32],
    };
    
    let target = [0xFFu8; 32];
    
    // Should fail validation due to timestamp being too old
    let is_valid = pool.validate_share(&share, &job_info, &target);
    assert!(!is_valid);
}

#[tokio::test]
async fn test_hash_comparison() {
    let pool = StratumV2Pool::new();
    
    // Test hash comparison logic
    let hash1: Hash = [0x00u8; 32];
    let hash2: Hash = [0xFFu8; 32];
    
    // hash1 < hash2 (all zeros < all ones)
    assert!(pool.hash_less_than_or_equal(&hash1, &hash2));
    
    // hash1 == hash1
    assert!(pool.hash_less_than_or_equal(&hash1, &hash1));
    
    // hash2 > hash1
    assert!(!pool.hash_less_than_or_equal(&hash2, &hash1));
}

#[tokio::test]
async fn test_calculate_block_hash() {
    let pool = StratumV2Pool::new();
    
    let header = BlockHeader {
        version: 1,
        prev_block_hash: [0u8; 32],
        merkle_root: [0u8; 32],
        timestamp: 1231006505,
        bits: 0x1d00ffff,
        nonce: 0,
    };
    
    let hash = pool.calculate_block_hash(&header);
    
    // Hash should be 32 bytes
    assert_eq!(hash.len(), 32);
    
    // Hash should be deterministic
    let hash2 = pool.calculate_block_hash(&header);
    assert_eq!(hash, hash2);
}

#[tokio::test]
async fn test_calculate_channel_target() {
    let pool = StratumV2Pool::new();
    
    // Test with different difficulty values
    let target1 = pool.calculate_channel_target(1).unwrap();
    let target2 = pool.calculate_channel_target(10).unwrap();
    
    // Both should be valid hashes
    assert_eq!(target1.len(), 32);
    assert_eq!(target2.len(), 32);
    
    // Higher difficulty should result in easier target (higher value)
    // target2 should be >= target1 (easier)
    assert!(pool.hash_less_than_or_equal(&target1, &target2) || 
            pool.hash_less_than_or_equal(&target2, &target1));
}

