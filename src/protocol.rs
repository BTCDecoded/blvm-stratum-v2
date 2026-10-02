//! Stratum V2 Protocol Encoding/Decoding
//!
//! Mining and Job Declaration messages use the official 6-byte SV2 header
//! (extension u16 BE, message type u8, length u24 BE, payload).
//! `TlvDecoder` still reads the older tag/length body for tests that build it by hand.

use crate::error::StratumV2Error;
use std::io::{Cursor, Read};

/// Result type for protocol operations
pub type ProtocolResult<T> = Result<T, StratumV2Error>;

/// TLV encoder for Stratum V2 messages
pub struct TlvEncoder {
    buffer: Vec<u8>,
}

impl TlvEncoder {
    /// Create a new TLV encoder
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    /// Encode a message as an official SV2 frame.
    ///
    /// `[ext u16 BE = 0][msg_type u8][len u24 BE][payload]`. The old 4-byte
    /// little-endian length prefix is not written.
    pub fn encode(&mut self, tag: u16, payload: &[u8]) -> ProtocolResult<Vec<u8>> {
        Ok(encode_sv2_frame((tag & 0xff) as u8, payload))
    }

    /// Get encoded buffer
    pub fn into_vec(self) -> Vec<u8> {
        self.buffer
    }
}

impl Default for TlvEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// TLV decoder for Stratum V2 messages
pub struct TlvDecoder {
    cursor: Cursor<Vec<u8>>,
}

impl TlvDecoder {
    /// Create a new TLV decoder from bytes
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            cursor: Cursor::new(data),
        }
    }

    /// Decode a TLV message from length-prefixed format
    ///
    /// Format: [4-byte length][2-byte tag][4-byte length][payload]
    /// Returns: (tag, payload)
    pub fn decode(&mut self) -> ProtocolResult<(u16, Vec<u8>)> {
        // Read 4-byte length prefix
        let mut length_bytes = [0u8; 4];
        self.cursor.read_exact(&mut length_bytes).map_err(|e| {
            StratumV2Error::ProtocolError(format!("Failed to read length prefix: {e}"))
        })?;
        let _total_length = u32::from_le_bytes(length_bytes);

        // Read tag (2 bytes, little-endian)
        let mut tag_bytes = [0u8; 2];
        self.cursor
            .read_exact(&mut tag_bytes)
            .map_err(|e| StratumV2Error::ProtocolError(format!("Failed to read tag: {e}")))?;
        let tag = u16::from_le_bytes(tag_bytes);

        // Read payload length (4 bytes, little-endian)
        let mut length_bytes = [0u8; 4];
        self.cursor.read_exact(&mut length_bytes).map_err(|e| {
            StratumV2Error::ProtocolError(format!("Failed to read payload length: {e}"))
        })?;
        let payload_len = u32::from_le_bytes(length_bytes) as usize;

        // Read payload
        let mut payload = vec![0u8; payload_len];
        self.cursor
            .read_exact(&mut payload)
            .map_err(|e| StratumV2Error::ProtocolError(format!("Failed to read payload: {e}")))?;

        Ok((tag, payload))
    }

    /// Decode from raw bytes (without length prefix)
    ///
    /// Used when receiving from transport that already handles framing
    pub fn decode_raw(data: &[u8]) -> ProtocolResult<(u16, Vec<u8>)> {
        if data.len() < 6 {
            return Err(StratumV2Error::ProtocolError(
                "Insufficient data for TLV header".to_string(),
            ));
        }

        let mut cursor = Cursor::new(data);

        // Read tag (2 bytes, little-endian)
        let mut tag_bytes = [0u8; 2];
        cursor
            .read_exact(&mut tag_bytes)
            .map_err(|e| StratumV2Error::ProtocolError(format!("Failed to read tag: {e}")))?;
        let tag = u16::from_le_bytes(tag_bytes);

        // Read payload length (4 bytes, little-endian)
        let mut length_bytes = [0u8; 4];
        cursor.read_exact(&mut length_bytes).map_err(|e| {
            StratumV2Error::ProtocolError(format!("Failed to read payload length: {e}"))
        })?;
        let payload_len = u32::from_le_bytes(length_bytes) as usize;

        // Validate payload length
        if data.len() < 6 + payload_len {
            return Err(StratumV2Error::ProtocolError(format!(
                "Insufficient data for payload: expected {} bytes, got {}",
                6 + payload_len,
                data.len()
            )));
        }

        // Read payload
        let mut payload = vec![0u8; payload_len];
        cursor
            .read_exact(&mut payload)
            .map_err(|e| StratumV2Error::ProtocolError(format!("Failed to read payload: {e}")))?;

        Ok((tag, payload))
    }
}

/// Official SV2 binary frame: `[ext u16 BE][msg_type u8][len u24 BE][payload]`.
/// https://stratumprotocol.org/specification/01-protocol-overview/
pub fn encode_sv2_frame(msg_type: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(6 + payload.len());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.push(msg_type);
    let len = payload.len() as u32;
    out.push(((len >> 16) & 0xff) as u8);
    out.push(((len >> 8) & 0xff) as u8);
    out.push((len & 0xff) as u8);
    out.extend_from_slice(payload);
    out
}

