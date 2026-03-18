//! blvm-stratum-v2 - Stratum V2 mining protocol module
//!
//! Library exports for testing and integration
//!
//! Note: Merge mining is now a separate module (blvm-merge-mining) that depends on this module

pub mod config;
pub mod module;
pub mod error;
pub mod messages;
pub mod module_api;
pub mod pool;
pub mod protocol;
pub mod server;
pub mod template;

pub use config::StratumConfig;
pub use module::StratumV2Module;

pub use error::StratumV2Error;
pub use module_api::{
    StratumV2ModuleAPI, API_METHOD_GET_BLOCK_MERGE_MINING_REWARDS, API_METHOD_GET_MERGE_MINING_TEMPLATE,
    API_METHOD_REGISTER_MERGE_MINING_CHANNEL, API_METHOD_SUBMIT_MERGE_MINING_SHARE,
};
pub use messages::*;
pub use pool::*;
pub use protocol::*;
pub use server::*;
pub use template::*;

