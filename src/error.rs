//! Error types for Stratum V2 module

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StratumV2Error {
    #[error("Module error: {0}")]
    ModuleError(String),
    
    #[error("Stratum V2 protocol error: {0}")]
    ProtocolError(String),
    
    #[error("Mining pool connection error: {0}")]
    PoolConnectionError(String),
    
    #[error("Block template generation error: {0}")]
    TemplateError(String),
    
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("Mining pool error: {0}")]
    PoolError(String),
    
    #[error("Server error: {0}")]
    ServerError(String),
}

