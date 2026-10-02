//! Module-owned TCP framing: official 6-byte SV2 header, then payload.
//! Responses are bare frames. A 4-byte little-endian length prefix is rejected.

mod common;

use blvm_protocol::{
    Block, BlockHeader, OutPoint, Transaction, TransactionInput, TransactionOutput,
};
use blvm_stratum_v2::{
    messages::{
        self, OpenMiningChannelMessage, SetupConnectionMessage, StratumV2Message,
        SubmitSharesMessage, message_types,
    },
    protocol::{parse_sv2_header, TlvEncoder},
    server::StratumV2Server,
};
use common::SubmittingMockNodeAPI;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn test_ctx_listen_random_port() -> blvm_node::module::traits::ModuleContext {
    blvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: HashMap::from([(
            "stratum_v2.listen_addr".to_string(),
            "127.0.0.1:0".to_string(),
        )]),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    }
}

fn encode_tlv(tag: u16, msg: &impl StratumV2Message) -> Vec<u8> {
    let payload = msg.to_bytes().unwrap();
    TlvEncoder::new().encode(tag, &payload).unwrap()
}

async fn read_framed_response(stream: &mut TcpStream) -> Vec<u8> {
    let mut hdr = [0u8; 6];
    stream.read_exact(&mut hdr).await.unwrap();
    let (_typ, len) = parse_sv2_header(&hdr).unwrap();
    let mut frame = vec![0u8; 6 + len];
    frame[..6].copy_from_slice(&hdr);
    if len > 0 {
        stream.read_exact(&mut frame[6..]).await.unwrap();
    }
    frame
}

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
                value: 5_000_000_000,
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
async fn module_tcp_setup_roundtrip_matches_handle_message_bytes() {
    let ctx = test_ctx_listen_random_port();
    let node_api = Arc::new(SubmittingMockNodeAPI::default());
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();
    server.start().await.unwrap();

    let bind = server
        .module_tcp_local_addr()
        .await
        .expect("module TCP should bind on 127.0.0.1:0");

    let setup = SetupConnectionMessage {
        protocol_version: 2,
        endpoint: "tcp-parity-miner".to_string(),
        capabilities: vec!["mining".to_string()],
    };
    let request_wire = encode_tlv(message_types::SETUP_CONNECTION, &setup);

    let mut stream = TcpStream::connect(bind).await.unwrap();
    let endpoint = stream.local_addr().unwrap().to_string();

    let expected_response = server
        .handle_message(request_wire.clone(), endpoint)
        .await
        .unwrap();

    stream.write_all(&request_wire).await.unwrap();

    let from_tcp = read_framed_response(&mut stream).await;

    assert_eq!(
        from_tcp, expected_response,
        "TCP response should match direct handle_message output (node listener uses same path)"
    );
}

/// Same connection: setup then open channel — `endpoint` for pool state is the peer [`SocketAddr`].
#[tokio::test]
async fn module_tcp_setup_then_open_channel_matches_sequential_handle_message() {
    let ctx = test_ctx_listen_random_port();
    let node_api = Arc::new(SubmittingMockNodeAPI::default());
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();
    server.start().await.unwrap();

    let bind = server.module_tcp_local_addr().await.unwrap();

    let mut stream = TcpStream::connect(bind).await.unwrap();
    let endpoint = stream.local_addr().unwrap().to_string();

    let setup = SetupConnectionMessage {
        protocol_version: 2,
        endpoint: "parity-miner-id".to_string(),
        capabilities: vec!["mining".to_string()],
    };
    let setup_wire = encode_tlv(message_types::SETUP_CONNECTION, &setup);
    let exp_setup = server
        .handle_message(setup_wire.clone(), endpoint.clone())
        .await
        .unwrap();

    stream.write_all(&setup_wire).await.unwrap();
    let got_setup = read_framed_response(&mut stream).await;
    assert_eq!(got_setup, exp_setup);

    let open = OpenMiningChannelMessage {
        channel_id: 1,
        request_id: 42,
        min_difficulty: 1,
    };
    let open_wire = encode_tlv(message_types::OPEN_MINING_CHANNEL, &open);
    let exp_open = server
        .handle_message(open_wire.clone(), endpoint.clone())
        .await
        .unwrap();

    stream.write_all(&open_wire).await.unwrap();
    let got_open = read_framed_response(&mut stream).await;
    assert_eq!(got_open, exp_open);
}

