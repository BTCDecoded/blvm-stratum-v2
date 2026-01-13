//! bllvm-stratum-v2 - Stratum V2 mining protocol module
//!
//! Library exports for testing and integration
//!
//! Note: Merge mining is now a separate module (blvm-merge-mining) that depends on this module

pub mod client;
pub mod error;
pub mod messages;
pub mod nodeapi_ipc;
pub mod pool;
pub mod protocol;
pub mod server;
pub mod template;

pub use error::StratumV2Error;
pub use messages::*;
pub use pool::*;
pub use protocol::*;
pub use server::*;
pub use template::*;

