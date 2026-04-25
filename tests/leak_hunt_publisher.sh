#!/usr/bin/env bash
# Hunt step 3 — local 2-daemon publisher repro under dhat.
#
# Launches 2 isolated x0xd daemons on loopback. daemon-1 publishes
# MSG_RATE msg/s × MSG_SIZE bytes to topic T for DURATION_MIN minutes.
# daemon-2 subscribes. Samples RSS every 30s on both. The publisher
# binary should be the dhat-instrumented build so dhat-heap.json is
# emitted on graceful shutdown.
#
# Usage:
#   tests/leak_hunt_publisher.sh \
#       [--duration-min 10] [--msg-rate 50] [--msg-size 4096] \
#       [--proof-dir proofs/leak-pub-<ts>]
#
# To exercise the dhat-instrumented build:
#   cargo build --bin x0xd --features profile-heap
#   X0XD_BIN=target/debug/x0xd tests/leak_hunt_publisher.sh

set -euo pipefail

DURATION_MIN=10
POST_IDLE_MIN=0
MSG_RATE=50
MSG_SIZE=4096
PROOF_DIR=""
TOPIC="leak-pub-$$"

while (( "$#" )); do
    case "$1" in
        --duration-min) DURATION_MIN="$2"; shift 2 ;;
        --post-idle-min) POST_IDLE_MIN="$2"; shift 2 ;;
        --msg-rate) MSG_RATE="$2"; shift 2 ;;
        --msg-size) MSG_SIZE="$2"; shift 2 ;;
        --proof-dir) PROOF_DIR="$2"; shift 2 ;;
        --topic) TOPIC="$2"; shift 2 ;;
        *) echo "unknown arg: $1"; exit 2 ;;
    esac
done

if [ -z "$PROOF_DIR" ]; then
    PROOF_DIR="proofs/leak-pub-$(date -u +%Y%m%dT%H%M%SZ)"
fi
mkdir -p "$PROOF_DIR/logs"

LOG="$PROOF_DIR/run.log"
log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$LOG"; }

BIN="${X0XD_BIN:-target/debug/x0xd}"
if [ ! -x "$BIN" ]; then
    echo "missing $BIN — run: cargo build --bin x0xd"; exit 2
fi

log "Config: duration=${DURATION_MIN}min rate=${MSG_RATE}msg/s size=${MSG_SIZE}B topic=$TOPIC"
log "Binary: $BIN"

PIDS=()
PORTS=()
TOKENS=()

cleanup() {
    log "Stopping daemons (graceful — needed for dhat-heap dump emission)"
    for pid in "${PIDS[@]}"; do
        kill -INT "$pid" 2>/dev/null || true
    done
    # ant-quic shutdown drains all QUIC connections + flushes state; can take
    # 30+s. dhat::Profiler::Drop only runs when main() returns normally.
    log "Waiting up to 60s for graceful shutdown + dhat flush"
    for s in $(seq 1 60); do
        local alive=0
        for pid in "${PIDS[@]}"; do
            kill -0 "$pid" 2>/dev/null && alive=$((alive+1))
        done
        [ $alive -eq 0 ] && break
        sleep 1
    done
    for pid in "${PIDS[@]}"; do
        kill -KILL "$pid" 2>/dev/null || true
    done
    wait "${PIDS[@]}" 2>/dev/null || true
    # Each daemon writes dhat-heap-<pid>.json to DHAT_OUT_DIR (proof dir).
    log "dhat dumps in proof dir:"
    ls -la "$PROOF_DIR"/dhat-heap-*.json 2>&1 | tee -a "$LOG"
}
trap cleanup EXIT

if [ "$(uname)" = "Darwin" ]; then
    DATA_BASE="$HOME/Library/Application Support"
else
    DATA_BASE="$HOME/.local/share"
fi

# Launch 2 daemons.
for i in 1 2; do
    INSTANCE="leak-pub-$i"
    ID_DIR="$PROOF_DIR/node-$i/identity"
    LOG_DIR="$PROOF_DIR/logs/node-$i"
    mkdir -p "$ID_DIR" "$LOG_DIR"
    PORT=$((12790 + i))

    log "Launching daemon $i on api-port $PORT"
    X0X_IDENTITY_DIR="$ID_DIR" \
    X0X_LOG_DIR="$LOG_DIR" \
    DHAT_OUT_DIR="$(cd "$PROOF_DIR" && pwd)" \
        "$BIN" --name "$INSTANCE" --api-port "$PORT" --no-hard-coded-bootstrap \
        > "$LOG_DIR/stdout.log" 2> "$LOG_DIR/stderr.log" &
    PIDS+=($!)
    PORTS+=("$PORT")
done

SETTLE=15
log "Waiting ${SETTLE}s for daemons to bind"
sleep "$SETTLE"

