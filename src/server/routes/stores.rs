//! Route handlers (`category: "stores"` in `src/api/mod.rs`).
//!
//! Extracted verbatim from `src/server/mod.rs` as part of the #125 / WS1.4
//! server decomposition. The router registrations stay in the parent module.

use super::super::crdt_subscriptions;
use super::super::state::AppState;
use super::super::{
    api_error, bad_request, direct_message_send_config, forbidden, not_found, parse_agent_id_hex,
};
use super::named_groups::GROUP_BACKGROUND_PUBLISH_DELAY;
use crate as x0x;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use x0x::contacts::TrustLevel;
use x0x::identity::AgentId;
use x0x::kv::encrypted::KvSecureContext;
use x0x::logging::LogHexId;

pub(in crate::server) const KV_STORE_DELTA_DM_PREFIX: &[u8] = b"X0X-KV-DELTA-V1\n";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::server) struct KvStoreDirectDelta {
    store_id: String,
    peer_id: saorsa_gossip_types::PeerId,
    delta: x0x::kv::KvStoreDelta,
}

fn encode_kv_store_delta_direct_payload(
    store_id: &str,
    peer_id: saorsa_gossip_types::PeerId,
    delta: &x0x::kv::KvStoreDelta,
) -> serde_json::Result<Vec<u8>> {
    let msg = KvStoreDirectDelta {
        store_id: store_id.to_string(),
        peer_id,
        delta: delta.clone(),
    };
    let json = serde_json::to_vec(&msg)?;
    let mut payload = Vec::with_capacity(KV_STORE_DELTA_DM_PREFIX.len() + json.len());
    payload.extend_from_slice(KV_STORE_DELTA_DM_PREFIX);
    payload.extend_from_slice(&json);
    Ok(payload)
}

fn kv_store_delta_direct_delivery_config() -> x0x::dm::DmSendConfig {
    let mut config = direct_message_send_config();
    config.require_gossip = true;
    config.require_gossip_ack = true;
    config
}

async fn kv_store_delta_direct_recipients(state: &AppState) -> Vec<String> {
    let local_agent_hex = hex::encode(state.agent.agent_id().as_bytes());
    let contacts = state.contacts.read().await;
    contacts
        .list()
        .into_iter()
        .filter_map(|contact| {
            let recipient = hex::encode(contact.agent_id.as_bytes());
            if recipient == local_agent_hex || contact.trust_level == TrustLevel::Blocked {
                return None;
            }
            let caps = contact.dm_capabilities.as_ref()?;
            if !caps.gossip_inbox || caps.kem_public_key.is_empty() {
                return None;
            }
            Some(recipient)
        })
        .collect()
}

fn spawn_kv_store_delta_delivery_one(
    state: &AppState,
    recipient_hex: &str,
    store_id: &str,
    peer_id: saorsa_gossip_types::PeerId,
    delta: &x0x::kv::KvStoreDelta,
    delay: Option<Duration>,
) {
    let recipient = match parse_agent_id_hex(recipient_hex) {
        Ok(id) => id,
        Err(e) => {
            tracing::warn!(
                recipient = %LogHexId::agent(&recipient_hex),
                "cannot direct-deliver kv-store delta: invalid recipient id: {e}"
            );
            return;
        }
    };
    let payload = match encode_kv_store_delta_direct_payload(store_id, peer_id, delta) {
        Ok(payload) => payload,
        Err(e) => {
            tracing::warn!(
                store_id,
                "failed to serialize kv-store delta for direct delivery: {e}"
            );
            return;
        }
    };
    let agent = Arc::clone(&state.agent);
    let recipient_label = recipient_hex.to_string();
    let store_label = store_id.to_string();
    tokio::spawn(async move {
        if let Some(delay) = delay {
            tokio::time::sleep(delay).await;
        }
        if let Err(e) = agent
            .send_direct_with_config(&recipient, payload, kv_store_delta_direct_delivery_config())
            .await
        {
            tracing::warn!(
                store_id = %store_label,
                recipient = %LogHexId::agent(&recipient_label),
                "failed to direct-deliver kv-store delta: {e}"
            );
        }
    });
}

fn spawn_kv_store_delta_delivery(
    state: &AppState,
    recipients: Vec<String>,
    store_id: &str,
    peer_id: saorsa_gossip_types::PeerId,
    delta: &x0x::kv::KvStoreDelta,
) {
    for recipient in recipients {
        spawn_kv_store_delta_delivery_one(state, &recipient, store_id, peer_id, delta, None);
        spawn_kv_store_delta_delivery_one(
            state,
            &recipient,
            store_id,
            peer_id,
            delta,
            Some(GROUP_BACKGROUND_PUBLISH_DELAY),
        );
    }
}

pub(in crate::server) async fn apply_direct_kv_store_delta(
    state: &AppState,
    sender: x0x::identity::AgentId,
    delta_msg: KvStoreDirectDelta,
) {
    let store_id = delta_msg.store_id.clone();
    let handle = {
        let stores = state.kv_stores.read().await;
        stores.get(&store_id).cloned()
    };
    let Some(handle) = handle else {
        tracing::debug!(
            store_id = %store_id,
            sender = %hex::encode(sender.as_bytes()),
            "ignoring direct kv-store delta for unjoined store"
        );
        return;
    };
    if let Err(e) = handle
        .apply_remote_delta(delta_msg.peer_id, &delta_msg.delta, Some(sender))
        .await
    {
        tracing::warn!(
            store_id = %store_id,
            "failed to apply direct kv-store delta: {e}"
        );
    }
}

/// Request body for POST /stores.
///
/// `policy` selects the access policy: `"signed"` (default — owner-only
/// writes), `"append_only"` (owner-only writes AND existing keys are
/// immutable, even to the owner), or `"self_keyed"` (owner-free open
/// directory: any joiner writes only keys prefixed by its own AgentId).
#[derive(Debug, Deserialize)]
pub(in crate::server) struct CreateStoreRequest {
    name: String,
    topic: String,
    policy: Option<String>,
}

/// Request body for PUT /stores/:id/:key.
#[derive(Debug, Deserialize)]
pub(in crate::server) struct PutValueRequest {
    value: String,
    content_type: Option<String>,
}

/// Request body for POST /stores/:id/join.
///
/// `expected_owner` is the optional hex-encoded AgentId of the authoritative
/// owner, supplied out-of-band (the local user/operator is the trust root).
/// Omitting it yields a permanently read-only replica (no permissive
/// fallback) — EXCEPT under `policy: "self_keyed"`, the owner-free directory
/// policy, which requires joining WITHOUT an owner.
#[derive(Debug, Default, Deserialize)]
pub(in crate::server) struct JoinStoreRequest {
    expected_owner: Option<String>,
    /// Optional policy discriminator for the join. `"self_keyed"` selects
    /// the owner-free directory join (no `expected_owner` allowed); any
    /// other value is ignored in favor of the owner-anchored path.
    policy: Option<String>,
}

/// Response entry for GET /stores.
#[derive(Debug, Serialize)]
pub(in crate::server) struct StoreListEntry {
    id: String,
    topic: String,
    /// Hex-encoded anchored owner, or `null` for a read-only no-anchor store.
    owner: Option<String>,
    /// Access policy string.
    policy: String,
    /// Store version.
    version: u64,
    /// Owner-announce policy freshness counter.
    policy_version: u64,
    /// Strongly-typed ownership discriminant.
    ownership_status: x0x::kv::OwnershipStatus,
    /// True while snapshot persistence is failing (local writes refused
    /// until a snapshot succeeds).
    durability_degraded: bool,
}

/// GET /stores
pub(in crate::server) async fn list_kv_stores(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    // Snapshot (id, handle) pairs without holding the read lock across the
    // per-store ownership_info() awaits.
    let pairs: Vec<(String, x0x::KvStoreHandle)> = {
        let stores = state.kv_stores.read().await;
        stores
            .iter()
            .map(|(id, h)| (id.clone(), h.clone()))
            .collect()
    };
    let mut entries = Vec::with_capacity(pairs.len());
    for (id, handle) in pairs {
        let info = handle.ownership_info().await;
        entries.push(StoreListEntry {
            topic: id.clone(),
            id,
            owner: info.owner,
            policy: info.policy,
            version: info.version,
            policy_version: info.policy_version,
            ownership_status: info.ownership_status,
            durability_degraded: info.durability_degraded,
        });
    }
    Json(serde_json::json!({ "ok": true, "stores": entries }))
}

/// POST /stores
pub(in crate::server) async fn create_kv_store(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateStoreRequest>,
) -> impl IntoResponse {
    let id = req.topic.clone();
    // Resolve the requested access policy before any state is reserved.
    let policy = match req.policy.as_deref() {
        None | Some("signed") => x0x::kv::AccessPolicy::Signed,
        Some("append_only") => x0x::kv::AccessPolicy::AppendOnly,
        Some("self_keyed") => x0x::kv::AccessPolicy::SelfKeyed,
        Some(other) => {
            return bad_request(format!(
            "unsupported policy {other:?}: expected \"signed\", \"append_only\", or \"self_keyed\""
        ))
        }
    };
    // Reserve the entire handle+manifest transaction for this (kind,id) so
    // a concurrent create/rehydrate for the same id cannot interleave handle
    // insertion with failure rollback, or spawn a duplicate listener.
    let reservation =
        crdt_subscriptions::handle_reservation(&state, crdt_subscriptions::KIND_KV_STORE, &id)
            .await;
    let _guard = reservation.lock().await;
    // Under the reservation: if a handle already exists (created by a prior
    // successful request or rehydration), return conflict rather than
    // overwriting it and leaking the existing sync listener.
    if state.kv_stores.read().await.contains_key(&id) {
        return api_error(StatusCode::CONFLICT, "store already exists");
    }
    let policy_str = policy.to_string();
    // A self_keyed directory is owner-free for life: no expected_owner is
    // recorded for it (I3/I4) — rehydrate derives everything from the topic.
    let is_self_keyed = matches!(policy, x0x::kv::AccessPolicy::SelfKeyed);
    match state
        .agent
        .create_kv_store_persistent(&req.name, &req.topic, policy, &state.kv_store_state_dir)
        .await
    {
        Ok(handle) => {
            let info = handle.ownership_info().await;
            state.kv_stores.write().await.insert(id.clone(), handle);
            // Persist the registration so it survives a daemon restart
            // (rehydrated after join_network — see crdt_subscriptions).
            // Record the owner so a restarted creator re-anchors on itself.
            let mut extra = serde_json::Map::new();
            if !is_self_keyed {
                let owner_hex = hex::encode(state.agent.agent_id().as_bytes());
                extra.insert(
                    "expected_owner".to_string(),
                    serde_json::Value::String(owner_hex),
                );
            }
            // Persist the policy so a restarted creator rehydrates with the
            // same policy (an append-only store must never come back Signed).
            extra.insert("policy".to_string(), serde_json::Value::String(policy_str));
            if let Err(e) = crdt_subscriptions::record(
                &state,
                crdt_subscriptions::CrdtSubscriptionEntry {
                    kind: crdt_subscriptions::KIND_KV_STORE.to_string(),
                    id: id.clone(),
                    name: req.name.clone(),
                    topic: req.topic.clone(),
                    role: crdt_subscriptions::ROLE_CREATED.to_string(),
                    extra,
                },
            )
            .await
            {
                // Durable write failed: roll back the live handle so success is
                // not acknowledged for an un-persisted registration, and STOP
                // its sync — the discarded handle's bootstrap requester is
                // infinite while unconverged (issue #238) and would otherwise
                // chatter until daemon shutdown.
                tracing::error!("failed to persist kv store registration {id}: {e}");
                if let Some(h) = state.kv_stores.write().await.remove(&id) {
                    h.cancel_sync();
                }
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to persist subscription registration: {e}"),
                );
            }
            let mut resp = serde_json::to_value(&info).unwrap_or_else(|_| serde_json::json!({}));
            if let Some(obj) = resp.as_object_mut() {
                obj.insert("ok".to_string(), serde_json::Value::Bool(true));
                obj.insert("id".to_string(), serde_json::Value::String(id));
            }
            (StatusCode::CREATED, Json(resp))
        }
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")),
    }
}

