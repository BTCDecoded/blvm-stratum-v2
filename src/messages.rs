//! Stratum V2 Protocol Message Types
//!
//! Implements all standard Stratum V2 message types according to the specification:
//! https://stratumprotocol.org/

use crate::error::StratumV2Error;
use blvm_protocol::Hash;
use serde::{Deserialize, Serialize};

/// Stratum V2 message type tags
pub mod message_types {
    // Setup Connection messages
    pub const SETUP_CONNECTION: u16 = 0x0001;
    pub const SETUP_CONNECTION_SUCCESS: u16 = 0x0002;
    pub const SETUP_CONNECTION_ERROR: u16 = 0x0003;

    // Mining channel messages
    pub const OPEN_MINING_CHANNEL: u16 = 0x0010;
    pub const OPEN_MINING_CHANNEL_SUCCESS: u16 = 0x0011;
    pub const OPEN_MINING_CHANNEL_ERROR: u16 = 0x0012;

    // Mining job messages
    pub const NEW_MINING_JOB: u16 = 0x0020;
    pub const SET_NEW_PREV_HASH: u16 = 0x0021;

    // Share submission messages
    pub const SUBMIT_SHARES: u16 = 0x0030;
    pub const SUBMIT_SHARES_SUCCESS: u16 = 0x0031;
    pub const SUBMIT_SHARES_ERROR: u16 = 0x0032;

    /// Official SV2 Job Declaration IDs (stratumprotocol.org/specification/08-message-types/).
    /// This crate's Setup/Open/Submit tags stay the existing local numbering.
    pub const ALLOCATE_MINING_JOB_TOKEN: u16 = 0x0050;
    pub const ALLOCATE_MINING_JOB_TOKEN_SUCCESS: u16 = 0x0051;
    pub const PROVIDE_MISSING_TRANSACTIONS: u16 = 0x0055;
    pub const PROVIDE_MISSING_TRANSACTIONS_SUCCESS: u16 = 0x0056;
    pub const DECLARE_MINING_JOB: u16 = 0x0057;
    pub const DECLARE_MINING_JOB_SUCCESS: u16 = 0x0058;
    pub const DECLARE_MINING_JOB_ERROR: u16 = 0x0059;
    pub const PUSH_SOLUTION: u16 = 0x0060;
}

/// Setup Connection message (client → server)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetupConnectionMessage {
    /// Protocol version
    pub protocol_version: u16,
    /// Miner endpoint (identifies the miner)
    pub endpoint: String,
    /// Capabilities flags
    pub capabilities: Vec<String>,
}

/// Setup Connection Success message (server → client)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetupConnectionSuccessMessage {
    /// Supported protocol versions
    pub supported_versions: Vec<u16>,
    /// Server capabilities
    pub capabilities: Vec<String>,
}

/// Setup Connection Error message (server → client)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetupConnectionErrorMessage {
    /// Error code
    pub error_code: u16,
    /// Error message
    pub error_message: String,
}

/// Open Mining Channel message (client → server)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenMiningChannelMessage {
    /// Channel identifier
    pub channel_id: u32,
    /// Request ID
    pub request_id: u32,
    /// Minimum difficulty
    pub min_difficulty: u32,
}

/// Open Mining Channel Success message (server → client)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenMiningChannelSuccessMessage {
    /// Channel identifier
    pub channel_id: u32,
    /// Request ID
    pub request_id: u32,
    /// Target difficulty
    pub target: Hash,
    /// Maximum number of jobs
    pub max_jobs: u32,
}

/// Open Mining Channel Error message (server → client)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenMiningChannelErrorMessage {
    /// Request ID
    pub request_id: u32,
    /// Error code
    pub error_code: u16,
    /// Error message
    pub error_message: String,
}

/// New Mining Job message (server → client)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewMiningJobMessage {
    /// Channel identifier
    pub channel_id: u32,
    /// Job identifier
    pub job_id: u32,
    /// Previous block hash
    pub prev_hash: Hash,
    /// Coinbase transaction prefix
    pub coinbase_prefix: Vec<u8>,
    /// Coinbase transaction suffix
    pub coinbase_suffix: Vec<u8>,
    /// Merkle path (for transaction inclusion)
    pub merkle_path: Vec<Hash>,
}

