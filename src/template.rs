//! Block template generation for Stratum V2

use crate::error::StratumV2Error;
use blvm_node::module::traits::NodeAPI;
use blvm_protocol::{
    mining::calculate_merkle_root, segwit::compute_witness_merkle_root, Block, Hash,
    TransactionOutput,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};

#[derive(Clone)]
struct PayoutOut {
    value: i64,
    script: Vec<u8>,
}

enum CommonsWork {
    Absent,
    Ready {
        outs: Vec<PayoutOut>,
        snapshot_id: Option<String>,
        banned_miners: Vec<[u8; 32]>,
    },
}

/// Block template generator
#[derive(Clone)]
pub struct BlockTemplateGenerator {
    /// Node API for querying node state
    node_api: Arc<dyn NodeAPI>,
    last_commons_snapshot: Arc<Mutex<Option<String>>>,
    last_commons_banned: Arc<Mutex<Vec<[u8; 32]>>>,
    /// Per-connection declared txids. Missing owner = node selects.
    /// Empty vec = coinbase-only for that miner.
    declared_txids: Arc<Mutex<HashMap<String, Vec<Hash>>>>,
}

impl BlockTemplateGenerator {
    /// Create a new block template generator
    pub fn new(node_api: Arc<dyn NodeAPI>) -> Self {
        Self {
            node_api,
            last_commons_snapshot: Arc::new(Mutex::new(None)),
            last_commons_banned: Arc::new(Mutex::new(Vec::new())),
            declared_txids: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn set_declared_txids(&self, owner: &str, txids: Option<Vec<Hash>>) {
        if let Ok(mut g) = self.declared_txids.lock() {
            match txids {
                Some(ids) => {
                    g.insert(owner.to_string(), ids);
                }
                None => {
                    g.remove(owner);
                }
            }
        }
    }

    pub fn declared_for(&self, owner: Option<&str>) -> Option<Vec<Hash>> {
        let owner = owner?;
        self.declared_txids.lock().ok()?.get(owner).cloned()
    }

    pub fn declared_owners(&self) -> Vec<String> {
        self.declared_txids
            .lock()
            .ok()
            .map(|g| g.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn last_commons_snapshot(&self) -> Option<String> {
        self.last_commons_snapshot
            .lock()
            .ok()
            .and_then(|g| g.clone())
    }

    pub fn last_commons_banned(&self) -> Vec<[u8; 32]> {
        self.last_commons_banned
            .lock()
            .ok()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// Generate a block template
    ///
    /// Commons Pool (`blvm-commons-pool`) supplies every coinbase output when loaded.
    /// DATUM still supplies a single first-output address. Neither binds Bitcoin.
    pub async fn generate_template(&self) -> Result<Block, StratumV2Error> {
        self.generate_template_inner(None).await
    }

    /// Use this miner's declaration only. Does not apply another connection's JD.
    pub async fn generate_template_for(&self, owner: &str) -> Result<Block, StratumV2Error> {
        self.generate_template_inner(Some(owner)).await
    }

    async fn generate_template_inner(
        &self,
        owner: Option<&str>,
    ) -> Result<Block, StratumV2Error> {
        debug!("Generating block template via NodeAPI");

        if let Ok(mut g) = self.last_commons_snapshot.lock() {
            *g = None;
        }
        if let Ok(mut g) = self.last_commons_banned.lock() {
            g.clear();
        }
        let commons = match self.get_commons_work().await? {
            CommonsWork::Absent => None,
            CommonsWork::Ready {
                outs,
                snapshot_id,
                banned_miners,
            } => {
                if let Ok(mut g) = self.last_commons_snapshot.lock() {
                    *g = snapshot_id;
                }
                if let Ok(mut g) = self.last_commons_banned.lock() {
                    *g = banned_miners;
                }
                Some(outs)
            }
        };
        let (coinbase_script, coinbase_address) = if let Some(ref outs) = commons {
            let hex_script = hex::encode(&outs[0].script);
            (None, Some(format!("hex:{hex_script}")))
        } else {
            self.get_coinbase_from_datum().await
        };

        let rules = vec!["segwit".to_string()];
        let commons_pairs: Vec<(i64, Vec<u8>)> = commons
            .as_ref()
            .map(|outs| outs.iter().map(|o| (o.value, o.script.clone())).collect())
            .unwrap_or_default();
        let declared = self.declared_for(owner);

        let template = if let Some(txids) = declared {
            self.node_api
                .get_block_template_declared(
                    rules,
                    coinbase_script,
                    commons_pairs,
                    txids,
                )
                .await
                .map_err(|e| {
                    StratumV2Error::TemplateError(format!(
                        "declared template refused (no mempool fallback): {e}"
                    ))
                })?
        } else if !commons_pairs.is_empty() {
            match self
                .node_api
                .get_block_template_with_outputs(
                    rules.clone(),
                    coinbase_script.clone(),
                    commons_pairs.clone(),
                )
                .await
            {
                Ok(t) => t,
                Err(e) => {
                    warn!("get_block_template_with_outputs unavailable, falling back: {e}");
                    self.node_api
                        .get_block_template(rules, coinbase_script, coinbase_address)
                        .await
                        .map_err(|e| {
                            StratumV2Error::TemplateError(format!(
                                "Failed to get block template: {e}"
                            ))
                        })?
                }
            }
        } else {
            self.node_api
                .get_block_template(rules, coinbase_script, coinbase_address)
                .await
                .map_err(|e| {
                    StratumV2Error::TemplateError(format!("Failed to get block template: {e}"))
                })?
        };

        info!(
            "Got block template: height={}, {} transactions",
            template.height,
            template.transactions.len()
        );

        let mut block = block_from_template(template);

        if let Some(outs) = commons {
            // Pass 28+ NodeAPI GBT already embeds Commons (fit + BIP141).
            // Splice only when the node coinbase is still a single address
            // (published pin without Commons GBT).
            if !node_already_pays_commons(&block.transactions[0], &outs) {
                apply_commons_payouts(&mut block, &outs)?;
            }
        }

        info!(
            "Converted template to block: prev_hash={:x?}, {} transactions, merkle_root={:x?}",
            &block.header.prev_block_hash[..8],
            block.transactions.len(),
            &block.header.merkle_root[..8]
        );

        Ok(block)
    }

    /// Explicit module id. Distinct methods — never `get_coinbase_payout`.
    /// Holding means do not issue pool work (node-default coinbase is not a substitute).
    async fn get_commons_work(&self) -> Result<CommonsWork, StratumV2Error> {
        if self
            .node_api
            .is_module_available("blvm-commons-pool")
            .await
            .ok()
            != Some(true)
        {
            return Ok(CommonsWork::Absent);
        }
        let response = self
            .node_api
            .call_module(
                Some("blvm-commons-pool"),
                "commons_get_coinbase_outputs",
                vec![],
            )
            .await
            .map_err(|e| StratumV2Error::TemplateError(format!("commons outputs: {e}")))?;
        let json: serde_json::Value = serde_json::from_slice(&response)
            .map_err(|e| StratumV2Error::TemplateError(format!("commons json: {e}")))?;
        if json.get("issue_work").and_then(|x| x.as_bool()) == Some(false) {
            return Err(StratumV2Error::TemplateError(
                "commons pool holding; not issuing work".into(),
            ));
        }
        let Some(outs) = json.get("outputs").and_then(|x| x.as_array()) else {
            return Ok(CommonsWork::Absent);
        };
        if outs.is_empty() {
            return Ok(CommonsWork::Absent);
        }
        let mut parsed = Vec::with_capacity(outs.len());
        for o in outs {
            let value = o
                .get("value_sats")
                .or_else(|| o.get("value"))
                .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|n| n.max(0) as u64)))
                .ok_or_else(|| {
                    StratumV2Error::TemplateError("commons output missing value".into())
                })? as i64;
            let script = hex::decode(o.get("script").and_then(|s| s.as_str()).ok_or_else(|| {
                StratumV2Error::TemplateError("commons output missing script".into())
            })?)
            .map_err(|e| StratumV2Error::TemplateError(format!("commons script: {e}")))?;
            parsed.push(PayoutOut { value, script });
        }
        let snapshot_id = json
            .get("snapshot_id")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        info!(
            "Using Commons Pool coinbase: {} outputs",
            parsed.len()
        );
        let banned_miners = json
            .get("banned_miners")
            .and_then(|x| x.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        let s = v.as_str()?;
                        let b = hex::decode(s).ok()?;
                        (b.len() == 32).then(|| {
                            let mut id = [0u8; 32];
                            id.copy_from_slice(&b);
                            id
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(CommonsWork::Ready {
            outs: parsed,
            snapshot_id,
            banned_miners,
        })
    }

    /// Record a valid Stratum share in the notebook. Not a Bitcoin bind.
    /// Miner id is sha256(endpoint); that is not an H9 roster key.
    pub async fn submit_commons_share(
        &self,
        endpoint: &str,
        share_hash: &[u8; 32],
        snapshot_id: Option<&str>,
        bound_height: u64,
    ) -> Result<(), StratumV2Error> {
        if self
            .node_api
            .is_module_available("blvm-commons-pool")
            .await
            .ok()
            != Some(true)
        {
            return Ok(());
        }
        let Some(snap) = snapshot_id.filter(|s| !s.is_empty()) else {
            return Ok(());
        };
        use sha2::{Digest, Sha256};
        let miner = Sha256::digest(endpoint.as_bytes());
        let body = serde_json::json!({
            "miner": hex::encode(miner),
            "share_hash": hex::encode(share_hash),
            "snapshot_id": snap,
            "bound_height": bound_height,
        });
        let params = serde_json::to_vec(&body).map_err(|e| {
            StratumV2Error::TemplateError(format!("commons share json: {e}"))
        })?;
        match self
            .node_api
            .call_module(Some("blvm-commons-pool"), "commons_submit_share", params)
            .await
        {
            Ok(_) => {
                debug!("commons_submit_share sent (pool tally only)");
                Ok(())
            }
            Err(e) => {
                warn!("commons_submit_share failed: {e}");
                Ok(())
            }
        }
    }

    /// Credit a found block's coinbase. Not a Bitcoin bind.
    pub async fn credit_commons_find(
        &self,
        block: &Block,
        snapshot_id: Option<&str>,
        endpoint: Option<&str>,
    ) -> Result<(), StratumV2Error> {
        if self
            .node_api
            .is_module_available("blvm-commons-pool")
            .await
            .ok()
            != Some(true)
        {
            return Ok(());
        }
        let coinbase = block.transactions.first().ok_or_else(|| {
            StratumV2Error::TemplateError("found block missing coinbase".into())
        })?;
        let outputs: Vec<serde_json::Value> = coinbase
            .outputs
            .iter()
            .map(|o| {
                serde_json::json!({
                    "script": hex::encode(&o.script_pubkey),
                    "value_sats": o.value.max(0) as u64,
                })
            })
            .collect();
        let mut body = serde_json::json!({ "outputs": outputs });
        if let Some(id) = snapshot_id {
            body["snapshot_id"] = serde_json::json!(id);
        }
        if let Some(ep) = endpoint {
            use sha2::{Digest, Sha256};
            body["miner"] = serde_json::json!(hex::encode(Sha256::digest(ep.as_bytes())));
        }
        body["block_hash"] = serde_json::json!(hex::encode(crate::pool::stratum_header_hash(
            &block.header
        )));
        let params = serde_json::to_vec(&body).map_err(|e| {
            StratumV2Error::TemplateError(format!("commons credit json: {e}"))
        })?;
        match self
            .node_api
            .call_module(Some("blvm-commons-pool"), "commons_credit_find", params)
            .await
        {
            Ok(_) => {
                info!("commons_credit_find sent (pool credit only)");
                Ok(())
            }
            Err(e) => {
                warn!("commons_credit_find failed: {e}");
                Ok(())
            }
        }
    }

    /// Query DATUM for coinbase payout when pool module is loaded.
    /// Returns (script, address) for get_block_template; address uses "hex:" prefix for raw script bytes.
    async fn get_coinbase_from_datum(&self) -> (Option<Vec<u8>>, Option<String>) {
        if self.node_api.is_module_available("datum").await.ok() != Some(true) {
            return (None, None);
        }
        let response = match self
            .node_api
            .call_module(Some("datum"), "get_coinbase_payout", vec![])
            .await
        {
            Ok(r) => r,
            Err(_) => return (None, None),
        };

        let json: serde_json::Value = match serde_json::from_slice(&response) {
            Ok(j) => j,
            Err(e) => {
                debug!("DATUM coinbase response parse error: {}", e);
                return (None, None);
            }
        };

        let outputs = match json.get("outputs").and_then(|o| o.as_array()) {
            Some(outs) if !outs.is_empty() => outs,
            _ => return (None, None),
        };

        // Datum path remains first-output only (existing single-output node API).
        let first = match outputs.first().and_then(|o| o.get("script")) {
            Some(s) => s,
            None => return (None, None),
        };

        let script_hex = match first.as_str() {
            Some(h) => h.to_string(),
            None => return (None, None),
        };

        let script = match hex::decode(&script_hex) {
            Ok(s) => s,
            Err(_) => return (None, None),
        };

        info!(
            "Using DATUM coinbase payout: {} bytes from pool",
            script.len()
        );
        (None, Some(format!("hex:{script_hex}")))
    }
}

fn block_from_template(template: blvm_protocol::mining::BlockTemplate) -> Block {
    let mut all_transactions = vec![template.coinbase_tx];
    all_transactions.extend(template.transactions);
    Block {
        header: template.header,
        transactions: all_transactions.into_boxed_slice(),
    }
}

/// Same shape as pool `claimed_matches_template`, compared on scripts:
/// first value may be a fee top-up; later outputs exact; trailing 0-value OK.
fn node_already_pays_commons(coinbase: &blvm_protocol::Transaction, outs: &[PayoutOut]) -> bool {
    if outs.is_empty() {
        return false;
    }
    let claimed = &coinbase.outputs;
    if claimed.len() < outs.len() {
        return false;
    }
    if claimed[0].script_pubkey != outs[0].script || claimed[0].value < outs[0].value {
        return false;
    }
    for (c, e) in claimed[1..outs.len()].iter().zip(outs[1..].iter()) {
        if c.script_pubkey != e.script || c.value != e.value {
            return false;
        }
    }
    claimed[outs.len()..].iter().all(|o| o.value == 0)
}

fn apply_commons_payouts(block: &mut Block, outs: &[PayoutOut]) -> Result<(), StratumV2Error> {
    if block.transactions.is_empty() {
        return Err(StratumV2Error::TemplateError(
            "template missing coinbase".into(),
        ));
    }
    let mut txs = block.transactions.to_vec();
    let budget = txs[0]
        .outputs
        .iter()
        .map(|o| o.value)
        .fold(0i64, i64::saturating_add);
    let kept = last_bip141_commitment(&txs[0].outputs);
    let fitted = fit_payouts_to_budget(outs, budget)?;
    let mut cb_outs: Vec<TransactionOutput> = fitted
        .iter()
        .map(|o| TransactionOutput {
            value: o.value,
            script_pubkey: o.script.clone(),
        })
        .collect();
    // Coinbase wtxid is 0: payout splice does not change the witness root.
    // Keep the node's commitment when present so empty-stack recompute cannot stale it.
    if let Some(script) = kept {
        cb_outs.push(TransactionOutput {
            value: 0,
            script_pubkey: script,
        });
    } else {
        let witnesses = vec![Vec::new(); txs.len()];
        txs[0].outputs = cb_outs.clone().into();
        let tmp = Block {
            header: block.header.clone(),
            transactions: txs.clone().into_boxed_slice(),
        };
        let root = compute_witness_merkle_root(&tmp, &witnesses).map_err(|e| {
            StratumV2Error::TemplateError(format!("witness merkle: {e}"))
        })?;
        cb_outs.push(TransactionOutput {
            value: 0,
            script_pubkey: bip141_commitment_script(&root, &[0u8; 32]),
        });
    }
    txs[0].outputs = cb_outs.into();
    let merkle = calculate_merkle_root(&txs).map_err(|e| {
        StratumV2Error::TemplateError(format!("merkle after commons payouts: {e}"))
    })?;
    block.header.merkle_root = merkle;
    block.transactions = txs.into_boxed_slice();
    Ok(())
}

/// Same rule as consensus `fit_payouts_to_reward`: overflow is an error;
/// shortfall tops up the first output. Budget is the node's coinbase total
/// (subsidy, and fees if the template already included them).
fn fit_payouts_to_budget(outs: &[PayoutOut], budget: i64) -> Result<Vec<PayoutOut>, StratumV2Error> {
    if outs.is_empty() {
        return Err(StratumV2Error::TemplateError(
            "commons payouts empty".into(),
        ));
    }
    let sum = outs
        .iter()
        .map(|o| o.value)
        .fold(0i64, i64::saturating_add);
    if sum > budget {
        return Err(StratumV2Error::TemplateError(format!(
            "commons payouts {sum} exceed coinbase budget {budget}"
        )));
    }
    let mut fitted = outs.to_vec();
    if sum < budget {
        fitted[0].value = fitted[0].value.saturating_add(budget - sum);
    }
    Ok(fitted)
}

fn last_bip141_commitment(outputs: &[TransactionOutput]) -> Option<Vec<u8>> {
    outputs.iter().rev().find_map(|o| {
        let s = &o.script_pubkey;
        if s.len() >= 38 && s[0] == 0x6a && s[1] == 0x24 && s[2..6] == [0xaa, 0x21, 0xa9, 0xed] {
            Some(s.clone())
        } else {
            None
        }
    })
}

/// BIP141 `OP_RETURN 0x24 0xaa21a9ed || sha256d(root || nonce)`. Inlined so
/// Stratum CI does not need an unpublished consensus export.
fn bip141_commitment_script(root: &[u8; 32], nonce: &[u8; 32]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut pre = [0u8; 64];
    pre[..32].copy_from_slice(root);
    pre[32..].copy_from_slice(nonce);
    let first = Sha256::digest(pre);
    let commit = Sha256::digest(first);
    let mut script = vec![0x6a, 0x24, 0xaa, 0x21, 0xa9, 0xed];
    script.extend_from_slice(&commit);
    script
}
