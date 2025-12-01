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
        
        // Serialize coinbase
        let coinbase_bytes = bincode::serialize(coinbase).unwrap_or_default();
        
        // Split coinbase (simplified - in production would split at specific points)
        // For Stratum V2, typically split after the first few bytes (version, input count, etc.)
        // For now, use a simple midpoint split
        let split_point = coinbase_bytes.len() / 2;
        let coinbase_prefix = coinbase_bytes[..split_point].to_vec();
        let coinbase_suffix = coinbase_bytes[split_point..].to_vec();
        
        // Calculate merkle path for transactions
        // Merkle path is the list of hashes needed to compute merkle root
        let mut merkle_path = Vec::new();
        
        if template.transactions.len() > 1 {
            // Calculate transaction hashes
            let mut tx_hashes = Vec::new();
            for tx in template.transactions.iter() {
                let tx_bytes = bincode::serialize(tx).unwrap_or_default();
                let hash = Sha256::digest(&tx_bytes);
                tx_hashes.push(hash);
            }
            
            // Build merkle tree and extract path from coinbase (index 0) to root
            // For Stratum V2, we need the merkle path: list of sibling hashes from coinbase to root
            // The path allows miners to compute the merkle root without all transactions
            
            // Coinbase is at index 0, so we need siblings at each level
            let mut level = tx_hashes.clone();
            let mut coinbase_index = 0; // Track coinbase position through tree levels
            
            while level.len() > 1 {
                let mut next_level = Vec::new();
                let mut current_path_hashes = Vec::new();
                
                // Build next level and track coinbase's sibling
                for i in (0..level.len()).step_by(2) {
                    if i + 1 < level.len() {
                        // Pair of hashes
                        let mut combined = level[i].to_vec();
                        combined.extend_from_slice(&level[i + 1]);
                        let hash = Sha256::digest(&combined);
                        next_level.push(hash);
                        
                        // If coinbase is in this pair, store its sibling
                        if coinbase_index == i {
                            // Coinbase is first, sibling is second
                            let mut sibling_hash = [0u8; 32];
                            sibling_hash.copy_from_slice(&level[i + 1]);
                            current_path_hashes.push(sibling_hash);
                            coinbase_index = next_level.len() - 1; // Update coinbase position
                        } else if coinbase_index == i + 1 {
                            // Coinbase is second, sibling is first
                            let mut sibling_hash = [0u8; 32];
                            sibling_hash.copy_from_slice(&level[i]);
                            current_path_hashes.push(sibling_hash);
                            coinbase_index = next_level.len() - 1; // Update coinbase position
                        }
                    } else {
                        // Odd one out, duplicate it
                        let mut combined = level[i].to_vec();
                        combined.extend_from_slice(&level[i]);
                        let hash = Sha256::digest(&combined);
                        next_level.push(hash);
                        
                        // If coinbase is the odd one, no sibling (duplicated)
                        if coinbase_index == i {
                            // No sibling, coinbase is duplicated
                            coinbase_index = next_level.len() - 1;
                        }
                    }
                }
                
                // Add path hashes from this level (in reverse order for Stratum V2)
                merkle_path.extend(current_path_hashes);
                
                level = next_level;
            }
            
            // Reverse merkle path (Stratum V2 expects path from coinbase to root, bottom-up)
            merkle_path.reverse();
        }
        
        (coinbase_prefix, coinbase_suffix, merkle_path)
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
                let response = self.handle_setup_connection(msg, &endpoint).await?;
                let response_payload = response.to_bytes()?;
                let mut encoder = crate::protocol::TlvEncoder::new();
                encoder.encode(response.message_type(), &response_payload)?
            }
            message_types::OPEN_MINING_CHANNEL => {
                let msg: OpenMiningChannelMessage = OpenMiningChannelMessage::from_bytes(&payload)?;
                let response = self.handle_open_channel(&endpoint, msg).await?;
                let response_payload = response.to_bytes()?;
                let mut encoder = crate::protocol::TlvEncoder::new();
                encoder.encode(response.message_type(), &response_payload)?
            }
            message_types::SUBMIT_SHARES => {
                let msg: SubmitSharesMessage = SubmitSharesMessage::from_bytes(&payload)?;
                let response = self.handle_submit_shares(&endpoint, msg).await?;
                let response_payload = response.to_bytes()?;
                let mut encoder = crate::protocol::TlvEncoder::new();
                encoder.encode(response.message_type(), &response_payload)?
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
        for share in msg.shares {
            let share_data = ShareData {
                channel_id: share.channel_id,
                job_id: share.job_id,
                nonce: share.nonce,
                version: share.version,
                merkle_root: share.merkle_root,
            };
            
            let _is_valid = pool.handle_share(endpoint, share_data)?;
            last_job_id = share.job_id;
        }
        
        Ok(crate::messages::SubmitSharesSuccessMessage {
            channel_id: msg.channel_id,
            last_job_id,
        })
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
}

/// Mining pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub miner_count: usize,
    pub total_shares: u64,
    pub accepted_shares: u64,
    pub rejected_shares: u64,
}

