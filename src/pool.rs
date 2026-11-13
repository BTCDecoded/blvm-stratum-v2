//! Mining pool management for Stratum V2

use crate::error::StratumV2Error;
pub use crate::messages::ShareData;
use blvm_protocol::{Block, Hash};
use std::collections::{HashMap, HashSet, VecDeque};
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

/// One share identity. A repeat of this tuple is not new work.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SeenShare {
    channel_id: u32,
    job_id: u32,
    nonce: u32,
    version: i64,
    merkle_root: Hash,
}

/// Channels one connection may open.
const MAX_CHANNELS_PER_MINER: usize = 16;
/// Accepted share identities kept so a replay is not credited again.
const MAX_SEEN_SHARES: usize = 4096;

/// Mining job information
#[derive(Debug, Clone)]
pub struct JobInfo {
    /// Job identifier
    pub job_id: u32,
    /// Previous block hash
    pub prev_hash: Hash,
    /// Merkle root of this job's block, not a root the miner invents
    pub merkle_root: Hash,
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
#[derive(Debug, Clone, Default)]
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

/// Bitcoin header hash: SHA256d of the 80-byte wire header (same as the node blockstore).
pub fn stratum_header_hash(header: &blvm_protocol::BlockHeader) -> Hash {
    use sha2::{Digest, Sha256};
    let mut data = [0u8; 80];
    data[0..4].copy_from_slice(&(header.version as u32).to_le_bytes());
    data[4..36].copy_from_slice(&header.prev_block_hash);
    data[36..68].copy_from_slice(&header.merkle_root);
    data[68..72].copy_from_slice(&(header.timestamp as u32).to_le_bytes());
    data[72..76].copy_from_slice(&(header.bits as u32).to_le_bytes());
    data[76..80].copy_from_slice(&(header.nonce as u32).to_le_bytes());
    let hash1 = Sha256::digest(data);
    let hash2 = Sha256::digest(hash1);
    let mut result = [0u8; 32];
    result.copy_from_slice(&hash2);
    result
}

/// Stratum V2 pool implementation
pub struct StratumV2Pool {
    /// Connected miners (endpoint -> connection info)
    pub miners: HashMap<String, MinerConnection>,
    /// Current block template (node-selected / last full broadcast)
    current_template: Option<Block>,
    /// Per-miner template from that connection's job declaration.
    miner_templates: HashMap<String, Block>,
    /// Snapshot id from `commons_get_coinbase_outputs` for this template.
    commons_snapshot_id: Option<String>,
    /// sha256(endpoint) miners cut from the notebook. They do not get jobs.
    banned_miners: HashSet<[u8; 32]>,
    /// Block each job was built from. Submission uses this, not a later template.
    job_blocks: HashMap<u32, Block>,
    /// Accepted shares, so a replay is not work.
    seen_shares: HashSet<SeenShare>,
    seen_order: VecDeque<SeenShare>,
    /// Current job ID counter
    job_id_counter: u32,
    /// Default min difficulty for new channels (when miner sends 0)
    default_min_difficulty: u32,
    /// Merge-mining hits recorded against a block hash.
    merge_hits: HashMap<Hash, Vec<u32>>,
}

impl StratumV2Pool {
    /// Get current block template (if any)
    pub fn current_template(&self) -> Option<&Block> {
        self.current_template.as_ref()
    }

    /// Template this miner is working. Falls back to the last broadcast template.
    pub fn template_for(&self, endpoint: &str) -> Option<&Block> {
        self.miner_templates
            .get(endpoint)
            .or(self.current_template.as_ref())
    }

    /// Block the job was created from.
    pub fn block_for_job(&self, job_id: u32) -> Option<&Block> {
        self.job_blocks.get(&job_id)
    }

    pub fn commons_snapshot_id(&self) -> Option<&str> {
        self.commons_snapshot_id.as_deref()
    }

    pub fn set_commons_snapshot_id(&mut self, id: Option<String>) {
        self.commons_snapshot_id = id;
    }

    pub fn commons_miner_id(endpoint: &str) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        Sha256::digest(endpoint.as_bytes()).into()
    }

