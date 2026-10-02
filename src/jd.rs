//! Job Declaration state. Message type IDs are official SV2 (0x50–0x60).
//! Incoming JD may be official 6-byte SV2 frames or this crate's TLV.

use std::collections::HashMap;

use blvm_protocol::block::calculate_tx_id;
use blvm_protocol::serialization::deserialize_transaction_with_offset;
use blvm_protocol::Hash;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct PendingDeclare {
    pub token: Vec<u8>,
    pub owner: String,
    pub txids: Vec<Hash>,
    pub missing: Vec<u16>,
}

const MAX_TOKENS: usize = 256;

#[derive(Debug, Default)]
pub struct JobDeclarationState {
    next_token: u64,
    /// token -> connection owner
    tokens: HashMap<Vec<u8>, String>,
    token_order: std::collections::VecDeque<Vec<u8>>,
    /// Pending is per (owner, request_id). Same id on two connections cannot overwrite.
    pending: HashMap<(String, u32), PendingDeclare>,
    /// Last accepted declaration per connection. Empty vec = coinbase-only.
    pub last_declared_by_owner: HashMap<String, Vec<Hash>>,
}

impl JobDeclarationState {
    pub fn allocate_token(&mut self, owner: &str) -> Vec<u8> {
        self.next_token = self.next_token.saturating_add(1);
        let mut h = Sha256::new();
        h.update(b"blvm-stratum-v2/jd-token/v1");
        h.update(owner.as_bytes());
        h.update(self.next_token.to_le_bytes());
        let token = h.finalize().to_vec();
        while self.tokens.len() >= MAX_TOKENS {
            if let Some(old) = self.token_order.pop_front() {
                self.tokens.remove(&old);
            } else {
                break;
            }
        }
        self.tokens.insert(token.clone(), owner.to_string());
        self.token_order.push_back(token.clone());
        token
    }

    pub fn token_ok(&self, token: &[u8], owner: &str) -> bool {
        self.tokens.get(token).map(String::as_str) == Some(owner)
    }

    pub fn consume_token(&mut self, token: &[u8]) {
        self.tokens.remove(token);
    }

    pub fn begin_declare(
        &mut self,
        request_id: u32,
        token: Vec<u8>,
        owner: String,
        txids: Vec<Hash>,
        missing: Vec<u16>,
    ) {
        let key = (owner.clone(), request_id);
        self.pending.insert(
            key,
            PendingDeclare {
                token,
                owner,
                txids,
                missing,
            },
        );
    }

    /// Each body must be a Bitcoin tx (exact wire bytes) whose txid is the declared hash.
    /// The parsed txs are not ingested; unknown declared ids still fail-close at GBT.
    pub fn provided_matches(pending: &PendingDeclare, bodies: &[Vec<u8>]) -> bool {
        if bodies.len() != pending.missing.len() {
            return false;
        }
        for (pos, raw) in pending.missing.iter().zip(bodies.iter()) {
            let i = *pos as usize;
            let Some(want) = pending.txids.get(i) else {
                return false;
            };
            let Ok((tx, consumed)) = deserialize_transaction_with_offset(raw) else {
                return false;
            };
            if consumed != raw.len() {
                return false;
            }
            if calculate_tx_id(&tx) != *want {
                return false;
            }
        }
        true
    }

    pub fn pending(&self, owner: &str, request_id: u32) -> Option<&PendingDeclare> {
        self.pending.get(&(owner.to_string(), request_id))
    }

    pub fn last_declared_for(&self, owner: &str) -> Option<&Vec<Hash>> {
        self.last_declared_by_owner.get(owner)
    }

    pub fn accept(&mut self, owner: &str, request_id: u32) -> Option<Vec<Hash>> {
        let p = self.pending.remove(&(owner.to_string(), request_id))?;
        if p.owner != owner {
            return None;
        }
        self.consume_token(&p.token);
        self.last_declared_by_owner
            .insert(p.owner, p.txids.clone());
        Some(p.txids)
    }

