//! E2E integration tests for Stratum V2 + DATUM coordination
//!
//! Tests the flow when both modules are loaded:
//! - BlockTemplateGenerator queries DATUM get_coinbase_payout and passes to get_block_template
//! - handle_submit_shares calls DATUM submit_pow when a valid block is found

use blvm_node::module::traits::NodeAPI;
use blvm_protocol::{
    Block, BlockHeader, Hash, OutPoint, Transaction, TransactionInput, TransactionOutput,
};
use blvm_stratum_v2::{
    messages::{self, *},
    pool::StratumV2Pool,
    protocol::TlvEncoder,
    server::StratumV2Server,
    template::BlockTemplateGenerator,
};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Mock NodeAPI that simulates DATUM + node for Stratum V2 integration tests
struct MockDatumNodeAPI {
    /// Track call_module invocations: (method, params_len)
    call_module_invocations: Arc<RwLock<Vec<(String, usize)>>>,
    /// DATUM get_coinbase_payout response (when Some, call_module returns it)
    datum_payout_response: Option<Vec<u8>>,
    /// Commons Pool commons_get_coinbase_outputs response.
    commons_payout_response: Option<Vec<u8>>,
    /// Block template to return from get_block_template
    block_template: blvm_protocol::mining::BlockTemplate,
    /// Last `commons_credit_find` JSON body.
    last_credit_params: Arc<RwLock<Option<Vec<u8>>>>,
    /// Last `commons_submit_share` JSON body.
    last_submit_share_params: Arc<RwLock<Option<Vec<u8>>>>,
    /// GridPool `gridpool_get_coinbase_outputs` response.
    gridpool_payout_response: Option<Vec<u8>>,
    submit_result: blvm_node::module::traits::SubmitBlockResult,
}

impl MockDatumNodeAPI {
    fn new_with_datum_payout(script_hex: &str) -> Self {
        let payout_json = serde_json::json!({
            "outputs": [{"script": script_hex, "value": 5000000000i64}],
            "primary_tag": "pool",
            "unique_id": "test-1"
        });
        Self {
            call_module_invocations: Arc::new(RwLock::new(Vec::new())),
            datum_payout_response: Some(serde_json::to_vec(&payout_json).unwrap()),
            commons_payout_response: None,
            block_template: create_test_block_template(),
            last_credit_params: Arc::new(RwLock::new(None)),
            last_submit_share_params: Arc::new(RwLock::new(None)),
            gridpool_payout_response: None,
            submit_result: blvm_node::module::traits::SubmitBlockResult::Accepted,
        }
    }

    fn new_without_datum() -> Self {
        Self {
            call_module_invocations: Arc::new(RwLock::new(Vec::new())),
            datum_payout_response: None,
            commons_payout_response: None,
            block_template: create_test_block_template(),
            last_credit_params: Arc::new(RwLock::new(None)),
            last_submit_share_params: Arc::new(RwLock::new(None)),
            gridpool_payout_response: None,
            submit_result: blvm_node::module::traits::SubmitBlockResult::Accepted,
        }
    }

    fn new_with_commons_payouts() -> Self {
        let fee = "00".repeat(32);
        let miner = "11".repeat(32);
        let payout_json = serde_json::json!({
            "outputs": [
                {"script": format!("0020{fee}"), "value_sats": 50_000_000},
                {"script": format!("0020{miner}"), "value_sats": 4_950_000_000u64},
            ],
            "issue_work": true,
            "snapshot_id": "aa".repeat(32),
        });
        Self {
            call_module_invocations: Arc::new(RwLock::new(Vec::new())),
            datum_payout_response: None,
            commons_payout_response: Some(serde_json::to_vec(&payout_json).unwrap()),
            block_template: create_test_block_template(),
            last_credit_params: Arc::new(RwLock::new(None)),
            last_submit_share_params: Arc::new(RwLock::new(None)),
            gridpool_payout_response: None,
            submit_result: blvm_node::module::traits::SubmitBlockResult::Accepted,
        }
    }

    /// Node coinbase already has a BIP141 commitment the splice must keep.
    fn new_with_commons_and_node_commitment() -> Self {
        let mut s = Self::new_with_commons_payouts();
        let mut script = vec![0x6a, 0x24, 0xaa, 0x21, 0xa9, 0xed];
        script.extend_from_slice(&[0xab; 32]);
        let mut cb = s.block_template.coinbase_tx.clone();
        let mut outs = cb.outputs.to_vec();
        outs.push(TransactionOutput {
            value: 0,
            script_pubkey: script,
        });
        cb.outputs = outs.into();
        s.block_template.coinbase_tx = cb;
        s
    }

    /// Commons pays less than the template coinbase; splice must top up first.
    fn new_with_commons_shortfall() -> Self {
        let fee = "00".repeat(32);
        let miner = "11".repeat(32);
        let payout_json = serde_json::json!({
            "outputs": [
                {"script": format!("0020{fee}"), "value_sats": 40_000_000},
                {"script": format!("0020{miner}"), "value_sats": 4_950_000_000u64},
            ],
            "issue_work": true,
            "snapshot_id": "aa".repeat(32),
        });
        Self {
            call_module_invocations: Arc::new(RwLock::new(Vec::new())),
            datum_payout_response: None,
            commons_payout_response: Some(serde_json::to_vec(&payout_json).unwrap()),
            block_template: create_test_block_template(),
            last_credit_params: Arc::new(RwLock::new(None)),
            last_submit_share_params: Arc::new(RwLock::new(None)),
            gridpool_payout_response: None,
            submit_result: blvm_node::module::traits::SubmitBlockResult::Accepted,
        }
    }