/// Full miner-style session on one socket: setup → open channel → (template injected) → submit shares.
#[tokio::test]
async fn module_tcp_submit_shares_matches_handle_message_after_job_ready() {
    let ctx = test_ctx_listen_random_port();
    let node_api = Arc::new(SubmittingMockNodeAPI::default());
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();
    server.start().await.unwrap();

    let bind = server.module_tcp_local_addr().await.unwrap();
    let mut stream = TcpStream::connect(bind).await.unwrap();
    let endpoint = stream.local_addr().unwrap().to_string();

    let setup = SetupConnectionMessage {
        protocol_version: 2,
        endpoint: "submit-parity".to_string(),
        capabilities: vec!["mining".to_string()],
    };
    let setup_wire = encode_tlv(message_types::SETUP_CONNECTION, &setup);
    stream.write_all(&setup_wire).await.unwrap();
    let _ = read_framed_response(&mut stream).await;

    let open = OpenMiningChannelMessage {
        channel_id: 1,
        request_id: 1,
        min_difficulty: 1,
    };
    let open_wire = encode_tlv(message_types::OPEN_MINING_CHANNEL, &open);
    stream.write_all(&open_wire).await.unwrap();
    let _ = read_framed_response(&mut stream).await;

    let pool = server.get_pool();
    pool.write().await.set_template(create_test_block());

    let submit = SubmitSharesMessage {
        channel_id: 1,
        shares: vec![messages::ShareData {
            channel_id: 1,
            job_id: 1,
            nonce: 0,
            version: 1,
            merkle_root: [0u8; 32],
        }],
    };
    let submit_wire = encode_tlv(message_types::SUBMIT_SHARES, &submit);
    let exp = server
        .handle_message(submit_wire.clone(), endpoint.clone())
        .await
        .unwrap();

    stream.write_all(&submit_wire).await.unwrap();
    let got = read_framed_response(&mut stream).await;
    assert_eq!(got, exp);
}

/// Header-only official frame is a complete read. A 4-byte LE length prefix is not.
#[tokio::test]
async fn module_tcp_header_only_accepted_legacy_prefix_rejected() {
    use blvm_stratum_v2::protocol::{encode_sv2_frame, take_sv2_frame};

    let header_only = encode_sv2_frame(0x01, b"");
    let (frame, n) = take_sv2_frame(&header_only).unwrap();
    assert_eq!(n, 6);
    assert_eq!(frame.len(), 6);

    let ctx = test_ctx_listen_random_port();
    let node_api = Arc::new(SubmittingMockNodeAPI::default());
    let server = StratumV2Server::new(&ctx, node_api).await.unwrap();
    server.start().await.unwrap();
    let bind = server.module_tcp_local_addr().await.unwrap();

    let mut ok_stream = TcpStream::connect(bind).await.unwrap();
    ok_stream.write_all(&header_only).await.unwrap();
    let mut buf = [0u8; 1];
    let nread = ok_stream.read(&mut buf).await.unwrap();
    assert_eq!(nread, 0, "header-only frame is consumed; empty payload is not a setup");

    let mut bad = TcpStream::connect(bind).await.unwrap();
    let mut legacy = 7u32.to_le_bytes().to_vec();
    legacy.extend_from_slice(&[0, 1]);
    bad.write_all(&legacy).await.unwrap();
    let nread = bad.read(&mut buf).await.unwrap();
    assert_eq!(nread, 0, "legacy 4-byte length prefix closes the connection");
}
