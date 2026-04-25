#!/usr/bin/env bash
# Hunt step 1 — local idle baseline.
#
# Launch a single x0xd, no peers, no publisher load. Sample VmRSS every
# 30s for 10 min. If RSS climbs at idle the leak is in background tasks
# (heartbeat / presence / SWIM / cache eviction). If flat the leak needs
# gossip traffic and we move to hunt step 3 with a publisher.
#
# Usage:
#   tests/leak_hunt_idle.sh [--duration-min 10] [--proof-dir proofs/leak-idle-<ts>]

set -euo pipefail

DURATION_MIN=10
PROOF_DIR=""
INSTANCE="leak-idle-$$"
PORT=12780

while (( "$#" )); do
    case "$1" in
        --duration-min) DURATION_MIN="$2"; shift 2 ;;
        --proof-dir) PROOF_DIR="$2"; shift 2 ;;
        *) echo "unknown arg: $1"; exit 2 ;;
    esac
done

if [ -z "$PROOF_DIR" ]; then
    PROOF_DIR="proofs/leak-idle-$(date -u +%Y%m%dT%H%M%SZ)"
fi
mkdir -p "$PROOF_DIR"

LOG="$PROOF_DIR/sampler.log"
RSS_CSV="$PROOF_DIR/rss.csv"
log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$LOG"; }

BIN="${X0XD_BIN:-target/debug/x0xd}"
if [ ! -x "$BIN" ]; then
    echo "missing $BIN — build first"; exit 2
fi

ID_DIR="$PROOF_DIR/identity"
LOG_DIR="$PROOF_DIR/x0xd-logs"
mkdir -p "$ID_DIR" "$LOG_DIR"
export X0X_IDENTITY_DIR="$ID_DIR"
export X0X_LOG_DIR="$LOG_DIR"

cleanup() {
    if [ -n "${PID:-}" ]; then
        log "Stopping x0xd PID=$PID"
        kill "$PID" 2>/dev/null || true
        wait "$PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

log "Starting x0xd on port $PORT (no bootstrap, no peers)"
"$BIN" --name "$INSTANCE" --api-port "$PORT" --no-hard-coded-bootstrap \
    > "$LOG_DIR/stdout.log" 2> "$LOG_DIR/stderr.log" &
PID=$!
log "PID=$PID"

# Give it 5s to bind + initialise
sleep 5

# CSV header
echo "ts_iso,uptime_s,rss_kb,vsz_kb,cpu_pct" > "$RSS_CSV"

START=$(date +%s)
END=$((START + DURATION_MIN * 60))

while [ "$(date +%s)" -lt "$END" ]; do
    NOW=$(date +%s)
    UP=$((NOW - START))

    # ps -o rss,vsz,%cpu (BSD on macOS, GNU on linux — both accept this)
    SAMPLE=$(ps -o rss=,vsz=,%cpu= -p "$PID" 2>/dev/null | tr -s ' ' | sed 's/^ //')
    if [ -z "$SAMPLE" ]; then
        log "x0xd died at uptime ${UP}s"
        echo "$(date -u +%Y-%m-%dT%H:%M:%SZ),$UP,DIED,DIED,DIED" >> "$RSS_CSV"
        exit 1
    fi
    RSS=$(echo "$SAMPLE" | awk '{print $1}')
    VSZ=$(echo "$SAMPLE" | awk '{print $2}')
    CPU=$(echo "$SAMPLE" | awk '{print $3}')

    echo "$(date -u +%Y-%m-%dT%H:%M:%SZ),$UP,$RSS,$VSZ,$CPU" >> "$RSS_CSV"
    log "uptime=${UP}s rss=$((RSS / 1024))MB vsz=$((VSZ / 1024))MB cpu=${CPU}%"

    sleep 30
done

# Summary: first vs last RSS, slope.
FIRST=$(awk -F, 'NR==2{print $3}' "$RSS_CSV")
LAST=$(awk -F, 'END{print $3}' "$RSS_CSV")
DELTA=$((LAST - FIRST))
log "RSS first=${FIRST}KB ($((FIRST / 1024))MB)"
log "RSS last=${LAST}KB ($((LAST / 1024))MB)"
log "RSS delta=${DELTA}KB ($((DELTA / 1024))MB) over ${DURATION_MIN}min"

# Verdict gate. Anything > 20 MB growth in 10 min idle is suspicious for
# a daemon that should be sitting on cached state only.
THRESHOLD_KB=$((20 * 1024))
if (( DELTA > THRESHOLD_KB )); then
    log "VERDICT: idle leak observed (delta > 20 MB in ${DURATION_MIN}min)"
    exit 0  # not a script failure — this is the finding we want
else
    log "VERDICT: idle RSS bounded (delta < 20 MB) — leak likely needs gossip traffic"
    exit 0
fi