/// POST /stores/:id/join
pub(in crate::server) async fn join_kv_store(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    body: Option<Json<JoinStoreRequest>>,
) -> impl IntoResponse {
    let body = body.map(|Json(r)| r).unwrap_or_default();
    // The `self_keyed` directory policy is the one owner-free join: knowing
    // only the topic is enough (I4). An `expected_owner` anchor is not
    // merely unnecessary there — it is contradictory (the store has no owner
    // for life, I3), so supplying one is a 422 rather than a silent ignore.
    if body.policy.as_deref() == Some("self_keyed") {
        if body.expected_owner.is_some() {
            return api_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "owner_not_allowed: policy \"self_keyed\" stores have no owner — join without expected_owner",
            );
        }
        return join_self_keyed_store(state, id).await;
    }
    // The out-of-band owner anchor is REQUIRED for every owner-anchored
    // policy: a replica with no anchor can never accept policy-restricted
    // data, so an unanchored join is a dead replica, not a successful join.
    // The local user/operator is the trust root for this param.
    let owner: AgentId = match body.expected_owner {
        Some(hex_owner) => match parse_agent_id_hex(&hex_owner) {
            Ok(agent) => agent,
            Err(e) => return bad_request(format!("invalid expected_owner: {e}")),
        },
        None => {
            return api_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "owner_required: an expected_owner anchor is required to join a store",
            )
        }
    };
    // Reserve the entire handle+manifest transaction for this (kind,id) so
    // a concurrent join/rehydrate for the same id cannot interleave handle
    // insertion with failure rollback, or spawn a duplicate listener.
    let reservation =
        crdt_subscriptions::handle_reservation(&state, crdt_subscriptions::KIND_KV_STORE, &id)
            .await;
    let _guard = reservation.lock().await;
    // Under the reservation: if a handle already exists (created by a prior
    // successful request or rehydration), return conflict rather than
    // overwriting it and leaking the existing sync listener.
    if state.kv_stores.read().await.contains_key(&id) {
        return api_error(StatusCode::CONFLICT, "store already joined");
    }
    match state
        .agent
        .join_kv_store_persistent(
            &id,
            owner,
            x0x::kv::store::AnchorChannel::RestParam,
            &state.kv_store_state_dir,
        )
        .await
    {
        Ok(handle) => {
            let info = handle.ownership_info().await;
            state.kv_stores.write().await.insert(id.clone(), handle);
            // Persist the registration so it survives a daemon restart
            // (rehydrated after join_network — see crdt_subscriptions). The
            // join path only knows the topic, so it doubles as the name.
            // Record the anchor so rehydrate re-anchors on the same owner.
            let mut extra = serde_json::Map::new();
            extra.insert(
                "expected_owner".to_string(),
                serde_json::Value::String(hex::encode(owner.as_bytes())),
            );
            if let Err(e) = crdt_subscriptions::record(
                &state,
                crdt_subscriptions::CrdtSubscriptionEntry {
                    kind: crdt_subscriptions::KIND_KV_STORE.to_string(),
                    id: id.clone(),
                    name: id.clone(),
                    topic: id.clone(),
                    role: crdt_subscriptions::ROLE_JOINED.to_string(),
                    extra,
                },
            )
            .await
            {
                // Durable write failed: roll back the live handle so success is
                // not acknowledged for an un-persisted registration, and STOP
                // its sync — the discarded handle's bootstrap requester is
                // infinite while unconverged (issue #238) and would otherwise
                // chatter until daemon shutdown.
                tracing::error!("failed to persist kv store join {id}: {e}");
                if let Some(h) = state.kv_stores.write().await.remove(&id) {
                    h.cancel_sync();
                }
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to persist subscription registration: {e}"),
                );
            }
            let mut resp = serde_json::to_value(&info).unwrap_or_else(|_| serde_json::json!({}));
            if let Some(obj) = resp.as_object_mut() {
                obj.insert("ok".to_string(), serde_json::Value::Bool(true));
                obj.insert("id".to_string(), serde_json::Value::String(id));
            }
            (StatusCode::OK, Json(resp))
        }
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")),
    }
}

/// Owner-free join for `self_keyed` directory stores (issue #340).
///
/// Shared tail of `POST /stores/:id/join` for the `policy: "self_keyed"`
/// body: reserves the (kind,id), joins by topic alone, and persists a
/// manifest entry whose `extra` records the policy but deliberately OMITS
/// `expected_owner` (the store has none — rehydrate must not require one).
async fn join_self_keyed_store(
    state: Arc<AppState>,
    id: String,
) -> (StatusCode, Json<serde_json::Value>) {
    let reservation =
        crdt_subscriptions::handle_reservation(&state, crdt_subscriptions::KIND_KV_STORE, &id)
            .await;
    let _guard = reservation.lock().await;
    if state.kv_stores.read().await.contains_key(&id) {
        return api_error(StatusCode::CONFLICT, "store already joined");
    }
    match state
        .agent
        .join_self_keyed_kv_store_persistent(&id, &state.kv_store_state_dir)
        .await
    {
        Ok(handle) => {
            let info = handle.ownership_info().await;
            state.kv_stores.write().await.insert(id.clone(), handle);
            let mut extra = serde_json::Map::new();
            // No expected_owner: a self_keyed store is owner-free for life.
            extra.insert(
                "policy".to_string(),
                serde_json::Value::String("self_keyed".to_string()),
            );
            if let Err(e) = crdt_subscriptions::record(
                &state,
                crdt_subscriptions::CrdtSubscriptionEntry {
                    kind: crdt_subscriptions::KIND_KV_STORE.to_string(),
                    id: id.clone(),
                    name: id.clone(),
                    topic: id.clone(),
                    role: crdt_subscriptions::ROLE_JOINED.to_string(),
                    extra,
                },
            )
            .await
            {
                tracing::error!("failed to persist kv store join {id}: {e}");
                if let Some(h) = state.kv_stores.write().await.remove(&id) {
                    h.cancel_sync();
                }
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to persist subscription registration: {e}"),
                );
            }
            let mut resp = serde_json::to_value(&info).unwrap_or_else(|_| serde_json::json!({}));
            if let Some(obj) = resp.as_object_mut() {
                obj.insert("ok".to_string(), serde_json::Value::Bool(true));
                obj.insert("id".to_string(), serde_json::Value::String(id));
            }
            (StatusCode::OK, Json(resp))
        }
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")),
    }
}

/// GET /stores/:id/keys
pub(in crate::server) async fn list_kv_keys(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let handle = {
        let stores = state.kv_stores.read().await;
        let Some(handle) = stores.get(&id) else {
            return not_found("store not found");
        };
        handle.clone()
    };

    match handle.keys().await {
        Ok(entries) => {
            let keys: Vec<serde_json::Value> = entries
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "key": e.key,
                        "content_type": e.content_type,
                        "content_hash": e.content_hash,
                        "size": e.value.len(),
                        "updated_at": e.updated_at,
                    })
                })
                .collect();
            (
                StatusCode::OK,
                Json(serde_json::json!({ "ok": true, "keys": keys })),
            )
        }
        Err(e) if matches!(e, x0x::error::IdentityError::Unauthorized(_)) => {
            api_error(StatusCode::FORBIDDEN, format!("{e}"))
        }
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")),
    }
}

/// PUT /stores/:id/:key
pub(in crate::server) async fn put_kv_value(
    State(state): State<Arc<AppState>>,
    Path((id, key)): Path<(String, String)>,
    Json(req): Json<PutValueRequest>,
) -> impl IntoResponse {
    let handle = {
        let stores = state.kv_stores.read().await;
        let Some(handle) = stores.get(&id) else {
            return not_found("store not found");
        };
        handle.clone()
    };

    use base64::Engine;
    let value = match BASE64.decode(&req.value) {
        Ok(v) => v,
        Err(e) => {
            return bad_request(format!("invalid base64: {e}"));
        }
    };

    let content_type = req
        .content_type
        .unwrap_or_else(|| "application/octet-stream".to_string());

    match handle.put_with_delta(key, value, content_type).await {
        Ok(delta) => {
            // #341 Phase B: encrypted stores replicate ONLY via the sealed
            // gossip path — never ship the plaintext local delta over the
            // DM direct-delivery side channel.
            if !handle.is_encrypted().await && !handle.is_group_signed().await {
                let recipients = kv_store_delta_direct_recipients(&state).await;
                spawn_kv_store_delta_delivery(&state, recipients, &id, handle.peer_id(), &delta);
            }
            (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
        }
        Err(e) => {
            let status = if matches!(e, x0x::error::IdentityError::ImmutableKey(_)) {
                // AppendOnly store: the key already exists and existing keys
                // are immutable, even to the owner.
                StatusCode::CONFLICT
            } else if matches!(e, x0x::error::IdentityError::Unauthorized(_)) {
                // Local write rejected by the store's access policy — the
                // caller is not the owner (or an allowlisted writer), or the
                // joined replica has not yet learned the authoritative owner.
                StatusCode::FORBIDDEN
            } else if format!("{e}").contains("value too large") {
                StatusCode::PAYLOAD_TOO_LARGE
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(serde_json::json!({ "ok": false, "error": format!("{e}") })),
            )
        }
    }
}

/// GET /stores/:id/:key
pub(in crate::server) async fn get_kv_value(
    State(state): State<Arc<AppState>>,
    Path((id, key)): Path<(String, String)>,
) -> impl IntoResponse {
    let handle = {
        let stores = state.kv_stores.read().await;
        let Some(handle) = stores.get(&id) else {
            return not_found("store not found");
        };
        handle.clone()
    };

    match handle.get(&key).await {
        Ok(Some(entry)) => {
            use base64::Engine;
            let value_b64 = BASE64.encode(&entry.value);
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "ok": true,
                    "key": entry.key,
                    "value": value_b64,
                    "content_type": entry.content_type,
                    "content_hash": entry.content_hash,
                    "metadata": entry.metadata,
                    "created_at": entry.created_at,
                    "updated_at": entry.updated_at,
                })),
            )
        }
        Ok(None) => not_found("key not found"),
        Err(e) if matches!(e, x0x::error::IdentityError::Unauthorized(_)) => {
            api_error(StatusCode::FORBIDDEN, format!("{e}"))
        }
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")),
    }
}

