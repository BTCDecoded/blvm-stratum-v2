//! bllvm-stratum-v2 - Stratum V2 mining protocol module
//!
//! This module provides Stratum V2 mining protocol support for bllvm-node,
//! including server implementation and mining pool management.
//!
//! Note: Merge mining is now a separate module (blvm-merge-mining) that depends on this module.

use anyhow::Result;
use bllvm_node::module::ipc::protocol::{EventMessage, EventPayload, EventType, LogLevel, ModuleMessage};
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info, warn};

mod server;
mod pool;
mod error;
mod client;
mod nodeapi_ipc;
mod messages;
mod protocol;
mod template;

use error::StratumV2Error;
use client::ModuleClient;
use nodeapi_ipc::NodeApiIpc;

/// Command-line arguments for the module
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Module ID (provided by node)
    #[arg(long)]
    module_id: Option<String>,

    /// IPC socket path (provided by node)
    #[arg(long)]
    socket_path: Option<PathBuf>,

    /// Data directory (provided by node)
    #[arg(long)]
    data_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    // Get module ID (from args or environment)
    let module_id = args.module_id
        .or_else(|| std::env::var("MODULE_NAME").ok())
        .unwrap_or_else(|| "bllvm-stratum-v2".to_string());

    // Get socket path (from args, env, or default)
    let socket_path = args.socket_path
        .or_else(|| std::env::var("BLLVM_MODULE_SOCKET").ok().map(PathBuf::from))
        .or_else(|| std::env::var("MODULE_SOCKET_DIR").ok().map(|d| PathBuf::from(d).join("modules.sock")))
        .unwrap_or_else(|| PathBuf::from("data/modules/modules.sock"));

    info!("bllvm-stratum-v2 module starting... (module_id: {}, socket: {:?})", module_id, socket_path);

    // Connect to node
    let mut client = match ModuleClient::connect(
        socket_path,
        module_id.clone(),
        "bllvm-stratum-v2".to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
    ).await {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to connect to node: {}", e);
            return Err(anyhow::anyhow!("Connection failed: {}", e));
        }
    };

    // Subscribe to mining events
    let event_types = vec![
        EventType::BlockMined,
        EventType::BlockTemplateUpdated,
        EventType::MiningDifficultyChanged,
        EventType::MiningJobCreated,
        EventType::ShareSubmitted,
    ];

    if let Err(e) = client.subscribe_events(event_types).await {
        error!("Failed to subscribe to events: {}", e);
        return Err(anyhow::anyhow!("Subscription failed: {}", e));
    }

    // Create NodeAPI wrapper
    let ipc_client = client.get_ipc_client();
    let node_api = Arc::new(NodeApiIpc::new(ipc_client));

    // Create server
    let ctx = bllvm_node::module::traits::ModuleContext {
        module_id: module_id.clone(),
        config: std::collections::HashMap::new(),
        data_dir: args.data_dir.unwrap_or_else(|| PathBuf::from("data/modules/bllvm-stratum-v2")),
        socket_path: socket_path.to_string_lossy().to_string(),
    };

    let server = server::StratumV2Server::new(&ctx, Arc::clone(&node_api)).await
        .map_err(|e| anyhow::anyhow!("Failed to create server: {}", e))?;
    
    // Note: Merge mining is now handled by separate blvm-merge-mining module

    info!("Stratum V2 module initialized and running");

    // Start the server
    if let Err(e) = server.start().await {
        error!("Failed to start Stratum V2 server: {}", e);
        return Err(anyhow::anyhow!("Server startup failed: {}", e));
    }

    // Event processing loop
    let mut event_receiver = client.event_receiver();
    while let Some(event) = event_receiver.recv().await {
        // Handle events
        if let Err(e) = server.handle_event(&event, node_api.as_ref()).await {
            warn!("Error handling event: {}", e);
        }
    }

    warn!("Event receiver closed, module shutting down");
    Ok(())
}