/// Parse a 6-byte SV2 header. Extension must be 0. A legacy 4-byte LE length
/// prefix does not have extension 0 and is rejected.
pub fn parse_sv2_header(hdr: &[u8; 6]) -> ProtocolResult<(u8, usize)> {
    let ext = u16::from_be_bytes([hdr[0], hdr[1]]);
    if ext != 0 {
        return Err(StratumV2Error::ProtocolError(
            "legacy length prefix rejected".into(),
        ));
    }
    let len = ((hdr[3] as usize) << 16) | ((hdr[4] as usize) << 8) | (hdr[5] as usize);
    Ok((hdr[2], len))
}

/// One official frame from `buf`. Header-only (payload length 0) is a complete frame.
pub fn take_sv2_frame(buf: &[u8]) -> ProtocolResult<(Vec<u8>, usize)> {
    if buf.len() < 6 {
        return Err(StratumV2Error::ProtocolError(
            "short SV2 header".into(),
        ));
    }
    let mut hdr = [0u8; 6];
    hdr.copy_from_slice(&buf[..6]);
    let (_typ, len) = parse_sv2_header(&hdr)?;
    let total = 6 + len;
    if buf.len() < total {
        return Err(StratumV2Error::ProtocolError(
            "truncated SV2 frame".into(),
        ));
    }
    Ok((buf[..total].to_vec(), total))
}

/// Decode an official SV2 frame of any message type. Returns `(tag as u16, payload)`.
pub fn decode_sv2_frame(data: &[u8]) -> Option<(u16, Vec<u8>)> {
    if data.len() < 6 {
        return None;
    }
    let mut hdr = [0u8; 6];
    hdr.copy_from_slice(&data[..6]);
    let (typ, len) = parse_sv2_header(&hdr).ok()?;
    if data.len() != 6 + len {
        return None;
    }
    Some((u16::from(typ), data[6..].to_vec()))
}

/// Decode an official SV2 JD frame (`msg_type` in `0x50..=0x60`).
/// Returns `(tag as u16, payload)`.
pub fn decode_sv2_jd_frame(data: &[u8]) -> Option<(u16, Vec<u8>)> {
    if data.len() < 6 {
        return None;
    }
    let ext = u16::from_be_bytes([data[0], data[1]]);
    if ext != 0 {
        return None;
    }
    let typ = data[2];
    if !(0x50..=0x60).contains(&typ) {
        return None;
    }
    let len = ((data[3] as usize) << 16) | ((data[4] as usize) << 8) | (data[5] as usize);
    if data.len() != 6 + len {
        return None;
    }
    Some((u16::from(typ), data[6..].to_vec()))
}

/// Official SV2 frame only. `official` is true. A 4-byte LE length prefix is an error.
pub fn decode_incoming(data: &[u8]) -> ProtocolResult<(u16, Vec<u8>, bool)> {
    if let Some((tag, payload)) = decode_sv2_frame(data) {
        return Ok((tag, payload, true));
    }
    Err(StratumV2Error::ProtocolError(
        "legacy length prefix rejected".into(),
    ))
}

#[cfg(test)]
mod sv2_frame_tests {
    use super::*;

    #[test]
    fn official_jd_frame_roundtrip() {
        let payload = b"declare";
        let enc = encode_sv2_frame(0x57, payload);
        let (tag, dec) = decode_sv2_jd_frame(&enc).unwrap();
        assert_eq!(tag, 0x0057);
        assert_eq!(dec, payload);
        assert!(decode_sv2_jd_frame(&enc).is_some());
        // Mining setup (type 0x01) is a frame, not a JD frame.
        let mut tlv = TlvEncoder::new();
        let setup = tlv.encode(0x0001, b"x").unwrap();
        assert!(decode_sv2_frame(&setup).is_some());
        assert!(decode_sv2_jd_frame(&setup).is_none());
    }

    #[test]
    fn official_jd_frame_rejects_garbage() {
        assert!(decode_sv2_jd_frame(&[]).is_none());
        assert!(decode_sv2_jd_frame(&[0u8; 5]).is_none());
        let mut short = encode_sv2_frame(0x50, b"tok");
        short.pop();
        assert!(decode_sv2_jd_frame(&short).is_none());
        let mut ext = encode_sv2_frame(0x57, b"x");
        ext[1] = 1;
        assert!(decode_sv2_jd_frame(&ext).is_none());
        assert!(decode_sv2_jd_frame(&encode_sv2_frame(0x01, b"setup")).is_none());
        let header_only = encode_sv2_frame(0x01, b"");
        let (frame, n) = take_sv2_frame(&header_only).unwrap();
        assert_eq!(n, 6);
        assert_eq!(frame, header_only);
        let mut legacy = Vec::new();
        legacy.extend_from_slice(&7u32.to_le_bytes());
        legacy.extend_from_slice(&[0, 1]);
        assert!(parse_sv2_header(&{
            let mut h = [0u8; 6];
            h.copy_from_slice(&legacy);
            h
        })
        .is_err());
        let (tag, payload, official) = decode_incoming(&encode_sv2_frame(0x50, b"ab")).unwrap();
        assert!(official);
        assert_eq!(tag, 0x0050);
        assert_eq!(payload, b"ab");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
