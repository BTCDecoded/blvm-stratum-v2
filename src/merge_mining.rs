//! Merge mining coordinator

use crate::error::StratumV2Error;
use bllvm_node::module::traits::{ModuleContext, NodeAPI};
use std::sync::Arc;
use tracing::debug;

/// Secondary chain for merge mining
#[derive(Debug, Clone)]
pub struct SecondaryChain {
    /// Chain name/identifier
    pub name: String,
    /// Chain-specific configuration
    pub config: std::collections::HashMap<String, String>,
}

/// Merge mining coordinator
pub struct MergeMiningCoordinator {
    /// Secondary chains being merge mined
    secondary_chains: Vec<SecondaryChain>,
    /// Node API for querying node state
    node_api: Arc<dyn NodeAPI>,
}

impl MergeMiningCoordinator {
    /// Create a new merge mining coordinator
    pub async fn new(
        ctx: &ModuleContext,
        node_api: Arc<dyn NodeAPI>,
    ) -> Result<Self, StratumV2Error> {
        // Load secondary chains from config or storage
        let mut secondary_chains = Vec::new();
        
        // Try to load from storage first
        if let Ok(tree_id) = node_api.storage_open_tree("merge_mining".to_string()).await {
            if let Ok(Some(chains_data)) = node_api.storage_get(tree_id.clone(), b"secondary_chains".to_vec()).await {
                if let Ok(loaded_chains) = bincode::deserialize::<Vec<SecondaryChain>>(&chains_data) {
                    secondary_chains = loaded_chains;
                    debug!("Loaded {} secondary chains from storage", secondary_chains.len());
                }
            }
        }
        
        // If no chains in storage, try config
        if secondary_chains.is_empty() {
            if let Some(chains_config) = ctx.get_config("merge_mining.secondary_chains") {
                // Parse comma-separated chain names
                for chain_name in chains_config.split(',') {
                    let chain_name = chain_name.trim();
                    if !chain_name.is_empty() {
                        secondary_chains.push(SecondaryChain {
                            name: chain_name.to_string(),
                            config: std::collections::HashMap::new(),
                        });
                    }
                }
            }
        }
        
        debug!("Merge mining coordinator initialized with {} secondary chains", secondary_chains.len());
        
        Ok(Self {
            secondary_chains,
            node_api,
        })
    }
    
    /// Get secondary chains
    pub fn get_secondary_chains(&self) -> &[SecondaryChain] {
        &self.secondary_chains
    }
    
    /// Add a secondary chain for merge mining
    pub fn add_secondary_chain(&mut self, chain: SecondaryChain) {
        self.secondary_chains.push(chain);
        debug!("Added secondary chain for merge mining: {}", self.secondary_chains.last().unwrap().name);
    }
}

