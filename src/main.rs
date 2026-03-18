//! blvm-stratum-v2 - Stratum V2 mining protocol module
//!
//! When spawned by the node: reads MODULE_ID, SOCKET_PATH, DATA_DIR from env.
//! For manual testing: blvm-stratum-v2 --module-id <id> --socket-path <path> --data-dir <dir>

use anyhow::Result;
use blvm_sdk::module::{ModuleBootstrap, ModuleDb};
use blvm_stratum_v2::{server::StratumV2Server, StratumConfig, StratumV2Module};
use std::sync::Arc;
use tracing::{error, warn};

const MODULE_NAME: &str = "blvm-stratum-v2";

#[tokio::main]
async fn main() -> Result<()> {
    let bootstrap = ModuleBootstrap::init_module(MODULE_NAME);
    let db = ModuleDb::open_or_temp(&bootstrap.data_dir, MODULE_NAME)?;

    let setup = |node_api: Arc<dyn blvm_node::module::traits::NodeAPI>,
                 _db: Arc<dyn blvm_node::storage::database::Database>,
                 data_dir: &std::path::Path| {
        let bootstrap = bootstrap.clone();
        let data_dir = data_dir.to_path_buf();
        async move {
            let (ctx, config) = bootstrap.context_with_config::<StratumConfig>(&data_dir);
            let listen_addr = config.listen_addr.clone();
            let server = StratumV2Server::new(&ctx, Arc::clone(&node_api))
                .await
                .map_err(|e| blvm_node::module::traits::ModuleError::Other(format!("Failed to create server: {}", e)))?;
            if let Err(e) = server.start().await {
                error!("Failed to start Stratum V2 server: {}", e);
                return Err(blvm_node::module::traits::ModuleError::Other(format!("Server startup failed: {}", e)));
            }
            tracing::info!("Stratum V2 module initialized and running");
            if let Err(e) = server.register_module_api(node_api.as_ref()).await {
                warn!("Failed to register StratumV2ModuleAPI (merge-mining may not work): {}", e);
            } else {
                tracing::info!("StratumV2ModuleAPI registered for merge-mining");
            }
            let server = Arc::new(server);
            let module = StratumV2Module {
                server: Arc::clone(&server),
                listen_addr,
                data_dir,
            };
            Ok((module.clone(), module))
        }
    };

    blvm_sdk::run_module! {
        bootstrap: &bootstrap,
        module_name: MODULE_NAME,
        module_type: StratumV2Module,
        cli_type: StratumV2Module,
        db: db.as_db(),
        setup: setup,
        event_types: StratumV2Module::event_types(),
    }?;

    Ok(())
}