    /// Commons pays more than subsidy+fees on the template; splice must refuse.
    fn new_with_commons_overpay() -> Self {
        let fee = "00".repeat(32);
        let miner = "11".repeat(32);
        let payout_json = serde_json::json!({
            "outputs": [
                {"script": format!("0020{fee}"), "value_sats": 100_000_000},
                {"script": format!("0020{miner}"), "value_sats": 4_950_000_000u64},
            ],
            "issue_work": true,
        });
        Self {
            call_module_invocations: Arc::new(RwLock::new(Vec::new())),
            datum_payout_response: None,
            commons_payout_response: Some(serde_json::to_vec(&payout_json).unwrap()),
            block_template: create_test_block_template(),
            last_credit_params: Arc::new(RwLock::new(None)),
            last_submit_share_params: Arc::new(RwLock::new(None)),
            gridpool_payout_response: None,
            submit_result: blvm_node::module::traits::SubmitBlockResult::Accepted,
        }
    }

    /// NodeAPI already embedded Commons (pass 28+). Splice must not rewrite.
    fn new_with_commons_already_in_node_template() -> Self {
        let mut s = Self::new_with_commons_payouts();
        let mut fee_script = vec![0x00, 0x20];
        fee_script.extend(std::iter::repeat_n(0u8, 32));
        let mut miner_script = vec![0x00, 0x20];
        miner_script.extend(std::iter::repeat_n(0x11u8, 32));
        let mut commit = vec![0x6a, 0x24, 0xaa, 0x21, 0xa9, 0xed];
        commit.extend_from_slice(&[0xcd; 32]);
        s.block_template.coinbase_tx.outputs = vec![
            TransactionOutput {
                value: 50_000_000,
                script_pubkey: fee_script,
            },
            TransactionOutput {
                value: 4_950_000_000,
                script_pubkey: miner_script,
            },
            TransactionOutput {
                value: 0,
                script_pubkey: commit,
            },
        ]
        .into();
        s.block_template.header.merkle_root = [0xee; 32];
        s
    }

    fn new_with_gridpool_payouts() -> Self {
        let script = format!("0020{}", "22".repeat(32));
        let payout_json = serde_json::json!({
            "outputs": [{"script": script, "value": 50_000_000}],
            "issue_work": true,
        });
        let mut node = Self::new_without_datum();
        node.gridpool_payout_response = Some(serde_json::to_vec(&payout_json).unwrap());
        node
    }

    fn new_with_gridpool_holding() -> Self {
        let mut node = Self::new_with_gridpool_payouts();
        let payout_json = serde_json::json!({
            "outputs": [],
            "issue_work": false,
        });
        node.gridpool_payout_response = Some(serde_json::to_vec(&payout_json).unwrap());
        node
    }

    fn new_with_commons_holding() -> Self {
        let mut s = Self::new_with_commons_payouts();
        let payout_json = serde_json::json!({
            "outputs": [
                {"script": format!("0020{}", "00".repeat(32)), "value_sats": 50_000_000},
            ],
            "issue_work": false,
        });
        s.commons_payout_response = Some(serde_json::to_vec(&payout_json).unwrap());
        s
    }

    async fn get_call_invocations(&self) -> Vec<(String, usize)> {
        self.call_module_invocations.read().await.clone()
    }

    async fn last_credit_params(&self) -> Option<Vec<u8>> {
        self.last_credit_params.read().await.clone()
    }

    async fn last_submit_share_params(&self) -> Option<Vec<u8>> {
        self.last_submit_share_params.read().await.clone()
    }
}

fn create_test_block_template() -> blvm_protocol::mining::BlockTemplate {
    let coinbase = Transaction {
        version: 1,
        inputs: vec![TransactionInput {
            prevout: OutPoint {
                hash: [0u8; 32],
                index: 0xFFFFFFFF,
            },
            script_sig: vec![0x51, 0x00],
            sequence: 0xFFFFFFFF,
        }]
        .into(),
        outputs: vec![TransactionOutput {
            value: 5000000000,
            script_pubkey: vec![0x51, 0x00],
        }]
        .into(),
        lock_time: 0,
    };
    blvm_protocol::mining::BlockTemplate {
        header: BlockHeader {
            version: 1,
            prev_block_hash: [0u8; 32],
            merkle_root: [0u8; 32],
            timestamp: 1700000000,
            bits: 0x1d00ffff,
            nonce: 0,
        },
        coinbase_tx: coinbase,
        transactions: vec![],
        target: 0x00000000ffff00000000000000000000u128,
        height: 100,
        timestamp: 1700000000,
    }
}

