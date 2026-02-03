//! Stratum V2 server implementation

use crate::error::StratumV2Error;
use crate::pool::StratumV2Pool;
use crate::template::BlockTemplateGenerator;
use bllvm_node::module::ipc::protocol::ModuleMessage;
use bllvm_node::module::traits::{EventPayload, EventType, NodeAPI};
use bllvm_protocol::{Block, Hash};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Stratum V2 server
pub struct StratumV2Server {
    /// Server configuration
    listen_addr: String,
    /// Mining pool for managing miners and jobs
    pool: Arc<RwLock<StratumV2Pool>>,
    /// Node API for querying node state
    node_api: Arc<dyn NodeAPI>,
    /// Block template generator
    template_generator: Arc<BlockTemplateGenerator>,
    /// Whether server is running
    running: Arc<RwLock<bool>>,
}

impl StratumV2Server {
    /// Create a new Stratum V2 server
    pub async fn new(
        ctx: &bllvm_node::module::traits::ModuleContext,
        node_api: Arc<dyn NodeAPI>,
    ) -> Result<Self, StratumV2Error> {
        let listen_addr = ctx.get_config_or("stratum_v2.listen_addr", "0.0.0.0:3333");
        
        let pool = Arc::new(RwLock::new(StratumV2Pool::new()));
        let template_generator = Arc::new(BlockTemplateGenerator::new(Arc::clone(&node_api)));
        
        Ok(Self {
            listen_addr,
            pool,
            node_api,
            template_generator,
            running: Arc::new(RwLock::new(false)),
        })
    }
    
    /// Start the server
    pub async fn start(&self) -> Result<(), StratumV2Error> {
        let mut running = self.running.write().await;
        if *running {
            return Err(StratumV2Error::ConfigError("Server already running".to_string()));
        }
        
        info!("Starting Stratum V2 server on {}", self.listen_addr);
        
        // Server startup:
        // 1. The node's network layer will handle incoming connections
        // 2. Messages will be routed to this module via IPC
        // 3. This module handles protocol messages via handle_message()
        // 4. For now, we mark the server as running and ready to accept messages
        
        *running = true;
        info!("Stratum V2 server started and ready to accept connections");
        Ok(())
    }
    
    /// Stop the server
    pub async fn stop(&self) -> Result<(), StratumV2Error> {
        let mut running = self.running.write().await;
        if !*running {
            return Ok(());
        }
        
        info!("Stopping Stratum V2 server");
        *running = false;
        Ok(())
    }
    