/// Set New Previous Hash message (server → client)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetNewPrevHashMessage {
    /// Channel identifier
    pub channel_id: u32,
    /// Job identifier
    pub job_id: u32,
    /// Previous block hash
    pub prev_hash: Hash,
    /// Minimum number of transactions
    pub min_txn_count: u32,
}

/// Share data for submission
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShareData {
    /// Channel identifier
    pub channel_id: u32,
    /// Job identifier
    pub job_id: u32,
    /// Nonce
    pub nonce: u32,
    /// Version
    pub version: i64,
    /// Merkle root
    pub merkle_root: Hash,
}

/// Submit Shares message (client → server)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubmitSharesMessage {
    /// Channel identifier
    pub channel_id: u32,
    /// Share data
    pub shares: Vec<ShareData>,
}

/// Submit Shares Success message (server → client)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubmitSharesSuccessMessage {
    /// Channel identifier
    pub channel_id: u32,
    /// Last submitted job ID
    pub last_job_id: u32,
}

/// Submit Shares Error message (server → client)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubmitSharesErrorMessage {
    /// Channel identifier
    pub channel_id: u32,
    /// Job identifier
    pub job_id: u32,
    /// Error code
    pub error_code: u16,
    /// Error message
    pub error_message: String,
}

/// Little-endian field codec for the structs this server already dispatches.
/// Strings and byte strings are `u16` length plus bytes. Sequences are `u16` count plus items.
mod wire {
    use super::*;

    pub struct W(Vec<u8>);

    impl W {
        pub fn new() -> Self {
            Self(Vec::new())
        }
        pub fn u16(&mut self, v: u16) {
            self.0.extend_from_slice(&v.to_le_bytes());
        }
        pub fn u32(&mut self, v: u32) {
            self.0.extend_from_slice(&v.to_le_bytes());
        }
        pub fn i64(&mut self, v: i64) {
            self.0.extend_from_slice(&v.to_le_bytes());
        }
        pub fn hash(&mut self, h: &Hash) {
            self.0.extend_from_slice(h);
        }
        pub fn bytes(&mut self, b: &[u8]) -> Result<(), StratumV2Error> {
            let n = u16::try_from(b.len()).map_err(|_| {
                StratumV2Error::ProtocolError("byte field longer than 65535".into())
            })?;
            self.u16(n);
            self.0.extend_from_slice(b);
            Ok(())
        }
        pub fn string(&mut self, s: &str) -> Result<(), StratumV2Error> {
            self.bytes(s.as_bytes())
        }
        pub fn finish(self) -> Vec<u8> {
            self.0
        }
    }

    pub struct R<'a> {
        data: &'a [u8],
        i: usize,
    }

    impl<'a> R<'a> {
        pub fn new(data: &'a [u8]) -> Self {
            Self { data, i: 0 }
        }
        fn take(&mut self, n: usize) -> Result<&'a [u8], StratumV2Error> {
            let end = self.i.checked_add(n).ok_or_else(|| {
                StratumV2Error::ProtocolError("truncated field".into())
            })?;
            if end > self.data.len() {
                return Err(StratumV2Error::ProtocolError("truncated field".into()));
            }
            let s = &self.data[self.i..end];
            self.i = end;
            Ok(s)
        }
        pub fn u16(&mut self) -> Result<u16, StratumV2Error> {
            let b = self.take(2)?;
            Ok(u16::from_le_bytes([b[0], b[1]]))
        }
        pub fn u32(&mut self) -> Result<u32, StratumV2Error> {
            let b = self.take(4)?;
            Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        }
        pub fn i64(&mut self) -> Result<i64, StratumV2Error> {
            let b = self.take(8)?;
            Ok(i64::from_le_bytes(b.try_into().expect("8 bytes")))
        }
        pub fn hash(&mut self) -> Result<Hash, StratumV2Error> {
            let b = self.take(32)?;
            let mut h = [0u8; 32];
            h.copy_from_slice(b);
            Ok(h)
        }
        pub fn bytes(&mut self) -> Result<Vec<u8>, StratumV2Error> {
            let n = self.u16()? as usize;
            Ok(self.take(n)?.to_vec())
        }
        pub fn string(&mut self) -> Result<String, StratumV2Error> {
            let b = self.bytes()?;
            String::from_utf8(b)
                .map_err(|e| StratumV2Error::ProtocolError(format!("utf8 field: {e}")))
        }
        pub fn finish(&self) -> Result<(), StratumV2Error> {
            if self.i != self.data.len() {
                return Err(StratumV2Error::ProtocolError(
                    "trailing bytes in payload".into(),
                ));
            }
            Ok(())
        }
    }
}

