//! Unit tests for merkle path calculation and coinbase splitting

mod common;

use blvm_stratum_v2::server::StratumV2Server;
use blvm_protocol::{Block, BlockHeader, Transaction, OutPoint, TransactionInput, TransactionOutput};
use common::MockNodeAPI;
use std::sync::Arc;

fn create_test_block_with_transactions(tx_count: usize) -> Block {
    let mut transactions = Vec::new();
    
    // Create coinbase transaction
    let coinbase = Transaction {
        version: 1,
        inputs: vec![TransactionInput {
            prevout: OutPoint {
                hash: [0u8; 32],
                index: 0xFFFFFFFF,
            },
            script_sig: vec![blvm_consensus::opcodes::PUSH_1_BYTE, 0x01],
            sequence: 0xFFFFFFFF,
        }].into(),
        outputs: vec![TransactionOutput {
            value: 5000000000,
            script_pubkey: {
                let mut s = vec![
                    blvm_consensus::opcodes::OP_DUP,
                    blvm_consensus::opcodes::OP_HASH160,
                    blvm_consensus::opcodes::PUSH_20_BYTES,
                ];
                s.extend_from_slice(&[0u8; 20]);
                s.push(blvm_consensus::opcodes::OP_EQUALVERIFY);
                s.push(blvm_consensus::opcodes::OP_CHECKSIG);
                s
            },
        }].into(),
        lock_time: 0,
    };
    transactions.push(coinbase);
    
    // Create additional transactions
    for _ in 0..tx_count {
        let tx = Transaction {
            version: 1,
            inputs: vec![TransactionInput {
                prevout: OutPoint {
                    hash: [0x01u8; 32],
                    index: 0,
                },
                script_sig: vec![0x01, 0x02],
                sequence: 0xFFFFFFFF,
            }].into(),
            outputs: vec![TransactionOutput {
                value: 1000000,
                script_pubkey: vec![
                    blvm_consensus::opcodes::OP_DUP,
                    blvm_consensus::opcodes::OP_HASH160,
                    blvm_consensus::opcodes::PUSH_20_BYTES,
                ]
                .into_iter()
                .chain(std::iter::repeat(0x00).take(20))
                .chain([blvm_consensus::opcodes::OP_EQUALVERIFY, blvm_consensus::opcodes::OP_CHECKSIG])
                .collect::<Vec<_>>(),
            }].into(),
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
    let ctx = blvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    
    let node_api = Arc::new(MockNodeAPI::default());
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
    let ctx = blvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    
    let node_api = Arc::new(MockNodeAPI::default());
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
    let ctx = blvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    
    let node_api = Arc::new(MockNodeAPI::default());
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
    let ctx = blvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    
    let node_api = Arc::new(MockNodeAPI::default());
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();
    
    let tx = Transaction {
        version: 1,
        inputs: vec![TransactionInput {
            prevout: OutPoint {
                hash: [0u8; 32],
                index: 0,
            },
            script_sig: vec![0x01, 0x02],
            sequence: 0xFFFFFFFF,
        }].into(),
        outputs: vec![TransactionOutput {
            value: 1000000,
            script_pubkey: vec![
                blvm_consensus::opcodes::OP_DUP,
                blvm_consensus::opcodes::OP_HASH160,
            ],
        }].into(),
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
    let ctx = blvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    
    let node_api = Arc::new(MockNodeAPI::default());
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
    let ctx = blvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    
    let node_api = Arc::new(MockNodeAPI::default());
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