    pub fn set_banned_miners(&mut self, ids: impl IntoIterator<Item = [u8; 32]>) {
        self.banned_miners = ids.into_iter().collect();
    }

    pub fn endpoint_is_banned(&self, endpoint: &str) -> bool {
        self.banned_miners
            .contains(&Self::commons_miner_id(endpoint))
    }

    /// Create a new pool instance
    pub fn new() -> Self {
        Self::with_default_difficulty(1)
    }

    /// Create a new pool with a default difficulty for new channels
    pub fn with_default_difficulty(default_min_difficulty: u32) -> Self {
        Self {
            miners: HashMap::new(),
            current_template: None,
            miner_templates: HashMap::new(),
            commons_snapshot_id: None,
            banned_miners: HashSet::new(),
            job_blocks: HashMap::new(),
            seen_shares: HashSet::new(),
            seen_order: VecDeque::new(),
            job_id_counter: 1,
            default_min_difficulty,
            merge_hits: HashMap::new(),
        }
    }

    /// Store a merge-mining channel on this pool.
    pub fn register_merge_channel(
        &mut self,
        endpoint: &str,
        channel_id: u32,
    ) -> Result<(), String> {
        if !self.miners.contains_key(endpoint) {
            self.register_miner(endpoint.to_string());
        }
        self.open_channel(endpoint, channel_id, 1)
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    /// Previous hash, merkle root, time, and bits of the current template.
    pub fn merge_template(&self) -> Option<(Hash, Hash, u64, u64)> {
        self.current_template.as_ref().map(|block| {
            (
                block.header.prev_block_hash,
                block.header.merkle_root,
                block.header.timestamp,
                block.header.bits,
            )
        })
    }

    /// Accept a share whose hash is within the current template target and record that hit.
    pub fn submit_merge_share(
        &mut self,
        channel_id: u32,
        share_hash: Hash,
    ) -> Result<bool, String> {
        let bits = self
            .current_template
            .as_ref()
            .map(|block| block.header.bits)
            .ok_or_else(|| "no template".to_string())?;
        let target = blvm_consensus::pow::expand_target(bits)
            .map_err(|err| err.to_string())?
            .to_be_bytes();
        if !self.hash_less_than_or_equal(&share_hash, &target) {
            return Ok(false);
        }
        self.merge_hits
            .entry(share_hash)
            .or_default()
            .push(channel_id);
        Ok(true)
    }

    /// Channels that recorded a hit for this block. An unrecorded block is an empty list.
    pub fn merge_rewards(&self, block_hash: &Hash) -> Vec<u32> {
        self.merge_hits.get(block_hash).cloned().unwrap_or_default()
    }

    /// Set default min difficulty for new channels
    pub fn set_default_difficulty(&mut self, d: u32) {
        self.default_min_difficulty = d;
    }

    /// Get default min difficulty
    pub fn default_min_difficulty(&self) -> u32 {
        self.default_min_difficulty
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
        // When miner sends 0, use 0 (easiest target, for tests). Otherwise use miner's value.
        // default_min_difficulty is for set-difficulty CLI / config; used when opening channels
        // programmatically without explicit difficulty.
        let effective = min_difficulty;
        // Calculate channel target before mutable borrow
        let channel_target = self.calculate_channel_target(effective)?;

        let miner = self.miners.get_mut(endpoint).ok_or_else(|| {
            StratumV2Error::ProtocolError(format!("Miner not registered: {endpoint}"))
        })?;
        if !miner.channels.contains_key(&channel_id)
            && miner.channels.len() >= MAX_CHANNELS_PER_MINER
        {
            return Err(StratumV2Error::ProtocolError(format!(
                "too many channels for {endpoint}"
            )));
        }

        let channel_info = ChannelInfo {
            channel_id,
            target: channel_target,
            current_job_id: None,
            min_difficulty,
            max_jobs: 10, // Default max jobs
            jobs: HashMap::new(),
        };

        miner.channels.insert(channel_id, channel_info.clone());
        info!(
            "Opened mining channel: miner={}, channel_id={}, target={:x?}",
            endpoint,
            channel_id,
            &channel_target[..8]
        );

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
            merkle_root: template.header.merkle_root,
            bits: template.header.bits as u32,
            timestamp: template.header.timestamp,
        };

        // Distribute job to all open channels
        let mut job_distributions = Vec::new();
        for miner in self.miners.values_mut() {
            if self
                .banned_miners
                .contains(&Self::commons_miner_id(&miner.endpoint))
            {
                continue;
            }
            for (channel_id, channel) in &mut miner.channels {
                channel.current_job_id = Some(job_id);
                channel.jobs.insert(job_id, job_info.clone());
                job_distributions.push((miner.endpoint.clone(), *channel_id));
            }
        }

        // Store template
        self.job_blocks.insert(job_id, template.clone());
        self.current_template = Some(template);
        self.miner_templates.clear();
        self.cleanup_old_jobs();

        info!(
            "Created new mining job: job_id={}, channels={}",
            job_id,
            job_distributions.len()
        );
        (job_id, job_distributions)
    }