/// Trait for Stratum V2 message serialization
pub trait StratumV2Message: Serialize + for<'de> Deserialize<'de> {
    /// Get message type tag
    fn message_type(&self) -> u16;

    /// Payload bytes in struct field order (little-endian integers).
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error>;

    /// Deserialize payload bytes.
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error>
    where
        Self: Sized;
}

// Implement StratumV2Message for all message types
impl StratumV2Message for SetupConnectionMessage {
    fn message_type(&self) -> u16 {
        message_types::SETUP_CONNECTION
    }
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error> {
        let mut w = wire::W::new();
        w.u16(self.protocol_version);
        w.string(&self.endpoint)?;
        let n = u16::try_from(self.capabilities.len())
            .map_err(|_| StratumV2Error::ProtocolError("too many capabilities".into()))?;
        w.u16(n);
        for c in &self.capabilities {
            w.string(c)?;
        }
        Ok(w.finish())
    }
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error> {
        let mut r = wire::R::new(data);
        let protocol_version = r.u16()?;
        let endpoint = r.string()?;
        let n = r.u16()? as usize;
        let mut capabilities = Vec::with_capacity(n);
        for _ in 0..n {
            capabilities.push(r.string()?);
        }
        r.finish()?;
        Ok(Self {
            protocol_version,
            endpoint,
            capabilities,
        })
    }
}

impl StratumV2Message for SetupConnectionSuccessMessage {
    fn message_type(&self) -> u16 {
        message_types::SETUP_CONNECTION_SUCCESS
    }
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error> {
        let mut w = wire::W::new();
        let n = u16::try_from(self.supported_versions.len())
            .map_err(|_| StratumV2Error::ProtocolError("too many versions".into()))?;
        w.u16(n);
        for v in &self.supported_versions {
            w.u16(*v);
        }
        let n = u16::try_from(self.capabilities.len())
            .map_err(|_| StratumV2Error::ProtocolError("too many capabilities".into()))?;
        w.u16(n);
        for c in &self.capabilities {
            w.string(c)?;
        }
        Ok(w.finish())
    }
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error> {
        let mut r = wire::R::new(data);
        let n = r.u16()? as usize;
        let mut supported_versions = Vec::with_capacity(n);
        for _ in 0..n {
            supported_versions.push(r.u16()?);
        }
        let n = r.u16()? as usize;
        let mut capabilities = Vec::with_capacity(n);
        for _ in 0..n {
            capabilities.push(r.string()?);
        }
        r.finish()?;
        Ok(Self {
            supported_versions,
            capabilities,
        })
    }
}

impl StratumV2Message for SetupConnectionErrorMessage {
    fn message_type(&self) -> u16 {
        message_types::SETUP_CONNECTION_ERROR
    }
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error> {
        let mut w = wire::W::new();
        w.u16(self.error_code);
        w.string(&self.error_message)?;
        Ok(w.finish())
    }
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error> {
        let mut r = wire::R::new(data);
        let error_code = r.u16()?;
        let error_message = r.string()?;
        r.finish()?;
        Ok(Self {
            error_code,
            error_message,
        })
    }
}

impl StratumV2Message for OpenMiningChannelMessage {
    fn message_type(&self) -> u16 {
        message_types::OPEN_MINING_CHANNEL
    }
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error> {
        let mut w = wire::W::new();
        w.u32(self.channel_id);
        w.u32(self.request_id);
        w.u32(self.min_difficulty);
        Ok(w.finish())
    }
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error> {
        let mut r = wire::R::new(data);
        let msg = Self {
            channel_id: r.u32()?,
            request_id: r.u32()?,
            min_difficulty: r.u32()?,
        };
        r.finish()?;
        Ok(msg)
    }
}

impl StratumV2Message for OpenMiningChannelSuccessMessage {
    fn message_type(&self) -> u16 {
        message_types::OPEN_MINING_CHANNEL_SUCCESS
    }
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error> {
        let mut w = wire::W::new();
        w.u32(self.channel_id);
        w.u32(self.request_id);
        w.hash(&self.target);
        w.u32(self.max_jobs);
        Ok(w.finish())
    }
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error> {
        let mut r = wire::R::new(data);
        let msg = Self {
            channel_id: r.u32()?,
            request_id: r.u32()?,
            target: r.hash()?,
            max_jobs: r.u32()?,
        };
        r.finish()?;
        Ok(msg)
    }
}

