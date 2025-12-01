//! Mining pool management for Stratum V2

use crate::error::StratumV2Error;
use bllvm_protocol::{Block, Hash};
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Miner connection information
#[derive(Debug, Clone)]
pub struct MinerConnection {
    /// Miner endpoint identifier
    pub endpoint: String,
    /// Open mining channels (channel_id -> ChannelInfo)
    pub channels: HashMap<u32, ChannelInfo>,
    /// Miner statistics
    pub stats: MinerStats,
}

/// Mining job information
#[derive(Debug, Clone)]
pub struct JobInfo {
    /// Job identifier
    pub job_id: u32,
    /// Previous block hash
    pub prev_hash: Hash,
    /// Difficulty bits
    pub bits: u32,
    /// Timestamp
    pub timestamp: u64,
}

/// Mining channel information
#[derive(Debug, Clone)]
pub struct ChannelInfo {
    /// Channel identifier
    pub channel_id: u32,
    /// Target difficulty (for share validation)
    pub target: Hash,
    /// Current job ID
    pub current_job_id: Option<u32>,
    /// Minimum difficulty requested by miner
    pub min_difficulty: u32,
    /// Maximum number of jobs
    pub max_jobs: u32,
    /// Active jobs (job_id -> job info)
    pub jobs: HashMap<u32, JobInfo>,
}

/// Miner statistics
#[derive(Debug, Clone)]
pub struct MinerStats {
    /// Total shares submitted
    pub total_shares: u64,
    /// Accepted shares
    pub accepted_shares: u64,
    /// Rejected shares
    pub rejected_shares: u64,
    /// Last share timestamp
    pub last_share_time: Option<u64>,
}

impl Default for MinerStats {
    fn default() -> Self {
        Self {
            total_shares: 0,
            accepted_shares: 0,
            rejected_shares: 0,
            last_share_time: None,
        }
    }
}

/// Share data for submission
#[derive(Debug, Clone)]
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

/// Stratum V2 pool implementation
pub struct StratumV2Pool {
    /// Connected miners (endpoint -> connection info)
    pub miners: HashMap<String, MinerConnection>,
    /// Current block template
    current_template: Option<Block>,
    /// Current job ID counter
    job_id_counter: u32,
}

impl StratumV2Pool {
    /// Create a new pool instance
    pub fn new() -> Self {
        Self {
            miners: HashMap::new(),
            current_template: None,
            job_id_counter: 1,
        }
    }

    /// Register a miner connection
    pub fn register_miner(&mut self, endpoint: String) {
        let connection = MinerConnection {
            endpoint: endpoint.clone(),
            channels: HashMap::new(),
            stats: MinerStats::default(),
        };
        self.miners.insert(endpoint.clone(), connection);
        info!("Registered miner: {}", endpoint);
    }

    /// Open a mining channel for a miner
    pub fn open_channel(
        &mut self,
        endpoint: &str,
        channel_id: u32,
        min_difficulty: u32,
    ) -> Result<Hash, StratumV2Error> {
        let miner = self.miners.get_mut(endpoint)
            .ok_or_else(|| StratumV2Error::ProtocolError(format!("Miner not registered: {}", endpoint)))?;

        // Calculate channel target from difficulty
        let channel_target = self.calculate_channel_target(min_difficulty)?;

        let channel_info = ChannelInfo {
            channel_id,
            target: channel_target,
            current_job_id: None,
            min_difficulty,
            max_jobs: 10, // Default max jobs
            jobs: HashMap::new(),
        };

        miner.channels.insert(channel_id, channel_info.clone());
        info!("Opened mining channel: miner={}, channel_id={}, target={:x?}", 
            endpoint, channel_id, &channel_target[..8]);

        Ok(channel_target)
    }

    /// Set current block template and create new mining job
    pub fn set_template(&mut self, template: Block) -> (u32, Vec<(String, u32)>) {
        // Generate new job ID
        let job_id = self.job_id_counter;
        self.job_id_counter = self.job_id_counter.wrapping_add(1);

        // Create job info from template
        let job_info = JobInfo {
            job_id,
            prev_hash: template.header.prev_block_hash,
            bits: template.header.bits as u32,
            timestamp: template.header.timestamp as u64,
        };

        // Distribute job to all open channels
        let mut job_distributions = Vec::new();
        for (endpoint, miner) in &mut self.miners {
            for (channel_id, channel) in &mut miner.channels {
                channel.current_job_id = Some(job_id);
                channel.jobs.insert(job_id, job_info.clone());
                job_distributions.push((endpoint.clone(), *channel_id));
            }
        }

        // Store template
        self.current_template = Some(template);

        info!("Created new mining job: job_id={}, channels={}", job_id, job_distributions.len());
        (job_id, job_distributions)
    }

    /// Handle share submission
    pub fn handle_share(
        &mut self,
        endpoint: &str,
        share: ShareData,
    ) -> Result<bool, StratumV2Error> {
        let miner = self.miners.get_mut(endpoint)
            .ok_or_else(|| StratumV2Error::ProtocolError(format!("Miner not registered: {}", endpoint)))?;

        // Update statistics
        miner.stats.total_shares += 1;

        // Get channel
        let channel = miner.channels.get(&share.channel_id)
            .ok_or_else(|| StratumV2Error::ProtocolError(format!("Channel not found: {}", share.channel_id)))?;

        // Get job info
        let job_info = channel.jobs.get(&share.job_id)
            .ok_or_else(|| StratumV2Error::ProtocolError(format!("Job not found: {}", share.job_id)))?;

        // Validate share (simplified - full validation would check PoW)
        let is_valid = self.validate_share(&share, job_info, &channel.target);

        if is_valid {
            miner.stats.accepted_shares += 1;
            info!("Accepted share from {}: channel={}, job={}", endpoint, share.channel_id, share.job_id);
        } else {
            miner.stats.rejected_shares += 1;
            warn!("Rejected share from {}: channel={}, job={}", endpoint, share.channel_id, share.job_id);
        }

        Ok(is_valid)
    }