    /// Assign a job to one miner. Does not replace other miners' work.
    pub fn set_template_for(
        &mut self,
        endpoint: &str,
        template: Block,
    ) -> (u32, Vec<(String, u32)>) {
        let job_id = self.job_id_counter;
        self.job_id_counter = self.job_id_counter.wrapping_add(1);

        let job_info = JobInfo {
            job_id,
            prev_hash: template.header.prev_block_hash,
            merkle_root: template.header.merkle_root,
            bits: template.header.bits as u32,
            timestamp: template.header.timestamp,
        };

        let mut job_distributions = Vec::new();
        if self.endpoint_is_banned(endpoint) {
            return (job_id, job_distributions);
        }
        if let Some(miner) = self.miners.get_mut(endpoint) {
            for (channel_id, channel) in &mut miner.channels {
                channel.current_job_id = Some(job_id);
                channel.jobs.insert(job_id, job_info.clone());
                job_distributions.push((miner.endpoint.clone(), *channel_id));
            }
        }
        self.job_blocks.insert(job_id, template.clone());
        self.miner_templates.insert(endpoint.to_string(), template);
        self.cleanup_old_jobs();

        info!(
            "Created miner job: job_id={}, endpoint={}, channels={}",
            job_id,
            endpoint,
            job_distributions.len()
        );
        (job_id, job_distributions)
    }

    /// Handle share submission
    pub fn handle_share(
        &mut self,
        endpoint: &str,
        share: ShareData,
    ) -> Result<(bool, bool), StratumV2Error> {
        // Returns (is_valid_share, is_valid_block)
        // Extract data and update initial stats in a block to release mutable borrow
        let (job_info, channel_target, endpoint_key) = {
            let miner = self.miners.get_mut(endpoint).ok_or_else(|| {
                StratumV2Error::ProtocolError(format!("Miner not registered: {endpoint}"))
            })?;

            miner.stats.total_shares += 1;
            miner.stats.last_share_time = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            );

            let channel = miner.channels.get(&share.channel_id).ok_or_else(|| {
                StratumV2Error::ProtocolError(format!("Channel not found: {}", share.channel_id))
            })?;

            let job_info = channel
                .jobs
                .get(&share.job_id)
                .ok_or_else(|| {
                    StratumV2Error::ProtocolError(format!("Job not found: {}", share.job_id))
                })?
                .clone();
            let channel_target = channel.target;

            (job_info, channel_target, endpoint.to_string())
        };

        let seen = SeenShare {
            channel_id: share.channel_id,
            job_id: share.job_id,
            nonce: share.nonce,
            version: share.version,
            merkle_root: share.merkle_root,
        };
        let replay = self.seen_shares.contains(&seen);

        // Validate share (no mutable borrow held)
        let is_valid_share = !replay && self.validate_share(&share, &job_info, &channel_target);

        // Check if this is a valid block (meets network difficulty)
        let is_valid_block = if is_valid_share {
            self.validate_block(&share, &job_info)
        } else {
            false
        };

