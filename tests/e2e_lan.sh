#!/usr/bin/env bash
# =============================================================================
# x0x LAN End-to-End Test Suite
# Tests mDNS discovery, direct messaging, presence, and connection sharing
# across Mac Studio machines on the local network (studio1.local, studio2.local)
# =============================================================================
set -euo pipefail

VERSION="$(grep '^version = ' Cargo.toml | head -1 | cut -d '"' -f2)"
BINARY="${X0XD:-$(pwd)/target/release/x0xd}"
PASS=0; FAIL=0; SKIP=0; TOTAL=0

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[0;33m'; CYAN='\033[0;36m'; NC='\033[0m'

# ── Studio configuration ────────────────────────────────────────────────
STUDIO1="studio1.local"
STUDIO2="studio2.local"
STUDIO_USER="${STUDIO_USER:-davidirvine}"
STUDIO_API_PORT=19501
STUDIO2_API_PORT=19502
STUDIO_BIND_PORT=19601
STUDIO2_BIND_PORT=19602
STUDIO_DATA_DIR="/tmp/x0x-e2e-lan"
SSH="ssh -o ConnectTimeout=10 -o ControlMaster=no -o ControlPath=none -o BatchMode=yes -o StrictHostKeyChecking=accept-new"

# ── Assertion helpers ───────────────────────────────────────────────────
b64() { echo -n "$1" | base64; }

check_json() {
    local n="$1" r="$2" k="$3"; TOTAL=$((TOTAL+1))
    if echo "$r"|python3 -c "import sys,json;d=json.load(sys.stdin);assert '$k' in d" 2>/dev/null; then
        PASS=$((PASS+1)); echo -e "  ${GREEN}PASS${NC} $n"
    else
        FAIL=$((FAIL+1)); echo -e "  ${RED}FAIL${NC} $n — no key '$k' in: $(echo "$r"|head -c200)"
    fi
}

check_contains() {
    local n="$1" r="$2" e="$3"; TOTAL=$((TOTAL+1))
    if echo "$r"|grep -qi "$e"; then
        PASS=$((PASS+1)); echo -e "  ${GREEN}PASS${NC} $n"
    else
        FAIL=$((FAIL+1)); echo -e "  ${RED}FAIL${NC} $n — want '$e' in: $(echo "$r"|head -c250)"
    fi
}

check_ok() {
    local n="$1" r="$2"; TOTAL=$((TOTAL+1))
    if echo "$r"|grep -q '"ok":true\|"ok": true'; then
        PASS=$((PASS+1)); echo -e "  ${GREEN}PASS${NC} $n"
    elif echo "$r"|grep -q '"error"'; then
        FAIL=$((FAIL+1)); echo -e "  ${RED}FAIL${NC} $n — $(echo "$r"|head -c250)"
    else
        PASS=$((PASS+1)); echo -e "  ${GREEN}PASS${NC} $n"
    fi
}

check_not_error() {
    local n="$1" r="$2"; TOTAL=$((TOTAL+1))
    if echo "$r"|grep -q '"error":"curl_failed"'; then
        FAIL=$((FAIL+1)); echo -e "  ${RED}FAIL${NC} $n — curl_failed (non-2xx)"
    elif echo "$r"|grep -q '"ok":false\|"ok": false'; then
        FAIL=$((FAIL+1)); echo -e "  ${RED}FAIL${NC} $n — $(echo "$r"|head -c250)"
    else
        PASS=$((PASS+1)); echo -e "  ${GREEN}PASS${NC} $n"
    fi
}

check_eq() {
    local n="$1" got="$2" want="$3"; TOTAL=$((TOTAL+1))
    if [ "$got" = "$want" ]; then
        PASS=$((PASS+1)); echo -e "  ${GREEN}PASS${NC} $n"
    else
        FAIL=$((FAIL+1)); echo -e "  ${RED}FAIL${NC} $n — got '$got', want '$want'"
    fi
}

skip() {
    local n="$1" reason="$2"
    TOTAL=$((TOTAL+1)); SKIP=$((SKIP+1))
    echo -e "  ${YELLOW}SKIP${NC} $n — $reason"
}

jq_field() { echo "$1" | python3 -c "import sys,json;d=json.load(sys.stdin);print(d.get('$2',''))" 2>/dev/null || echo ""; }

check_connect_outcome() {
    local n="$1" r="$2" outcome
    outcome=$(jq_field "$r" "outcome")
    TOTAL=$((TOTAL+1))
    case "$outcome" in
        Direct|Coordinated|AlreadyConnected)
            PASS=$((PASS+1)); echo -e "  ${GREEN}PASS${NC} $n ($outcome)"; return 0 ;;
        Unreachable|NotFound|"")
            FAIL=$((FAIL+1)); echo -e "  ${RED}FAIL${NC} $n — outcome=${outcome:-missing}: $(echo "$r"|head -c250)"; return 1 ;;
        *)
            PASS=$((PASS+1)); echo -e "  ${GREEN}PASS${NC} $n (outcome=$outcome)"; return 0 ;;
    esac
}

