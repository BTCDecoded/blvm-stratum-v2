//! Tests for Stratum V2 Pool

use blvm_protocol::{Block, BlockHeader};
use blvm_stratum_v2::pool::{MinerStats, StratumV2Pool};

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
    let _pool = StratumV2Pool::new();
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
async fn test_set_template_for_does_not_pin_other_miners() {
    let mut pool = StratumV2Pool::new();
    pool.register_miner("alice".to_string());
    pool.register_miner("bob".to_string());
    pool.open_channel("alice", 1, 1).unwrap();
    pool.open_channel("bob", 2, 1).unwrap();

    let mut alice_block = create_test_block();
    alice_block.header.merkle_root = [0xaa; 32];
    let (job_id, dist) = pool.set_template_for("alice", alice_block);
    assert_eq!(dist, vec![("alice".to_string(), 1)]);
    assert_eq!(
        pool.miners["alice"].channels[&1].current_job_id,
        Some(job_id)
    );
    assert!(pool.miners["bob"].channels[&2].current_job_id.is_none());
    assert!(pool.template_for("alice").is_some());
    assert!(pool.template_for("bob").is_none());
}

#[tokio::test]
async fn test_set_template_clears_per_miner_declared_job() {
    let mut pool = StratumV2Pool::new();
    pool.register_miner("alice".to_string());
    pool.register_miner("bob".to_string());
    pool.open_channel("alice", 1, 1).unwrap();
    pool.open_channel("bob", 2, 1).unwrap();

    let mut alice_block = create_test_block();
    alice_block.header.merkle_root = [0xaa; 32];
    pool.set_template_for("alice", alice_block);
    assert_eq!(
        pool.template_for("alice").unwrap().header.merkle_root,
        [0xaa; 32]
    );

    let broadcast = create_test_block();
    let (job_id, dist) = pool.set_template(broadcast);
    assert_eq!(dist.len(), 2);
    assert_eq!(
        pool.template_for("alice").unwrap().header.merkle_root,
        [0u8; 32]
    );
    assert_eq!(
        pool.template_for("bob").unwrap().header.merkle_root,
        [0u8; 32]
    );
    assert_eq!(
        pool.miners["alice"].channels[&1].current_job_id,
        Some(job_id)
    );
    assert_eq!(pool.miners["bob"].channels[&2].current_job_id, Some(job_id));
}

#[tokio::test]
async fn test_miner_stats_default() {
    let stats = MinerStats::default();
    assert_eq!(stats.total_shares, 0);
    assert_eq!(stats.accepted_shares, 0);
    assert_eq!(stats.rejected_shares, 0);
    assert!(stats.last_share_time.is_none());
}

#[tokio::test]
async fn test_set_template_skips_banned_miner() {
    let mut pool = StratumV2Pool::new();
    pool.register_miner("honest".to_string());
    pool.register_miner("rogue".to_string());
    pool.open_channel("honest", 1, 1).unwrap();
    pool.open_channel("rogue", 2, 1).unwrap();
    pool.set_banned_miners([StratumV2Pool::commons_miner_id("rogue")]);

    let (job_id, dist) = pool.set_template(create_test_block());
    assert_eq!(dist.len(), 1);
    assert_eq!(dist[0].0, "honest");
    assert_eq!(
        pool.miners["honest"].channels[&1].current_job_id,
        Some(job_id)
    );
    assert_eq!(pool.miners["rogue"].channels[&2].current_job_id, None);

    let (_, dist) = pool.set_template_for("rogue", create_test_block());
    assert!(dist.is_empty());
    assert!(pool.endpoint_is_banned("rogue"));
    assert!(!pool.endpoint_is_banned("honest"));
}