/// DELETE /stores/:id/:key
pub(in crate::server) async fn delete_kv_value(
    State(state): State<Arc<AppState>>,
    Path((id, key)): Path<(String, String)>,
) -> impl IntoResponse {
    let handle = {
        let stores = state.kv_stores.read().await;
        let Some(handle) = stores.get(&id) else {
            return not_found("store not found");
        };
        handle.clone()
    };

    match handle.remove_with_delta(&key).await {
        Ok(delta) => {
            // #341 Phase B: see put_kv_value — no plaintext DM fallback for
            // encrypted stores.
            if !handle.is_encrypted().await && !handle.is_group_signed().await {
                let recipients = kv_store_delta_direct_recipients(&state).await;
                spawn_kv_store_delta_delivery(&state, recipients, &id, handle.peer_id(), &delta);
            }
            (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
        }
        Err(e) if matches!(e, x0x::error::IdentityError::ImmutableKey(_)) => {
            // AppendOnly store: keys can never be deleted, even by the owner.
            api_error(StatusCode::CONFLICT, format!("{e}"))
        }
        Err(e) if matches!(e, x0x::error::IdentityError::Unauthorized(_)) => {
            api_error(StatusCode::FORBIDDEN, format!("{e}"))
        }
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")),
    }
}

// ---------------------------------------------------------------------------
// Group-scoped encrypted stores (#341 Phase B)
// ---------------------------------------------------------------------------

/// Request body for `POST /groups/:id/stores`.
#[derive(Debug, Deserialize)]
pub(in crate::server) struct CreateGroupStoreRequest {
    name: String,
}

type GroupStoreResponse = (StatusCode, Json<serde_json::Value>);

/// Creation-fixed identity; app names retain the existing trim-only semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
struct GssGroupStoreBinding {
    group_key: String,
    stable_group_id: String,
    creator: AgentId,
    name: String,
    store_id: x0x::kv::KvStoreId,
    topic: String,
}

/// Live TreeKEM adapter for one deterministic group store.
///
/// The group mutex covers crypto and the durable snapshot write. A failed
/// write restores the pre-operation ratchet before releasing the mutex, so a
/// record is never acknowledged or published from state that only existed in
/// memory.
struct TreeKemGroupStoreProtector {
    state: Arc<AppState>,
    group_key: String,
    stable_group_id: String,
    authorization: Arc<x0x::groups::TreeKemKvAuthorizationContext>,
    invalid: std::sync::atomic::AtomicBool,
}

impl TreeKemGroupStoreProtector {
    fn new(
        state: &Arc<AppState>,
        binding: &GssGroupStoreBinding,
        authorization: Arc<x0x::groups::TreeKemKvAuthorizationContext>,
    ) -> Self {
        Self {
            state: Arc::clone(state),
            group_key: binding.group_key.clone(),
            stable_group_id: binding.stable_group_id.clone(),
            authorization,
            invalid: std::sync::atomic::AtomicBool::new(false),
        }
    }

    async fn current_info(&self) -> x0x::kv::Result<x0x::groups::GroupInfo> {
        if self.invalid.load(std::sync::atomic::Ordering::Acquire) {
            return Err(x0x::kv::KvError::Unauthorized(
                "TreeKEM group store is retired".to_string(),
            ));
        }
        let groups = self.state.named_groups.read().await;
        let info = groups.get(&self.group_key).cloned().ok_or_else(|| {
            x0x::kv::KvError::Unauthorized("TreeKEM group is unavailable".to_string())
        })?;
        if info.withdrawn
            || info.is_fork_quarantined()
            || info.stable_group_id() != self.stable_group_id
            || info.policy.confidentiality != x0x::groups::GroupConfidentiality::MlsEncrypted
            || info.secure_plane != x0x::mls::SecureGroupPlane::TreeKem
        {
            return Err(x0x::kv::KvError::Unauthorized(
                "TreeKEM group binding is no longer eligible".to_string(),
            ));
        }
        self.authorization.update_from_group(&info);
        Ok(info)
    }

    fn permits(info: &x0x::groups::GroupInfo, agent: &AgentId, writer: bool) -> bool {
        let Some(member) = info.members_v2.get(&hex::encode(agent.as_bytes())) else {
            return false;
        };
        if !member.is_active() {
            return false;
        }
        if !writer {
            return true;
        }
        match info.policy.write_access {
            x0x::groups::GroupWriteAccess::MembersOnly => true,
            x0x::groups::GroupWriteAccess::AdminOnly => {
                member.role.at_least(x0x::groups::GroupRole::Admin)
            }
            x0x::groups::GroupWriteAccess::ModeratedPublic => false,
        }
    }

    fn authorization_binding(info: &x0x::groups::GroupInfo) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"x0x.kv.treekem-roster-policy.v1");
        hasher.update(info.stable_group_id().as_bytes());
        hasher.update(&info.state_revision.to_le_bytes());
        hasher.update(x0x::groups::compute_roster_root(&info.members_v2).as_bytes());
        if let Some(binding) = info.security_binding.as_deref() {
            hasher.update(binding.as_bytes());
        }
        hasher.update(&[match info.policy.read_access {
            x0x::groups::GroupReadAccess::Public => 0,
            x0x::groups::GroupReadAccess::MembersOnly => 1,
        }]);
        hasher.update(&[match info.policy.write_access {
            x0x::groups::GroupWriteAccess::MembersOnly => 0,
            x0x::groups::GroupWriteAccess::ModeratedPublic => 1,
            x0x::groups::GroupWriteAccess::AdminOnly => 2,
        }]);
        *hasher.finalize().as_bytes()
    }

    async fn live_group(
        &self,
    ) -> x0x::kv::Result<Arc<tokio::sync::Mutex<x0x::mls::TreeKemMlsGroup>>> {
        self.state
            .treekem_groups
            .read()
            .await
            .get(&self.group_key)
            .cloned()
            .ok_or_else(|| {
                x0x::kv::KvError::SecureRecord("live TreeKEM ratchet is unavailable".to_string())
            })
    }

    fn map_crypto_error(error: impl std::fmt::Display) -> x0x::kv::KvError {
        x0x::kv::KvError::SecureRecord(format!("TreeKEM group-store crypto failed: {error}"))
    }

    async fn rollback(
        &self,
        info: &x0x::groups::GroupInfo,
        snapshot: &[u8],
        group: &mut x0x::mls::TreeKemMlsGroup,
    ) {
        match super::named_groups::restore_local_treekem_group_from_snapshot(
            &self.state,
            info,
            snapshot,
        ) {
            Ok(restored) => *group = restored,
            Err(error) => {
                self.invalid
                    .store(true, std::sync::atomic::Ordering::Release);
                tracing::error!("failed to rollback TreeKEM store ratchet: {error}");
            }
        }
    }
}

#[async_trait::async_trait]
impl x0x::kv::TreeKemKvProtector for TreeKemGroupStoreProtector {
    fn group_id(&self) -> Vec<u8> {
        self.stable_group_id.as_bytes().to_vec()
    }

    async fn seal_record(
        &self,
        signing: &x0x::kv::AuthorSigning,
        kind: x0x::kv::KvMutationKind,
        store_id: &x0x::kv::KvStoreId,
        payload: &[u8],
        reader_only: bool,
    ) -> x0x::kv::Result<x0x::kv::TreeKemKvStoreRecordV1> {
        let membership =
            super::named_groups::group_membership_lock(&self.state, &self.group_key).await;
        let _membership_guard = membership.lock().await;
        let live = self.live_group().await?;
        let mut group = live.lock().await;
        // Re-read authority only after acquiring the ratchet mutex. Membership
        // commits use the same mutex, so this snapshot cannot predate a commit
        // that won while this operation was waiting.
        let info = self.current_info().await?;
        let reader_admission = kind == x0x::kv::KvMutationKind::Control && reader_only;
        if !Self::permits(&info, &signing.agent_id, !reader_admission) {
            return Err(x0x::kv::KvError::Unauthorized(
                "TreeKEM mutation author is not currently authorized".to_string(),
            ));
        }
        let rollback = group.to_snapshot_bytes().map_err(Self::map_crypto_error)?;
        let epoch = group.epoch();
        let inner = x0x::kv::treekem::sign_inner_mutation(
            signing,
            kind,
            payload,
            x0x::kv::treekem::TreeKemInnerBinding {
                group_id: self.group_id(),
                epoch,
                store_id,
                authorization_binding: Self::authorization_binding(&info),
                reader_only,
            },
        )?;
        let ciphertext = match group.encrypt_message(&inner) {
            Ok(ciphertext) => ciphertext,
            Err(error) => {
                self.rollback(&info, &rollback, &mut group).await;
                return Err(Self::map_crypto_error(error));
            }
        };
        if let Err(error) = super::named_groups::persist_treekem_snapshot_bound(
            &self.state,
            &self.group_key,
            &group,
        )
        .await
        {
            self.rollback(&info, &rollback, &mut group).await;
            return Err(x0x::kv::KvError::Gossip(format!(
                "persist TreeKEM send ratchet: {error}"
            )));
        }
        Ok(x0x::kv::TreeKemKvStoreRecordV1 {
            version: 1,
            group_id: self.group_id(),
            store_id: *store_id.as_bytes(),
            epoch,
            reader_only,
            ciphertext,
        })
    }

    async fn open_record(
        &self,
        store_id: &x0x::kv::KvStoreId,
        record: &x0x::kv::TreeKemKvStoreRecordV1,
    ) -> x0x::kv::Result<x0x::kv::treekem::OpenedTreeKemKvRecord> {
        if record.version != 1
            || record.group_id != self.group_id()
            || record.store_id != *store_id.as_bytes()
        {
            return Err(x0x::kv::KvError::SecureRecord(
                "TreeKEM record binding mismatch".to_string(),
            ));
        }
        let membership =
            super::named_groups::group_membership_lock(&self.state, &self.group_key).await;
        let _membership_guard = membership.lock().await;
        let live = self.live_group().await?;
        let mut group = live.lock().await;
        let info = self.current_info().await?;
        if record.epoch != group.epoch() {
            return Err(x0x::kv::KvError::SecureRecord(
                "TreeKEM record epoch is stale or ahead".to_string(),
            ));
        }
        let rollback = group.to_snapshot_bytes().map_err(Self::map_crypto_error)?;
        let plaintext = match group.decrypt_message(&record.ciphertext) {
            Ok(plaintext) => plaintext,
            Err(error) => {
                self.rollback(&info, &rollback, &mut group).await;
                return Err(Self::map_crypto_error(error));
            }
        };
        let opened = match x0x::kv::treekem::open_inner_mutation(
            self.stable_group_id.as_bytes(),
            record.epoch,
            store_id,
            &plaintext,
            Self::authorization_binding(&info),
        ) {
            Ok(opened) => opened,
            Err(error) => {
                self.rollback(&info, &rollback, &mut group).await;
                return Err(error);
            }
        };
        if opened.reader_only != record.reader_only
            || opened.reader_only && opened.mutation.kind != x0x::kv::KvMutationKind::Control
            || !Self::permits(&info, &opened.mutation.author_id, !opened.reader_only)
        {
            self.rollback(&info, &rollback, &mut group).await;
            return Err(x0x::kv::KvError::Unauthorized(
                "TreeKEM mutation author is not currently authorized".to_string(),
            ));
        }
        if let Err(error) = super::named_groups::persist_treekem_snapshot_bound(
            &self.state,
            &self.group_key,
            &group,
        )
        .await
        {
            self.rollback(&info, &rollback, &mut group).await;
            return Err(x0x::kv::KvError::Gossip(format!(
                "persist TreeKEM receive ratchet: {error}"
            )));
        }
        Ok(opened)
    }

    async fn is_authorized_reader(&self, agent: &AgentId) -> bool {
        self.current_info()
            .await
            .is_ok_and(|info| Self::permits(&info, agent, false))
    }

    async fn is_authorized_writer(&self, agent: &AgentId) -> bool {
        self.current_info()
            .await
            .is_ok_and(|info| Self::permits(&info, agent, true))
    }

    async fn merge_main_record(
        &self,
        opened: x0x::kv::treekem::OpenedTreeKemKvRecord,
        sender_peer: saorsa_gossip_types::PeerId,
        store: &Arc<tokio::sync::RwLock<x0x::kv::KvStore>>,
        retained_image: Option<Vec<u8>>,
    ) -> x0x::kv::Result<()> {
        if opened.reader_only || opened.mutation.kind == x0x::kv::KvMutationKind::Control {
            return Err(x0x::kv::KvError::Unauthorized(
                "read-side TreeKEM record cannot mutate a store".to_string(),
            ));
        }
        let membership =
            super::named_groups::group_membership_lock(&self.state, &self.group_key).await;
        let _membership_guard = membership.lock().await;
        let info = self.current_info().await?;
        if opened.authorization_binding != Self::authorization_binding(&info)
            || !Self::permits(&info, &opened.mutation.author_id, true)
        {
            return Err(x0x::kv::KvError::Unauthorized(
                "TreeKEM record authority changed before merge".to_string(),
            ));
        }
        let mut target = store.write().await;
        match opened.mutation.kind {
            x0x::kv::KvMutationKind::Delta | x0x::kv::KvMutationKind::FullState => {
                let delta: x0x::kv::KvStoreDelta =
                    bincode::deserialize(&opened.mutation.payload)
                        .map_err(|e| x0x::kv::KvError::Gossip(format!("bad TreeKEM delta: {e}")))?;
                target.merge_delta(&delta, sender_peer, Some(&opened.mutation.author_id))
            }
            x0x::kv::KvMutationKind::RetainedState => {
                let image: x0x::kv::KvStore =
                    bincode::deserialize(retained_image.as_deref().ok_or_else(|| {
                        x0x::kv::KvError::Gossip(
                            "complete TreeKEM retained image required".to_string(),
                        )
                    })?)
                    .map_err(|e| x0x::kv::KvError::Gossip(format!("bad retained image: {e}")))?;
                target.merge_group_retained_image(&image, opened.mutation.author_id)
            }
            x0x::kv::KvMutationKind::Control => Err(x0x::kv::KvError::Unauthorized(
                "TreeKEM control record on main topic".to_string(),
            )),
        }
    }

    fn invalidate(&self) {
        self.invalid
            .store(true, std::sync::atomic::Ordering::Release);
        self.authorization.invalidate();
    }
}