# ── Cleanup ─────────────────────────────────────────────────────────────
cleanup() {
    echo -e "\n${YELLOW}Cleaning up...${NC}"
    for host in "$STUDIO1" "$STUDIO2"; do
        $SSH "$STUDIO_USER@$host" "pkill -f 'x0xd.*e2e-lan' 2>/dev/null; rm -rf $STUDIO_DATA_DIR" 2>/dev/null || true
    done
}
trap cleanup EXIT

echo -e "${CYAN}╔═══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║      x0x LAN E2E Test Suite v${VERSION}                         ║${NC}"
echo -e "${CYAN}║      Studios: ${STUDIO1}, ${STUDIO2}                    ║${NC}"
echo -e "${CYAN}╚═══════════════════════════════════════════════════════════════╝${NC}"

# ═════════════════════════════════════════════════════════════════════════
# 0. PREREQUISITES
# ═════════════════════════════════════════════════════════════════════════
echo -e "\n${CYAN}[0/6] Prerequisites${NC}"

# Check SSH access
for host in "$STUDIO1" "$STUDIO2"; do
    if $SSH "$STUDIO_USER@$host" "echo ok" &>/dev/null; then
        TOTAL=$((TOTAL+1)); PASS=$((PASS+1))
        echo -e "  ${GREEN}PASS${NC} SSH to $host"
    else
        echo -e "  ${RED}FATAL${NC} Cannot SSH to $host — aborting"
        exit 1
    fi
done

# Check binary exists
if [ ! -f "$BINARY" ]; then
    echo -e "  ${RED}FATAL${NC} Binary not found: $BINARY — run 'cargo build --release' first"
    exit 1
fi
echo -e "  ${GREEN}OK${NC} Binary: $BINARY"

# ═════════════════════════════════════════════════════════════════════════
# 1. DEPLOY
# ═════════════════════════════════════════════════════════════════════════
echo -e "\n${CYAN}[1/6] Deploy x0xd to studios${NC}"

for host in "$STUDIO1" "$STUDIO2"; do
    # Kill any existing test instances and clean up
    $SSH "$STUDIO_USER@$host" "pkill -f 'x0xd.*e2e-lan' 2>/dev/null; rm -rf $STUDIO_DATA_DIR; mkdir -p $STUDIO_DATA_DIR" 2>/dev/null || true

    # Copy binary
    scp -q "$BINARY" "$STUDIO_USER@$host:$STUDIO_DATA_DIR/x0xd"
    $SSH "$STUDIO_USER@$host" "chmod +x $STUDIO_DATA_DIR/x0xd"
    echo -e "  ${GREEN}OK${NC} Deployed to $host"
done

# Start instances with NO bootstrap peers — rely on mDNS for discovery
echo "  Starting studio1 (no bootstrap)..."
$SSH "$STUDIO_USER@$STUDIO1" "cat > $STUDIO_DATA_DIR/config.toml << 'TOML'
instance_name = \"e2e-lan-studio1\"
data_dir = \"$STUDIO_DATA_DIR/data\"
bind_address = \"0.0.0.0:$STUDIO_BIND_PORT\"
api_address = \"127.0.0.1:$STUDIO_API_PORT\"
log_level = \"info\"
bootstrap_peers = []
TOML
$STUDIO_DATA_DIR/x0xd --config $STUDIO_DATA_DIR/config.toml &> $STUDIO_DATA_DIR/log &"

echo "  Starting studio2 (no bootstrap)..."
$SSH "$STUDIO_USER@$STUDIO2" "cat > $STUDIO_DATA_DIR/config.toml << 'TOML'
instance_name = \"e2e-lan-studio2\"
data_dir = \"$STUDIO_DATA_DIR/data\"
bind_address = \"0.0.0.0:$STUDIO2_BIND_PORT\"
api_address = \"127.0.0.1:$STUDIO2_API_PORT\"
log_level = \"info\"
bootstrap_peers = []
TOML
$STUDIO_DATA_DIR/x0xd --config $STUDIO_DATA_DIR/config.toml &> $STUDIO_DATA_DIR/log &"

