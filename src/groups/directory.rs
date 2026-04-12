//! Discoverable group cards (Phase C + Phase D.3).
//!
//! A [`GroupCard`] is the public projection of a group's committed state —
//! the information a non-member needs to decide whether to request access.
//!
//! Phase D.3: cards carry an **authority signature** over canonical card
//! bytes and a `state_hash` tying them to the current signed state commit.
//! Relays may re-publish exact signed card blobs but cannot mint new
//! revisions unless they are group-state authorities. Receivers drop cards
//! whose signature fails or whose revision is stale.

use crate::groups::policy::GroupPolicySummary;
use crate::groups::state_commit::{ApplyError, CARD_SIGNATURE_DOMAIN, DEFAULT_CARD_TTL_SECS};
use crate::identity::AgentKeypair;
use ant_quic::crypto::raw_public_keys::pqc::{
    sign_with_ml_dsa, verify_with_ml_dsa, MlDsaSignature,
};
use ant_quic::MlDsaPublicKey;
use serde::{Deserialize, Serialize};

/// Public-facing card for a discoverable group.
///
/// Contains the information a non-member needs to decide whether to request
/// access. Does NOT include private content, roster, or encrypted data.
///
/// Phase D.3 fields (`revision`, `state_hash`, `prev_state_hash`,
/// `issued_at`, `expires_at`, `authority_agent_id`, `authority_public_key`,
/// `withdrawn`, `signature`) are serde-default for backward compatibility
/// with pre-D.3 blobs. Newly-issued cards set all of them and are signed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupCard {
    pub group_id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub banner_url: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub policy_summary: GroupPolicySummary,
    pub owner_agent_id: String,
    pub admin_count: u32,
    pub member_count: u32,
    pub created_at: u64,
    pub updated_at: u64,
    pub request_access_enabled: bool,

    // ── Phase D.3: state-commit binding and authority signature ─────────
    /// Monotonic revision of the signed state commit this card represents.
    /// Higher revisions supersede lower ones immediately.
    #[serde(default)]
    pub revision: u64,
    /// `state_hash` of the commit this card derives from.
    #[serde(default)]
    pub state_hash: String,
    /// Previous `state_hash` for chain linking on the directory plane.
    #[serde(default)]
    pub prev_state_hash: Option<String>,
    /// Unix milliseconds when the card was issued.
    #[serde(default)]
    pub issued_at: u64,
    /// Unix milliseconds after which the card is cache-cleanup candidate.
    /// TTL is **not** the primary validity mechanism — higher revisions
    /// supersede older cards immediately regardless of expiry.
    #[serde(default)]
    pub expires_at: u64,
    /// Hex agent_id of the authority who signed this card (owner/admin).
    #[serde(default)]
    pub authority_agent_id: String,
    /// Hex ML-DSA-65 public key of the authority, for standalone verify.
    #[serde(default)]
    pub authority_public_key: String,
    /// Withdrawal marker — `true` means this card supersedes any previous
    /// public card and signals the group has been hidden/deleted.
    #[serde(default)]
    pub withdrawn: bool,
    /// Hex ML-DSA-65 signature over the canonical card bytes.
    #[serde(default)]
    pub signature: String,
}