fn find_store_group<'a>(
    groups: &'a std::collections::HashMap<String, x0x::groups::GroupInfo>,
    id: &str,
) -> Result<(&'a String, &'a x0x::groups::GroupInfo), GroupStoreResponse> {
    if let Some(pair) = groups.get_key_value(id) {
        return Ok(pair);
    }
    let mut matches = groups
        .iter()
        .filter(|(_, info)| info.stable_group_id() == id);
    let pair = matches.next().ok_or_else(|| not_found("group not found"))?;
    if matches.next().is_some() {
        return Err(api_error(
            StatusCode::CONFLICT,
            "ambiguous local group binding",
        ));
    }
    Ok(pair)
}

fn validate_gss_store_group(
    info: &x0x::groups::GroupInfo,
    caller: &AgentId,
) -> Result<(), GroupStoreResponse> {
    if info.withdrawn {
        return Err(api_error(StatusCode::CONFLICT, "group is withdrawn"));
    }
    if !info.has_active_member(&hex::encode(caller.as_bytes())) {
        return Err(forbidden("not a member"));
    }
    if info.policy.confidentiality != x0x::groups::GroupConfidentiality::MlsEncrypted {
        return Err(bad_request(
            "encrypted stores require an MlsEncrypted group",
        ));
    }
    if info.secure_plane != x0x::mls::SecureGroupPlane::Gss {
        return Err(bad_request(
            "encrypted stores v1 are GSS-backed; other planes are not supported yet",
        ));
    }
    if info.shared_secret.is_none() {
        return Err(api_error(
            StatusCode::CONFLICT,
            "local daemon holds no shared secret for this group yet",
        ));
    }
    Ok(())
}

fn resolve_gss_group_store(
    groups: &std::collections::HashMap<String, x0x::groups::GroupInfo>,
    id: &str,
    name: &str,
    caller: &AgentId,
) -> Result<GssGroupStoreBinding, GroupStoreResponse> {
    let name = name.trim();
    if name.is_empty() {
        return Err(bad_request("store name must not be empty"));
    }
    let (group_key, info) = find_store_group(groups, id)?;
    validate_gss_store_group(info, caller)?;
    let stable_group_id = info.stable_group_id().to_string();
    let (store_id, topic) = x0x::kv::encrypted::group_store_identity(&stable_group_id, name);
    Ok(GssGroupStoreBinding {
        group_key: group_key.clone(),
        stable_group_id,
        creator: info.creator,
        name: name.to_string(),
        store_id,
        topic,
    })
}

fn resolve_treekem_group_store(
    groups: &std::collections::HashMap<String, x0x::groups::GroupInfo>,
    id: &str,
    name: &str,
    caller: &AgentId,
) -> Result<GssGroupStoreBinding, GroupStoreResponse> {
    let name = name.trim();
    if name.is_empty() {
        return Err(bad_request("store name must not be empty"));
    }
    let (group_key, info) = find_store_group(groups, id)?;
    if info.withdrawn || info.is_fork_quarantined() {
        return Err(api_error(StatusCode::CONFLICT, "group is unavailable"));
    }
    if !info.has_active_member(&hex::encode(caller.as_bytes())) {
        return Err(forbidden("not a member"));
    }
    if info.policy.confidentiality != x0x::groups::GroupConfidentiality::MlsEncrypted
        || info.secure_plane != x0x::mls::SecureGroupPlane::TreeKem
    {
        return Err(bad_request("store requires a real-TreeKEM encrypted group"));
    }
    let stable_group_id = info.stable_group_id().to_string();
    let (store_id, topic) = x0x::kv::encrypted::group_store_identity(&stable_group_id, name);
    Ok(GssGroupStoreBinding {
        group_key: group_key.clone(),
        stable_group_id,
        creator: info.creator,
        name: name.to_string(),
        store_id,
        topic,
    })
}

fn resolve_public_group_store(
    groups: &std::collections::HashMap<String, x0x::groups::GroupInfo>,
    id: &str,
    name: &str,
    caller: &AgentId,
) -> Result<GssGroupStoreBinding, GroupStoreResponse> {
    let name = name.trim();
    if name.is_empty() {
        return Err(bad_request("store name must not be empty"));
    }
    let (group_key, info) = find_store_group(groups, id)?;
    if info.withdrawn {
        return Err(api_error(StatusCode::CONFLICT, "group is withdrawn"));
    }
    if info.policy.confidentiality != x0x::groups::GroupConfidentiality::SignedPublic {
        return Err(bad_request("public stores require a SignedPublic group"));
    }
    if info.policy.write_access == x0x::groups::GroupWriteAccess::ModeratedPublic {
        return Err(bad_request(
            "ModeratedPublic group stores are unsupported without a moderation protocol",
        ));
    }
    if info.policy.read_access == x0x::groups::GroupReadAccess::MembersOnly
        && !info.has_active_member(&hex::encode(caller.as_bytes()))
    {
        return Err(forbidden(
            "public group store is restricted to current members",
        ));
    }
    let stable_group_id = info.stable_group_id().to_string();
    let (store_id, topic) = x0x::kv::encrypted::group_store_identity(&stable_group_id, name);
    Ok(GssGroupStoreBinding {
        group_key: group_key.clone(),
        stable_group_id,
        creator: info.creator,
        name: name.to_string(),
        store_id,
        topic,
    })
}

fn refresh_public_store_binding(
    ctx: &x0x::groups::PublicGroupKvContext,
    info: Option<&x0x::groups::GroupInfo>,
    creator: AgentId,
    caller: &AgentId,
) -> bool {
    if let Some(info) = info {
        if info.creator == creator
            && info.stable_group_id().as_bytes() == ctx.group_id()
            && !info.withdrawn
            && info.policy.confidentiality == x0x::groups::GroupConfidentiality::SignedPublic
            && info.policy.write_access != x0x::groups::GroupWriteAccess::ModeratedPublic
            && (info.policy.read_access == x0x::groups::GroupReadAccess::Public
                || info.has_active_member(&hex::encode(caller.as_bytes())))
        {
            ctx.update_from_group(info);
            return true;
        }
    }
    ctx.invalidate();
    false
}

fn public_kv_refresh(
    state: &Arc<AppState>,
    ctx: Arc<x0x::groups::PublicGroupKvContext>,
    group_key: String,
    topic: String,
    creator: AgentId,
) -> x0x::kv::sync::SecureRefreshFn {
    let state = Arc::clone(state);
    Arc::new(move || {
        let ctx = Arc::clone(&ctx);
        let state = Arc::clone(&state);
        let group_key = group_key.clone();
        let topic = topic.clone();
        Box::pin(async move {
            let valid = {
                let groups = state.named_groups.read().await;
                refresh_public_store_binding(
                    &ctx,
                    groups.get(&group_key),
                    creator,
                    &state.agent.agent_id(),
                )
            };
            if !valid {
                tracing::warn!(target: "x0x::kv", "retiring public group store {topic}: group binding is no longer eligible");
                if let Some(handle) = state.kv_stores.write().await.remove(&topic) {
                    handle.retire();
                }
            }
        }) as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
    })
}

/// Used by the real per-record refresh hook; invalidation also fences clones.
fn refresh_gss_store_binding(
    ctx: &x0x::groups::GssKvSecureContext,
    info: Option<&x0x::groups::GroupInfo>,
    creator: AgentId,
    caller: &AgentId,
) -> bool {
    if let Some(info) = info {
        if info.creator == creator
            && info.stable_group_id().as_bytes() == ctx.group_id()
            && validate_gss_store_group(info, caller).is_ok()
        {
            ctx.update_from_group(info);
            return true;
        }
    }
    ctx.invalidate();
    false
}

/// Deterministic refresh hook for a GSS encrypted-store context: re-reads
/// the authoritative group from the daemon's named-groups map (under the
/// read guard — no `GroupInfo` clone) and refreshes the context snapshot.
/// The sync loops call this before every seal/open, so a rekey or roster
/// change takes effect on the very next record.
pub(in crate::server) fn gss_kv_refresh(
    state: &Arc<AppState>,
    ctx: Arc<x0x::groups::GssKvSecureContext>,
    group_key: String,
    topic: String,
    creator: AgentId,
) -> x0x::kv::sync::SecureRefreshFn {
    let state = Arc::clone(state);
    Arc::new(move || {
        let ctx = Arc::clone(&ctx);
        let state = Arc::clone(&state);
        let group_key = group_key.clone();
        let topic = topic.clone();
        Box::pin(async move {
            let valid = {
                let groups = state.named_groups.read().await;
                refresh_gss_store_binding(
                    &ctx,
                    groups.get(&group_key),
                    creator,
                    &state.agent.agent_id(),
                )
            };
            if !valid {
                tracing::warn!(target: "x0x::kv", "retiring encrypted store {topic}: group binding is no longer eligible");
                if let Some(h) = state.kv_stores.write().await.remove(&topic) {
                    h.retire();
                }
            }
        }) as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
    })
}

/// Retire EVERY encrypted store handle bound to `stable_group_id` (topic
/// prefix `x0x/group/<gid>/kv/`): invalidate its secure context (local
/// authorization fails closed immediately, even for handles cloned
/// elsewhere) and cancel its sync loops.
///
/// Called from the group-lifecycle paths — leave, group deletion, and
/// state withdrawal — so a departed member cannot keep reading, writing,
/// or publishing old-epoch records through a live handle. The per-store
/// refresh hook ([`gss_kv_refresh`]) is the second layer: it invalidates
/// and retires on the next sync activity even if a lifecycle path is
/// missed.
pub(in crate::server) async fn retire_group_kv_stores(state: &AppState, stable_group_id: &str) {
    let prefix = format!("x0x/group/{stable_group_id}/kv/");
    let mut stores = state.kv_stores.write().await;
    let doomed: Vec<String> = stores
        .keys()
        .filter(|topic| topic.starts_with(&prefix))
        .cloned()
        .collect();
    for topic in &doomed {
        if let Some(h) = stores.remove(topic) {
            tracing::info!(
                target: "x0x::kv",
                "retiring encrypted store {topic}: group {stable_group_id} left/removed/withdrawn"
            );
            h.retire();
        }
    }
}

/// Called only while the canonical store reservation and group membership
/// guard are held. Re-resolve before touching a cached handle or starting sync.
async fn open_bound_gss_store(
    state: &Arc<AppState>,
    expected: &GssGroupStoreBinding,
) -> Result<
    (
        x0x::KvStoreHandle,
        Arc<x0x::groups::GssKvSecureContext>,
        bool,
    ),
    GroupStoreResponse,
> {
    let secure = {
        let groups = state.named_groups.read().await;
        let current = resolve_gss_group_store(
            &groups,
            &expected.group_key,
            &expected.name,
            &state.agent.agent_id(),
        )?;
        if &current != expected {
            return Err(api_error(
                StatusCode::CONFLICT,
                "group store binding changed during open",
            ));
        }
        let info = groups
            .get(&current.group_key)
            .ok_or_else(|| not_found("group not found"))?;
        Arc::new(
            x0x::groups::GssKvSecureContext::from_group(info)
                .ok_or_else(|| api_error(StatusCode::CONFLICT, "group secret unavailable"))?,
        )
    };
    let cached = { state.kv_stores.read().await.get(&expected.topic).cloned() };
    if let Some(handle) = cached {
        if handle
            .validate_group_binding(
                &expected.name,
                &expected.stable_group_id,
                expected.creator,
                x0x::GroupStoreProtection::Encrypted,
            )
            .await
            .is_err()
        {
            handle.retire();
            state.kv_stores.write().await.remove(&expected.topic);
            return Err(api_error(
                StatusCode::CONFLICT,
                "cached group store binding mismatch or retired context",
            ));
        }
        return Ok((handle, secure, false));
    }
    let refresh = gss_kv_refresh(
        state,
        Arc::clone(&secure),
        expected.group_key.clone(),
        expected.topic.clone(),
        expected.creator,
    );
    let handle = state
        .agent
        .open_group_kv_store_persistent(
            &expected.name,
            &expected.stable_group_id,
            expected.creator,
            Arc::clone(&secure) as Arc<dyn KvSecureContext>,
            refresh,
            &state.kv_store_state_dir,
        )
        .await
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;
    Ok((handle, secure, true))
}