# Wait for health
echo "  Waiting for health..."
for host in "$STUDIO1" "$STUDIO2"; do
    port=$STUDIO_API_PORT
    [ "$host" = "$STUDIO2" ] && port=$STUDIO2_API_PORT
    for i in $(seq 1 30); do
        if $SSH "$STUDIO_USER@$host" "curl -sf http://127.0.0.1:$port/health" &>/dev/null; then
            break
        fi
        sleep 1
    done
done

# Get tokens
S1_TK=$($SSH "$STUDIO_USER@$STUDIO1" "cat $STUDIO_DATA_DIR/data/api-token 2>/dev/null" || echo "")
S2_TK=$($SSH "$STUDIO_USER@$STUDIO2" "cat $STUDIO_DATA_DIR/data/api-token 2>/dev/null" || echo "")

# Health checks
S1() { $SSH "$STUDIO_USER@$STUDIO1" "curl -sf -m 10 -H 'Authorization: Bearer $S1_TK' 'http://127.0.0.1:$STUDIO_API_PORT$1'" 2>/dev/null || echo '{"error":"curl_failed"}'; }
S1p() { $SSH "$STUDIO_USER@$STUDIO1" "curl -sf -m 10 -X POST -H 'Authorization: Bearer $S1_TK' -H 'Content-Type: application/json' -d '$2' 'http://127.0.0.1:$STUDIO_API_PORT$1'" 2>/dev/null || echo '{"error":"curl_failed"}'; }
S2() { $SSH "$STUDIO_USER@$STUDIO2" "curl -sf -m 10 -H 'Authorization: Bearer $S2_TK' 'http://127.0.0.1:$STUDIO2_API_PORT$1'" 2>/dev/null || echo '{"error":"curl_failed"}'; }
S2p() { $SSH "$STUDIO_USER@$STUDIO2" "curl -sf -m 10 -X POST -H 'Authorization: Bearer $S2_TK' -H 'Content-Type: application/json' -d '$2' 'http://127.0.0.1:$STUDIO2_API_PORT$1'" 2>/dev/null || echo '{"error":"curl_failed"}'; }

R=$(S1 /health); check_json "studio1 health" "$R" "ok"
R=$(S2 /health); check_json "studio2 health" "$R" "ok"

# Get agent IDs
R=$(S1 /agent); S1_AID=$(jq_field "$R" "agent_id")
R=$(S2 /agent); S2_AID=$(jq_field "$R" "agent_id")
echo "  studio1 agent: ${S1_AID:0:16}..."
echo "  studio2 agent: ${S2_AID:0:16}..."

# ═════════════════════════════════════════════════════════════════════════
# 2. mDNS DISCOVERY
# ═════════════════════════════════════════════════════════════════════════
echo -e "\n${CYAN}[2/6] mDNS LAN Discovery (no bootstrap peers)${NC}"

echo "  Waiting up to 60s for mDNS discovery..."
FOUND=false
for i in $(seq 1 60); do
    R=$(S1 /agents/discovered)
    if echo "$R" | grep -q "$S2_AID"; then
        FOUND=true
        break
    fi
    sleep 1
done

TOTAL=$((TOTAL+1))
if $FOUND; then
    PASS=$((PASS+1)); echo -e "  ${GREEN}PASS${NC} studio1 discovered studio2 via mDNS"
else
    FAIL=$((FAIL+1)); echo -e "  ${RED}FAIL${NC} studio1 did not discover studio2 via mDNS within 60s"
fi

# Check reverse direction too
R=$(S2 /agents/discovered)
TOTAL=$((TOTAL+1))
if echo "$R" | grep -q "$S1_AID"; then
    PASS=$((PASS+1)); echo -e "  ${GREEN}PASS${NC} studio2 discovered studio1 via mDNS"
else
    FAIL=$((FAIL+1)); echo -e "  ${RED}FAIL${NC} studio2 did not discover studio1"
fi

# ═════════════════════════════════════════════════════════════════════════
# 3. DIRECT MESSAGING ACROSS LAN
# ═════════════════════════════════════════════════════════════════════════
echo -e "\n${CYAN}[3/6] Direct Messaging${NC}"

# Connect studio1 to studio2
R=$(S1p /agents/connect "{\"agent_id\":\"$S2_AID\"}"); check_connect_outcome "studio1 connect to studio2" "$R"

# Send message
DM_B64=$(echo -n "hello from studio1" | base64)
R=$(S1p /direct/send "{\"agent_id\":\"$S2_AID\",\"payload\":\"$DM_B64\"}"); check_ok "studio1 direct send" "$R"

# Check connections
R=$(S1 /direct/connections); check_json "studio1 connections" "$R" "connections"