# Read tokens.
for i in 1 2; do
    TOKEN_FILE="$DATA_BASE/x0x-leak-pub-$i/api-token"
    if [ -f "$TOKEN_FILE" ]; then
        TOKENS+=("$(cat "$TOKEN_FILE")")
    else
        log "warn: no token at $TOKEN_FILE"
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

# Subscribe both nodes (so publisher self-delivery + remote delivery both
# stress the pipeline).
for i in 1 2; do
    api "$i" POST /subscribe "{\"topic\":\"$TOPIC\"}" > "$PROOF_DIR/logs/node-$i/subscribe.json" || true
done

# Pre-publish gossip stats snapshot.
for i in 1 2; do
    api "$i" GET /diagnostics/gossip > "$PROOF_DIR/logs/node-$i/gossip-pre.json" || true
done

# RSS sampler in background.
RSS_CSV="$PROOF_DIR/rss.csv"
echo "ts_iso,uptime_s,node1_rss_kb,node1_cpu,node2_rss_kb,node2_cpu" > "$RSS_CSV"
START=$(date +%s)
PUB_PID="${PIDS[0]}"
SUB_PID="${PIDS[1]}"

(
    while kill -0 "$PUB_PID" 2>/dev/null; do
        NOW=$(date +%s)
        UP=$((NOW - START))
        S1=$(ps -o rss=,%cpu= -p "$PUB_PID" 2>/dev/null | tr -s ' ' | sed 's/^ //')
        S2=$(ps -o rss=,%cpu= -p "$SUB_PID" 2>/dev/null | tr -s ' ' | sed 's/^ //')
        R1=$(echo "$S1" | awk '{print $1}')
        C1=$(echo "$S1" | awk '{print $2}')
        R2=$(echo "$S2" | awk '{print $1}')
        C2=$(echo "$S2" | awk '{print $2}')
        echo "$(date -u +%Y-%m-%dT%H:%M:%SZ),$UP,$R1,$C1,$R2,$C2" >> "$RSS_CSV"
        sleep 30
    done
) &
SAMPLER_PID=$!

# Build a base64 payload of MSG_SIZE bytes.
PAYLOAD_RAW=$(printf '%*s' "$MSG_SIZE" | tr ' ' 'x')
PAYLOAD_B64=$(printf '%s' "$PAYLOAD_RAW" | base64 | tr -d '\n')

# Publisher loop. Target rate = MSG_RATE msg/s.
SLEEP_SEC=$(awk -v r="$MSG_RATE" 'BEGIN { printf "%.4f", 1.0 / r }')
END=$((START + DURATION_MIN * 60))
COUNT=0
log "Publishing — rate=${MSG_RATE}/s sleep_per_msg=${SLEEP_SEC}s, ending at $(date -u -r $END '+%H:%M:%S' 2>/dev/null || date -u --date="@$END" '+%H:%M:%S')"

while [ "$(date +%s)" -lt "$END" ]; do
    api 1 POST /publish "{\"topic\":\"$TOPIC\",\"payload\":\"$PAYLOAD_B64\"}" >/dev/null 2>&1 || true
    COUNT=$((COUNT + 1))
    # python sleep is more accurate than `sleep` for sub-second.
    python3 -c "import time; time.sleep($SLEEP_SEC)" 2>/dev/null || sleep 0.02
done

log "Published $COUNT messages over ${DURATION_MIN}min"

# Drain delivery for 5s before post-idle phase.
sleep 5

# Optional post-publish idle phase — observes whether RSS returns to baseline
# after sustained load stops. Tells us if memory is just "active working set"
# (returns to baseline) or actually retained.
if (( POST_IDLE_MIN > 0 )); then
    log "Post-publish idle observation: ${POST_IDLE_MIN} min (publishers stopped, daemons still running)"
    POST_END=$(($(date +%s) + POST_IDLE_MIN * 60))
    while [ "$(date +%s)" -lt "$POST_END" ]; do
        sleep 30
    done
    log "Post-idle phase done"
fi

# Stop sampler.
kill "$SAMPLER_PID" 2>/dev/null || true
wait "$SAMPLER_PID" 2>/dev/null || true
for i in 1 2; do
    api "$i" GET /diagnostics/gossip > "$PROOF_DIR/logs/node-$i/gossip-post.json" || true
done

# Summary.
FIRST=$(awk -F, 'NR==2{print $3}' "$RSS_CSV")
LAST=$(awk -F, 'END{print $3}' "$RSS_CSV")
DELTA=$((LAST - FIRST))
log "Publisher RSS first=${FIRST}KB ($((FIRST/1024))MB) last=${LAST}KB ($((LAST/1024))MB) delta=$((DELTA/1024))MB"
F2=$(awk -F, 'NR==2{print $5}' "$RSS_CSV")
L2=$(awk -F, 'END{print $5}' "$RSS_CSV")
log "Subscriber RSS first=${F2}KB ($((F2/1024))MB) last=${L2}KB ($((L2/1024))MB) delta=$(((L2-F2)/1024))MB"
log "Proof dir: $PROOF_DIR"
