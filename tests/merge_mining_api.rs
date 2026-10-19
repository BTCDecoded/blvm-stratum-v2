use blvm_node::module::inter_module::api::ModuleAPI;
use blvm_protocol::{Block, BlockHeader};
use blvm_stratum_v2::module_api::{
    API_METHOD_GET_BLOCK_MERGE_MINING_REWARDS, API_METHOD_GET_MERGE_MINING_TEMPLATE,
    API_METHOD_REGISTER_MERGE_MINING_CHANNEL, API_METHOD_SUBMIT_MERGE_MINING_SHARE,
    StratumV2ModuleAPI,
};
use blvm_stratum_v2::pool::StratumV2Pool;
use std::sync::Arc;
use tokio::sync::RwLock;

fn template_block() -> Block {
    Block {
        header: BlockHeader {
            version: 1,
            prev_block_hash: [0xab; 32],
            merkle_root: [0xcd; 32],
            timestamp: 1_700_000_000,
            bits: 0x1d00ffff,
            nonce: 0,
        },
        transactions: vec![].into_boxed_slice(),
    }
}

#[tokio::test]
async fn merge_mining_methods_use_the_pool() {
    let pool = Arc::new(RwLock::new(StratumV2Pool::new()));
    let api = StratumV2ModuleAPI::new(pool.clone());
    assert!(
        api.list_methods()
            .iter()
            .any(|name| name == API_METHOD_REGISTER_MERGE_MINING_CHANNEL)
    );

    let empty = api
        .handle_request(API_METHOD_GET_MERGE_MINING_TEMPLATE, b"", "caller")
        .await
        .unwrap();
    let empty_json: serde_json::Value = serde_json::from_slice(&empty).unwrap();
    assert!(empty_json.get("template").unwrap().is_null());
    assert!(!String::from_utf8_lossy(&empty).contains("not implemented"));

    let registered = api
        .handle_request(
            API_METHOD_REGISTER_MERGE_MINING_CHANNEL,
            br#"{"endpoint":"miner-a","channel_id":4}"#,
            "caller",
        )
        .await
        .unwrap();
    let registered: serde_json::Value = serde_json::from_slice(&registered).unwrap();
    assert_eq!(registered["channel_id"], 4);
    assert!(
        pool.read().await.miners["miner-a"]
            .channels
            .contains_key(&4)
    );

    pool.write().await.set_template(template_block());
    let template = api
        .handle_request(API_METHOD_GET_MERGE_MINING_TEMPLATE, b"", "caller")
        .await
        .unwrap();
    let template: serde_json::Value = serde_json::from_slice(&template).unwrap();
    assert_eq!(template["prev_hash"], hex::encode([0xabu8; 32]));
    assert_eq!(template["merkle_root"], hex::encode([0xcdu8; 32]));
    assert_eq!(template["timestamp"], 1_700_000_000u64);
    assert_eq!(template["bits"], 0x1d00ffffu64);

    let hit = hex::encode([0u8; 32]);
    let accepted = api
        .handle_request(
            API_METHOD_SUBMIT_MERGE_MINING_SHARE,
            format!(r#"{{"channel_id":4,"hash":"{hit}"}}"#).as_bytes(),
            "caller",
        )
        .await
        .unwrap();
    let accepted: serde_json::Value = serde_json::from_slice(&accepted).unwrap();
    assert_eq!(accepted["accepted"], true);

    let miss = hex::encode([0xffu8; 32]);
    let rejected = api
        .handle_request(
            API_METHOD_SUBMIT_MERGE_MINING_SHARE,
            format!(r#"{{"channel_id":4,"hash":"{miss}"}}"#).as_bytes(),
            "caller",
        )
        .await
        .unwrap();
    let rejected: serde_json::Value = serde_json::from_slice(&rejected).unwrap();
    assert_eq!(rejected["accepted"], false);

    let rewards = api
        .handle_request(
            API_METHOD_GET_BLOCK_MERGE_MINING_REWARDS,
            format!(r#"{{"block_hash":"{hit}"}}"#).as_bytes(),
            "caller",
        )
        .await
        .unwrap();
    let rewards: serde_json::Value = serde_json::from_slice(&rewards).unwrap();
    assert_eq!(rewards["rewards"], serde_json::json!([4]));

    let none = api
        .handle_request(
            API_METHOD_GET_BLOCK_MERGE_MINING_REWARDS,
            format!(r#"{{"block_hash":"{miss}"}}"#).as_bytes(),
            "caller",
        )
        .await
        .unwrap();
    let none: serde_json::Value = serde_json::from_slice(&none).unwrap();
    assert_eq!(none["rewards"], serde_json::json!([]));
}
