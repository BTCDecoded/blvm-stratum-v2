//! Tests for Stratum V2 Protocol Encoding/Decoding

use blvm_stratum_v2::protocol::{decode_sv2_frame, TlvDecoder, TlvEncoder};

#[test]
fn test_tlv_encode_decode() {
    let tag = 0x0001u16;
    let payload = b"test payload";

    let mut encoder = TlvEncoder::new();
    let encoded = encoder.encode(tag, payload).unwrap();

    let (decoded_tag, decoded_payload) = decode_sv2_frame(&encoded).unwrap();

    assert_eq!(tag, decoded_tag);
    assert_eq!(payload, decoded_payload.as_slice());
}

#[test]
fn test_tlv_decode_raw() {
    let tag = 0x0002u16;
    let payload = b"raw payload";

    // Create raw TLV (tag + length + payload)
    let mut raw = Vec::new();
    raw.extend_from_slice(&tag.to_le_bytes());
    raw.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    raw.extend_from_slice(payload);

    let (decoded_tag, decoded_payload) = TlvDecoder::decode_raw(&raw).unwrap();

    assert_eq!(tag, decoded_tag);
    assert_eq!(payload, decoded_payload.as_slice());
}

#[test]
fn test_tlv_encode_decode_large_payload() {
    let tag = 0x0003u16;
    let payload: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();

    let mut encoder = TlvEncoder::new();
    let encoded = encoder.encode(tag, &payload).unwrap();

    let (decoded_tag, decoded_payload) = decode_sv2_frame(&encoded).unwrap();

    assert_eq!(tag, decoded_tag);
    assert_eq!(payload, decoded_payload);
}

#[test]
fn test_tlv_decode_raw_insufficient_data() {
    let insufficient_data = vec![0u8; 5]; // Less than 6 bytes (tag + length)

    let result = TlvDecoder::decode_raw(&insufficient_data);
    assert!(result.is_err());
}

#[test]
fn official_jd_message_type_ids() {
    use blvm_stratum_v2::messages::message_types::*;
    assert_eq!(ALLOCATE_MINING_JOB_TOKEN, 0x0050);
    assert_eq!(ALLOCATE_MINING_JOB_TOKEN_SUCCESS, 0x0051);
    assert_eq!(PROVIDE_MISSING_TRANSACTIONS, 0x0055);
    assert_eq!(PROVIDE_MISSING_TRANSACTIONS_SUCCESS, 0x0056);
    assert_eq!(DECLARE_MINING_JOB, 0x0057);
    assert_eq!(DECLARE_MINING_JOB_SUCCESS, 0x0058);
    assert_eq!(DECLARE_MINING_JOB_ERROR, 0x0059);
    assert_eq!(PUSH_SOLUTION, 0x0060);
}