#[async_trait::async_trait]
impl NodeAPI for MockDatumNodeAPI {
    async fn get_block(
        &self,
        _: &Hash,
    ) -> Result<Option<Block>, blvm_node::module::traits::ModuleError> {
        Ok(None)
    }
    async fn get_block_header(
        &self,
        _: &Hash,
    ) -> Result<Option<BlockHeader>, blvm_node::module::traits::ModuleError> {
        Ok(None)
    }
    async fn get_transaction(
        &self,
        _: &Hash,
    ) -> Result<Option<Transaction>, blvm_node::module::traits::ModuleError> {
        Ok(None)
    }
    async fn has_transaction(
        &self,
        _: &Hash,
    ) -> Result<bool, blvm_node::module::traits::ModuleError> {
        Ok(false)
    }
    async fn get_chain_tip(&self) -> Result<Hash, blvm_node::module::traits::ModuleError> {
        Ok([0u8; 32])
    }
    async fn get_block_height(&self) -> Result<u64, blvm_node::module::traits::ModuleError> {
        Ok(100)
    }
    async fn get_utxo(
        &self,
        _: &OutPoint,
    ) -> Result<Option<blvm_protocol::UTXO>, blvm_node::module::traits::ModuleError> {
        Ok(None)
    }
    async fn subscribe_events(
        &self,
        _: Vec<blvm_node::module::traits::EventType>,
    ) -> Result<
        tokio::sync::mpsc::Receiver<blvm_node::module::ipc::protocol::ModuleMessage>,
        blvm_node::module::traits::ModuleError,
    > {
        let (_tx, rx) = tokio::sync::mpsc::channel(100);
        Ok(rx)
    }
    async fn get_mempool_transactions(
        &self,
    ) -> Result<Vec<Hash>, blvm_node::module::traits::ModuleError> {
        Ok(Vec::new())
    }
    async fn get_mempool_transaction(
        &self,
        _: &Hash,
    ) -> Result<Option<Transaction>, blvm_node::module::traits::ModuleError> {
        Ok(None)
    }
    async fn get_mempool_size(
        &self,
    ) -> Result<blvm_node::module::traits::MempoolSize, blvm_node::module::traits::ModuleError>
    {
        Ok(blvm_node::module::traits::MempoolSize {
            transaction_count: 0,
            size_bytes: 0,
            total_fee_sats: 0,
        })
    }
    async fn get_network_stats(
        &self,
    ) -> Result<blvm_node::module::traits::NetworkStats, blvm_node::module::traits::ModuleError>
    {
        Ok(blvm_node::module::traits::NetworkStats {
            peer_count: 0,
            hash_rate: 0.0,
            bytes_sent: 0,
            bytes_received: 0,
        })
    }
    async fn get_network_peers(
        &self,
    ) -> Result<Vec<blvm_node::module::traits::PeerInfo>, blvm_node::module::traits::ModuleError>
    {
        Ok(Vec::new())
    }
    async fn get_chain_info(
        &self,
    ) -> Result<blvm_node::module::traits::ChainInfo, blvm_node::module::traits::ModuleError> {
        Ok(blvm_node::module::traits::ChainInfo {
            tip_hash: [0u8; 32],
            height: 100,
            difficulty: 1,
            chain_work: 0,
            is_synced: true,
        })
    }
    async fn get_block_by_height(
        &self,
        _: u64,
    ) -> Result<Option<Block>, blvm_node::module::traits::ModuleError> {
        Ok(None)
    }
    async fn get_lightning_node_url(
        &self,
    ) -> Result<Option<String>, blvm_node::module::traits::ModuleError> {
        Ok(None)
    }
    async fn get_lightning_info(
        &self,
    ) -> Result<
        Option<blvm_node::module::traits::LightningInfo>,
        blvm_node::module::traits::ModuleError,
    > {
        Ok(None)
    }
    async fn get_payment_state(
        &self,
        _: &str,
    ) -> Result<
        Option<blvm_node::module::traits::PaymentState>,
        blvm_node::module::traits::ModuleError,
    > {
        Ok(None)
    }
    async fn check_transaction_in_mempool(
        &self,
        _: &Hash,
    ) -> Result<bool, blvm_node::module::traits::ModuleError> {
        Ok(false)
    }
    async fn get_fee_estimate(
        &self,
        _: u32,
    ) -> Result<u64, blvm_node::module::traits::ModuleError> {
        Ok(1)
    }
    async fn read_file(
        &self,
        _: String,
    ) -> Result<Vec<u8>, blvm_node::module::traits::ModuleError> {
        Ok(Vec::new())
    }
    async fn write_file(
        &self,
        _: String,
        _: Vec<u8>,
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn delete_file(&self, _: String) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn list_directory(
        &self,
        _: String,
    ) -> Result<Vec<String>, blvm_node::module::traits::ModuleError> {
        Ok(Vec::new())
    }
    async fn create_directory(
        &self,
        _: String,
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn get_file_metadata(
        &self,
        _: String,
    ) -> Result<
        blvm_node::module::ipc::protocol::FileMetadata,
        blvm_node::module::traits::ModuleError,
    > {
        Ok(blvm_node::module::ipc::protocol::FileMetadata {
            path: String::new(),
            size: 0,
            is_file: false,
            is_directory: false,
            modified: None,
            created: None,
        })
    }
    async fn get_all_metrics(
        &self,
    ) -> Result<
        std::collections::HashMap<String, Vec<blvm_node::module::metrics::manager::Metric>>,
        blvm_node::module::traits::ModuleError,
    > {
        Ok(std::collections::HashMap::new())
    }
    async fn register_rpc_endpoint(
        &self,
        _: String,
        _: String,
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn unregister_rpc_endpoint(
        &self,
        _: &str,
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn register_timer(
        &self,
        _: u64,
        _: Arc<dyn blvm_node::module::timers::manager::TimerCallback>,
    ) -> Result<blvm_node::module::timers::manager::TimerId, blvm_node::module::traits::ModuleError>
    {
        Ok(0)
    }
    async fn cancel_timer(
        &self,
        _: blvm_node::module::timers::manager::TimerId,
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn schedule_task(
        &self,
        _: u64,
        _: Arc<dyn blvm_node::module::timers::manager::TaskCallback>,
    ) -> Result<blvm_node::module::timers::manager::TaskId, blvm_node::module::traits::ModuleError>
    {
        Ok(0)
    }
    async fn report_metric(
        &self,
        _: blvm_node::module::metrics::manager::Metric,
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn get_module_metrics(
        &self,
        _: &str,
    ) -> Result<
        Vec<blvm_node::module::metrics::manager::Metric>,
        blvm_node::module::traits::ModuleError,
    > {
        Ok(Vec::new())
    }
    async fn initialize_module(
        &self,
        _: String,
        _: std::path::PathBuf,
        _: std::path::PathBuf,
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn discover_modules(
        &self,
    ) -> Result<Vec<blvm_node::module::traits::ModuleInfo>, blvm_node::module::traits::ModuleError>
    {
        Ok(Vec::new())
    }
    async fn get_module_info(
        &self,
        _: &str,
    ) -> Result<Option<blvm_node::module::traits::ModuleInfo>, blvm_node::module::traits::ModuleError>
    {
        Ok(None)
    }
    async fn is_module_available(
        &self,
        id: &str,
    ) -> Result<bool, blvm_node::module::traits::ModuleError> {
        Ok(match id {
            "datum" => self.datum_payout_response.is_some(),
            "blvm-commons-pool" => self.commons_payout_response.is_some(),
            "blvm-gridpool" => self.gridpool_payout_response.is_some(),
            _ => false,
        })
    }
    async fn publish_event(
        &self,
        _: blvm_node::module::traits::EventType,
        _: blvm_node::module::ipc::protocol::EventPayload,
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn call_module(
        &self,
        _: Option<&str>,
        method: &str,
        params: Vec<u8>,
    ) -> Result<Vec<u8>, blvm_node::module::traits::ModuleError> {
        self.call_module_invocations
            .write()
            .await
            .push((method.to_string(), params.len()));
        if method == "get_coinbase_payout" {
            if let Some(ref resp) = self.datum_payout_response {
                return Ok(resp.clone());
            }
        }
        if method == "commons_get_coinbase_outputs" {
            if let Some(ref resp) = self.commons_payout_response {
                return Ok(resp.clone());
            }
        }
        if method == "gridpool_get_coinbase_outputs" {
            if let Some(ref resp) = self.gridpool_payout_response {
                return Ok(resp.clone());
            }
        }
        if method == "gridpool_note_payment" {
            return Ok(b"{}".to_vec());
        }
        if method == "submit_pow" {
            return Ok(serde_json::to_vec(&serde_json::json!({ "accepted": true })).unwrap());
        }
        if method == "commons_submit_share" {
            *self.last_submit_share_params.write().await = Some(params.clone());
            return Ok(serde_json::to_vec(&serde_json::json!({ "ok": true })).unwrap());
        }
        if method == "commons_credit_find" {
            *self.last_credit_params.write().await = Some(params.clone());
            return Ok(serde_json::to_vec(&serde_json::json!({
                "credited": true,
                "outcome": "Credited",
            }))
            .unwrap());
        }
        Ok(Vec::new())
    }
    async fn register_module_api(
        &self,
        _: Arc<dyn blvm_node::module::inter_module::api::ModuleAPI>,
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn unregister_module_api(&self) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn get_module_health(
        &self,
        _: &str,
    ) -> Result<
        Option<blvm_node::module::process::monitor::ModuleHealth>,
        blvm_node::module::traits::ModuleError,
    > {
        Ok(None)
    }
    async fn get_all_module_health(
        &self,
    ) -> Result<
        Vec<(String, blvm_node::module::process::monitor::ModuleHealth)>,
        blvm_node::module::traits::ModuleError,
    > {
        Ok(Vec::new())
    }
    async fn report_module_health(
        &self,
        _: blvm_node::module::process::monitor::ModuleHealth,
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn send_mesh_packet_to_module(
        &self,
        _: &str,
        _: Vec<u8>,
        _: String,
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn send_mesh_packet_to_peer(
        &self,
        _: String,
        _: Vec<u8>,
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn send_peer_transport_payload(
        &self,
        _: String,
        _: Vec<u8>,
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn get_block_template(
        &self,
        _: Vec<String>,
        _: Option<Vec<u8>>,
        _: Option<String>,
    ) -> Result<blvm_protocol::mining::BlockTemplate, blvm_node::module::traits::ModuleError> {
        Ok(self.block_template.clone())
    }
    async fn submit_block(
        &self,
        _: Block,
    ) -> Result<blvm_node::module::traits::SubmitBlockResult, blvm_node::module::traits::ModuleError>
    {
        Ok(self.submit_result.clone())
    }
    async fn submit_mempool_transaction(
        &self,
        _: blvm_protocol::Transaction,
        _: Option<Vec<blvm_protocol::Witness>>,
    ) -> Result<bool, blvm_node::module::traits::ModuleError> {
        Ok(true)
    }
    async fn register_core_rpc_override(
        &self,
        _: String,
        _: String,
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn unregister_core_rpc_override(
        &self,
        _: &str,
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn merge_block_serve_denylist(
        &self,
        _: &[Hash],
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn get_block_serve_denylist_snapshot(
        &self,
    ) -> Result<
        blvm_node::module::traits::BlockServeDenylistSnapshot,
        blvm_node::module::traits::ModuleError,
    > {
        Ok(blvm_node::module::traits::BlockServeDenylistSnapshot {
            total_count: 0,
            truncated: false,
            hashes: vec![],
        })
    }
    async fn clear_block_serve_denylist(
        &self,
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn replace_block_serve_denylist(
        &self,
        _: &[Hash],
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn merge_tx_serve_denylist(
        &self,
        _: &[Hash],
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn get_tx_serve_denylist_snapshot(
        &self,
    ) -> Result<
        blvm_node::module::traits::TxServeDenylistSnapshot,
        blvm_node::module::traits::ModuleError,
    > {
        Ok(blvm_node::module::traits::TxServeDenylistSnapshot {
            total_count: 0,
            truncated: false,
            hashes: vec![],
        })
    }
    async fn clear_tx_serve_denylist(&self) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn replace_tx_serve_denylist(
        &self,
        _: &[Hash],
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn get_sync_status(
        &self,
    ) -> Result<blvm_node::module::traits::SyncStatus, blvm_node::module::traits::ModuleError> {
        Ok(blvm_node::module::traits::SyncStatus {
            phase: "idle".to_string(),
            progress: 1.0,
            is_synced: true,
            error_message: None,
        })
    }
    async fn ban_peer(
        &self,
        _: &str,
        _: Option<u64>,
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
    async fn set_block_serve_maintenance_mode(
        &self,
        _: bool,
    ) -> Result<(), blvm_node::module::traits::ModuleError> {
        Ok(())
    }
}

#[tokio::test]
async fn test_template_generator_queries_datum_for_coinbase() {
    let script_hex = "76a9141234567890abcdef1234567890abcdef123456789088ac";
    let node_api = Arc::new(MockDatumNodeAPI::new_with_datum_payout(script_hex));
    let generator = BlockTemplateGenerator::new(node_api.clone());

    let block = generator.generate_template().await.unwrap();
    assert!(!block.transactions.is_empty());

    let invocations = node_api.get_call_invocations().await;
    assert!(
        invocations.iter().any(|(m, _)| m == "get_coinbase_payout"),
        "Expected get_coinbase_payout to be called: {invocations:?}"
    );
}

#[tokio::test]
async fn test_template_generator_works_without_datum() {
    let node_api = Arc::new(MockDatumNodeAPI::new_without_datum());
    let generator = BlockTemplateGenerator::new(node_api.clone());

    let block = generator.generate_template().await.unwrap();
    assert!(!block.transactions.is_empty());

    let invocations = node_api.get_call_invocations().await;
    assert!(
        invocations.is_empty(),
        "No call_module expected when DATUM not loaded: {invocations:?}"
    );
}

#[tokio::test]
async fn test_template_embeds_commons_multi_output() {
    let node_api = Arc::new(MockDatumNodeAPI::new_with_commons_payouts());
    let generator = BlockTemplateGenerator::new(node_api.clone());
    let block = generator.generate_template().await.unwrap();
    assert_eq!(block.transactions[0].outputs.len(), 3);
    assert_eq!(block.transactions[0].outputs[0].value, 50_000_000);
    assert_eq!(block.transactions[0].outputs[1].value, 4_950_000_000);
    assert_eq!(block.transactions[0].outputs[2].value, 0);
    assert_eq!(block.transactions[0].outputs[2].script_pubkey[0], 0x6a);
    let merkle = blvm_protocol::mining::calculate_merkle_root(&block.transactions).unwrap();
    assert_eq!(block.header.merkle_root, merkle);
    let invocations = node_api.get_call_invocations().await;
    assert!(
        invocations
            .iter()
            .any(|(m, _)| m == "commons_get_coinbase_outputs"),
        "{invocations:?}"
    );
    let snap = generator.last_commons_snapshot();
    let expected = "aa".repeat(32);
    assert_eq!(snap.as_deref(), Some(expected.as_str()));
    generator
        .credit_commons_find(&block, snap.as_deref(), Some("miner.example:3333"))
        .await
        .unwrap();
    let after = node_api.get_call_invocations().await;
    assert!(
        after.iter().any(|(m, _)| m == "commons_credit_find"),
        "{after:?}"
    );
    let credit = node_api.last_credit_params().await.expect("credit body");
    let v: serde_json::Value = serde_json::from_slice(&credit).unwrap();
    let hash = v
        .get("block_hash")
        .and_then(|x| x.as_str())
        .expect("block_hash");
    assert_eq!(hash.len(), 64);
    let expected = blvm_stratum_v2::pool::stratum_header_hash(&block.header);
    let expected_hex: String = expected.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(hash, expected_hex);
    let miner = v.get("miner").and_then(|x| x.as_str()).expect("miner");
    assert_eq!(miner.len(), 64);
}

#[tokio::test]
async fn test_template_submits_commons_share() {
    let node_api = Arc::new(MockDatumNodeAPI::new_with_commons_payouts());
    let generator = BlockTemplateGenerator::new(node_api.clone());
    let snap = "aa".repeat(32);
    generator
        .submit_commons_share("miner-1", &[0xab; 32], Some(snap.as_str()), 100)
        .await
        .unwrap();
    let after = node_api.get_call_invocations().await;
    assert!(
        after.iter().any(|(m, _)| m == "commons_submit_share"),
        "{after:?}"
    );
    let body = node_api
        .last_submit_share_params()
        .await
        .expect("submit body");
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        v.get("share_hash").and_then(|x| x.as_str()).unwrap(),
        "ab".repeat(32)
    );
    assert_eq!(v.get("snapshot_id").and_then(|x| x.as_str()).unwrap(), snap);
    assert_eq!(v.get("bound_height").and_then(|x| x.as_u64()).unwrap(), 100);
    let miner = v.get("miner").and_then(|x| x.as_str()).unwrap();
    assert_eq!(miner.len(), 64);
}

#[tokio::test]
async fn test_template_refuses_when_commons_holding() {
    let node_api = Arc::new(MockDatumNodeAPI::new_with_commons_holding());
    let generator = BlockTemplateGenerator::new(node_api);
    let err = generator.generate_template().await.unwrap_err();
    assert!(err.to_string().contains("holding"), "{err}");
}

#[tokio::test]
async fn test_template_skips_splice_when_node_already_paid_commons() {
    let node_api = Arc::new(MockDatumNodeAPI::new_with_commons_already_in_node_template());
    let generator = BlockTemplateGenerator::new(node_api);
    let block = generator.generate_template().await.unwrap();
    assert_eq!(block.transactions[0].outputs.len(), 3);
    assert_eq!(block.transactions[0].outputs[0].value, 50_000_000);
    assert_eq!(block.transactions[0].outputs[1].value, 4_950_000_000);
    assert_eq!(
        &block.transactions[0].outputs[2].script_pubkey[6..],
        &[0xcd; 32]
    );
    assert_eq!(
        block.header.merkle_root, [0xee; 32],
        "splice must not rewrite a node-built Commons coinbase"
    );
}

#[tokio::test]
async fn test_template_keeps_node_bip141_commitment() {
    let node_api = Arc::new(MockDatumNodeAPI::new_with_commons_and_node_commitment());
    let generator = BlockTemplateGenerator::new(node_api);
    let block = generator.generate_template().await.unwrap();
    assert_eq!(block.transactions[0].outputs.len(), 3);
    let last = &block.transactions[0].outputs[2];
    assert_eq!(last.value, 0);
    assert_eq!(&last.script_pubkey[6..], &[0xab; 32]);
}

#[tokio::test]
async fn test_template_tops_up_commons_shortfall_to_coinbase_budget() {
    let node_api = Arc::new(MockDatumNodeAPI::new_with_commons_shortfall());
    let generator = BlockTemplateGenerator::new(node_api);
    let block = generator.generate_template().await.unwrap();
    assert_eq!(block.transactions[0].outputs.len(), 3);
    assert_eq!(block.transactions[0].outputs[0].value, 50_000_000);
    assert_eq!(block.transactions[0].outputs[1].value, 4_950_000_000);
    assert_eq!(block.transactions[0].outputs[2].value, 0);
}

#[tokio::test]
async fn test_template_refuses_commons_over_coinbase_budget() {
    let node_api = Arc::new(MockDatumNodeAPI::new_with_commons_overpay());
    let generator = BlockTemplateGenerator::new(node_api);
    let err = generator.generate_template().await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("exceed coinbase budget"), "{msg}");
}

#[tokio::test]
async fn test_pool_accepts_mined_block_share() {
    use blvm_protocol::genesis;

    // Direct pool test: use regtest genesis (has valid nonce) + share
    let mut pool = StratumV2Pool::new();
    pool.register_miner("miner-1".to_string());
    pool.open_channel("miner-1", 1, 0).unwrap(); // min_difficulty=0 => 1000x easier target

    // Use mainnet genesis (bits 0x1d00ffff, valid nonce)
    let block = genesis::mainnet_genesis();

    let (job_id, _) = pool.set_template(block.clone());
    let share_data = blvm_stratum_v2::pool::ShareData {
        channel_id: 1,
        job_id,
        nonce: block.header.nonce as u32,
        version: block.header.version as i64,
        merkle_root: block.header.merkle_root,
    };
    let (is_valid_share, is_valid_block) = pool.handle_share("miner-1", share_data).unwrap();
    assert!(is_valid_share, "Share should be valid (channel target)");
    assert!(
        is_valid_block,
        "Share should be valid block (network target)"
    );
}

#[tokio::test]
async fn test_submit_shares_calls_datum_submit_pow_on_valid_block() {
    use blvm_protocol::genesis;

    let ctx = blvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };

    let node_api = Arc::new(MockDatumNodeAPI::new_with_datum_payout("76a91400"));
    let server = StratumV2Server::new(&ctx, node_api.clone()).await.unwrap();

    // Use mainnet genesis (bits 0x1d00ffff, valid nonce)
    let block = genesis::mainnet_genesis();

    let pool_handle = server.get_pool();
    let mut pool = pool_handle.write().await;
    pool.register_miner("miner-1".to_string());
    pool.open_channel("miner-1", 1, 0).unwrap(); // min_difficulty=0 => 1000x easier target
    let pool_template = pool.set_template(block.clone());
    drop(pool);

    let share_data = messages::ShareData {
        channel_id: 1,
        job_id: pool_template.0,
        nonce: block.header.nonce as u32,
        version: block.header.version as i64,
        merkle_root: block.header.merkle_root,
    };

    let msg = SubmitSharesMessage {
        channel_id: 1,
        shares: vec![share_data],
    };

    let msg_bytes = msg.to_bytes().unwrap();
    let mut encoder = TlvEncoder::new();
    let encoded = encoder.encode(msg.message_type(), &msg_bytes).unwrap();
    let msg_result = server.handle_message(encoded, "miner-1".to_string()).await;
    assert!(
        msg_result.is_ok(),
        "handle_message failed: {:?}",
        msg_result.err()
    );

    let invocations = node_api.get_call_invocations().await;
    let submit_pow_calls: Vec<_> = invocations
        .iter()
        .filter(|(m, _)| m == "submit_pow")
        .collect();
    assert!(
        !submit_pow_calls.is_empty(),
        "Expected submit_pow to be called on valid block: {invocations:?}"
    );
}

#[tokio::test]
async fn test_submit_shares_calls_commons_submit_share() {
    use blvm_protocol::genesis;

    let ctx = blvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };

    let node_api = Arc::new(MockDatumNodeAPI::new_with_commons_payouts());
    let server = StratumV2Server::new(&ctx, node_api.clone()).await.unwrap();

    let block = genesis::mainnet_genesis();
    let snap = "aa".repeat(32);

    let pool_handle = server.get_pool();
    let mut pool = pool_handle.write().await;
    pool.register_miner("miner-1".to_string());
    pool.open_channel("miner-1", 1, 0).unwrap();
    let pool_template = pool.set_template(block.clone());
    pool.set_commons_snapshot_id(Some(snap.clone()));
    drop(pool);

    let share_data = messages::ShareData {
        channel_id: 1,
        job_id: pool_template.0,
        nonce: block.header.nonce as u32,
        version: block.header.version as i64,
        merkle_root: block.header.merkle_root,
    };

    let msg = SubmitSharesMessage {
        channel_id: 1,
        shares: vec![share_data],
    };

    let msg_bytes = msg.to_bytes().unwrap();
    let mut encoder = TlvEncoder::new();
    let encoded = encoder.encode(msg.message_type(), &msg_bytes).unwrap();
    let msg_result = server.handle_message(encoded, "miner-1".to_string()).await;
    assert!(
        msg_result.is_ok(),
        "handle_message failed: {:?}",
        msg_result.err()
    );

    let invocations = node_api.get_call_invocations().await;
    assert!(
        invocations.iter().any(|(m, _)| m == "commons_submit_share"),
        "{invocations:?}"
    );
    let body = node_api
        .last_submit_share_params()
        .await
        .expect("submit body");
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v.get("snapshot_id").and_then(|x| x.as_str()).unwrap(), snap);
    assert_eq!(v.get("bound_height").and_then(|x| x.as_u64()).unwrap(), 100);
    assert_eq!(v.get("miner").and_then(|x| x.as_str()).unwrap().len(), 64);
}

fn gridpool_script() -> String {
    format!("0020{}", "22".repeat(32))
}

async fn submit_genesis_share(node_api: Arc<MockDatumNodeAPI>) -> Vec<(String, usize)> {
    use blvm_protocol::genesis;

    let ctx = blvm_node::module::traits::ModuleContext {
        module_id: "test".to_string(),
        config: std::collections::HashMap::new(),
        data_dir: "test".to_string(),
        socket_path: "test".to_string(),
    };
    let server = StratumV2Server::new(&ctx, node_api.clone()).await.unwrap();
    server
        .template_generator()
        .generate_template()
        .await
        .expect("template");
    let block = genesis::mainnet_genesis();
    let pool_handle = server.get_pool();
    let mut pool = pool_handle.write().await;
    pool.register_miner("miner-1".to_string());
    pool.open_channel("miner-1", 1, 0).unwrap();
    let job_id = pool.set_template(block.clone()).0;
    drop(pool);
    let share_data = messages::ShareData {
        channel_id: 1,
        job_id,
        nonce: block.header.nonce as u32,
        version: block.header.version as i64,
        merkle_root: block.header.merkle_root,
    };
    let msg = SubmitSharesMessage {
        channel_id: 1,
        shares: vec![share_data],
    };
    let msg_bytes = msg.to_bytes().unwrap();
    let mut encoder = TlvEncoder::new();
    let encoded = encoder.encode(msg.message_type(), &msg_bytes).unwrap();
    server
        .handle_message(encoded, "miner-1".to_string())
        .await
        .unwrap();
    node_api.get_call_invocations().await
}

#[tokio::test]
async fn template_uses_gridpool_outputs_only_when_commons_is_absent() {
    let grid = Arc::new(MockDatumNodeAPI::new_with_gridpool_payouts());
    let generator = BlockTemplateGenerator::new(grid.clone());
    generator.generate_template().await.unwrap();
    assert!(generator.from_gridpool());
    let calls = grid.get_call_invocations().await;
    assert!(
        calls
            .iter()
            .any(|(method, _)| method == "gridpool_get_coinbase_outputs"),
        "{calls:?}"
    );
    assert!(
        !calls
            .iter()
            .any(|(method, _)| method == "commons_get_coinbase_outputs"
                || method == "get_coinbase_payout")
    );

    let mut both = MockDatumNodeAPI::new_with_commons_payouts();
    both.gridpool_payout_response = Some(
        serde_json::to_vec(&serde_json::json!({
            "outputs": [{"script": gridpool_script(), "value": 50_000_000}],
            "issue_work": true,
        }))
        .unwrap(),
    );
    let both = Arc::new(both);
    let generator = BlockTemplateGenerator::new(both.clone());
    generator.generate_template().await.unwrap();
    assert!(!generator.from_gridpool());
    let calls = both.get_call_invocations().await;
    assert!(
        calls
            .iter()
            .any(|(method, _)| method == "commons_get_coinbase_outputs")
    );
    assert!(
        !calls
            .iter()
            .any(|(method, _)| method == "gridpool_get_coinbase_outputs")
    );
}

#[tokio::test]
async fn empty_gridpool_list_does_not_issue_work() {
    let node = Arc::new(MockDatumNodeAPI::new_with_gridpool_holding());
    let generator = BlockTemplateGenerator::new(node);
    let err = generator.generate_template().await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("not issuing work"), "{msg}");
}

#[tokio::test]
async fn accepted_gridpool_block_notes_payment_once() {
    let node = Arc::new(MockDatumNodeAPI::new_with_gridpool_payouts());
    let calls = submit_genesis_share(node).await;
    let notes = calls
        .iter()
        .filter(|(method, _)| method == "gridpool_note_payment")
        .count();
    assert_eq!(notes, 1, "{calls:?}");
}

#[tokio::test]
async fn rejected_or_duplicate_gridpool_block_does_not_note_payment() {
    for result in [
        blvm_node::module::traits::SubmitBlockResult::Rejected("no".into()),
        blvm_node::module::traits::SubmitBlockResult::Duplicate,
    ] {
        let mut node = MockDatumNodeAPI::new_with_gridpool_payouts();
        node.submit_result = result;
        let calls = submit_genesis_share(Arc::new(node)).await;
        assert!(
            !calls
                .iter()
                .any(|(method, _)| method == "gridpool_note_payment"),
            "{calls:?}"
        );
    }
}

#[tokio::test]
async fn commons_template_does_not_note_a_gridpool_payment() {
    let node = Arc::new(MockDatumNodeAPI::new_with_commons_payouts());
    let calls = submit_genesis_share(node).await;
    assert!(
        !calls
            .iter()
            .any(|(method, _)| method == "gridpool_note_payment"),
        "{calls:?}"
    );
}

#[test]
fn module_toml_declares_stage3a_permissions() {
    let toml = include_str!("../module.toml");
    for cap in [
        "read_blockchain",
        "read_chain_state",
        "subscribe_events",
        "submit_block",
        "send_transactions",
        "call_module",
        "discover_modules",
        "publish_events",
    ] {
        assert!(toml.contains(cap), "missing capability {cap}");
    }
}

#[tokio::test]
async fn generate_template_does_not_use_another_miners_declaration() {
    let node_api = Arc::new(MockDatumNodeAPI::new_without_datum());
    let generator = BlockTemplateGenerator::new(node_api);
    generator.set_declared_txids("alice", Some(vec![[0xab; 32]]));
    assert_eq!(
        generator.declared_for(Some("alice")),
        Some(vec![[0xab; 32]])
    );
    assert_eq!(generator.declared_for(Some("bob")), None);
    assert_eq!(generator.declared_for(None), None);

    generator
        .generate_template()
        .await
        .expect("tip broadcast must not take Alice's JD path");
    generator
        .generate_template_for("bob")
        .await
        .expect("Bob has no declaration");
    let err = generator.generate_template_for("alice").await.unwrap_err();
    assert!(
        err.to_string().contains("declared template refused"),
        "{err}"
    );

    generator.set_declared_txids("alice", None);
    generator
        .generate_template_for("alice")
        .await
        .expect("cleared JD returns to node-selected");
}