    /// Handle an event from the node
    pub async fn handle_event(
        &self,
        event: &ModuleMessage,
        node_api: &dyn NodeAPI,
    ) -> Result<(), StratumV2Error> {
        match event {
            ModuleMessage::Event(event_msg) => {
                match event_msg.event_type {
                    EventType::BlockMined => {
                        debug!("Block mined event received");
                        if let EventPayload::BlockMined { block_hash, height } = &event_msg.payload {
                            info!("Block mined: hash={:x?}, height={}", &block_hash[..8], height);
                            // Update pool with new block
                            // Notify miners of new block via SetNewPrevHashMessage
                            if let Some(block) = node_api.get_block(block_hash).await
                                .map_err(|e| StratumV2Error::ServerError(format!("Failed to get block: {}", e)))? {
                                // Update pool with new block
                                self.pool.write().await.set_template(block, node_api.as_ref()).await
                                    .map_err(|e| StratumV2Error::ServerError(format!("Failed to update pool: {}", e)))?;
                                
                                // Broadcast SetNewPrevHashMessage to all miners
                                // Get all active miners and their channels
                                let pool = self.pool.read().await;
                                let miners: Vec<(String, Vec<u32>)> = pool.miners.iter()
                                    .map(|(endpoint, miner)| {
                                        let channels: Vec<u32> = miner.channels.keys().cloned().collect();
                                        (endpoint.clone(), channels)
                                    })
                                    .collect();
                                drop(pool);
                                
                                // Send SetNewPrevHashMessage to each miner's channels
                                for (endpoint, channels) in miners {
                                    for channel_id in channels {
                                        let msg = messages::SetNewPrevHashMessage {
                                            channel_id,
                                            job_id: 0, // Will be set per job
                                            prev_hash: *block_hash,
                                            min_ntime: block.header.timestamp,
                                        };
                                        
                                        // Serialize and encode message
                                        let payload = msg.to_bytes()
                                            .map_err(|e| StratumV2Error::ProtocolError(format!("Failed to serialize SetNewPrevHash: {}", e)))?;
                                        
                                        let mut encoder = crate::protocol::TlvEncoder::new();
                                        let encoded = encoder.encode(msg.message_type(), &payload)
                                            .map_err(|e| StratumV2Error::ProtocolError(format!("Failed to encode SetNewPrevHash: {}", e)))?;
                                        
                                        // Send message to miner via network layer
                                        if let Err(e) = self.node_api.send_stratum_v2_message_to_peer(
                                            endpoint.clone(),
                                            encoded.clone(),
                                        ).await {
                                            warn!("Failed to send SetNewPrevHash to miner {}: {}", endpoint, e);
                                        } else {
                                            debug!("Sent SetNewPrevHash to miner {} channel {}", endpoint, channel_id);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    EventType::BlockTemplateUpdated => {
                        debug!("Block template updated event received");
                        if let EventPayload::BlockTemplateUpdated { prev_hash, height, tx_count } = &event_msg.payload {
                            info!(
                                "Block template updated: prev_hash={:x?}, height={}, tx_count={}",
                                &prev_hash[..8],
                                height,
                                tx_count
                            );
                            
                            // Generate new block template
                            match self.template_generator.generate_template().await {
                                Ok(template) => {
                                    self.update_block_template(template).await?;
                                }
                                Err(e) => {
                                    warn!("Failed to generate block template: {}", e);
                                }
                            }
                        }
                    }
                    EventType::MiningDifficultyChanged => {
                        debug!("Mining difficulty changed event received");
                        if let EventPayload::MiningDifficultyChanged { old_difficulty, new_difficulty, height } = &event_msg.payload {
                            info!(
                                "Mining difficulty changed: old={}, new={}, height={}",
                                old_difficulty, new_difficulty, height
                            );
                            // Update pool difficulty settings
                            // The pool will recalculate channel targets on next job creation
                            debug!("Pool difficulty will be updated on next job creation");
                        }
                    }
                    EventType::MiningJobCreated => {
                        debug!("Mining job created event received");
                        // Job creation is handled by BlockTemplateUpdated
                    }
                    EventType::ShareSubmitted => {
                        debug!("Share submitted event received");
                        // Share submission is handled by pool
                    }
                    EventType::StratumV2MessageReceived => {
                        debug!("Stratum V2 message received event");
                        if let EventPayload::StratumV2MessageReceived { message_data, peer_addr } = &event_msg.payload {
                            // Handle incoming Stratum V2 message
                            match self.handle_message(message_data.clone(), peer_addr.clone()).await {
                                Ok(response_data) => {
                                    // Send response back to miner
                                    if let Err(e) = self.node_api.send_stratum_v2_message_to_peer(
                                        peer_addr.clone(),
                                        response_data,
                                    ).await {
                                        warn!("Failed to send Stratum V2 response to {}: {}", peer_addr, e);
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to handle Stratum V2 message from {}: {}", peer_addr, e);
                                }
                            }
                        }
                    }
                    _ => {
                        // Ignore other events
                    }
                }
            }
            _ => {
                // Not an event message
            }
        }
        
        Ok(())
    }
    
    /// Update block template and distribute to miners
    async fn update_block_template(&self, template: Block) -> Result<(), StratumV2Error> {
        use crate::messages::{NewMiningJobMessage, StratumV2Message};
        use crate::protocol::TlvEncoder;

        let mut pool = self.pool.write().await;
        let (job_id, job_distributions) = pool.set_template(template.clone());
        
        info!("Updated block template: job_id={}, channels={}", job_id, job_distributions.len());
        
        // Extract merkle path and coinbase from template
        let (coinbase_prefix, coinbase_suffix, merkle_path) = self.extract_template_parts(&template);
        
        // Send NewMiningJob messages to all channels
        for (endpoint, channel_id) in job_distributions {
            let job_msg = NewMiningJobMessage {
                channel_id,
                job_id,
                prev_hash: template.header.prev_block_hash,
                coinbase_prefix: coinbase_prefix.clone(),
                coinbase_suffix: coinbase_suffix.clone(),
                merkle_path: merkle_path.clone(),
            };
            
            // Serialize message
            let payload = job_msg.to_bytes()
                .map_err(|e| StratumV2Error::ProtocolError(format!("Failed to serialize job message: {}", e)))?;
            
            // Encode with TLV
            let mut encoder = TlvEncoder::new();
            let encoded = encoder.encode(job_msg.message_type(), &payload)
                .map_err(|e| StratumV2Error::ProtocolError(format!("Failed to encode job message: {}", e)))?;
            
            debug!("Prepared job {} for miner {} channel {}", job_id, endpoint, channel_id);
            
            // Send encoded message to miner via network layer
            if let Err(e) = self.node_api.send_stratum_v2_message_to_peer(endpoint.clone(), encoded.clone()).await {
                warn!("Failed to send job message to miner {}: {}", endpoint, e);
            } else {
                debug!("Sent job {} to miner {} channel {}", job_id, endpoint, channel_id);
            }
        }
        
        Ok(())
    }
    
    /// Extract template parts for Stratum V2 job message
    fn extract_template_parts(&self, template: &Block) -> (Vec<u8>, Vec<u8>, Vec<Hash>) {
        use sha2::{Digest, Sha256};
        
        // For Stratum V2, we need to split the coinbase transaction
        // and provide merkle path for transaction inclusion
        
        let coinbase = if !template.transactions.is_empty() {
            &template.transactions[0]
        } else {
            // No coinbase - return empty
            return (Vec::new(), Vec::new(), Vec::new());
        };
        
        // Serialize coinbase transaction properly
        // For Stratum V2, we split at a specific point to allow miners to modify
        // the coinbase script while keeping the structure intact
        let coinbase_bytes = self.serialize_transaction(coinbase);
        
        // Split coinbase for Stratum V2
        // Split after: version (4) + input count (varint, typically 1 byte) + 
        // first input (prevout hash 32 + index 4 + script length varint + script + sequence 4)
        // We'll split after the script length varint, allowing miners to modify the script
        let mut split_point = 4; // version
        if coinbase_bytes.len() > split_point {
            // Skip input count varint (typically 1 byte for count=1)
            split_point += 1;
        }
        if coinbase_bytes.len() > split_point + 36 {
            // Skip prevout (32 bytes hash + 4 bytes index)
            split_point += 36;
        }
        if coinbase_bytes.len() > split_point {
            // Read script length varint (1-9 bytes)
            let (script_len, script_len_bytes) = self.read_varint(&coinbase_bytes[split_point..]);
            split_point += script_len_bytes;
        }
        // Split after script length - miners can modify script
        // Ensure we have a reasonable split point
        if split_point >= coinbase_bytes.len() {
            split_point = coinbase_bytes.len() / 2;
        }
        
        let coinbase_prefix = coinbase_bytes[..split_point].to_vec();
        let coinbase_suffix = coinbase_bytes[split_point..].to_vec();
        
        // Calculate merkle path for transactions
        // Merkle path is the list of sibling hashes needed to compute merkle root from coinbase
        let mut merkle_path = Vec::new();
        
        if template.transactions.len() > 1 {
            // Calculate transaction hashes (double SHA256)
            let mut tx_hashes = Vec::new();
            for tx in template.transactions.iter() {
                let tx_bytes = self.serialize_transaction(tx);
                let hash1 = Sha256::digest(&tx_bytes);
                let hash2 = Sha256::digest(hash1);
                let mut tx_hash = [0u8; 32];
                tx_hash.copy_from_slice(&hash2);
                tx_hashes.push(tx_hash);
            }
            
            // Build merkle tree and extract path from coinbase (index 0) to root
            // Track coinbase position through tree levels
            let mut level = tx_hashes;
            let mut coinbase_pos = 0; // Coinbase is always at index 0 in first level
            
            while level.len() > 1 {
                let mut next_level = Vec::new();
                let mut sibling_hash: Option<Hash> = None;
                
                // Process pairs
                for i in (0..level.len()).step_by(2) {
                    if i + 1 < level.len() {
                        // Pair of hashes - combine and hash
                        let mut combined = level[i].to_vec();
                        combined.extend_from_slice(&level[i + 1]);
                        let hash1 = Sha256::digest(&combined);
                        let hash2 = Sha256::digest(hash1);
                        let mut parent_hash = [0u8; 32];
                        parent_hash.copy_from_slice(&hash2);
                        next_level.push(parent_hash);
                        
                        // Track coinbase's sibling
                        let pair_index = i / 2;
                        if coinbase_pos == i {
                            // Coinbase is first in pair, sibling is second
                            sibling_hash = Some(level[i + 1]);
                            coinbase_pos = pair_index;
                        } else if coinbase_pos == i + 1 {
                            // Coinbase is second in pair, sibling is first
                            sibling_hash = Some(level[i]);
                            coinbase_pos = pair_index;
                        } else if coinbase_pos == pair_index {
                            // Coinbase is in this pair's parent, no sibling at this level
                        }
                    } else {
                        // Odd one out - duplicate and hash
                        let mut combined = level[i].to_vec();
                        combined.extend_from_slice(&level[i]);
                        let hash1 = Sha256::digest(&combined);
                        let hash2 = Sha256::digest(hash1);
                        let mut parent_hash = [0u8; 32];
                        parent_hash.copy_from_slice(&hash2);
                        next_level.push(parent_hash);
                        
                        // If coinbase is the odd one, no sibling
                        let pair_index = i / 2;
                        if coinbase_pos == i {
                            coinbase_pos = pair_index;
                            // No sibling when duplicated
                        }
                    }
                }
                
                // Add sibling to path if found
                if let Some(sibling) = sibling_hash {
                    merkle_path.push(sibling);
                }
                
                level = next_level;
            }
        }
        
        (coinbase_prefix, coinbase_suffix, merkle_path)
    }
    
    /// Serialize transaction for hashing (matches consensus layer format)
    pub fn serialize_transaction(&self, tx: &bllvm_protocol::Transaction) -> Vec<u8> {
        let mut data = Vec::new();
        
        // Version (4 bytes, little-endian)
        data.extend_from_slice(&(tx.version as u32).to_le_bytes());
        
        // Input count (varint)
        data.extend_from_slice(&self.encode_varint(tx.inputs.len() as u64));
        
        // Inputs
        for input in &tx.inputs {
            // Previous output hash (32 bytes)
            data.extend_from_slice(&input.prevout.hash);
            // Previous output index (4 bytes, little-endian)
            data.extend_from_slice(&(input.prevout.index as u32).to_le_bytes());
            // Script length (varint)
            data.extend_from_slice(&self.encode_varint(input.script_sig.len() as u64));
            // Script
            data.extend_from_slice(&input.script_sig);
            // Sequence (4 bytes, little-endian)
            data.extend_from_slice(&(input.sequence as u32).to_le_bytes());
        }
        
        // Output count (varint)
        data.extend_from_slice(&self.encode_varint(tx.outputs.len() as u64));
        
        // Outputs
        for output in &tx.outputs {
            // Value (8 bytes, little-endian)
            data.extend_from_slice(&output.value.to_le_bytes());
            // Script length (varint)
            data.extend_from_slice(&self.encode_varint(output.script_pubkey.len() as u64));
            // Script
            data.extend_from_slice(&output.script_pubkey);
        }
        
        // Lock time (4 bytes, little-endian)
        data.extend_from_slice(&(tx.lock_time as u32).to_le_bytes());
        
        data
    }
    
    /// Encode varint (variable-length integer)
    pub fn encode_varint(&self, value: u64) -> Vec<u8> {
        let mut result = Vec::new();
        if value < 0xfd {
            result.push(value as u8);
        } else if value <= 0xffff {
            result.push(0xfd);
            result.extend_from_slice(&(value as u16).to_le_bytes());
        } else if value <= 0xffffffff {
            result.push(0xfe);
            result.extend_from_slice(&(value as u32).to_le_bytes());
        } else {
            result.push(0xff);
            result.extend_from_slice(&value.to_le_bytes());
        }
        result
    }
    
    /// Read varint from bytes
    pub fn read_varint(&self, data: &[u8]) -> (u64, usize) {
        if data.is_empty() {
            return (0, 0);
        }
        match data[0] {
            n if n < 0xfd => (n as u64, 1),
            0xfd => {
                if data.len() < 3 {
                    return (0, 0);
                }
                let value = u16::from_le_bytes([data[1], data[2]]);
                (value as u64, 3)
            }
            0xfe => {
                if data.len() < 5 {
                    return (0, 0);
                }
                let mut bytes = [0u8; 4];
                bytes.copy_from_slice(&data[1..5]);
                let value = u32::from_le_bytes(bytes);
                (value as u64, 5)
            }
            0xff => {
                if data.len() < 9 {
                    return (0, 0);
                }
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&data[1..9]);
                let value = u64::from_le_bytes(bytes);
                (value, 9)
            }
            _ => (0, 0),
        }
    }
    
    /// Handle incoming Stratum V2 message
    pub async fn handle_message(
        &self,
        data: Vec<u8>,
        endpoint: String,
    ) -> Result<Vec<u8>, StratumV2Error> {
        use crate::messages::*;
        use crate::protocol::TlvDecoder;
        
        // Decode TLV message
        let (tag, payload) = TlvDecoder::decode_raw(&data)?;
        
        // Deserialize message based on tag
        let response_bytes = match tag {
            message_types::SETUP_CONNECTION => {
                let msg: SetupConnectionMessage = SetupConnectionMessage::from_bytes(&payload)?;
                match self.handle_setup_connection(msg, &endpoint).await {
                    Ok(response) => {
                        let response_payload = response.to_bytes()?;
                        let mut encoder = crate::protocol::TlvEncoder::new();
                        encoder.encode(response.message_type(), &response_payload)?
                    }
                    Err(e) => {
                        // Return error response
                        let error_msg = crate::messages::SetupConnectionErrorMessage {
                            error_code: 1, // Setup failed
                            error_message: format!("Setup connection failed: {}", e),
                        };
                        let response_payload = error_msg.to_bytes()?;
                        let mut encoder = crate::protocol::TlvEncoder::new();
                        encoder.encode(error_msg.message_type(), &response_payload)?
                    }
                }
            }
            message_types::OPEN_MINING_CHANNEL => {
                let msg: OpenMiningChannelMessage = OpenMiningChannelMessage::from_bytes(&payload)?;
                match self.handle_open_channel(&endpoint, msg.clone()).await {
                    Ok(response) => {
                        let response_payload = response.to_bytes()?;
                        let mut encoder = crate::protocol::TlvEncoder::new();
                        encoder.encode(response.message_type(), &response_payload)?
                    }
                    Err(e) => {
                        // Return error response
                        let error_msg = crate::messages::OpenMiningChannelErrorMessage {
                            channel_id: msg.channel_id,
                            request_id: msg.request_id,
                            error_code: 2, // Channel open failed
                            error_message: format!("Failed to open channel: {}", e),
                        };
                        let response_payload = error_msg.to_bytes()?;
                        let mut encoder = crate::protocol::TlvEncoder::new();
                        encoder.encode(error_msg.message_type(), &response_payload)?
                    }
                }
            }
            message_types::SUBMIT_SHARES => {
                let msg: SubmitSharesMessage = SubmitSharesMessage::from_bytes(&payload)?;
                match self.handle_submit_shares(&endpoint, msg.clone()).await {
                    Ok(response) => {
                        let response_payload = response.to_bytes()?;
                        let mut encoder = crate::protocol::TlvEncoder::new();
                        encoder.encode(response.message_type(), &response_payload)?
                    }
                    Err(e) => {
                        // Return error response
                        let error_msg = crate::messages::SubmitSharesErrorMessage {
                            channel_id: msg.channel_id,
                            job_id: msg.shares.first().map(|s| s.job_id).unwrap_or(0),
                            error_code: 3, // Share submission failed
                            error_message: format!("Failed to process shares: {}", e),
                        };
                        let response_payload = error_msg.to_bytes()?;
                        let mut encoder = crate::protocol::TlvEncoder::new();
                        encoder.encode(error_msg.message_type(), &response_payload)?
                    }
                }
            }
            _ => {
                return Err(StratumV2Error::ProtocolError(format!("Unknown message type: {}", tag)));
            }
        };
        
        Ok(response_bytes)
    }
    
    /// Handle Setup Connection message
    async fn handle_setup_connection(
        &self,
        msg: crate::messages::SetupConnectionMessage,
        endpoint: &str,
    ) -> Result<crate::messages::SetupConnectionSuccessMessage, StratumV2Error> {
        // Validate protocol version
        if msg.protocol_version != 2 {
            return Err(StratumV2Error::ProtocolError(format!(
                "Unsupported protocol version: {}. Only version 2 is supported.",
                msg.protocol_version
            )));
        }
        
        let mut pool = self.pool.write().await;
        pool.register_miner(endpoint.to_string());
        
        Ok(crate::messages::SetupConnectionSuccessMessage {
            supported_versions: vec![2], // Stratum V2
            capabilities: vec!["mining".to_string()],
        })
    }
    
    /// Handle Open Mining Channel message
    async fn handle_open_channel(
        &self,
        endpoint: &str,
        msg: crate::messages::OpenMiningChannelMessage,
    ) -> Result<crate::messages::OpenMiningChannelSuccessMessage, StratumV2Error> {
        let mut pool = self.pool.write().await;
        let target = pool.open_channel(endpoint, msg.channel_id, msg.min_difficulty)?;
        
        Ok(crate::messages::OpenMiningChannelSuccessMessage {
            channel_id: msg.channel_id,
            request_id: msg.request_id,
            target,
            max_jobs: 10,
        })
    }
    
    /// Handle Submit Shares message
    async fn handle_submit_shares(
        &self,
        endpoint: &str,
        msg: crate::messages::SubmitSharesMessage,
    ) -> Result<crate::messages::SubmitSharesSuccessMessage, StratumV2Error> {
        use crate::pool::ShareData;
        
        let mut pool = self.pool.write().await;
        
        // Process each share
        let mut last_job_id = 0;
        let mut has_valid_block = false;
        
        for share in msg.shares {
            let share_data = ShareData {
                channel_id: share.channel_id,
                job_id: share.job_id,
                nonce: share.nonce,
                version: share.version,
                merkle_root: share.merkle_root,
            };
            
            let (is_valid_share, is_valid_block) = pool.handle_share(endpoint, share_data)?;
            
            if is_valid_block {
                has_valid_block = true;
                // Submit block to node
                if let Some(template) = &pool.current_template {
                    // Reconstruct block from share and template
                    if let Err(e) = self.submit_block_from_share(template, &share_data).await {
                        warn!("Failed to submit block: {}", e);
                    }
                }
            }
            
            last_job_id = share.job_id;
        }
        
        // Cleanup old jobs periodically
        pool.cleanup_old_jobs();
        
        Ok(crate::messages::SubmitSharesSuccessMessage {
            channel_id: msg.channel_id,
            last_job_id,
        })
    }
    
    /// Submit a valid block to the node
    async fn submit_block_from_share(
        &self,
        template: &bllvm_protocol::Block,
        share: &crate::pool::ShareData,
    ) -> Result<(), StratumV2Error> {
        use bllvm_protocol::{Block, BlockHeader};
        
        // Reconstruct block header with share data
        let mut header = template.header.clone();
        header.version = share.version as i32;
        header.merkle_root = share.merkle_root;
        header.nonce = share.nonce as u64;
        // Timestamp can be adjusted by miner, but we'll use template timestamp for now
        // In production, miners would provide timestamp in share
        
        // Reconstruct full block
        let block = Block {
            header,
            transactions: template.transactions.clone(),
        };
        
        // Submit to node
        match self.node_api.submit_block(block).await {
            Ok(result) => {
                match result {
                    bllvm_node::module::traits::SubmitBlockResult::Accepted => {
                        info!("Block submitted and accepted by node");
                    }
                    bllvm_node::module::traits::SubmitBlockResult::Rejected(reason) => {
                        warn!("Block submitted but rejected: {}", reason);
                    }
                    bllvm_node::module::traits::SubmitBlockResult::Duplicate => {
                        debug!("Block already known to node");
                    }
                }
                Ok(())
            }
            Err(e) => {
                Err(StratumV2Error::ServerError(format!("Failed to submit block: {}", e)))
            }
        }
    }
    
    /// Get mining pool statistics
    pub async fn get_pool_stats(&self) -> PoolStats {
        let pool = self.pool.read().await;
        PoolStats {
            miner_count: pool.miners.len(),
            total_shares: pool.miners.values()
                .map(|m| m.stats.total_shares)
                .sum(),
            accepted_shares: pool.miners.values()
                .map(|m| m.stats.accepted_shares)
                .sum(),
            rejected_shares: pool.miners.values()
                .map(|m| m.stats.rejected_shares)
                .sum(),
        }
    }
    
    /// Get pool reference for cleanup operations
    pub fn get_pool(&self) -> Arc<RwLock<StratumV2Pool>> {
        Arc::clone(&self.pool)
    }
}

/// Mining pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub miner_count: usize,
    pub total_shares: u64,
    pub accepted_shares: u64,
    pub rejected_shares: u64,
}