async fn open_bound_treekem_store(
    state: &Arc<AppState>,
    expected: &GssGroupStoreBinding,
) -> Result<(x0x::KvStoreHandle, u64, bool), GroupStoreResponse> {
    let authorization = {
        let groups = state.named_groups.read().await;
        let current = resolve_treekem_group_store(
            &groups,
            &expected.group_key,
            &expected.name,
            &state.agent.agent_id(),
        )?;
        if &current != expected {
            return Err(api_error(
                StatusCode::CONFLICT,
                "TreeKEM group store binding changed during open",
            ));
        }
        let info = groups
            .get(&current.group_key)
            .ok_or_else(|| not_found("group not found"))?;
        let authorization = x0x::groups::TreeKemKvAuthorizationContext::from_group(info)
            .ok_or_else(|| api_error(StatusCode::CONFLICT, "TreeKEM group unavailable"))?;
        Arc::new(authorization)
    };
    let live = state
        .treekem_groups
        .read()
        .await
        .get(&expected.group_key)
        .cloned()
        .ok_or_else(|| api_error(StatusCode::CONFLICT, "TreeKEM ratchet unavailable"))?;
    let epoch = live.lock().await.epoch();
    let cached = { state.kv_stores.read().await.get(&expected.topic).cloned() };
    if let Some(handle) = cached {
        if handle
            .validate_group_binding(
                &expected.name,
                &expected.stable_group_id,
                expected.creator,
                x0x::GroupStoreProtection::TreeKemEncrypted,
            )
            .await
            .is_ok()
        {
            return Ok((handle, epoch, false));
        }
        handle.retire();
        state.kv_stores.write().await.remove(&expected.topic);
        return Err(api_error(
            StatusCode::CONFLICT,
            "cached TreeKEM group store binding mismatch",
        ));
    }
    let protector: x0x::kv::SharedTreeKemKvProtector = Arc::new(TreeKemGroupStoreProtector::new(
        state,
        expected,
        Arc::clone(&authorization),
    ));
    let handle = state
        .agent
        .open_treekem_group_kv_store_persistent(
            &expected.name,
            &expected.stable_group_id,
            expected.creator,
            authorization,
            protector,
            &state.kv_store_state_dir,
        )
        .await
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;
    Ok((handle, epoch, true))
}

async fn open_bound_public_store(
    state: &Arc<AppState>,
    expected: &GssGroupStoreBinding,
) -> Result<
    (
        x0x::KvStoreHandle,
        Arc<x0x::groups::PublicGroupKvContext>,
        bool,
    ),
    GroupStoreResponse,
> {
    let context = {
        let groups = state.named_groups.read().await;
        let current = resolve_public_group_store(
            &groups,
            &expected.group_key,
            &expected.name,
            &state.agent.agent_id(),
        )?;
        if &current != expected {
            return Err(api_error(
                StatusCode::CONFLICT,
                "public group store binding changed during open",
            ));
        }
        let info = groups
            .get(&current.group_key)
            .ok_or_else(|| not_found("group not found"))?;
        Arc::new(
            x0x::groups::PublicGroupKvContext::from_group(info)
                .ok_or_else(|| bad_request("group is not SignedPublic"))?,
        )
    };
    // Drop the map read guard before any mismatch cleanup takes the write
    // guard. In edition 2021 an `if let` scrutinee temporary otherwise lives
    // through the whole arm and self-deadlocks on `write().await` below.
    let cached = {
        let stores = state.kv_stores.read().await;
        stores.get(&expected.topic).cloned()
    };
    if let Some(handle) = cached {
        if handle
            .validate_group_binding(
                &expected.name,
                &expected.stable_group_id,
                expected.creator,
                x0x::GroupStoreProtection::PublicSigned,
            )
            .await
            .is_ok()
        {
            return Ok((handle, context, false));
        }
        handle.retire();
        state.kv_stores.write().await.remove(&expected.topic);
        return Err(api_error(
            StatusCode::CONFLICT,
            "cached public group store binding mismatch",
        ));
    }
    let refresh = public_kv_refresh(
        state,
        Arc::clone(&context),
        expected.group_key.clone(),
        expected.topic.clone(),
        expected.creator,
    );
    let handle = state
        .agent
        .open_public_group_kv_store_persistent(
            &expected.name,
            &expected.stable_group_id,
            expected.creator,
            Arc::clone(&context) as Arc<dyn KvSecureContext>,
            refresh,
            &state.kv_store_state_dir,
        )
        .await
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;
    Ok((handle, context, true))
}

/// Shared metadata payload for create / idempotent re-open responses.
async fn group_store_json(
    handle: &x0x::KvStoreHandle,
    topic: &str,
    store_id: &x0x::kv::KvStoreId,
    stable_group_id: &str,
    epoch: u64,
    policy: &str,
) -> serde_json::Value {
    let ownership = handle.ownership_info().await;
    serde_json::json!({
        "ok": true,
        "id": topic,
        "store_id": hex::encode(store_id.as_bytes()),
        "group_id": stable_group_id,
        "topic": topic,
        "policy": policy,
        "epoch": epoch,
        "checkpoint_available": handle.has_checkpoint().await,
        "ownership": ownership,
    })
}

/// `POST /groups/:id/stores` — open (create or re-open) a group-scoped
/// ENCRYPTED KvStore bound to the named group (#341 Phase B, design:
/// `docs/design/encrypted-kvstore.md`).
///
/// Store identity is deterministic from `(stable group id, name)`, so every
/// member computes the same store id and topic with no out-of-band anchor;
/// ownership is anchored on the GROUP CREATOR. Every publication is
/// sign-then-encrypt sealed under the group's current secret epoch and the
/// v1 write rule is active group membership.
///
/// Guards: caller must be an active member, the group must be
/// `MlsEncrypted` on the GSS plane (the v1 backend, ADR-0010), and a rider
/// token must explicitly cover the group (ADR-0039 deny-by-default).
///
/// Idempotent: opening an already-open store returns 200 with its metadata
/// instead of a conflict.
pub(in crate::server) async fn create_group_kv_store(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Extension(actor): Extension<crate::server::rider_auth::ActorContext>,
    Json(req): Json<CreateGroupStoreRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    #[derive(Clone, Copy)]
    enum StorePlane {
        Gss,
        TreeKem,
        Public,
    }
    let (binding, plane) = {
        let groups = state.named_groups.read().await;
        let plane = match find_store_group(&groups, &id) {
            Ok((_, info))
                if info.policy.confidentiality
                    == x0x::groups::GroupConfidentiality::SignedPublic =>
            {
                StorePlane::Public
            }
            Ok((_, info)) if info.secure_plane == x0x::mls::SecureGroupPlane::TreeKem => {
                StorePlane::TreeKem
            }
            Ok(_) => StorePlane::Gss,
            Err(response) => return response,
        };
        let resolved = match plane {
            StorePlane::Public => {
                resolve_public_group_store(&groups, &id, &req.name, &state.agent.agent_id())
            }
            StorePlane::TreeKem => {
                resolve_treekem_group_store(&groups, &id, &req.name, &state.agent.agent_id())
            }
            StorePlane::Gss => {
                resolve_gss_group_store(&groups, &id, &req.name, &state.agent.agent_id())
            }
        };
        match resolved {
            Ok(binding) => (binding, plane),
            Err(response) => return response,
        }
    };
    if !actor.rider_allows_group(&binding.stable_group_id) {
        return forbidden("rider token is not granted this group");
    }
    let reservation = crdt_subscriptions::handle_reservation(
        &state,
        crdt_subscriptions::KIND_KV_STORE,
        &binding.topic,
    )
    .await;
    let _reservation_guard = reservation.lock().await;
    // Use the same alias-canonicalizing mutex as all group membership writers.
    let membership = super::named_groups::group_membership_lock(&state, &binding.group_key).await;
    let _membership_guard = membership.lock().await;
    let (handle, epoch, created) = match plane {
        StorePlane::Public => match open_bound_public_store(&state, &binding).await {
            Ok((handle, context, created)) => (handle, context.current_epoch(), created),
            Err(response) => return response,
        },
        StorePlane::TreeKem => match open_bound_treekem_store(&state, &binding).await {
            Ok(opened) => opened,
            Err(response) => return response,
        },
        StorePlane::Gss => match open_bound_gss_store(&state, &binding).await {
            Ok((handle, context, created)) => (handle, context.current_epoch(), created),
            Err(response) => return response,
        },
    };
    if created {
        state
            .kv_stores
            .write()
            .await
            .insert(binding.topic.clone(), handle.clone());
        let mut extra = serde_json::Map::new();
        extra.insert(
            "policy".into(),
            serde_json::Value::String(if matches!(plane, StorePlane::Public) {
                "group_signed".into()
            } else {
                "encrypted".into()
            }),
        );
        extra.insert(
            "expected_owner".into(),
            serde_json::Value::String(hex::encode(binding.creator.as_bytes())),
        );
        extra.insert(
            "stable_group_id".into(),
            serde_json::Value::String(binding.stable_group_id.clone()),
        );
        if !matches!(plane, StorePlane::Public) {
            extra.insert(
                "secure_plane".into(),
                serde_json::Value::String(match plane {
                    StorePlane::TreeKem => "treekem".into(),
                    StorePlane::Gss | StorePlane::Public => "gss".into(),
                }),
            );
        }
        if let Err(e) = crdt_subscriptions::record(
            &state,
            crdt_subscriptions::CrdtSubscriptionEntry {
                kind: crdt_subscriptions::KIND_KV_STORE.to_string(),
                id: binding.topic.clone(),
                name: binding.name.clone(),
                topic: binding.topic.clone(),
                role: crdt_subscriptions::ROLE_CREATED.to_string(),
                extra,
            },
        )
        .await
        {
            handle.retire();
            state.kv_stores.write().await.remove(&binding.topic);
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to persist subscription registration: {e}"),
            );
        }
    }
    (
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(
            group_store_json(
                &handle,
                &binding.topic,
                &binding.store_id,
                &binding.stable_group_id,
                epoch,
                if matches!(plane, StorePlane::Public) {
                    "group_signed"
                } else {
                    "encrypted"
                },
            )
            .await,
        ),
    )
}

/// Manifest binding must agree with current group authority, not supply it.
fn validate_gss_store_manifest(
    entry: &crdt_subscriptions::CrdtSubscriptionEntry,
    binding: &GssGroupStoreBinding,
    expected_policy: &str,
    expected_secure_plane: Option<&str>,
) -> Result<(), GroupStoreResponse> {
    let recorded_plane = entry.extra.get("secure_plane").and_then(|v| v.as_str());
    let plane_matches = match expected_secure_plane {
        Some("gss") => recorded_plane.is_none() || recorded_plane == Some("gss"),
        Some(expected) => recorded_plane == Some(expected),
        None => recorded_plane.is_none(),
    };
    if entry.id != binding.topic
        || entry.topic != binding.topic
        || entry.name != binding.name
        || entry.extra.get("stable_group_id").and_then(|v| v.as_str())
            != Some(binding.stable_group_id.as_str())
        || entry
            .extra
            .get("expected_owner")
            .and_then(|v| v.as_str())
            .and_then(|owner| parse_agent_id_hex(owner).ok())
            != Some(binding.creator)
        || entry.extra.get("policy").and_then(|v| v.as_str()) != Some(expected_policy)
        || !plane_matches
    {
        return Err(api_error(
            StatusCode::CONFLICT,
            "encrypted store manifest binding mismatch",
        ));
    }
    Ok(())
}

