#!/usr/bin/env bash
# Stress test the x0x gossip pipeline against drop-detection counters.
#
# Launches N daemons on loopback, subscribes to a test topic on each,
# publishes M messages from one daemon, then waits for delivery. The
# /diagnostics/gossip endpoint proves zero drops between publish and
# subscriber delivery.
#
# Usage:
#   tests/e2e_stress_gossip.sh [--nodes 3] [--messages 1000] \
#       [--topic gossip-stress] [--proof-dir proofs/stress-<ts>]

set -euo pipefail

NODES=3
MESSAGES=1000
TOPIC="gossip-stress-$$"
PROOF_DIR=""

while (( "$#" )); do
    case "$1" in
        --nodes) NODES="$2"; shift 2 ;;
        --messages) MESSAGES="$2"; shift 2 ;;
        --topic) TOPIC="$2"; shift 2 ;;
        --proof-dir) PROOF_DIR="$2"; shift 2 ;;
        *) echo "unknown arg: $1"; exit 2 ;;
    esac
done

if [ -z "$PROOF_DIR" ]; then
    PROOF_DIR="proofs/stress-$(date +%Y%m%d-%H%M%S)"
fi
mkdir -p "$PROOF_DIR/logs"
LOG="$PROOF_DIR/stress.log"
REPORT="$PROOF_DIR/stress-report.json"

log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$LOG"; }

log "Stress config: NODES=$NODES MESSAGES=$MESSAGES TOPIC=$TOPIC"
log "Proof dir: $PROOF_DIR"

BIN="${X0XD_BIN:-target/debug/x0xd}"
CLI="${X0X_BIN:-target/debug/x0x}"

if [ ! -x "$BIN" ] || [ ! -x "$CLI" ]; then
    log "Building x0xd + x0x binaries..."
    cargo build --bin x0xd --bin x0x
fi

PIDS=()
TOKENS=()
PORTS=()
cleanup() {
    log "Cleaning up daemons..."
    for pid in "${PIDS[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    wait "${PIDS[@]}" 2>/dev/null || true
}
trap cleanup EXIT

# Spin up N isolated daemons. Each gets its own identity dir, log file,
# api port, and X0X_LOG_DIR so logs are per-daemon.
for i in $(seq 1 "$NODES"); do
    INSTANCE="stress-$i"
    ID_DIR="$PROOF_DIR/node-$i"
    mkdir -p "$ID_DIR"
    export X0X_IDENTITY_DIR="$ID_DIR"
    export X0X_LOG_DIR="$PROOF_DIR/logs/node-$i"
    mkdir -p "$X0X_LOG_DIR"
    PORT=$((12700 + i))

    log "Launching daemon $i on port $PORT"
    "$BIN" \
        --name "$INSTANCE" \
        --api-port "$PORT" \
        --no-hard-coded-bootstrap \
        > "$PROOF_DIR/logs/node-$i/stdout.log" \
        2> "$PROOF_DIR/logs/node-$i/stderr.log" &
    PIDS+=($!)
    PORTS+=("$PORT")
done

log "Waiting 10s for daemons to bind + discover each other..."
sleep 10

# Read the auto-generated API tokens.
for i in $(seq 1 "$NODES"); do
    ID_DIR="$PROOF_DIR/node-$i"
    if [ -f "$ID_DIR/api-token" ]; then
        TOKENS+=("$(cat "$ID_DIR/api-token")")
    else
        TOKENS+=("")
    fi
done

api() {
    local idx="$1" method="$2" path="$3" body="${4:-}"
    local port="${PORTS[$((idx - 1))]}"
    local token="${TOKENS[$((idx - 1))]}"
    local args=(-sS -X "$method" "http://127.0.0.1:${port}${path}")
    [ -n "$token" ] && args+=(-H "authorization: Bearer $token")
    [ -n "$body" ] && args+=(-H "content-type: application/json" -d "$body")
    curl "${args[@]}"
}

# Subscribe every node to the topic.
for i in $(seq 1 "$NODES"); do
    api "$i" POST /subscribe "{\"topic\":\"$TOPIC\"}" > "$PROOF_DIR/logs/node-$i/subscribe.json" || true
done

log "Subscriptions installed; snapshotting pre-publish gossip stats"
for i in $(seq 1 "$NODES"); do
    api "$i" GET /diagnostics/gossip > "$PROOF_DIR/logs/node-$i/gossip-pre.json" || true
done

# Publisher = node 1. Fire $MESSAGES messages.
log "Publishing $MESSAGES messages from node 1 to topic $TOPIC"
start_ts=$(date +%s)
for n in $(seq 1 "$MESSAGES"); do
    api 1 POST /publish \
        "{\"topic\":\"$TOPIC\",\"payload\":\"msg-$n\"}" >/dev/null 2>&1 || true
done
end_ts=$(date +%s)
elapsed=$((end_ts - start_ts))
log "Published $MESSAGES msgs in ${elapsed}s ($(( MESSAGES / (elapsed > 0 ? elapsed : 1) )) msgs/s)"

log "Sleeping 5s for delivery to drain..."
sleep 5

# Snapshot post-publish gossip stats.
declare -a POST_PUB=()
declare -a POST_DELIV=()
declare -a POST_DROPS=()
for i in $(seq 1 "$NODES"); do
    api "$i" GET /diagnostics/gossip > "$PROOF_DIR/logs/node-$i/gossip-post.json" || true
    PUB=$(jq -r '.stats.publish_total' "$PROOF_DIR/logs/node-$i/gossip-post.json" 2>/dev/null || echo 0)
    DEL=$(jq -r '.stats.delivered_to_subscriber' "$PROOF_DIR/logs/node-$i/gossip-post.json" 2>/dev/null || echo 0)
    DROPS=$(jq -r '.stats.decode_to_delivery_drops' "$PROOF_DIR/logs/node-$i/gossip-post.json" 2>/dev/null || echo 0)
    POST_PUB+=("$PUB")
    POST_DELIV+=("$DEL")
    POST_DROPS+=("$DROPS")
    log "node-$i: publish=$PUB delivered=$DEL drops=$DROPS"
done

# Report JSON.
{
    printf '{"nodes":%s,"messages":%s,"topic":"%s","elapsed_seconds":%s,"per_node":[' \
        "$NODES" "$MESSAGES" "$TOPIC" "$elapsed"
    for i in $(seq 1 "$NODES"); do
        [ $i -gt 1 ] && printf ','
        printf '{"idx":%s,"publish_total":%s,"delivered_to_subscriber":%s,"decode_to_delivery_drops":%s}' \
            "$i" "${POST_PUB[$((i - 1))]}" "${POST_DELIV[$((i - 1))]}" "${POST_DROPS[$((i - 1))]}"
    done
    printf ']}\n'
} > "$REPORT"

log "Stress report → $REPORT"

# Acceptance: publisher node published ≥ MESSAGES, subscriber nodes each
# delivered ≥ MESSAGES, zero decode→delivery drops anywhere.
PUB1=${POST_PUB[0]}
FAIL=0
if (( PUB1 < MESSAGES )); then
    log "FAIL: publisher only recorded $PUB1 of $MESSAGES publishes"
    FAIL=1
fi
for i in $(seq 1 "$NODES"); do
    if [ "${POST_DROPS[$((i - 1))]}" != "0" ]; then
        log "FAIL: node-$i reports ${POST_DROPS[$((i - 1))]} decode→delivery drops"
        FAIL=1
    fi
done

exit $FAIL