impl StratumV2Message for OpenMiningChannelErrorMessage {
    fn message_type(&self) -> u16 {
        message_types::OPEN_MINING_CHANNEL_ERROR
    }
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error> {
        let mut w = wire::W::new();
        w.u32(self.request_id);
        w.u16(self.error_code);
        w.string(&self.error_message)?;
        Ok(w.finish())
    }
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error> {
        let mut r = wire::R::new(data);
        let msg = Self {
            request_id: r.u32()?,
            error_code: r.u16()?,
            error_message: r.string()?,
        };
        r.finish()?;
        Ok(msg)
    }
}

impl StratumV2Message for NewMiningJobMessage {
    fn message_type(&self) -> u16 {
        message_types::NEW_MINING_JOB
    }
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error> {
        let mut w = wire::W::new();
        w.u32(self.channel_id);
        w.u32(self.job_id);
        w.hash(&self.prev_hash);
        w.bytes(&self.coinbase_prefix)?;
        w.bytes(&self.coinbase_suffix)?;
        let n = u16::try_from(self.merkle_path.len())
            .map_err(|_| StratumV2Error::ProtocolError("merkle path too long".into()))?;
        w.u16(n);
        for h in &self.merkle_path {
            w.hash(h);
        }
        Ok(w.finish())
    }
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error> {
        let mut r = wire::R::new(data);
        let channel_id = r.u32()?;
        let job_id = r.u32()?;
        let prev_hash = r.hash()?;
        let coinbase_prefix = r.bytes()?;
        let coinbase_suffix = r.bytes()?;
        let n = r.u16()? as usize;
        let mut merkle_path = Vec::with_capacity(n);
        for _ in 0..n {
            merkle_path.push(r.hash()?);
        }
        r.finish()?;
        Ok(Self {
            channel_id,
            job_id,
            prev_hash,
            coinbase_prefix,
            coinbase_suffix,
            merkle_path,
        })
    }
}

impl StratumV2Message for SetNewPrevHashMessage {
    fn message_type(&self) -> u16 {
        message_types::SET_NEW_PREV_HASH
    }
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error> {
        let mut w = wire::W::new();
        w.u32(self.channel_id);
        w.u32(self.job_id);
        w.hash(&self.prev_hash);
        w.u32(self.min_txn_count);
        Ok(w.finish())
    }
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error> {
        let mut r = wire::R::new(data);
        let msg = Self {
            channel_id: r.u32()?,
            job_id: r.u32()?,
            prev_hash: r.hash()?,
            min_txn_count: r.u32()?,
        };
        r.finish()?;
        Ok(msg)
    }
}

impl StratumV2Message for SubmitSharesMessage {
    fn message_type(&self) -> u16 {
        message_types::SUBMIT_SHARES
    }
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error> {
        let mut w = wire::W::new();
        w.u32(self.channel_id);
        let n = u16::try_from(self.shares.len())
            .map_err(|_| StratumV2Error::ProtocolError("too many shares".into()))?;
        w.u16(n);
        for s in &self.shares {
            w.u32(s.channel_id);
            w.u32(s.job_id);
            w.u32(s.nonce);
            w.i64(s.version);
            w.hash(&s.merkle_root);
        }
        Ok(w.finish())
    }
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error> {
        let mut r = wire::R::new(data);
        let channel_id = r.u32()?;
        let n = r.u16()? as usize;
        let mut shares = Vec::with_capacity(n);
        for _ in 0..n {
            shares.push(ShareData {
                channel_id: r.u32()?,
                job_id: r.u32()?,
                nonce: r.u32()?,
                version: r.i64()?,
                merkle_root: r.hash()?,
            });
        }
        r.finish()?;
        Ok(Self { channel_id, shares })
    }
}

impl StratumV2Message for SubmitSharesSuccessMessage {
    fn message_type(&self) -> u16 {
        message_types::SUBMIT_SHARES_SUCCESS
    }
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error> {
        let mut w = wire::W::new();
        w.u32(self.channel_id);
        w.u32(self.last_job_id);
        Ok(w.finish())
    }
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error> {
        let mut r = wire::R::new(data);
        let msg = Self {
            channel_id: r.u32()?,
            last_job_id: r.u32()?,
        };
        r.finish()?;
        Ok(msg)
    }
}