/// Encrypted restore's final decision. Caller holds the per-entry reservation;
/// canonical ID validation precedes cached lookup and prevents alternate keys
/// from evading that reservation. Membership stays serialized through install.
pub(in crate::server) async fn restore_bound_gss_store(
    state: &Arc<AppState>,
    entry: &crdt_subscriptions::CrdtSubscriptionEntry,
) -> Result<bool, GroupStoreResponse> {
    let stable = entry
        .extra
        .get("stable_group_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_request("encrypted store manifest has no stable group ID"))?;
    let public = entry.extra.get("policy").and_then(|v| v.as_str()) == Some("group_signed");
    let (binding, treekem) = {
        let groups = state.named_groups.read().await;
        if public {
            (
                resolve_public_group_store(&groups, stable, &entry.name, &state.agent.agent_id())?,
                false,
            )
        } else {
            let (_, info) = find_store_group(&groups, stable)?;
            if info.secure_plane == x0x::mls::SecureGroupPlane::TreeKem {
                (
                    resolve_treekem_group_store(
                        &groups,
                        stable,
                        &entry.name,
                        &state.agent.agent_id(),
                    )?,
                    true,
                )
            } else {
                (
                    resolve_gss_group_store(&groups, stable, &entry.name, &state.agent.agent_id())?,
                    false,
                )
            }
        }
    };
    validate_gss_store_manifest(
        entry,
        &binding,
        if public { "group_signed" } else { "encrypted" },
        if public {
            None
        } else if treekem {
            Some("treekem")
        } else {
            Some("gss")
        },
    )?;
    let membership = super::named_groups::group_membership_lock(state, &binding.group_key).await;
    let _membership_guard = membership.lock().await;
    let (handle, created) = if public {
        let (handle, _, created) = open_bound_public_store(state, &binding).await?;
        (handle, created)
    } else if treekem {
        let (handle, _, created) = open_bound_treekem_store(state, &binding).await?;
        (handle, created)
    } else {
        let (handle, _, created) = open_bound_gss_store(state, &binding).await?;
        (handle, created)
    };
    if created {
        state
            .kv_stores
            .write()
            .await
            .insert(binding.topic.clone(), handle);
    }
    Ok(created)
}

// ---------------------------------------------------------------------------
// Direct messaging handlers
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kv_store_delta_direct_payload_is_prefixed_json() {
        let peer_id = saorsa_gossip_types::PeerId::new([9; 32]);
        let delta = x0x::kv::KvStoreDelta::new(42);

        let payload = encode_kv_store_delta_direct_payload("store-1", peer_id, &delta)
            .expect("payload should encode");
        assert!(payload.starts_with(KV_STORE_DELTA_DM_PREFIX));

        let decoded: KvStoreDirectDelta =
            serde_json::from_slice(&payload[KV_STORE_DELTA_DM_PREFIX.len()..])
                .expect("payload JSON should decode");
        assert_eq!(decoded.store_id, "store-1");
        assert_eq!(decoded.peer_id, peer_id);
        assert_eq!(decoded.delta.version, delta.version);
    }

    // -- #341 Phase B: POST /groups/:id/stores ---------------------------------

    use crate::groups::{GroupConfidentiality, GroupInfo, GroupPolicy, GssKvSecureContext};
    use crate::mls::SecureGroupPlane;

    fn binding_fixture(id: &str) -> GroupInfo {
        let mut info = GroupInfo::with_policy(
            "group".into(),
            String::new(),
            AgentId([1; 32]),
            id.into(),
            GroupPolicy::default(),
        );
        info.policy.confidentiality = GroupConfidentiality::MlsEncrypted;
        info.secure_plane = SecureGroupPlane::Gss;
        info.add_member(
            hex::encode(AgentId([2; 32]).as_bytes()),
            x0x::groups::GroupRole::Member,
            None,
            None,
        );
        info.shared_secret = Some(vec![7; 32]);
        info
    }

    #[test]
    fn issue565_full_group_and_trim_only_application_identity() {
        let first = "abcdef0123456789aaaaaaaaaaaaaaaa";
        let second = "abcdef0123456789bbbbbbbbbbbbbbbb";
        let groups = std::collections::HashMap::from([
            ("alias".into(), binding_fixture(first)),
            (second.into(), binding_fixture(second)),
        ]);
        let owner =
            resolve_gss_group_store(&groups, "alias", "  Wiki  ", &AgentId([1; 32])).unwrap();
        let member = resolve_gss_group_store(&groups, first, "Wiki", &AgentId([2; 32])).unwrap();
        assert_eq!(
            owner, member,
            "creator and member resolve the same full identity through alias/stable ID"
        );
        assert_ne!(
            owner.store_id,
            resolve_gss_group_store(&groups, second, "Wiki", &AgentId([2; 32]))
                .unwrap()
                .store_id
        );
        assert_ne!(
            owner.store_id,
            resolve_gss_group_store(&groups, first, "wiki", &AgentId([2; 32]))
                .unwrap()
                .store_id
        );
        assert!(resolve_gss_group_store(&groups, first, "  ", &AgentId([2; 32])).is_err());
    }

    #[test]
    fn signed_public_store_resolver_enforces_current_read_axis() {
        let group_id = "13".repeat(16);
        let creator = AgentId([1; 32]);
        let outsider = AgentId([2; 32]);
        let mut info = GroupInfo::new(
            "public".to_string(),
            String::new(),
            creator,
            group_id.clone(),
        );
        info.migrate_from_v1();
        info.policy.confidentiality = GroupConfidentiality::SignedPublic;
        info.policy.read_access = crate::groups::GroupReadAccess::Public;
        let mut groups = std::collections::HashMap::from([(group_id.clone(), info)]);
        assert!(resolve_public_group_store(&groups, &group_id, "Wiki", &outsider).is_ok());

        let info = groups.get_mut(&group_id).expect("group");
        info.policy.read_access = crate::groups::GroupReadAccess::MembersOnly;
        assert!(resolve_public_group_store(&groups, &group_id, "Wiki", &outsider).is_err());
        groups.get_mut(&group_id).expect("group").add_member(
            hex::encode(outsider.as_bytes()),
            crate::groups::GroupRole::Member,
            Some(hex::encode(creator.as_bytes())),
            None,
        );
        assert!(resolve_public_group_store(&groups, &group_id, "Wiki", &outsider).is_ok());
    }

    #[test]
    fn issue565_resolver_and_refresh_reject_ineligible_binding_and_fence_clones() {
        let gid = "ab".repeat(16);
        let base = binding_fixture(&gid);
        for case in 0..10 {
            let mut info = base.clone();
            match case {
                0 => info.withdrawn = true,
                1 => {
                    info.remove_member(&hex::encode(AgentId([2; 32]).as_bytes()), None);
                }
                2 => info.policy.confidentiality = GroupConfidentiality::SignedPublic,
                3 => info.secure_plane = SecureGroupPlane::TreeKem,
                4 => info.shared_secret = None,
                5 => info.creator = AgentId([9; 32]),
                6 => info = binding_fixture(&"cd".repeat(16)),
                8 | 9 => {
                    info.members_v2
                        .get_mut(&hex::encode(AgentId([2; 32]).as_bytes()))
                        .unwrap()
                        .state = if case == 8 {
                        x0x::groups::GroupMemberState::Pending
                    } else {
                        x0x::groups::GroupMemberState::Banned
                    };
                }
                _ => {}
            }
            let ctx = GssKvSecureContext::from_group(&base).unwrap();
            let cloned = ctx.clone();
            let current = (case != 7).then_some(&info);
            assert!(
                !refresh_gss_store_binding(&ctx, current, base.creator, &AgentId([2; 32])),
                "case {case}"
            );
            assert!(
                !cloned.is_active_member(&AgentId([2; 32])),
                "clone fenced case {case}"
            );
            let id = x0x::kv::encrypted::group_store_identity(&gid, "Wiki").0;
            assert!(cloned.seal(&id, b"private").is_err());
            if !(5..8).contains(&case) {
                let groups = std::collections::HashMap::from([(gid.clone(), info)]);
                assert!(resolve_gss_group_store(&groups, &gid, "Wiki", &AgentId([2; 32])).is_err());
            }
        }
        let ctx = GssKvSecureContext::from_group(&base).unwrap();
        let mut advanced = base.clone();
        advanced.secret_epoch += 1;
        assert!(refresh_gss_store_binding(
            &ctx,
            Some(&advanced),
            base.creator,
            &AgentId([2; 32])
        ));
        assert_eq!(ctx.current_epoch(), advanced.secret_epoch);
        let groups = std::collections::HashMap::from([(gid.clone(), base)]);
        assert!(resolve_gss_group_store(&groups, &gid, "Wiki", &AgentId([3; 32])).is_err());
        assert!(resolve_gss_group_store(&groups, "missing", "Wiki", &AgentId([2; 32])).is_err());
    }

    #[test]
    fn issue565_restore_manifest_cannot_supply_identity_or_authority() {
        let gid = "ab".repeat(16);
        let groups = std::collections::HashMap::from([(gid.clone(), binding_fixture(&gid))]);
        let binding = resolve_gss_group_store(&groups, &gid, "Wiki", &AgentId([2; 32])).unwrap();
        let good = crdt_subscriptions::CrdtSubscriptionEntry {
            kind: crdt_subscriptions::KIND_KV_STORE.into(),
            id: binding.topic.clone(),
            topic: binding.topic.clone(),
            name: binding.name.clone(),
            role: crdt_subscriptions::ROLE_CREATED.into(),
            extra: serde_json::Map::from_iter([
                ("stable_group_id".into(), serde_json::Value::String(gid)),
                (
                    "expected_owner".into(),
                    serde_json::Value::String(hex::encode(binding.creator.as_bytes())),
                ),
                (
                    "policy".into(),
                    serde_json::Value::String("encrypted".into()),
                ),
            ]),
        };
        assert!(validate_gss_store_manifest(&good, &binding, "encrypted", Some("gss")).is_ok());
        let mut hex_binding = binding.clone();
        hex_binding.creator = AgentId([0xab; 32]);
        let mut upper_owner = good.clone();
        upper_owner.extra.insert(
            "expected_owner".into(),
            serde_json::Value::String("AB".repeat(32)),
        );
        assert!(
            validate_gss_store_manifest(&upper_owner, &hex_binding, "encrypted", Some("gss"))
                .is_ok(),
            "preserve parsed owner-ID spelling compatibility"
        );
        for case in 0..6 {
            let mut entry = good.clone();
            match case {
                0 => entry.id = "different-registry-key".into(),
                1 => entry.topic = "different-topic".into(),
                2 => {
                    entry.extra.insert(
                        "expected_owner".into(),
                        serde_json::Value::String(hex::encode(AgentId([9; 32]).as_bytes())),
                    );
                }
                3 => {
                    entry.extra.insert(
                        "stable_group_id".into(),
                        serde_json::Value::String("foreign".into()),
                    );
                }
                4 => {
                    entry
                        .extra
                        .insert("policy".into(), serde_json::Value::String("signed".into()));
                }
                _ => entry.name = " Wiki ".into(),
            }
            assert!(
                validate_gss_store_manifest(&entry, &binding, "encrypted", Some("gss")).is_err(),
                "case {case}"
            );
        }
    }

    /// Explicit test-only network config (#417/#337): loopback bind, no
    /// seeds, discovery/port-mapping off. Still a real socket constructor.
    fn test_network_config() -> x0x::network::NetworkConfig {
        x0x::network::NetworkConfig {
            bind_addr: Some("127.0.0.1:0".parse().expect("loopback addr literal")),
            bootstrap_nodes: Vec::new(),
            mdns_enabled: false,
            port_mapping_enabled: false,
            ..x0x::network::NetworkConfig::default()
        }
    }

    /// Agent + AppState over a temp dir, WITH an in-process gossip runtime
    /// (the encrypted-store happy path spawns real sync loops).
    async fn encrypted_store_test_state() -> (Arc<AppState>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().to_path_buf();
        let agent = Arc::new(
            x0x::Agent::builder()
                .with_identity_dir(&data_dir)
                .with_machine_key(data_dir.join("machine.key"))
                .with_agent_key(x0x::identity::AgentKeypair::generate().unwrap())
                .with_agent_cert_path(data_dir.join("agent.cert"))
                .with_peer_cache_disabled()
                .with_contact_store_path(data_dir.join("contacts.json"))
                .with_network_config(test_network_config())
                .build()
                .await
                .unwrap(),
        );
        let state = crate::server::routes::named_groups::tests::secure_endpoint_test_state_at(
            &data_dir, agent,
        )
        .await
        .unwrap();
        (state, dir)
    }

    /// Seed an MlsEncrypted/GSS group owned by the DAEMON AGENT (the caller),
    /// unless a foreign creator is requested.
    async fn seed_group(state: &AppState, group_key: &str, creator: x0x::identity::AgentId) {
        let mut info = GroupInfo::new(
            "kv-group".to_string(),
            String::new(),
            creator,
            group_key.to_string(),
        );
        info.migrate_from_v1();
        let _ = info.rotate_shared_secret();
        state
            .named_groups
            .write()
            .await
            .insert(group_key.to_string(), info);
    }

    async fn seed_treekem_group(state: &AppState, group_key: &str) {
        let group_id = hex::decode(group_key).expect("hex group id");
        let creator = state.agent.agent_id();
        let seed = crate::server::routes::named_groups::agent_treekem_seed(
            state.agent.as_ref(),
            &group_id,
        );
        let live =
            x0x::mls::TreeKemMlsGroup::create(group_id, creator, &seed).expect("TreeKEM group");
        let mut info = GroupInfo::new(
            "treekem".to_string(),
            String::new(),
            creator,
            group_key.to_string(),
        );
        info.migrate_from_v1();
        info.secure_plane = SecureGroupPlane::TreeKem;
        info.shared_secret = None;
        info.secret_epoch = live.epoch();
        info.security_binding = Some(format!("treekem:epoch={}", live.epoch()));
        info.recompute_state_hash();
        state
            .named_groups
            .write()
            .await
            .insert(group_key.to_string(), info);
        state.treekem_groups.write().await.insert(
            group_key.to_string(),
            Arc::new(tokio::sync::Mutex::new(live)),
        );
    }

    #[tokio::test]
    async fn treekem_non_owner_endorses_retained_history_and_revocation_fences_merge() {
        use x0x::kv::TreeKemKvProtector;

        let (writer_state, _writer_dir) = encrypted_store_test_state().await;
        let (reader_state, _reader_dir) = encrypted_store_test_state().await;
        let owner = AgentId([77; 32]);
        let writer = writer_state.agent.agent_id();
        let reader = reader_state.agent.agent_id();
        let group_key = "45".repeat(16);
        let group_id = hex::decode(&group_key).expect("group id");
        let writer_seed = crate::server::routes::named_groups::agent_treekem_seed(
            writer_state.agent.as_ref(),
            &group_id,
        );
        let reader_seed = crate::server::routes::named_groups::agent_treekem_seed(
            reader_state.agent.as_ref(),
            &group_id,
        );
        let mut owner_group =
            x0x::mls::TreeKemMlsGroup::create(group_id, owner, &[77; 32]).expect("owner group");
        let writer_prepared =
            x0x::mls::TreeKemMlsGroup::prepare_member(writer, &writer_seed).expect("writer kp");
        let writer_add = owner_group
            .add_member(writer, writer_prepared.key_package_bytes())
            .expect("add writer");
        let mut writer_group =
            x0x::mls::TreeKemMlsGroup::join_from_welcome(writer_prepared, &writer_add.welcome)
                .expect("writer join");
        let reader_prepared =
            x0x::mls::TreeKemMlsGroup::prepare_member(reader, &reader_seed).expect("reader kp");
        let reader_add = owner_group
            .add_member(reader, reader_prepared.key_package_bytes())
            .expect("add reader");
        writer_group
            .process_commit(&reader_add.commit)
            .expect("writer advances for reader");
        let reader_group =
            x0x::mls::TreeKemMlsGroup::join_from_welcome(reader_prepared, &reader_add.welcome)
                .expect("reader join");

        let mut info = GroupInfo::new(
            "private".to_string(),
            String::new(),
            owner,
            group_key.clone(),
        );
        info.migrate_from_v1();
        info.secure_plane = SecureGroupPlane::TreeKem;
        info.shared_secret = None;
        info.policy.write_access = x0x::groups::GroupWriteAccess::AdminOnly;
        info.add_member(
            hex::encode(writer.as_bytes()),
            x0x::groups::GroupRole::Admin,
            Some(hex::encode(owner.as_bytes())),
            None,
        );
        info.add_member(
            hex::encode(reader.as_bytes()),
            x0x::groups::GroupRole::Member,
            Some(hex::encode(owner.as_bytes())),
            None,
        );
        info.secret_epoch = writer_group.epoch();
        info.security_binding = Some(format!("treekem:epoch={}", writer_group.epoch()));
        info.recompute_state_hash();
        writer_state
            .named_groups
            .write()
            .await
            .insert(group_key.clone(), info.clone());
        reader_state
            .named_groups
            .write()
            .await
            .insert(group_key.clone(), info.clone());
        writer_state.treekem_groups.write().await.insert(
            group_key.clone(),
            Arc::new(tokio::sync::Mutex::new(writer_group)),
        );
        reader_state.treekem_groups.write().await.insert(
            group_key.clone(),
            Arc::new(tokio::sync::Mutex::new(reader_group)),
        );
        let writer_binding = {
            let groups = writer_state.named_groups.read().await;
            resolve_treekem_group_store(&groups, &group_key, "Home", &writer)
                .expect("writer binding")
        };
        let reader_binding = {
            let groups = reader_state.named_groups.read().await;
            resolve_treekem_group_store(&groups, &group_key, "Home", &reader)
                .expect("reader binding")
        };
        assert_eq!(writer_binding.store_id, reader_binding.store_id);
        let writer_auth = Arc::new(
            x0x::groups::TreeKemKvAuthorizationContext::from_group(&info).expect("writer auth"),
        );
        let reader_auth = Arc::new(
            x0x::groups::TreeKemKvAuthorizationContext::from_group(&info).expect("reader auth"),
        );
        let writer_protector = TreeKemGroupStoreProtector::new(
            &writer_state,
            &writer_binding,
            Arc::clone(&writer_auth),
        );
        let reader_protector = TreeKemGroupStoreProtector::new(
            &reader_state,
            &reader_binding,
            Arc::clone(&reader_auth),
        );
        assert!(reader_protector.is_authorized_reader(&reader).await);
        assert!(!reader_protector.is_authorized_writer(&reader).await);

        let group_id = group_key.as_bytes().to_vec();
        let mut source = x0x::kv::KvStore::new_treekem_encrypted(
            writer_binding.store_id,
            "Home".to_string(),
            owner,
            group_id.clone(),
            writer_auth,
        )
        .expect("source store");
        source
            .put(
                "removed".to_string(),
                b"old".to_vec(),
                "text/plain".to_string(),
                saorsa_gossip_types::PeerId::new([1; 32]),
            )
            .expect("seed removed key");
        for index in 0..17 {
            source
                .put(
                    format!("large-{index}"),
                    vec![index as u8; x0x::kv::entry::MAX_INLINE_SIZE],
                    "application/octet-stream".to_string(),
                    saorsa_gossip_types::PeerId::new([1; 32]),
                )
                .expect("large retained value");
        }
        let mut target = source.clone();
        target
            .set_secure_context(reader_auth)
            .expect("reader context");
        source.remove("removed").expect("retained tombstone");
        target
            .put(
                "concurrent".to_string(),
                b"local".to_vec(),
                "text/plain".to_string(),
                saorsa_gossip_types::PeerId::new([2; 32]),
            )
            .expect("concurrent reader state");
        let retained = bincode::serialize(&source).expect("retained image");
        assert!(retained.len() > 1024 * 1024, "history requires paging");
        let signing =
            x0x::kv::AuthorSigning::from_keypair(writer_state.agent.identity().agent_keypair())
                .expect("writer signing");
        let record = writer_protector
            .seal_record(
                &signing,
                x0x::kv::KvMutationKind::RetainedState,
                &writer_binding.store_id,
                b"paged-image-complete",
                false,
            )
            .await
            .expect("non-owner writer endorsement");
        let opened = reader_protector
            .open_record(&reader_binding.store_id, &record)
            .await
            .expect("reader opens endorsed history");
        let target = Arc::new(tokio::sync::RwLock::new(target));
        reader_protector
            .merge_main_record(
                opened,
                saorsa_gossip_types::PeerId::new([1; 32]),
                &target,
                Some(retained),
            )
            .await
            .expect("current writer retained merge");
        let merged = target.read().await;
        assert!(merged.get("removed").is_none());
        assert_eq!(
            merged.get("concurrent").expect("concurrent").value,
            b"local"
        );
        assert_eq!(merged.last_history_endorser(), Some(&writer));
        drop(merged);

        let stale_record = writer_protector
            .seal_record(
                &signing,
                x0x::kv::KvMutationKind::RetainedState,
                &writer_binding.store_id,
                b"paged-image-complete",
                false,
            )
            .await
            .expect("record before removal");
        let stale_opened = reader_protector
            .open_record(&reader_binding.store_id, &stale_record)
            .await
            .expect("opened before removal");
        {
            let mut groups = reader_state.named_groups.write().await;
            groups
                .get_mut(&group_key)
                .expect("reader group")
                .remove_member(&hex::encode(writer.as_bytes()), None);
        }
        assert!(reader_protector
            .merge_main_record(
                stale_opened,
                saorsa_gossip_types::PeerId::new([1; 32]),
                &target,
                Some(bincode::serialize(&source).expect("stale image")),
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn create_group_kv_store_route_creates_encrypted_store() {
        let (state, _dir) = encrypted_store_test_state().await;
        let group_key = "ab".repeat(16);
        seed_group(&state, &group_key, state.agent.agent_id()).await;

        let (code, resp) = create_group_kv_store(
            State(Arc::clone(&state)),
            Path(group_key.clone()),
            Extension(crate::server::rider_auth::ActorContext::Owner { durable: true }),
            Json(CreateGroupStoreRequest {
                name: "workspace".to_string(),
            }),
        )
        .await;
        assert_eq!(code, StatusCode::CREATED, "{resp:?}");
        assert_eq!(resp.0["policy"], "encrypted");
        assert_eq!(resp.0["group_id"], group_key);
        assert!(resp.0["store_id"].as_str().is_some());
        let topic = resp.0["topic"].as_str().expect("topic").to_string();

        // The deterministic identity is what got registered.
        let (store_id, derived_topic) =
            x0x::kv::encrypted::group_store_identity(&group_key, "workspace");
        assert_eq!(topic, derived_topic);
        assert!(state.kv_stores.read().await.contains_key(&topic));
        let handle = state.kv_stores.read().await.get(&topic).cloned().unwrap();
        assert!(
            handle.is_encrypted().await,
            "registered handle is encrypted"
        );

        // A member write goes through the sealed publish path and reads back.
        handle
            .put_with_delta(
                "royalty-split".to_string(),
                b"hush".to_vec(),
                "text/plain".to_string(),
            )
            .await
            .expect("member put on encrypted store");
        let entry = handle
            .get("royalty-split")
            .await
            .expect("get")
            .expect("present");
        assert_eq!(entry.value, b"hush".to_vec());

        // Idempotent re-open returns 200 with the same store id.
        let (code2, resp2) = create_group_kv_store(
            State(Arc::clone(&state)),
            Path(group_key),
            Extension(crate::server::rider_auth::ActorContext::Owner { durable: true }),
            Json(CreateGroupStoreRequest {
                name: "workspace".to_string(),
            }),
        )
        .await;
        assert_eq!(code2, StatusCode::OK, "{resp2:?}");
        assert_eq!(resp2.0["store_id"], resp.0["store_id"]);
        let _ = store_id;
    }

    /// WHY (PR #508 review P1): the leave/removal path deletes the group
    /// from `named_groups`. The refresh hook must treat a MISSING group as
    /// lifecycle: invalidate the context AND retire the live handle, so the
    /// departed member cannot keep reading, writing, or publishing
    /// old-epoch records on a stale secret/roster snapshot.
    #[tokio::test]
    async fn leave_invalidates_and_retires_group_encrypted_store() {
        let (state, _dir) = encrypted_store_test_state().await;
        let group_key = "ab".repeat(16);
        seed_group(&state, &group_key, state.agent.agent_id()).await;
        let (code, resp) = create_group_kv_store(
            State(Arc::clone(&state)),
            Path(group_key.clone()),
            Extension(crate::server::rider_auth::ActorContext::Owner { durable: true }),
            Json(CreateGroupStoreRequest {
                name: "ws".to_string(),
            }),
        )
        .await;
        assert_eq!(code, StatusCode::CREATED, "{resp:?}");
        let topic = resp.0["topic"].as_str().expect("topic").to_string();
        let handle = state.kv_stores.read().await.get(&topic).cloned().unwrap();

        // The member LEAVES: the group entry disappears from the map.
        state.named_groups.write().await.remove(&group_key);

        // Next sync activity runs the refresh hook -> lifecycle branch. The
        // hook is rebuilt exactly as create wired it, bound to the context
        // snapshot the live sync captured BEFORE the leave.
        let pre_leave_info = crate::groups::GroupInfo::new(
            group_key.clone(),
            String::new(),
            state.agent.agent_id(),
            group_key.clone(),
        );
        let ctx =
            Arc::new(x0x::groups::GssKvSecureContext::from_group(&pre_leave_info).expect("ctx"));
        assert!(ctx.is_active_member(&state.agent.agent_id()));
        let hook = gss_kv_refresh(
            &state,
            ctx,
            group_key.clone(),
            topic.clone(),
            state.agent.agent_id(),
        );
        hook().await;

        // The handle is retired out of the registry...
        assert!(
            !state.kv_stores.read().await.contains_key(&topic),
            "retired handle must leave the registry"
        );
        // ...and a STALE clone (held elsewhere) fails closed on writes.
        let err = handle
            .put_with_delta("k".to_string(), b"v".to_vec(), "text/plain".to_string())
            .await
            .expect_err("post-leave write must be refused");
        assert!(
            matches!(err, x0x::error::IdentityError::Unauthorized(_)),
            "got {err:?}"
        );
    }

    /// WHY (PR #508 review P1): a WITHDRAWN group is equally terminal — the
    /// tombstone keeps the roster entry but the store must not operate.
    #[tokio::test]
    async fn withdrawn_group_invalidates_encrypted_store() {
        let (state, _dir) = encrypted_store_test_state().await;
        let group_key = "cd".repeat(16);
        seed_group(&state, &group_key, state.agent.agent_id()).await;
        let (code, resp) = create_group_kv_store(
            State(Arc::clone(&state)),
            Path(group_key.clone()),
            Extension(crate::server::rider_auth::ActorContext::Owner { durable: true }),
            Json(CreateGroupStoreRequest {
                name: "ws".to_string(),
            }),
        )
        .await;
        assert_eq!(code, StatusCode::CREATED, "{resp:?}");
        let topic = resp.0["topic"].as_str().expect("topic").to_string();
        let handle = state.kv_stores.read().await.get(&topic).cloned().unwrap();

        // The group is WITHDRAWN (tombstone retained, roster intact).
        state
            .named_groups
            .write()
            .await
            .get_mut(&group_key)
            .expect("group")
            .withdrawn = true;

        // Directly retire the way the withdraw lifecycle path does.
        retire_group_kv_stores(&state, &group_key).await;
        assert!(
            !state.kv_stores.read().await.contains_key(&topic),
            "withdrawal must retire the handle"
        );
        // A stale clone fails closed on writes even though the roster entry
        // would still say "active member".
        let err = handle
            .put_with_delta("k".to_string(), b"v".to_vec(), "text/plain".to_string())
            .await
            .expect_err("post-withdrawal write must be refused");
        assert!(matches!(err, x0x::error::IdentityError::Unauthorized(_)));

        // And the refresh hook independently invalidates on withdrawn state
        // (second layer, for handles the lifecycle paths miss).
        let ctx = Arc::new(
            x0x::groups::GssKvSecureContext::from_group(
                state
                    .named_groups
                    .read()
                    .await
                    .get(&group_key)
                    .expect("group"),
            )
            .expect("ctx"),
        );
        assert!(ctx.is_active_member(&state.agent.agent_id()));
        let hook = gss_kv_refresh(
            &state,
            Arc::clone(&ctx),
            group_key.clone(),
            topic,
            state.agent.agent_id(),
        );
        hook().await;
        assert!(
            !ctx.is_active_member(&state.agent.agent_id()),
            "withdrawn group must invalidate the context"
        );
    }

    #[tokio::test]
    async fn create_group_kv_store_route_guards() {
        let (state, _dir) = encrypted_store_test_state().await;
        let owner_actor = crate::server::rider_auth::ActorContext::Owner { durable: true };

        // Unknown group -> 404.
        let (code, resp) = create_group_kv_store(
            State(Arc::clone(&state)),
            Path("missing".to_string()),
            Extension(owner_actor.clone()),
            Json(CreateGroupStoreRequest {
                name: "n".to_string(),
            }),
        )
        .await;
        assert_eq!(code, StatusCode::NOT_FOUND, "{resp:?}");

        // Non-member: the group exists but the daemon agent is not in it.
        let outsider = x0x::identity::AgentId([9; 32]);
        seed_group(&state, &"cd".repeat(16), outsider).await;
        let (code, resp) = create_group_kv_store(
            State(Arc::clone(&state)),
            Path("cd".repeat(16)),
            Extension(owner_actor.clone()),
            Json(CreateGroupStoreRequest {
                name: "n".to_string(),
            }),
        )
        .await;
        assert_eq!(code, StatusCode::FORBIDDEN, "{resp:?}");

        // SignedPublic group -> creator-anchored group-signed store.
        let signed_key = "ef".repeat(16);
        {
            let mut info = GroupInfo::new(
                "public".to_string(),
                String::new(),
                state.agent.agent_id(),
                signed_key.clone(),
            );
            info.migrate_from_v1();
            info.policy.confidentiality = GroupConfidentiality::SignedPublic;
            state
                .named_groups
                .write()
                .await
                .insert(signed_key.clone(), info);
        }
        let (code, resp) = create_group_kv_store(
            State(Arc::clone(&state)),
            Path(signed_key),
            Extension(owner_actor.clone()),
            Json(CreateGroupStoreRequest {
                name: "n".to_string(),
            }),
        )
        .await;
        assert_eq!(code, StatusCode::CREATED, "{resp:?}");
        assert_eq!(resp.0["policy"], "group_signed");

        // TreeKEM-plane group uses the distinct mutable-ratchet backend.
        let treekem_key = "12".repeat(16);
        seed_treekem_group(&state, &treekem_key).await;
        let (code, resp) = create_group_kv_store(
            State(Arc::clone(&state)),
            Path(treekem_key.clone()),
            Extension(owner_actor.clone()),
            Json(CreateGroupStoreRequest {
                name: "n".to_string(),
            }),
        )
        .await;
        assert_eq!(code, StatusCode::CREATED, "{resp:?}");
        assert_eq!(resp.0["policy"], "encrypted");
        let topic = resp.0["topic"].as_str().expect("topic").to_string();
        let handle = state
            .kv_stores
            .read()
            .await
            .get(&topic)
            .cloned()
            .expect("TreeKEM handle");
        handle
            .validate_group_binding(
                "n",
                &treekem_key,
                state.agent.agent_id(),
                x0x::GroupStoreProtection::TreeKemEncrypted,
            )
            .await
            .expect("distinct TreeKEM binding");
        handle
            .put_with_delta("k".into(), b"v".to_vec(), "text/plain".into())
            .await
            .expect("TreeKEM store write");
        assert_eq!(
            handle.get("k").await.expect("read").expect("stored").value,
            b"v"
        );
        handle.retire();
        state.kv_stores.write().await.remove(&topic);
        let binding = {
            let groups = state.named_groups.read().await;
            resolve_treekem_group_store(&groups, &treekem_key, "n", &state.agent.agent_id())
                .expect("restart binding")
        };
        let (restored, _, _) = open_bound_treekem_store(&state, &binding)
            .await
            .expect("restore TreeKEM store snapshot");
        assert_eq!(
            restored
                .get("k")
                .await
                .expect("restored read")
                .expect("restored value")
                .value,
            b"v",
            "restart must retain the durable store image"
        );
        let _ = GssKvSecureContext::from_group; // keep backend import referenced
        let _ = GroupPolicy::default();
    }

    #[tokio::test]
    async fn public_store_cached_binding_mismatch_returns_without_deadlock() {
        let (state, _dir) = encrypted_store_test_state().await;
        let group_key = "34".repeat(16);
        let mut info = GroupInfo::new(
            "public".to_string(),
            String::new(),
            state.agent.agent_id(),
            group_key.clone(),
        );
        info.migrate_from_v1();
        info.policy.confidentiality = GroupConfidentiality::SignedPublic;
        info.policy.read_access = crate::groups::GroupReadAccess::Public;
        state
            .named_groups
            .write()
            .await
            .insert(group_key.clone(), info);
        let binding = {
            let groups = state.named_groups.read().await;
            resolve_public_group_store(&groups, &group_key, "Wiki", &state.agent.agent_id())
                .expect("binding")
        };
        let wrong = state
            .agent
            .create_kv_store("wrong", "wrong/topic")
            .await
            .expect("wrong cached handle");
        state
            .kv_stores
            .write()
            .await
            .insert(binding.topic.clone(), wrong);

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            open_bound_public_store(&state, &binding),
        )
        .await
        .expect("mismatch cleanup must not deadlock");
        assert!(result.is_err(), "mismatched cached handle must fail closed");
        assert!(!state.kv_stores.read().await.contains_key(&binding.topic));
    }

    #[tokio::test]
    async fn cached_public_store_read_refresh_rejects_removed_member() {
        let (state, _dir) = encrypted_store_test_state().await;
        let group_key = "56".repeat(16);
        let local = state.agent.agent_id();
        let creator = AgentId([8; 32]);
        let mut info = GroupInfo::new(
            "public".to_string(),
            String::new(),
            creator,
            group_key.clone(),
        );
        info.migrate_from_v1();
        info.policy.confidentiality = GroupConfidentiality::SignedPublic;
        info.policy.read_access = crate::groups::GroupReadAccess::MembersOnly;
        info.add_member(
            hex::encode(local.as_bytes()),
            crate::groups::GroupRole::Member,
            Some(hex::encode(creator.as_bytes())),
            None,
        );
        state
            .named_groups
            .write()
            .await
            .insert(group_key.clone(), info);
        let binding = {
            let groups = state.named_groups.read().await;
            resolve_public_group_store(&groups, &group_key, "Wiki", &local).expect("binding")
        };
        let (handle, _, _) = open_bound_public_store(&state, &binding)
            .await
            .expect("open member store");
        state
            .kv_stores
            .write()
            .await
            .insert(binding.topic.clone(), handle.clone());
        handle
            .put(
                "visible".to_string(),
                b"value".to_vec(),
                "text/plain".to_string(),
            )
            .await
            .expect("member write");
        assert!(handle.get("visible").await.expect("member read").is_some());
        let version_before = handle.ownership_info().await.version;

        state
            .named_groups
            .write()
            .await
            .get_mut(&group_key)
            .expect("group")
            .remove_member(&hex::encode(local.as_bytes()), None);
        assert!(handle
            .put(
                "late".to_string(),
                b"denied".to_vec(),
                "text/plain".to_string()
            )
            .await
            .is_err());
        assert_eq!(handle.ownership_info().await.version, version_before);
        let mut direct = x0x::kv::KvStoreDelta::new(version_before + 1);
        direct.added.insert(
            "direct".to_string(),
            (
                x0x::kv::KvEntry::new(
                    "direct".to_string(),
                    b"denied".to_vec(),
                    "text/plain".to_string(),
                ),
                (saorsa_gossip_types::PeerId::new([9; 32]), 1),
            ),
        );
        assert!(handle
            .apply_remote_delta(
                saorsa_gossip_types::PeerId::new([9; 32]),
                &direct,
                Some(local)
            )
            .await
            .is_err());
        assert_eq!(handle.ownership_info().await.version, version_before);

        state
            .kv_stores
            .write()
            .await
            .insert(binding.topic.clone(), handle.clone());
        let get_response = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            get_kv_value(
                State(Arc::clone(&state)),
                Path((binding.topic.clone(), "visible".to_string())),
            ),
        )
        .await
        .expect("GET must not deadlock")
        .into_response();
        assert_eq!(get_response.status(), StatusCode::FORBIDDEN);

        state
            .kv_stores
            .write()
            .await
            .insert(binding.topic.clone(), handle);
        let keys_response = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            list_kv_keys(State(Arc::clone(&state)), Path(binding.topic.clone())),
        )
        .await
        .expect("keys listing must not deadlock")
        .into_response();
        assert_eq!(keys_response.status(), StatusCode::FORBIDDEN);
        assert!(!state.kv_stores.read().await.contains_key(&binding.topic));
    }
}
