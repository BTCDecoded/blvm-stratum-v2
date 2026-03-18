//! Stratum V2 module: unified CLI via #[module] macro.

use blvm_node::module::ipc::protocol::{EventMessage, ModuleMessage};
use blvm_sdk::module::prelude::*;
use blvm_sdk_macros::module;
use std::path::PathBuf;
use std::sync::Arc;

use crate::server::StratumV2Server;

/// Stratum V2 module: server + CLI in one struct.
#[derive(Clone)]
pub struct StratumV2Module {
    pub server: Arc<StratumV2Server>,
    pub listen_addr: String,
    pub data_dir: PathBuf,
}

#[module]
impl StratumV2Module {
    #[on_event(BlockMined, BlockTemplateUpdated, MiningDifficultyChanged, MiningJobCreated, ShareSubmitted, StratumV2MessageReceived)]
    async fn on_stratum_event(&self, event: &EventMessage, ctx: &InvocationContext) -> Result<(), ModuleError> {
        let msg = ModuleMessage::Event(event.clone());
        let api = ctx.node_api().expect("node_api required");
        self.server
            .handle_event(&msg, api.as_ref())
            .await
            .map_err(|e| ModuleError::Other(e.to_string().into()))
    }

    /// Show server state, connected miners, pool stats.
    #[command]
    fn status(&self, _ctx: &InvocationContext) -> Result<String, ModuleError> {
        let server = Arc::clone(&self.server);
        let listen_addr = self.listen_addr.clone();
        run_async(async move {
            let stats = server.get_pool_stats().await;
            let pool = server.get_pool();
            let default_diff = pool.read().await.default_min_difficulty();
            Ok::<_, String>(format!(
                "Stratum V2 server\n\
                 Listen: {}\n\
                 Default difficulty: {}\n\
                 Miners: {}\n\
                 Total shares: {}\n\
                 Accepted: {}\n\
                 Rejected: {}",
                listen_addr,
                default_diff,
                stats.miner_count,
                stats.total_shares,
                stats.accepted_shares,
                stats.rejected_shares,
            ))
        })
    }

    /// List pool listen address and connected miners.
    #[command]
    fn list_pools(&self, _ctx: &InvocationContext) -> Result<String, ModuleError> {
        let server = Arc::clone(&self.server);
        let listen_addr = self.listen_addr.clone();
        run_async(async move {
            let pool = server.get_pool();
            let pool_guard = pool.read().await;
            let miner_list: Vec<String> = pool_guard.miners.keys().cloned().collect();
            Ok::<_, String>(format!(
                "Pool: {}\n\
                 Connected miners ({}):\n{}",
                listen_addr,
                miner_list.len(),
                if miner_list.is_empty() {
                    "  (none)".into()
                } else {
                    miner_list
                        .iter()
                        .enumerate()
                        .map(|(i, m)| format!("  {}. {}", i + 1, m))
                        .collect::<Vec<_>>()
                        .join("\n")
                },
            ))
        })
    }

    /// List connected miners with channel and share stats.
    #[command]
    fn list_miners(&self, _ctx: &InvocationContext) -> Result<String, ModuleError> {
        let server = Arc::clone(&self.server);
        run_async(async move {
            let pool = server.get_pool();
            let pool_guard = pool.read().await;
            let lines: Vec<String> = pool_guard
                .miners
                .iter()
                .map(|(miner_id, conn)| {
                    format!(
                        "  {}: channels={} shares={} accepted={} rejected={}",
                        miner_id,
                        conn.channels.len(),
                        conn.stats.total_shares,
                        conn.stats.accepted_shares,
                        conn.stats.rejected_shares,
                    )
                })
                .collect();
            Ok::<_, String>(format!(
                "Connected miners ({}):\n{}",
                lines.len(),
                if lines.is_empty() {
                    "  (none)".into()
                } else {
                    lines.join("\n")
                },
            ))
        })
    }

    /// Print path to config file (config.toml).
    #[command]
    fn config_path(&self, _ctx: &InvocationContext) -> Result<String, ModuleError> {
        Ok(self.data_dir.join("config.toml").display().to_string())
    }

    /// Set default difficulty for new channels.
    #[command]
    fn set_difficulty(&self, _ctx: &InvocationContext, difficulty: u32) -> Result<String, ModuleError> {
        if difficulty == 0 {
            return Err(ModuleError::Other("Usage: set-difficulty <value> (positive integer)".into()));
        }
        let server = Arc::clone(&self.server);
        run_async(async move {
            let pool = server.get_pool();
            pool.write().await.set_default_difficulty(difficulty);
            Ok::<_, String>(format!("Default difficulty set to {} for new channels", difficulty))
        })
    }
}