impl StratumV2Message for SubmitSharesErrorMessage {
    fn message_type(&self) -> u16 {
        message_types::SUBMIT_SHARES_ERROR
    }
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error> {
        let mut w = wire::W::new();
        w.u32(self.channel_id);
        w.u32(self.job_id);
        w.u16(self.error_code);
        w.string(&self.error_message)?;
        Ok(w.finish())
    }
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error> {
        let mut r = wire::R::new(data);
        let msg = Self {
            channel_id: r.u32()?,
            job_id: r.u32()?,
            error_code: r.u16()?,
            error_message: r.string()?,
        };
        r.finish()?;
        Ok(msg)
    }
}

/// JDC → JDS. Spec 6.4.2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AllocateMiningJobTokenMessage {
    pub request_id: u32,
    pub user_identifier: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AllocateMiningJobTokenSuccessMessage {
    pub request_id: u32,
    pub mining_job_token: Vec<u8>,
}

/// JDC → JDS. Spec 6.4.5. `tx_id_list` is non-coinbase txids in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeclareMiningJobMessage {
    pub request_id: u32,
    pub mining_job_token: Vec<u8>,
    pub version: u32,
    pub tx_id_list: Vec<Hash>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeclareMiningJobSuccessMessage {
    pub request_id: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeclareMiningJobErrorMessage {
    pub request_id: u32,
    pub error_code: u16,
    pub error_message: String,
}

/// JDS → JDC. Spec 6.4.7. Positions into `tx_id_list` (not coinbase).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProvideMissingTransactionsMessage {
    pub request_id: u32,
    pub unknown_tx_position_list: Vec<u16>,
}

/// JDC → JDS. Spec 6.4.8.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProvideMissingTransactionsSuccessMessage {
    pub request_id: u32,
    pub transaction_list: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PushSolutionMessage {
    pub mining_job_token: Vec<u8>,
    pub version: u32,
    pub ntime: u32,
    pub nonce: u32,
    pub nbits: u32,
    pub extranonce: Vec<u8>,
}