impl GroupCard {
    /// Canonical bytes signed by the authority to produce `signature`.
    #[must_use]
    pub fn signable_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(512);
        buf.extend_from_slice(CARD_SIGNATURE_DOMAIN);
        push_len_prefixed(&mut buf, self.group_id.as_bytes());
        buf.extend_from_slice(&self.revision.to_le_bytes());
        push_len_prefixed(&mut buf, self.state_hash.as_bytes());
        push_len_prefixed(
            &mut buf,
            self.prev_state_hash.as_deref().unwrap_or("").as_bytes(),
        );
        buf.extend_from_slice(&self.issued_at.to_le_bytes());
        buf.extend_from_slice(&self.expires_at.to_le_bytes());
        push_len_prefixed(&mut buf, self.name.as_bytes());
        push_len_prefixed(&mut buf, self.description.as_bytes());
        push_len_prefixed(
            &mut buf,
            self.avatar_url.as_deref().unwrap_or("").as_bytes(),
        );
        push_len_prefixed(
            &mut buf,
            self.banner_url.as_deref().unwrap_or("").as_bytes(),
        );
        // Tags: length-prefixed count, sorted-deduped bytes so tag reorder
        // or duplicates don't change the signature input (matches
        // compute_public_meta_hash ordering in state_commit.rs).
        let mut tags = self.tags.clone();
        tags.sort();
        tags.dedup();
        buf.extend_from_slice(&(tags.len() as u32).to_le_bytes());
        for t in &tags {
            push_len_prefixed(&mut buf, t.as_bytes());
        }
        // Policy summary: bincode is deterministic for simple structs.
        let policy_bytes = bincode::serialize(&self.policy_summary).unwrap_or_default();
        push_len_prefixed(&mut buf, &policy_bytes);
        push_len_prefixed(&mut buf, self.owner_agent_id.as_bytes());
        buf.extend_from_slice(&self.admin_count.to_le_bytes());
        buf.extend_from_slice(&self.member_count.to_le_bytes());
        buf.extend_from_slice(&self.created_at.to_le_bytes());
        buf.extend_from_slice(&self.updated_at.to_le_bytes());
        buf.push(if self.request_access_enabled { 1 } else { 0 });
        buf.push(if self.withdrawn { 1 } else { 0 });
        push_len_prefixed(&mut buf, self.authority_agent_id.as_bytes());
        push_len_prefixed(&mut buf, self.authority_public_key.as_bytes());
        buf
    }

    /// Sign this card with the given authority keypair. Populates
    /// `authority_agent_id`, `authority_public_key`, and `signature`.
    pub fn sign(&mut self, keypair: &AgentKeypair) -> Result<(), ApplyError> {
        self.authority_agent_id = hex::encode(keypair.agent_id().as_bytes());
        self.authority_public_key = hex::encode(keypair.public_key().as_bytes());
        self.signature = String::new();
        let sig = sign_with_ml_dsa(keypair.secret_key(), &self.signable_bytes())
            .map_err(|e| ApplyError::InvalidSignature(format!("card sign: {e:?}")))?;
        self.signature = hex::encode(sig.as_bytes());
        Ok(())
    }

    /// Verify the authority signature on this card.
    ///
    /// Checks:
    /// - `authority_public_key` hex decodes to a valid ML-DSA-65 key,
    /// - the derived AgentId matches `authority_agent_id`,
    /// - `signature` verifies over `signable_bytes()`.
    ///
    /// Returns `Ok(())` on success. Does **not** check whether the signer
    /// is currently authorised for the group (that is done at
    /// apply-time against the local roster view).
    pub fn verify_signature(&self) -> Result<(), ApplyError> {
        if self.signature.is_empty() || self.authority_public_key.is_empty() {
            return Err(ApplyError::InvalidSignature("missing signature".into()));
        }
        let pubkey_bytes = hex::decode(&self.authority_public_key)
            .map_err(|e| ApplyError::InvalidSignature(format!("bad pubkey hex: {e}")))?;
        let pubkey = MlDsaPublicKey::from_bytes(&pubkey_bytes)
            .map_err(|e| ApplyError::InvalidSignature(format!("bad pubkey: {e:?}")))?;
        let derived = hex::encode(ant_quic::derive_peer_id_from_public_key(&pubkey).0);
        if derived != self.authority_agent_id {
            return Err(ApplyError::InvalidSignature(format!(
                "authority_agent_id {} != derived {}",
                self.authority_agent_id, derived
            )));
        }
        let sig_bytes = hex::decode(&self.signature)
            .map_err(|e| ApplyError::InvalidSignature(format!("bad sig hex: {e}")))?;
        let sig = MlDsaSignature::from_bytes(&sig_bytes)
            .map_err(|e| ApplyError::InvalidSignature(format!("bad sig: {e:?}")))?;
        verify_with_ml_dsa(&pubkey, &self.signable_bytes(), &sig)
            .map_err(|e| ApplyError::InvalidSignature(format!("card verify failed: {e:?}")))
    }

    /// Seconds-since-epoch convenience for default card TTL.
    #[must_use]
    pub fn default_ttl_secs() -> u64 {
        DEFAULT_CARD_TTL_SECS
    }

    /// Helper: decide whether `other` supersedes `self` for the same `group_id`.
    /// Higher `revision` wins; on ties the higher `issued_at` wins. Caller
    /// must have verified both signatures first.
    #[must_use]
    pub fn supersedes(&self, other: &GroupCard) -> bool {
        if self.group_id != other.group_id {
            return false;
        }
        self.revision > other.revision
            || (self.revision == other.revision && self.issued_at > other.issued_at)
    }
}

