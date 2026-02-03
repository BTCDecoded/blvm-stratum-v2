//! Tests for concurrent operations and thread safety

use bllvm_stratum_v2::pool::{ShareData, StratumV2Pool};
use bllvm_protocol::{Block, BlockHeader};
use std::sync::Arc;
use tokio::sync::RwLock;
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
async fn test_concurrent_miner_registration() {
    let pool = Arc::new(RwLock::new(StratumV2Pool::new()));
    
    // Spawn multiple tasks to register miners concurrently
    let mut handles = Vec::new();
    for i in 0..10 {
        let pool_clone = Arc::clone(&pool);
        let handle = tokio::spawn(async move {
            let mut pool = pool_clone.write().await;
            pool.register_miner(format!("miner-{}", i));
        });
        handles.push(handle);
    }
    
    // Wait for all registrations
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Verify all miners registered
    let pool = pool.read().await;
    assert_eq!(pool.miners.len(), 10);
    for i in 0..10 {
        assert!(pool.miners.contains_key(&format!("miner-{}", i)));
    }
}

#[tokio::test]
async fn test_concurrent_channel_opens() {
    let pool = Arc::new(RwLock::new(StratumV2Pool::new()));
    
    // Register a miner
    {
        let mut pool = pool.write().await;
        pool.register_miner("test-miner".to_string());
    }
    
    // Spawn multiple tasks to open channels concurrently
    let mut handles = Vec::new();
    for i in 0..5 {
        let pool_clone = Arc::clone(&pool);
        let handle = tokio::spawn(async move {
            let mut pool = pool_clone.write().await;
            pool.open_channel("test-miner", i, 1).unwrap();
        });
        handles.push(handle);
    }
    
    // Wait for all channel opens
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Verify all channels opened
    let pool = pool.read().await;
    let miner = pool.miners.get("test-miner").unwrap();
    assert_eq!(miner.channels.len(), 5);
}

#[tokio::test]
async fn test_concurrent_share_submissions() {
    let pool = Arc::new(RwLock::new(StratumV2Pool::new()));
    
    // Setup: register miner, open channel, create job
    {
        let mut pool = pool.write().await;
        pool.register_miner("test-miner".to_string());
        pool.open_channel("test-miner", 1, 1).unwrap();
        let block = create_test_block();
        pool.set_template(block);
    }
    
    // Spawn multiple tasks to submit shares concurrently
    let mut handles = Vec::new();
    for i in 0..10 {
        let pool_clone = Arc::clone(&pool);
        let handle = tokio::spawn(async move {
            let mut pool = pool_clone.write().await;
            let share = ShareData {
                channel_id: 1,
                job_id: 1,
                nonce: i,
                version: 1,
                merkle_root: [0u8; 32],
            };
            let _ = pool.handle_share("test-miner", share);
        });
        handles.push(handle);
    }
    
    // Wait for all share submissions
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Verify shares were processed
    let pool = pool.read().await;
    let miner = pool.miners.get("test-miner").unwrap();
    assert_eq!(miner.stats.total_shares, 10);
}

#[tokio::test]
async fn test_concurrent_template_updates() {
    let pool = Arc::new(RwLock::new(StratumV2Pool::new()));
    
    // Register miner and open channel
    {
        let mut pool = pool.write().await;
        pool.register_miner("test-miner".to_string());
        pool.open_channel("test-miner", 1, 1).unwrap();
    }
    
    // Spawn multiple tasks to update templates concurrently
    let mut handles = Vec::new();
    for i in 0..5 {
        let pool_clone = Arc::clone(&pool);
        let handle = tokio::spawn(async move {
            let mut pool = pool_clone.write().await;
            let mut block = create_test_block();
            block.header.prev_block_hash[0] = i as u8;
            pool.set_template(block);
        });
        handles.push(handle);
    }
    
    // Wait for all template updates
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Verify template was updated (should have latest job)
    let pool = pool.read().await;
    let miner = pool.miners.get("test-miner").unwrap();
    let channel = miner.channels.get(&1).unwrap();
    // Should have multiple jobs (up to max_jobs)
    assert!(channel.jobs.len() > 0);
}

#[tokio::test]
async fn test_read_write_concurrency() {
    let pool = Arc::new(RwLock::new(StratumV2Pool::new()));
    
    // Setup
    {
        let mut pool = pool.write().await;
        pool.register_miner("test-miner".to_string());
        pool.open_channel("test-miner", 1, 1).unwrap();
    }
    
    // Spawn reader and writer tasks
    let pool_clone = Arc::clone(&pool);
    let reader = tokio::spawn(async move {
        for _ in 0..100 {
            let pool = pool_clone.read().await;
            let _ = pool.miners.get("test-miner");
            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        }
    });
    
    let pool_clone = Arc::clone(&pool);
    let writer = tokio::spawn(async move {
        for i in 0..10 {
            let mut pool = pool_clone.write().await;
            let block = create_test_block();
            pool.set_template(block);
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    });
    
    // Wait for both tasks
    tokio::try_join!(reader, writer).unwrap();
    
    // Verify final state
    let pool = pool.read().await;
    assert!(pool.miners.contains_key("test-miner"));
}

#[tokio::test]
async fn test_multiple_miners_concurrent() {
    let pool = Arc::new(RwLock::new(StratumV2Pool::new()));
    
    // Spawn tasks for multiple miners
    let mut handles = Vec::new();
    for miner_id in 0..5 {
        let pool_clone = Arc::clone(&pool);
        let handle = tokio::spawn(async move {
            let mut pool = pool_clone.write().await;
            let miner_name = format!("miner-{}", miner_id);
            pool.register_miner(miner_name.clone());
            pool.open_channel(&miner_name, 1, 1).unwrap();
            
            // Submit shares
            let share = ShareData {
                channel_id: 1,
                job_id: 1,
                nonce: miner_id,
                version: 1,
                merkle_root: [0u8; 32],
            };
            let _ = pool.handle_share(&miner_name, share);
        });
        handles.push(handle);
    }
    
    // Wait for all miners
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Verify all miners processed
    let pool = pool.read().await;
    assert_eq!(pool.miners.len(), 5);
    for miner_id in 0..5 {
        let miner = pool.miners.get(&format!("miner-{}", miner_id)).unwrap();
        assert_eq!(miner.stats.total_shares, 1);
    }
}