# Reverse: studio2 sends to studio1
DM_B64=$(echo -n "hello from studio2" | base64)
R=$(S2p /direct/send "{\"agent_id\":\"$S1_AID\",\"payload\":\"$DM_B64\"}"); check_ok "studio2 direct send" "$R"

# ═════════════════════════════════════════════════════════════════════════
# 4. PRESENCE WITH REACHABILITY
# ═════════════════════════════════════════════════════════════════════════
echo -e "\n${CYAN}[4/6] Presence & Reachability${NC}"

# Wait for presence beacons to propagate
echo "  Waiting 35s for presence beacons..."
sleep 35

R=$(S1 /presence/online); check_json "studio1 presence online" "$R" "agents"

# Check studio1 sees studio2 in presence
TOTAL=$((TOTAL+1))
if echo "$R" | grep -q "$S2_AID"; then
    PASS=$((PASS+1)); echo -e "  ${GREEN}PASS${NC} studio2 appears in studio1 presence"
else
    FAIL=$((FAIL+1)); echo -e "  ${RED}FAIL${NC} studio2 not in studio1 presence"
fi

# ═════════════════════════════════════════════════════════════════════════
# 5. SEEDLESS BOOTSTRAP VIA LAN
# ═════════════════════════════════════════════════════════════════════════
echo -e "\n${CYAN}[5/6] Seedless Bootstrap via LAN${NC}"

# Start a THIRD instance on studio2 with different ports — no bootstrap, should find network via mDNS
STUDIO2_INST2_API=19503
STUDIO2_INST2_BIND=19603
$SSH "$STUDIO_USER@$STUDIO2" "mkdir -p $STUDIO_DATA_DIR/data2 && cat > $STUDIO_DATA_DIR/config2.toml << 'TOML'
instance_name = \"e2e-lan-studio2-b\"
data_dir = \"$STUDIO_DATA_DIR/data2\"
bind_address = \"0.0.0.0:$STUDIO2_INST2_BIND\"
api_address = \"127.0.0.1:$STUDIO2_INST2_API\"
log_level = \"info\"
bootstrap_peers = []
TOML
$STUDIO_DATA_DIR/x0xd --config $STUDIO_DATA_DIR/config2.toml &> $STUDIO_DATA_DIR/log2 &"

# Wait for health
for i in $(seq 1 30); do
    if $SSH "$STUDIO_USER@$STUDIO2" "curl -sf http://127.0.0.1:$STUDIO2_INST2_API/health" &>/dev/null; then
        break
    fi
    sleep 1
done

S2B_TK=$($SSH "$STUDIO_USER@$STUDIO2" "cat $STUDIO_DATA_DIR/data2/api-token 2>/dev/null" || echo "")
S2B() { $SSH "$STUDIO_USER@$STUDIO2" "curl -sf -m 10 -H 'Authorization: Bearer $S2B_TK' 'http://127.0.0.1:$STUDIO2_INST2_API$1'" 2>/dev/null || echo '{"error":"curl_failed"}'; }

R=$(S2B /health); check_json "studio2-b health" "$R" "ok"

# Wait for mDNS to propagate
echo "  Waiting 45s for seedless instance to discover network via mDNS..."
sleep 45

# Check if the third instance found studio1 (on a different machine)
R=$(S2B /agents/discovered)
TOTAL=$((TOTAL+1))
if echo "$R" | grep -q "$S1_AID"; then
    PASS=$((PASS+1)); echo -e "  ${GREEN}PASS${NC} seedless studio2-b discovered studio1 via gossip/mDNS"
else
    FAIL=$((FAIL+1)); echo -e "  ${RED}FAIL${NC} seedless studio2-b did not discover studio1"
fi

# ═════════════════════════════════════════════════════════════════════════
# 6. SUMMARY
# ═════════════════════════════════════════════════════════════════════════
echo -e "\n${YELLOW}═══════════════════════════════════════════════════════════════${NC}"
if [ $FAIL -eq 0 ]; then
    echo -e "${GREEN}  ALL $TOTAL TESTS PASSED ($PASS passed, $SKIP skipped)${NC}"
else
    echo -e "${RED}  $FAIL FAILED${NC} / $TOTAL TOTAL ($PASS passed, $SKIP skipped)"
    echo ""
    echo "studio1 log tail:"
    $SSH "$STUDIO_USER@$STUDIO1" "tail -20 $STUDIO_DATA_DIR/log" 2>/dev/null || true
    echo "studio2 log tail:"
    $SSH "$STUDIO_USER@$STUDIO2" "tail -20 $STUDIO_DATA_DIR/log" 2>/dev/null || true
fi
echo -e "${YELLOW}═══════════════════════════════════════════════════════════════${NC}"

exit $FAIL