impl StratumV2Message for AllocateMiningJobTokenMessage {
    fn message_type(&self) -> u16 {
        message_types::ALLOCATE_MINING_JOB_TOKEN
    }
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error> {
        let mut w = wire::W::new();
        w.u32(self.request_id);
        w.string(&self.user_identifier)?;
        Ok(w.finish())
    }
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error> {
        let mut r = wire::R::new(data);
        let msg = Self {
            request_id: r.u32()?,
            user_identifier: r.string()?,
        };
        r.finish()?;
        Ok(msg)
    }
}
impl StratumV2Message for AllocateMiningJobTokenSuccessMessage {
    fn message_type(&self) -> u16 {
        message_types::ALLOCATE_MINING_JOB_TOKEN_SUCCESS
    }
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error> {
        let mut w = wire::W::new();
        w.u32(self.request_id);
        w.bytes(&self.mining_job_token)?;
        Ok(w.finish())
    }
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error> {
        let mut r = wire::R::new(data);
        let msg = Self {
            request_id: r.u32()?,
            mining_job_token: r.bytes()?,
        };
        r.finish()?;
        Ok(msg)
    }
}
impl StratumV2Message for DeclareMiningJobMessage {
    fn message_type(&self) -> u16 {
        message_types::DECLARE_MINING_JOB
    }
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error> {
        let mut w = wire::W::new();
        w.u32(self.request_id);
        w.bytes(&self.mining_job_token)?;
        w.u32(self.version);
        let n = u16::try_from(self.tx_id_list.len())
            .map_err(|_| StratumV2Error::ProtocolError("tx id list too long".into()))?;
        w.u16(n);
        for h in &self.tx_id_list {
            w.hash(h);
        }
        Ok(w.finish())
    }
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error> {
        let mut r = wire::R::new(data);
        let request_id = r.u32()?;
        let mining_job_token = r.bytes()?;
        let version = r.u32()?;
        let n = r.u16()? as usize;
        let mut tx_id_list = Vec::with_capacity(n);
        for _ in 0..n {
            tx_id_list.push(r.hash()?);
        }
        r.finish()?;
        Ok(Self {
            request_id,
            mining_job_token,
            version,
            tx_id_list,
        })
    }
}
impl StratumV2Message for DeclareMiningJobSuccessMessage {
    fn message_type(&self) -> u16 {
        message_types::DECLARE_MINING_JOB_SUCCESS
    }
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error> {
        let mut w = wire::W::new();
        w.u32(self.request_id);
        Ok(w.finish())
    }
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error> {
        let mut r = wire::R::new(data);
        let msg = Self {
            request_id: r.u32()?,
        };
        r.finish()?;
        Ok(msg)
    }
}
impl StratumV2Message for DeclareMiningJobErrorMessage {
    fn message_type(&self) -> u16 {
        message_types::DECLARE_MINING_JOB_ERROR
    }
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error> {
        let mut w = wire::W::new();
        w.u32(self.request_id);
        w.u16(self.error_code);
        w.string(&self.error_message)?;
        Ok(w.finish())
    }
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error> {
        let mut r = wire::R::new(data);
        let msg = Self {
            request_id: r.u32()?,
            error_code: r.u16()?,
            error_message: r.string()?,
        };
        r.finish()?;
        Ok(msg)
    }
}
impl StratumV2Message for ProvideMissingTransactionsMessage {
    fn message_type(&self) -> u16 {
        message_types::PROVIDE_MISSING_TRANSACTIONS
    }
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error> {
        let mut w = wire::W::new();
        w.u32(self.request_id);
        let n = u16::try_from(self.unknown_tx_position_list.len())
            .map_err(|_| StratumV2Error::ProtocolError("position list too long".into()))?;
        w.u16(n);
        for p in &self.unknown_tx_position_list {
            w.u16(*p);
        }
        Ok(w.finish())
    }
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error> {
        let mut r = wire::R::new(data);
        let request_id = r.u32()?;
        let n = r.u16()? as usize;
        let mut unknown_tx_position_list = Vec::with_capacity(n);
        for _ in 0..n {
            unknown_tx_position_list.push(r.u16()?);
        }
        r.finish()?;
        Ok(Self {
            request_id,
            unknown_tx_position_list,
        })
    }
}
impl StratumV2Message for ProvideMissingTransactionsSuccessMessage {
    fn message_type(&self) -> u16 {
        message_types::PROVIDE_MISSING_TRANSACTIONS_SUCCESS
    }
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error> {
        let mut w = wire::W::new();
        w.u32(self.request_id);
        let n = u16::try_from(self.transaction_list.len())
            .map_err(|_| StratumV2Error::ProtocolError("transaction list too long".into()))?;
        w.u16(n);
        for tx in &self.transaction_list {
            w.bytes(tx)?;
        }
        Ok(w.finish())
    }
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error> {
        let mut r = wire::R::new(data);
        let request_id = r.u32()?;
        let n = r.u16()? as usize;
        let mut transaction_list = Vec::with_capacity(n);
        for _ in 0..n {
            transaction_list.push(r.bytes()?);
        }
        r.finish()?;
        Ok(Self {
            request_id,
            transaction_list,
        })
    }
}
impl StratumV2Message for PushSolutionMessage {
    fn message_type(&self) -> u16 {
        message_types::PUSH_SOLUTION
    }
    fn to_bytes(&self) -> Result<Vec<u8>, StratumV2Error> {
        let mut w = wire::W::new();
        w.bytes(&self.mining_job_token)?;
        w.u32(self.version);
        w.u32(self.ntime);
        w.u32(self.nonce);
        w.u32(self.nbits);
        w.bytes(&self.extranonce)?;
        Ok(w.finish())
    }
    fn from_bytes(data: &[u8]) -> Result<Self, StratumV2Error> {
        let mut r = wire::R::new(data);
        let msg = Self {
            mining_job_token: r.bytes()?,
            version: r.u32()?,
            ntime: r.u32()?,
            nonce: r.u32()?,
            nbits: r.u32()?,
            extranonce: r.bytes()?,
        };
        r.finish()?;
        Ok(msg)
    }
}

#[cfg(test)]
mod field_roundtrip {
    use super::*;
    use crate::protocol::encode_sv2_frame;

    fn roundtrip<T: StratumV2Message + PartialEq + std::fmt::Debug>(msg: &T) {
        let bytes = msg.to_bytes().unwrap();
        let back = T::from_bytes(&bytes).unwrap();
        assert_eq!(&back, msg);
        let frame = encode_sv2_frame((msg.message_type() & 0xff) as u8, &bytes);
        assert_eq!(&frame[..2], &[0, 0]);
        assert_eq!(frame[2], (msg.message_type() & 0xff) as u8);
        assert!(frame.len() == 6 + bytes.len());
    }