    pub fn mark_provided(&mut self, owner: &str, request_id: u32) -> Option<&PendingDeclare> {
        let p = self.pending.get_mut(&(owner.to_string(), request_id))?;
        if p.owner != owner {
            return None;
        }
        p.missing.clear();
        Some(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blvm_protocol::serialization::serialize_transaction;
    use blvm_protocol::{OutPoint, Transaction, TransactionInput, TransactionOutput};

    fn sample_tx(tag: u8) -> (Vec<u8>, Hash) {
        let tx = Transaction {
            version: 1,
            inputs: vec![TransactionInput {
                prevout: OutPoint {
                    hash: [tag; 32],
                    index: 0,
                },
                script_sig: vec![],
                sequence: 0xFFFFFFFF,
            }]
            .into(),
            outputs: vec![TransactionOutput {
                value: 1000,
                script_pubkey: vec![0x51],
            }]
            .into(),
            lock_time: 0,
        };
        (serialize_transaction(&tx), calculate_tx_id(&tx))
    }

    fn sha256d(raw: &[u8]) -> [u8; 32] {
        let first = Sha256::digest(raw);
        Sha256::digest(first).into()
    }

    #[test]
    fn allocate_token_is_unique() {
        let mut s = JobDeclarationState::default();
        let a = s.allocate_token("ep-a");
        let b = s.allocate_token("ep-b");
        assert_ne!(a, b);
        assert!(s.token_ok(&a, "ep-a"));
        assert!(!s.token_ok(&a, "ep-b"));
    }

    #[test]
    fn empty_declare_is_coinbase_only() {
        let mut s = JobDeclarationState::default();
        let tok = s.allocate_token("ep");
        s.begin_declare(1, tok, "ep".into(), vec![], vec![]);
        let ids = s.accept("ep", 1).unwrap();
        assert!(ids.is_empty());
        assert_eq!(s.last_declared_for("ep"), Some(&vec![]));
    }

    #[test]
    fn unknown_token_is_rejected() {
        let mut s = JobDeclarationState::default();
        let tok = s.allocate_token("ep");
        assert!(!s.token_ok(b"not-a-token", "ep"));
        assert!(s.token_ok(&tok, "ep"));
        assert!(!s.token_ok(&tok, "other"));
        assert!(s.accept("ep", 99).is_none());
    }

    #[test]
    fn provide_missing_requires_parsed_txid() {
        let mut s = JobDeclarationState::default();
        let tok = s.allocate_token("ep");
        let (body, txid) = sample_tx(1);
        let (other, _) = sample_tx(2);
        let mut leftover = body.clone();
        leftover.push(0xff);
        s.begin_declare(7, tok, "ep".into(), vec![txid], vec![0]);
        assert!(!JobDeclarationState::provided_matches(
            s.pending("ep", 7).unwrap(),
            &[b"declared-tx".to_vec()]
        ));
        assert!(!JobDeclarationState::provided_matches(
            s.pending("ep", 7).unwrap(),
            &[leftover]
        ));
        assert!(!JobDeclarationState::provided_matches(
            s.pending("ep", 7).unwrap(),
            &[other]
        ));
        assert!(JobDeclarationState::provided_matches(
            s.pending("ep", 7).unwrap(),
            &[body]
        ));
        s.mark_provided("ep", 7);
        let tok = s.pending("ep", 7).unwrap().token.clone();
        let ids = s.accept("ep", 7).unwrap();
        assert_eq!(ids, vec![txid]);
        assert!(!s.token_ok(&tok, "ep"));
    }

    #[test]
    fn same_request_id_does_not_overwrite_other_owner() {
        let mut s = JobDeclarationState::default();
        let a = s.allocate_token("alice");
        let b = s.allocate_token("bob");
        let tx_a: Hash = sha256d(b"a").into();
        let tx_b: Hash = sha256d(b"b").into();
        s.begin_declare(1, a, "alice".into(), vec![tx_a], vec![0]);
        s.begin_declare(1, b, "bob".into(), vec![tx_b], vec![0]);
        let pa = s.pending("alice", 1).unwrap();
        assert_eq!(pa.owner, "alice");
        assert_eq!(pa.txids, vec![tx_a]);
        let pb = s.pending("bob", 1).unwrap();
        assert_eq!(pb.owner, "bob");
        assert_eq!(pb.txids, vec![tx_b]);
        assert!(s.pending("bob", 1).is_some());
        assert!(s.accept("carol", 1).is_none());
        assert_eq!(s.accept("alice", 1).unwrap(), vec![tx_a]);
        assert_eq!(s.last_declared_for("bob"), None);
        assert_eq!(s.last_declared_for("alice"), Some(&vec![tx_a]));
    }

    #[test]
    fn same_owner_can_replace_own_pending() {
        let mut s = JobDeclarationState::default();
        let a1 = s.allocate_token("alice");
        let a2 = s.allocate_token("alice");
        let b = s.allocate_token("bob");
        let tx_a: Hash = sha256d(b"a").into();
        let tx_retry: Hash = sha256d(b"retry").into();
        let tx_b: Hash = sha256d(b"b").into();
        s.begin_declare(1, a1, "alice".into(), vec![tx_a], vec![0]);
        s.begin_declare(1, b, "bob".into(), vec![tx_b], vec![0]);
        s.begin_declare(1, a2, "alice".into(), vec![tx_retry], vec![]);
        assert_eq!(s.pending("alice", 1).unwrap().txids, vec![tx_retry]);
        assert_eq!(s.pending("bob", 1).unwrap().txids, vec![tx_b]);
        assert_eq!(s.accept("alice", 1).unwrap(), vec![tx_retry]);
        assert_eq!(s.accept("bob", 1).unwrap(), vec![tx_b]);
        assert_eq!(s.last_declared_for("alice"), Some(&vec![tx_retry]));
        assert_eq!(s.last_declared_for("bob"), Some(&vec![tx_b]));
    }

    #[test]
    fn provide_rejects_wrong_owner() {
        let mut s = JobDeclarationState::default();
        let tok = s.allocate_token("alice");
        let (body, txid) = sample_tx(3);
        s.begin_declare(3, tok, "alice".into(), vec![txid], vec![0]);
        assert!(s.pending("bob", 3).is_none());
        assert!(s.mark_provided("bob", 3).is_none());
        let p = s.pending("alice", 3).unwrap();
        assert_eq!(p.owner, "alice");
        assert!(JobDeclarationState::provided_matches(p, &[body]));
    }
}