fn push_len_prefixed(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::groups::policy::{
        GroupAdmission, GroupConfidentiality, GroupDiscoverability, GroupReadAccess,
        GroupWriteAccess,
    };

    fn sample_summary() -> GroupPolicySummary {
        GroupPolicySummary {
            discoverability: GroupDiscoverability::PublicDirectory,
            admission: GroupAdmission::RequestAccess,
            confidentiality: GroupConfidentiality::MlsEncrypted,
            read_access: GroupReadAccess::MembersOnly,
            write_access: GroupWriteAccess::MembersOnly,
        }
    }

    fn sample_card() -> GroupCard {
        GroupCard {
            group_id: "abcd".repeat(16),
            name: "Test".into(),
            description: "desc".into(),
            avatar_url: None,
            banner_url: None,
            tags: vec!["rust".into()],
            policy_summary: sample_summary(),
            owner_agent_id: "ff".repeat(32),
            admin_count: 1,
            member_count: 5,
            created_at: 0,
            updated_at: 0,
            request_access_enabled: true,
            revision: 1,
            state_hash: "sh-1".into(),
            prev_state_hash: None,
            issued_at: 100,
            expires_at: 200,
            authority_agent_id: String::new(),
            authority_public_key: String::new(),
            withdrawn: false,
            signature: String::new(),
        }
    }

    #[test]
    fn card_roundtrip() {
        let c = sample_card();
        let json = serde_json::to_string(&c).unwrap();
        let c2: GroupCard = serde_json::from_str(&json).unwrap();
        assert_eq!(c, c2);
    }

    #[test]
    fn card_sign_and_verify_roundtrip() {
        let kp = AgentKeypair::generate().unwrap();
        let mut c = sample_card();
        c.sign(&kp).unwrap();
        assert!(!c.signature.is_empty());
        c.verify_signature().unwrap();
    }

    #[test]
    fn card_signature_detects_tamper() {
        let kp = AgentKeypair::generate().unwrap();
        let mut c = sample_card();
        c.sign(&kp).unwrap();

        let mut bad = c.clone();
        bad.name = "Tampered".into();
        assert!(bad.verify_signature().is_err());

        let mut bad = c.clone();
        bad.revision = 999;
        assert!(bad.verify_signature().is_err());

        let mut bad = c.clone();
        bad.withdrawn = true;
        assert!(bad.verify_signature().is_err());
    }

    #[test]
    fn card_signature_rejects_wrong_authority() {
        let kp1 = AgentKeypair::generate().unwrap();
        let kp2 = AgentKeypair::generate().unwrap();
        let mut c = sample_card();
        c.sign(&kp1).unwrap();
        // Swap the claimed agent_id to kp2 — mismatch must be detected.
        c.authority_agent_id = hex::encode(kp2.agent_id().as_bytes());
        assert!(c.verify_signature().is_err());
    }

    #[test]
    fn supersedes_by_revision() {
        let kp = AgentKeypair::generate().unwrap();
        let mut lo = sample_card();
        lo.revision = 1;
        lo.sign(&kp).unwrap();
        let mut hi = sample_card();
        hi.revision = 2;
        hi.sign(&kp).unwrap();
        assert!(hi.supersedes(&lo));
        assert!(!lo.supersedes(&hi));
    }

    #[test]
    fn supersedes_by_issued_at_on_revision_tie() {
        let kp = AgentKeypair::generate().unwrap();
        let mut a = sample_card();
        a.revision = 1;
        a.issued_at = 100;
        a.sign(&kp).unwrap();
        let mut b = sample_card();
        b.revision = 1;
        b.issued_at = 200;
        b.sign(&kp).unwrap();
        assert!(b.supersedes(&a));
        assert!(!a.supersedes(&b));
    }

    #[test]
    fn supersedes_requires_same_group_id() {
        let kp = AgentKeypair::generate().unwrap();
        let mut a = sample_card();
        a.revision = 1;
        a.sign(&kp).unwrap();
        let mut b = sample_card();
        b.revision = 2;
        b.group_id = "different".into();
        b.sign(&kp).unwrap();
        assert!(!b.supersedes(&a));
    }

    #[test]
    fn unsigned_card_verify_fails() {
        let c = sample_card();
        assert!(c.verify_signature().is_err());
    }
}