        // Banned miners do not land shares. A valid header is still submitted (H8).
        let is_valid_share =
            if is_valid_share && self.endpoint_is_banned(endpoint) && !is_valid_block {
                false
            } else {
                is_valid_share
            };

        // Update stats (re-acquire mutable borrow)
        if is_valid_share {
            self.remember_share(seen);
        }
        if let Some(miner) = self.miners.get_mut(&endpoint_key) {
            if is_valid_share {
                miner.stats.accepted_shares += 1;
                if is_valid_block {
                    info!(
                        "Valid block found from {}: channel={}, job={}",
                        endpoint_key, share.channel_id, share.job_id
                    );
                } else {
                    debug!(
                        "Accepted share from {}: channel={}, job={}",
                        endpoint_key, share.channel_id, share.job_id
                    );
                }
            } else {
                miner.stats.rejected_shares += 1;
                warn!(
                    "Rejected share from {}: channel={}, job={}",
                    endpoint_key, share.channel_id, share.job_id
                );
            }
        }

        Ok((is_valid_share, is_valid_block))
    }

    fn remember_share(&mut self, seen: SeenShare) {
        if self.seen_shares.contains(&seen) {
            return;
        }
        while self.seen_order.len() >= MAX_SEEN_SHARES {
            if let Some(old) = self.seen_order.pop_front() {
                self.seen_shares.remove(&old);
            } else {
                break;
            }
        }
        self.seen_shares.insert(seen.clone());
        self.seen_order.push_back(seen);
    }

    /// Validate if share meets network difficulty (is a valid block)
    fn validate_block(&self, share: &ShareData, job_info: &JobInfo) -> bool {
        use blvm_consensus::ConsensusProof;
        use blvm_protocol::BlockHeader;

        // Construct block header
        let header = BlockHeader {
            version: share.version,
            prev_block_hash: job_info.prev_hash,
            merkle_root: share.merkle_root,
            timestamp: job_info.timestamp,
            bits: job_info.bits as u64,
            nonce: share.nonce as u64,
        };

        // Verify proof of work using consensus (validates against network target)
        let consensus = ConsensusProof::new();
        consensus.check_proof_of_work(&header).unwrap_or_default()
    }

    /// Clean up old jobs from channels
    /// Removes jobs older than max_jobs per channel
    pub fn cleanup_old_jobs(&mut self) {
        let mut live = HashSet::new();
        for miner in self.miners.values_mut() {
            for channel in miner.channels.values_mut() {
                // Keep only the most recent max_jobs
                if channel.jobs.len() > channel.max_jobs as usize {
                    // Sort jobs by ID (higher ID = newer)
                    let mut job_ids: Vec<u32> = channel.jobs.keys().cloned().collect();
                    job_ids.sort();
                    job_ids.reverse(); // Newest first

                    // Remove oldest jobs
                    let to_remove = job_ids.split_off(channel.max_jobs as usize);
                    for job_id in to_remove {
                        channel.jobs.remove(&job_id);
                    }
                }
                live.extend(channel.jobs.keys().copied());
            }
        }
        self.job_blocks.retain(|job_id, _| live.contains(job_id));
    }

    /// Remove disconnected miners (those with no recent shares).
    /// Returns list of removed endpoint identifiers for event publishing.
    pub fn cleanup_disconnected_miners(&mut self, timeout_seconds: u64) -> Vec<String> {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut to_remove = Vec::new();
        for (endpoint, miner) in &self.miners {
            if let Some(last_share) = miner.stats.last_share_time {
                if current_time.saturating_sub(last_share) > timeout_seconds {
                    to_remove.push(endpoint.clone());
                }
            } else {
                // No shares ever submitted, remove if channels are empty
                if miner.channels.is_empty() {
                    to_remove.push(endpoint.clone());
                }
            }
        }

        for endpoint in &to_remove {
            self.miners.remove(endpoint);
            info!("Removed disconnected miner: {}", endpoint);
        }
        self.cleanup_old_jobs();
        to_remove
    }

    /// Validate a share using blvm-consensus functions
    pub fn validate_share(&self, share: &ShareData, job_info: &JobInfo, target: &Hash) -> bool {
        use blvm_protocol::BlockHeader;
        use std::time::{SystemTime, UNIX_EPOCH};

        // 1. Basic validation - check job exists
        if share.job_id != job_info.job_id {
            return false;
        }
        if share.merkle_root != job_info.merkle_root {
            return false;
        }

        // 2. Timestamp validation - check it's within reasonable bounds
        // Allow timestamp within 2 hours (network time drift) or 20 years (genesis/test blocks)
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let timestamp = job_info.timestamp as i64;
        let time_diff = (timestamp - current_time).abs();
        if time_diff > 630_720_000 {
            // Reject if >20 years (allows genesis blocks ~14 years old)
            return false;
        }

        // 3. Construct block header from share data and job info
        let header = BlockHeader {
            version: share.version,
            prev_block_hash: job_info.prev_hash,
            merkle_root: share.merkle_root,
            timestamp: job_info.timestamp,
            bits: job_info.bits as u64,
            nonce: share.nonce as u64,
        };

        // 4. A share meets the channel target. Network proof of work is `validate_block`.
        // The header hash is a little-endian Bitcoin uint256. The channel target is big-endian.
        let mut block_hash = self.calculate_block_hash(&header);
        block_hash.reverse();
        self.hash_less_than_or_equal(&block_hash, target)
    }

    /// Get share hash for a valid share (for event publishing)
    /// Returns None if job not found
    pub fn get_share_hash(&self, endpoint: &str, share: &ShareData) -> Option<Hash> {
        use blvm_protocol::BlockHeader;

        let miner = self.miners.get(endpoint)?;
        let channel = miner.channels.get(&share.channel_id)?;
        let job_info = channel.jobs.get(&share.job_id)?;
        let header = BlockHeader {
            version: share.version,
            prev_block_hash: job_info.prev_hash,
            merkle_root: share.merkle_root,
            timestamp: job_info.timestamp,
            bits: job_info.bits as u64,
            nonce: share.nonce as u64,
        };
        Some(self.calculate_block_hash(&header))
    }

    /// Calculate block hash (double SHA256 of header)
    /// Uses the same serialization as consensus layer for consistency
    pub fn calculate_block_hash(&self, header: &blvm_protocol::BlockHeader) -> Hash {
        stratum_header_hash(header)
    }

    /// Compare two hashes as big-endian integers
    /// Returns true if hash1 <= hash2 (hash1 is easier/higher value)
    pub fn hash_less_than_or_equal(&self, hash1: &Hash, hash2: &Hash) -> bool {
        // Compare byte by byte (big-endian)
        for i in 0..32 {
            if hash1[i] < hash2[i] {
                return true;
            } else if hash1[i] > hash2[i] {
                return false;
            }
        }
        // Equal
        true
    }

    /// Calculate channel target from difficulty.
    ///
    /// Zero accepts any header hash (tests and open-ended channels).
    /// A positive value starts at Bitcoin difficulty 1 (`0x1d00ffff`) and
    /// grows easier as the value increases. The old compact expansion kept
    /// the target in the low 128 bits, so every real header failed it.
    pub fn calculate_channel_target(&self, min_difficulty: u32) -> Result<Hash, StratumV2Error> {
        if min_difficulty == 0 {
            return Ok([0xFFu8; 32]);
        }
        let base = blvm_consensus::pow::expand_target(0x1d00ffff)
            .map_err(|err| StratumV2Error::PoolError(err.to_string()))?
            .to_be_bytes();
        Ok(scale_target(base, min_difficulty))
    }
}

/// Multiply a 256-bit big-endian target. Overflow becomes the maximum target.
fn scale_target(target: [u8; 32], factor: u32) -> [u8; 32] {
    if factor <= 1 {
        return target;
    }
    let mut acc = [0u8; 32];
    let mut carry = 0u64;
    for i in (0..32).rev() {
        let value = u64::from(target[i]) * u64::from(factor) + carry;
        acc[i] = (value & 0xff) as u8;
        carry = value >> 8;
    }
    if carry > 0 { [0xff; 32] } else { acc }
}

impl Default for StratumV2Pool {
    fn default() -> Self {
        Self::new()
    }
}
