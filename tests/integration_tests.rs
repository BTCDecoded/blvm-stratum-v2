//! Integration tests for Stratum V2 protocol flow

mod common;

use blvm_protocol::block::calculate_tx_id;
use blvm_protocol::serialization::serialize_transaction;
use blvm_protocol::{
    Block, BlockHeader, OutPoint, Transaction, TransactionInput, TransactionOutput,
};
use blvm_stratum_v2::{
    messages::{self, *},
    protocol::{decode_sv2_frame, TlvEncoder},
    server::StratumV2Server,
};
use common::SubmittingMockNodeAPI;
use std::sync::Arc;

fn create_test_block() -> Block {
    Block {
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
        transactions: vec![Transaction {
            version: 1,
            inputs: vec![TransactionInput {
                prevout: OutPoint {
                    hash: [0u8; 32],
                    index: 0xFFFFFFFF,
                },
                script_sig: vec![blvm_consensus::opcodes::PUSH_1_BYTE, 0x00],
                sequence: 0xFFFFFFFF,
            }]
            .into(),
            outputs: vec![TransactionOutput {
                value: 5000000000,
                script_pubkey: vec![
                    blvm_consensus::opcodes::OP_DUP,
                    blvm_consensus::opcodes::OP_HASH160,
                    blvm_consensus::opcodes::PUSH_20_BYTES,
                ],
            }]
            .into(),
            lock_time: 0,
        }]
        .into_boxed_slice(),
    }
}

#[tokio::test]
async fn test_setup_connection_flow() {
    let ctx = blvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };

    let node_api = Arc::new(SubmittingMockNodeAPI::default());
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();

    // Create Setup Connection message
    let setup_msg = SetupConnectionMessage {
        protocol_version: 2,
        endpoint: "test-miner".to_string(),
        capabilities: vec!["mining".to_string()],
    };

    // Handle message
    let response = server
        .handle_setup_connection(setup_msg, "test-miner")
        .await;

    assert!(response.is_ok());
    let success = response.unwrap();
    assert_eq!(success.supported_versions, vec![2]);
    assert!(success.capabilities.contains(&"mining".to_string()));
}

#[tokio::test]
async fn test_setup_connection_invalid_version() {
    let ctx = blvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };

    let node_api = Arc::new(SubmittingMockNodeAPI::default());
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();

    // Create Setup Connection message with invalid version
    let setup_msg = SetupConnectionMessage {
        protocol_version: 1, // Invalid version
        endpoint: "test-miner".to_string(),
        capabilities: vec!["mining".to_string()],
    };

    // Handle message - should fail
    let response = server
        .handle_setup_connection(setup_msg, "test-miner")
        .await;
    assert!(response.is_err());
}

