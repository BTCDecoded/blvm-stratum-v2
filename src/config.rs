//! Stratum V2 module configuration.
//!
//! Loaded from config.toml in module data dir. Node overrides via [modules.stratum-v2] and
//! MODULE_CONFIG_* env vars.

use blvm_sdk_macros::config;
use serde::{Deserialize, Serialize};

/// Stratum V2 server configuration.
///
/// Config file: `config.toml` in module data dir.
/// Node override: `[modules.stratum-v2]` or `[modules.blvm-stratum-v2]` in node config.
/// Env override: `MODULE_CONFIG_LISTEN_ADDR`.
#[config(name = "stratum-v2")]
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct StratumConfig {
    /// Address to listen for Stratum V2 connections (e.g. "0.0.0.0:3333")
    #[serde(default = "default_listen_addr")]
    #[config_env]
    pub listen_addr: String,
    /// Default difficulty target for new channels (when miner sends 0)
    #[serde(default = "default_difficulty_target")]
    #[config_env]
    pub difficulty_target: u32,

    /// Max concurrent connections.
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    /// Pool display name.
    #[serde(default)]
    pub pool_name: Option<String>,
    /// Extra extranonce bytes.
    #[serde(default)]
    pub extra_extranonce: Option<String>,
}

fn default_max_connections() -> u32 {
    100
}

fn default_listen_addr() -> String {
    "0.0.0.0:3333".to_string()
}

fn default_difficulty_target() -> u32 {
    1
}

blvm_sdk::impl_module_config!(StratumConfig);

impl StratumConfig {
    /// Convert to ModuleContext config map for server compatibility.
    pub fn to_context_map(&self) -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert(
            "stratum_v2.listen_addr".to_string(),
            self.listen_addr.clone(),
        );
        m.insert(
            "stratum_v2.difficulty_target".to_string(),
            self.difficulty_target.to_string(),
        );
        m
    }
}
