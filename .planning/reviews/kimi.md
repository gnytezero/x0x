Review this git diff for security, errors, quality issues. Rate A-F. Diff:

diff --git a/src/lib.rs b/src/lib.rs
index b1bf0b5..f6667f5 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -3156,6 +3156,7 @@ impl AgentBuilder {
                 peer_id,
                 std::sync::Arc::clone(net),
                 presence::PresenceConfig::default(),
+                bootstrap_cache.clone(),
             )
             .map_err(|e| {
                 error::IdentityError::Storage(std::io::Error::other(format!(
diff --git a/src/presence.rs b/src/presence.rs
index 2a2c381..8b87dfa 100644
--- a/src/presence.rs
+++ b/src/presence.rs
@@ -7,11 +7,13 @@
 //! - [`PresenceConfig`](crate::presence::PresenceConfig) — tunable parameters 
for beacon interval, FOAF TTL, etc.
 //! - [`PresenceEvent`](crate::presence::PresenceEvent) — online/offline 
notifications for discovered agents.
 //! - [`PresenceWrapper`](crate::presence::PresenceWrapper) — lifecycle wrapper
around the underlying `PresenceManager`.
+//! - `PeerBeaconStats` — per-peer inter-arrival tracking for adaptive failure 
detection.
 //! - `global_presence_topic` — the canonical presence topic for FOAF queries.
 //! - `peer_to_agent_id` — resolve a gossip `PeerId` to an `AgentId` via the 
discovery cache.
 //! - `presence_record_to_discovered_agent` — convert a `PresenceRecord` into a
`DiscoveredAgent`.
+//! - `foaf_peer_score` — quality score for FOAF routing (lower jitter = higher
score).
 
-use std::collections::{HashMap, HashSet};
+use std::collections::{HashMap, HashSet, VecDeque};
 use std::sync::Arc;
 
 use saorsa_gossip_groups::GroupContext;
@@ -27,6 +29,24 @@ use crate::network::NetworkNode;
 use crate::trust::{TrustContext, TrustDecision, TrustEvaluator};
 use crate::DiscoveredAgent;
 
+/// Maximum number of beacon inter-arrival intervals tracked per peer.
+///
+/// A window of 10 gives a 95 % confidence interval on the mean that is
+/// tight enough for practical failure detection without excessive memory.
+const INTER_ARRIVAL_WINDOW: usize = 10;
+
+/// Lower bound on the adaptive offline timeout (seconds).
+///
+/// Even for peers with very stable, frequent beacons we never declare them
+/// offline in fewer than 3 minutes — protects against brief connectivity 
blips.
+const ADAPTIVE_TIMEOUT_FLOOR_SECS: f64 = 180.0;
+
+/// Upper bound on the adaptive offline timeout (seconds).
+///
+/// We never wait more than 10 minutes before declaring a peer offline,
+/// regardless of how infrequent or jittery its beacons are.
+const ADAPTIVE_TIMEOUT_CEILING_SECS: f64 = 600.0;
+
 /// The global presence topic used for FOAF queries.
 ///
 /// All x0x agents publish beacons to this topic so that FOAF random-walk
@@ -186,6 +206,113 @@ pub fn filter_by_trust(
         .collect()
 }
 
+/// Compute the FOAF routing quality score for a peer.
+///
+/// Peers with stable, low-jitter beacon intervals score close to 1.0 and are
+/// preferred as FOAF random-walk forwarding targets. Peers with no observation
+/// history score 0.5 (neutral). Peers with high jitter score close to 0.0.
+///
+/// Score formula: `1.0 / (1.0 + stddev)` where `stddev` is in seconds.
+///
+/// # Range
+///
+/// Always returns a value in `[0.0, 1.0]`.
+#
+pub fn foaf_peer_score(stats: &PeerBeaconStats) -> f64 {
+    match stats.inter_arrival_stats() {
+        Some((_, stddev)) => 1.0 / (1.0 + stddev),
+        None => 0.5, // Unknown stability: neutral score.
+    }
+}
+
+/// Sliding-window inter-arrival statistics for a single peer's beacons.
+///
+/// Tracks the arrival timestamps of the last 10 beacons and exposes mean and
+/// standard deviation of the inter-arrival intervals. Used by the
+/// Phi-Accrual-lite adaptive timeout and by FOAF peer scoring.
+#
+pub struct PeerBeaconStats {
+    /// Wall-clock timestamps (unix seconds) of the last N beacon arrivals.
+    /// The VecDeque is capped at `INTER_ARRIVAL_WINDOW` entries.
+    last_seen: VecDeque<u64>,
+}