#[tokio::test]
async fn test_open_channel_flow() {
    let ctx = blvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };

    let node_api = Arc::new(SubmittingMockNodeAPI::default());
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();

    // First setup connection
    let setup_msg = SetupConnectionMessage {
        protocol_version: 2,
        endpoint: "test-miner".to_string(),
        capabilities: vec!["mining".to_string()],
    };
    server
        .handle_setup_connection(setup_msg, "test-miner")
        .await
        .unwrap();

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
    let ctx = blvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };

    let node_api = Arc::new(SubmittingMockNodeAPI::default());
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
    let ctx = blvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };

    let node_api = Arc::new(SubmittingMockNodeAPI::default());
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();

    // Setup connection
    let setup_msg = SetupConnectionMessage {
        protocol_version: 2,
        endpoint: "test-miner".to_string(),
        capabilities: vec!["mining".to_string()],
    };
    server
        .handle_setup_connection(setup_msg, "test-miner")
        .await
        .unwrap();

    // Open channel
    let open_msg = OpenMiningChannelMessage {
        channel_id: 1,
        request_id: 1,
        min_difficulty: 1,
    };
    server
        .handle_open_channel("test-miner", open_msg)
        .await
        .unwrap();

    // Set template to create a job
    let block = create_test_block();
    let pool = server.get_pool();
    pool.write().await.set_template(block);

    // Submit shares
    let submit_msg = SubmitSharesMessage {
        channel_id: 1,
        shares: vec![messages::ShareData {
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
async fn test_binary_format_not_json() {
    // Verify we use binary encoding, not JSON
    let setup_msg = SetupConnectionMessage {
        protocol_version: 2,
        endpoint: "test-miner".to_string(),
        capabilities: vec!["mining".to_string()],
    };
    let payload = setup_msg.to_bytes().unwrap();
    assert!(
        !payload.starts_with(b"{") && !payload.starts_with(b"["),
        "Payload should be binary, not JSON"
    );
}

#[tokio::test]
async fn test_message_encoding_decoding() {
    // Test full message encoding/decoding flow (binary format)
    let setup_msg = SetupConnectionMessage {
        protocol_version: 2,
        endpoint: "test-miner".to_string(),
        capabilities: vec!["mining".to_string()],
    };

    // Encode message
    let payload = setup_msg.to_bytes().unwrap();
    let mut encoder = TlvEncoder::new();
    let encoded = encoder.encode(setup_msg.message_type(), &payload).unwrap();

    let (tag, decoded_payload) = decode_sv2_frame(&encoded).unwrap();
    assert_eq!(tag, message_types::SETUP_CONNECTION);

    let decoded_msg = SetupConnectionMessage::from_bytes(&decoded_payload).unwrap();
    assert_eq!(decoded_msg.protocol_version, 2);
    assert_eq!(decoded_msg.endpoint, "test-miner");
}

#[tokio::test]
async fn test_error_responses() {
    let ctx = blvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };

    let node_api = Arc::new(SubmittingMockNodeAPI::default());
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

    let response = server
        .handle_message(encoded, "test-miner".to_string())
        .await;
    assert!(response.is_ok());

    let (tag, payload) = decode_sv2_frame(&response.unwrap()).unwrap();
    assert_eq!(tag, message_types::SETUP_CONNECTION_ERROR);

    let error_msg = SetupConnectionErrorMessage::from_bytes(&payload).unwrap();
    assert_eq!(error_msg.error_code, 1);
}

fn encode_inner(tag: u16, msg: &impl StratumV2Message) -> Vec<u8> {
    let payload = msg.to_bytes().unwrap();
    TlvEncoder::new().encode(tag, &payload).unwrap()
}

fn decode_response(wire: &[u8]) -> (u16, Vec<u8>) {
    decode_sv2_frame(wire).unwrap()
}

fn sample_declared_tx() -> (Vec<u8>, [u8; 32]) {
    let tx = Transaction {
        version: 1,
        inputs: vec![TransactionInput {
            prevout: OutPoint {
                hash: [0x11u8; 32],
                index: 0,
            },
            script_sig: vec![],
            sequence: 0xFFFFFFFF,
        }]
        .into(),
        outputs: vec![TransactionOutput {
            value: 1000,
            script_pubkey: vec![0x51],
        }]
        .into(),
        lock_time: 0,
    };
    (serialize_transaction(&tx), calculate_tx_id(&tx))
}

#[tokio::test]
async fn provide_missing_from_other_endpoint_is_rejected() {
    let ctx = blvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    let node_api = Arc::new(SubmittingMockNodeAPI::default());
    let server = StratumV2Server::new(&ctx, node_api.clone()).await.unwrap();

    let alloc = AllocateMiningJobTokenMessage {
        request_id: 1,
        user_identifier: "alice".into(),
    };
    let alloc_resp = server
        .handle_message(
            encode_inner(message_types::ALLOCATE_MINING_JOB_TOKEN, &alloc),
            "alice".into(),
        )
        .await
        .unwrap();
    let (tag, payload) = decode_response(&alloc_resp);
    assert_eq!(tag, message_types::ALLOCATE_MINING_JOB_TOKEN_SUCCESS);
    let token = AllocateMiningJobTokenSuccessMessage::from_bytes(&payload)
        .unwrap()
        .mining_job_token;

    let (body, txid) = sample_declared_tx();
    let declare = DeclareMiningJobMessage {
        request_id: 7,
        mining_job_token: token,
        version: 1,
        tx_id_list: vec![txid],
    };
    let declare_resp = server
        .handle_message(
            encode_inner(message_types::DECLARE_MINING_JOB, &declare),
            "alice".into(),
        )
        .await
        .unwrap();
    let (tag, payload) = decode_response(&declare_resp);
    assert_eq!(tag, message_types::PROVIDE_MISSING_TRANSACTIONS);
    let miss = ProvideMissingTransactionsMessage::from_bytes(&payload).unwrap();
    assert_eq!(miss.unknown_tx_position_list, vec![0]);

    let provide = ProvideMissingTransactionsSuccessMessage {
        request_id: 7,
        transaction_list: vec![body.clone()],
    };
    let bob = server
        .handle_message(
            encode_inner(message_types::PROVIDE_MISSING_TRANSACTIONS_SUCCESS, &provide),
            "bob".into(),
        )
        .await
        .unwrap();
    let (tag, payload) = decode_response(&bob);
    assert_eq!(tag, message_types::DECLARE_MINING_JOB_ERROR);
    let err = DeclareMiningJobErrorMessage::from_bytes(&payload).unwrap();
    assert!(
        err.error_message.contains("this connection"),
        "{}",
        err.error_message
    );

    let alice = server
        .handle_message(
            encode_inner(message_types::PROVIDE_MISSING_TRANSACTIONS_SUCCESS, &provide),
            "alice".into(),
        )
        .await
        .unwrap();
    let (tag, _) = decode_response(&alice);
    assert_eq!(tag, message_types::DECLARE_MINING_JOB_SUCCESS);
    assert_eq!(*node_api.ingested_txids.read().await, vec![txid]);
}

#[tokio::test]
async fn provide_missing_mempool_reject_does_not_accept() {
    let ctx = blvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    let node_api = Arc::new(SubmittingMockNodeAPI::default());
    node_api
        .reject_mempool
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let server = StratumV2Server::new(&ctx, node_api.clone()).await.unwrap();

    let alloc = AllocateMiningJobTokenMessage {
        request_id: 1,
        user_identifier: "alice".into(),
    };
    let alloc_resp = server
        .handle_message(
            encode_inner(message_types::ALLOCATE_MINING_JOB_TOKEN, &alloc),
            "alice".into(),
        )
        .await
        .unwrap();
    let (tag, payload) = decode_response(&alloc_resp);
    assert_eq!(tag, message_types::ALLOCATE_MINING_JOB_TOKEN_SUCCESS);
    let token = AllocateMiningJobTokenSuccessMessage::from_bytes(&payload)
        .unwrap()
        .mining_job_token;

    let (body, txid) = sample_declared_tx();
    let declare = DeclareMiningJobMessage {
        request_id: 8,
        mining_job_token: token,
        version: 1,
        tx_id_list: vec![txid],
    };
    let declare_resp = server
        .handle_message(
            encode_inner(message_types::DECLARE_MINING_JOB, &declare),
            "alice".into(),
        )
        .await
        .unwrap();
    let (tag, _) = decode_response(&declare_resp);
    assert_eq!(tag, message_types::PROVIDE_MISSING_TRANSACTIONS);

    let provide = ProvideMissingTransactionsSuccessMessage {
        request_id: 8,
        transaction_list: vec![body],
    };
    let alice = server
        .handle_message(
            encode_inner(message_types::PROVIDE_MISSING_TRANSACTIONS_SUCCESS, &provide),
            "alice".into(),
        )
        .await
        .unwrap();
    let (tag, payload) = decode_response(&alice);
    assert_eq!(tag, message_types::DECLARE_MINING_JOB_ERROR);
    let err = DeclareMiningJobErrorMessage::from_bytes(&payload).unwrap();
    assert!(
        err.error_message.contains("mempool"),
        "{}",
        err.error_message
    );
    assert!(node_api.ingested_txids.read().await.is_empty());
}