    /// Validate a share using consensus-proof functions
    fn validate_share(&self, share: &ShareData, job_info: &JobInfo, target: &Hash) -> bool {
        use bllvm_consensus::ConsensusProof;
        use bllvm_protocol::BlockHeader;

        // 1. Basic validation
        if share.job_id != job_info.job_id {
            return false;
        }

        // 2. Construct block header from share data and job info
        let header = BlockHeader {
            version: share.version,
            prev_block_hash: job_info.prev_hash,
            merkle_root: share.merkle_root,
            timestamp: job_info.timestamp as i64,
            bits: job_info.bits as u32,
            nonce: share.nonce as u64,
        };

        // 3. Verify proof of work using consensus-proof
        let consensus = ConsensusProof::new();
        match consensus.check_proof_of_work(&header) {
            Ok(pow_valid) => {
                if !pow_valid {
                    return false;
                }
            }
            Err(_) => {
                return false;
            }
        }

        // 4. Check if hash meets channel target (for share validation)
        // Channel targets are typically easier than network targets
        let block_hash = self.calculate_block_hash(&header);
        block_hash <= *target
    }

    /// Calculate block hash (double SHA256 of header)
    fn calculate_block_hash(&self, header: &bllvm_protocol::BlockHeader) -> Hash {
        use sha2::{Digest, Sha256};

        // Serialize header
        let mut data = Vec::new();
        data.extend_from_slice(&(header.version as u32).to_le_bytes());
        data.extend_from_slice(&header.prev_block_hash);
        data.extend_from_slice(&header.merkle_root);
        data.extend_from_slice(&(header.timestamp as u32).to_le_bytes());
        data.extend_from_slice(&header.bits.to_le_bytes());
        data.extend_from_slice(&(header.nonce as u32).to_le_bytes());

        // Double SHA256
        let hash1 = Sha256::digest(&data);
        let hash2 = Sha256::digest(hash1);

        let mut result = [0u8; 32];
        result.copy_from_slice(&hash2);
        result
    }

    /// Calculate channel target from difficulty
    fn calculate_channel_target(&self, min_difficulty: u32) -> Result<Hash, StratumV2Error> {
        // Use proper consensus functions for target calculation
        // For channel targets, we adjust the difficulty to be easier than network difficulty
        // Channel targets are typically 10-100x easier for share validation
        
        // Base target (genesis difficulty: 0x1d00ffff)
        // Convert to compact format: 0x1d00ffff = exponent 0x1d, mantissa 0x00ffff
        let base_bits = 0x1d00ffffu32;
        
        // For channel validation, we make the target easier (higher target value)
        // min_difficulty represents the minimum difficulty for share validation
        // Higher min_difficulty = easier target (for share validation)
        
        // Calculate target multiplier: for min_difficulty=1, use 100x easier
        // For higher min_difficulty, use progressively easier targets
        let difficulty_multiplier = if min_difficulty == 0 {
            1000u64 // Very easy for testing
        } else {
            // Higher min_difficulty = easier target
            // Formula: multiplier = min_difficulty * 100 (capped at 10000x)
            (min_difficulty as u64 * 100).min(10000)
        };
        
        // Expand base target from compact format using bllvm-consensus logic
        let exponent = (base_bits >> 24) as u8;
        let mantissa = base_bits & 0x00ffffff;
        
        let base_target = if exponent <= 3 {
            let shift = 8 * (3 - exponent);
            (mantissa >> shift) as u128
        } else {
            let shift = 8 * (exponent - 3);
            if shift >= 104 {
                return Err(StratumV2Error::PoolError("Target too large".to_string()));
            }
            (mantissa << shift) as u128
        };
        
        // Multiply target by difficulty multiplier (makes it easier)
        let channel_target = base_target
            .checked_mul(difficulty_multiplier as u128)
            .ok_or_else(|| StratumV2Error::PoolError("Target multiplication overflow".to_string()))?;
        
        // Convert to 32-byte hash (big-endian)
        // Target is stored as a 256-bit value, but we only need 32 bytes
        let mut target_bytes = [0u8; 32];
        let target_be_bytes = channel_target.to_be_bytes();
        // Copy to end of array (big-endian, right-aligned)
        let bytes_len = target_be_bytes.len();
        let start_idx = 32.saturating_sub(bytes_len);
        let copy_len = (32 - start_idx).min(bytes_len);
        target_bytes[start_idx..start_idx + copy_len].copy_from_slice(&target_be_bytes[bytes_len - copy_len..]);
        
        // Clamp to maximum target (0xFFFF...FFFF) if overflow
        // For very easy targets, we might exceed 32 bytes, so clamp
        if channel_target > u128::MAX / 2 {
            // Use maximum target (all 0xFF)
            target_bytes = [0xFFu8; 32];
        }
        
        Ok(target_bytes)
    }
}

impl Default for StratumV2Pool {
    fn default() -> Self {
        Self::new()
    }
}