    #[test]
    fn setup_connection_roundtrip() {
        roundtrip(&SetupConnectionMessage {
            protocol_version: 2,
            endpoint: "miner-a".into(),
            capabilities: vec!["ext".into()],
        });
    }
    #[test]
    fn setup_connection_success_roundtrip() {
        roundtrip(&SetupConnectionSuccessMessage {
            supported_versions: vec![2],
            capabilities: vec!["job".into()],
        });
    }
    #[test]
    fn setup_connection_error_roundtrip() {
        roundtrip(&SetupConnectionErrorMessage {
            error_code: 1,
            error_message: "no".into(),
        });
    }
    #[test]
    fn open_channel_roundtrip() {
        roundtrip(&OpenMiningChannelMessage {
            channel_id: 7,
            request_id: 3,
            min_difficulty: 1,
        });
    }
    #[test]
    fn open_channel_success_roundtrip() {
        roundtrip(&OpenMiningChannelSuccessMessage {
            channel_id: 7,
            request_id: 3,
            target: [9u8; 32],
            max_jobs: 4,
        });
    }
    #[test]
    fn open_channel_error_roundtrip() {
        roundtrip(&OpenMiningChannelErrorMessage {
            request_id: 3,
            error_code: 2,
            error_message: "closed".into(),
        });
    }
    #[test]
    fn new_job_roundtrip() {
        roundtrip(&NewMiningJobMessage {
            channel_id: 1,
            job_id: 2,
            prev_hash: [1u8; 32],
            coinbase_prefix: vec![0xaa],
            coinbase_suffix: vec![0xbb],
            merkle_path: vec![[2u8; 32]],
        });
    }
    #[test]
    fn set_prevhash_roundtrip() {
        roundtrip(&SetNewPrevHashMessage {
            channel_id: 1,
            job_id: 2,
            prev_hash: [3u8; 32],
            min_txn_count: 1,
        });
    }
    #[test]
    fn submit_shares_roundtrip() {
        roundtrip(&SubmitSharesMessage {
            channel_id: 1,
            shares: vec![ShareData {
                channel_id: 1,
                job_id: 2,
                nonce: 9,
                version: 1,
                merkle_root: [4u8; 32],
            }],
        });
    }
    #[test]
    fn submit_shares_success_roundtrip() {
        roundtrip(&SubmitSharesSuccessMessage {
            channel_id: 1,
            last_job_id: 2,
        });
    }
    #[test]
    fn submit_shares_error_roundtrip() {
        roundtrip(&SubmitSharesErrorMessage {
            channel_id: 1,
            job_id: 2,
            error_code: 3,
            error_message: "bad".into(),
        });
    }
    #[test]
    fn allocate_token_roundtrip() {
        roundtrip(&AllocateMiningJobTokenMessage {
            request_id: 1,
            user_identifier: "jdc".into(),
        });
    }
    #[test]
    fn allocate_token_success_roundtrip() {
        roundtrip(&AllocateMiningJobTokenSuccessMessage {
            request_id: 1,
            mining_job_token: vec![1, 2, 3],
        });
    }
    #[test]
    fn declare_job_roundtrip() {
        roundtrip(&DeclareMiningJobMessage {
            request_id: 1,
            mining_job_token: vec![9],
            version: 0x20000000,
            tx_id_list: vec![[5u8; 32]],
        });
    }
    #[test]
    fn declare_job_success_roundtrip() {
        roundtrip(&DeclareMiningJobSuccessMessage { request_id: 1 });
    }
    #[test]
    fn declare_job_error_roundtrip() {
        roundtrip(&DeclareMiningJobErrorMessage {
            request_id: 1,
            error_code: 4,
            error_message: "decl".into(),
        });
    }
    #[test]
    fn provide_missing_roundtrip() {
        roundtrip(&ProvideMissingTransactionsMessage {
            request_id: 1,
            unknown_tx_position_list: vec![0, 2],
        });
    }
    #[test]
    fn provide_missing_success_roundtrip() {
        roundtrip(&ProvideMissingTransactionsSuccessMessage {
            request_id: 1,
            transaction_list: vec![vec![0x01, 0x02]],
        });
    }
    #[test]
    fn push_solution_roundtrip() {
        roundtrip(&PushSolutionMessage {
            mining_job_token: vec![7],
            version: 1,
            ntime: 2,
            nonce: 3,
            nbits: 4,
            extranonce: vec![8],
        });
    }
}
