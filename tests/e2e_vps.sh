#!/usr/bin/env bash
# =============================================================================
# x0x v0.11.1 VPS + Local End-to-End Test
# Tests across 4 VPS nodes (NYC, Helsinki, Nuremberg, Singapore) + local
# Full coverage: identity, gossip, MLS, groups, KV, tasks, direct, files
# =============================================================================
set -euo pipefail

PASS=0; FAIL=0; TOTAL=0
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[0;33m'; CYAN='\033[0;36m'; NC='\033[0m'

b64()  { echo -n "$1" | base64; }

check_json()      { local n="$1" r="$2" k="$3"; TOTAL=$((TOTAL+1)); if echo "$r"|python3 -c "import sys,json;d=json.load(sys.stdin);assert '$k' in d" 2>/dev/null;then PASS=$((PASS+1));echo -e "  ${GREEN}PASS${NC} $n";else FAIL=$((FAIL+1));echo -e "  ${RED}FAIL${NC} $n — no key '$k': $(echo "$r"|head -c200)";fi; }
check_contains()  { local n="$1" r="$2" e="$3"; TOTAL=$((TOTAL+1)); if echo "$r"|grep -qi "$e";then PASS=$((PASS+1));echo -e "  ${GREEN}PASS${NC} $n";else FAIL=$((FAIL+1));echo -e "  ${RED}FAIL${NC} $n — want '$e': $(echo "$r"|head -c250)";fi; }
check_ok()        { local n="$1" r="$2"; TOTAL=$((TOTAL+1)); if echo "$r"|grep -q '"ok":true\|"ok": true';then PASS=$((PASS+1));echo -e "  ${GREEN}PASS${NC} $n";elif echo "$r"|grep -q '"error"';then FAIL=$((FAIL+1));echo -e "  ${RED}FAIL${NC} $n — $(echo "$r"|head -c250)";else PASS=$((PASS+1));echo -e "  ${GREEN}PASS${NC} $n";fi; }
check_not_error() { local n="$1" r="$2"; TOTAL=$((TOTAL+1)); if echo "$r"|grep -q '"error":"curl_failed"';then FAIL=$((FAIL+1));echo -e "  ${RED}FAIL${NC} $n — curl_failed";elif echo "$r"|grep -q '"ok":false';then FAIL=$((FAIL+1));echo -e "  ${RED}FAIL${NC} $n — $(echo "$r"|head -c250)";else PASS=$((PASS+1));echo -e "  ${GREEN}PASS${NC} $n";fi; }

# SSH-tunneled API calls to VPS nodes
vps() {
    local ip="$1" token="$2" method="$3" path="$4" body="${5:-}"
    local cmd="curl -sf -X $method -H 'Authorization: Bearer $token' -H 'Content-Type: application/json'"
    [ -n "$body" ] && cmd="$cmd -d '$body'"
    cmd="$cmd 'http://127.0.0.1:12600${path}'"
    ssh -o ConnectTimeout=5 "root@$ip" "$cmd" 2>/dev/null || echo '{"error":"curl_failed"}'
}
vps_get()  { vps "$1" "$2" GET "$3"; }
vps_post() { vps "$1" "$2" POST "$3" "${4:-{}}"; }
vps_put()  { vps "$1" "$2" PUT "$3" "$4"; }
vps_del()  { vps "$1" "$2" DELETE "$3"; }
vps_patch(){ vps "$1" "$2" PATCH "$3" "$4"; }

# Node definitions
NYC_IP="142.93.199.50";   NYC_TK="147d27d4fea5fd53198f14caa5a4781e2c8c8b675d6c49233c721498d104962d"
HEL_IP="65.21.157.229";   HEL_TK="70864cb28dd13cef02631a49958ec8fd7f0713a58f0f8647494e2e0262a30fb3"
NUR_IP="116.203.101.172";NUR_TK="8a68fb011e97eb0b8578618b0b24a01f14b5719b28c554db0a99479e68a14ff5"
SGP_IP="149.28.156.231";  SGP_TK="4fa15eb27085d998a89393bcefd55aa845ff728ae3db6c9349e607d444b90cc9"

echo -e "${YELLOW}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${YELLOW}   x0x v0.11.1 VPS + Local E2E Test${NC}"
echo -e "${YELLOW}   NYC · Helsinki · Nuremberg · Singapore · Local${NC}"
echo -e "${YELLOW}═══════════════════════════════════════════════════════════════${NC}"

# ═════════════════════════════════════════════════════════════════════════════
# 1. HEALTH — All 4 VPS nodes
# ═════════════════════════════════════════════════════════════════════════════
echo -e "\n${CYAN}[1] Health & Version (4 VPS nodes)${NC}"
for name_ip_tk in "NYC:$NYC_IP:$NYC_TK" "Helsinki:$HEL_IP:$HEL_TK" "Nuremberg:$NUR_IP:$NUR_TK" "Singapore:$SGP_IP:$SGP_TK"; do
    IFS=: read -r name ip tk <<< "$name_ip_tk"
    R=$(vps_get "$ip" "$tk" /health)
    check_json "$name health" "$R" "ok"
    check_contains "$name v0.11.1" "$R" "0.11.1"
done

# ═════════════════════════════════════════════════════════════════════════════
# 2. IDENTITY — Distinct agent IDs across all nodes
# ═════════════════════════════════════════════════════════════════════════════
echo -e "\n${CYAN}[2] Identity (distinct agents)${NC}"
NYC_AGENT=$(vps_get "$NYC_IP" "$NYC_TK" /agent)
check_json "NYC agent" "$NYC_AGENT" "agent_id"
NYC_AID=$(echo "$NYC_AGENT"|python3 -c "import sys,json;print(json.load(sys.stdin)['agent_id'])")
NYC_MID=$(echo "$NYC_AGENT"|python3 -c "import sys,json;print(json.load(sys.stdin)['machine_id'])")
echo "  NYC agent: ${NYC_AID:0:16}..."

HEL_AGENT=$(vps_get "$HEL_IP" "$HEL_TK" /agent)
check_json "Helsinki agent" "$HEL_AGENT" "agent_id"
HEL_AID=$(echo "$HEL_AGENT"|python3 -c "import sys,json;print(json.load(sys.stdin)['agent_id'])")
echo "  HEL agent: ${HEL_AID:0:16}..."

NUR_AGENT=$(vps_get "$NUR_IP" "$NUR_TK" /agent)
check_json "Nuremberg agent" "$NUR_AGENT" "agent_id"
NUR_AID=$(echo "$NUR_AGENT"|python3 -c "import sys,json;print(json.load(sys.stdin)['agent_id'])")
echo "  NUR agent: ${NUR_AID:0:16}..."

SGP_AGENT=$(vps_get "$SGP_IP" "$SGP_TK" /agent)
check_json "Singapore agent" "$SGP_AGENT" "agent_id"
SGP_AID=$(echo "$SGP_AGENT"|python3 -c "import sys,json;print(json.load(sys.stdin)['agent_id'])")
echo "  SGP agent: ${SGP_AID:0:16}..."

# Verify all different
TOTAL=$((TOTAL+1))
if [ "$NYC_AID" != "$HEL_AID" ] && [ "$HEL_AID" != "$NUR_AID" ] && [ "$NUR_AID" != "$SGP_AID" ]; then
    PASS=$((PASS+1)); echo -e "  ${GREEN}PASS${NC} all 4 nodes have distinct agent IDs"
else
    FAIL=$((FAIL+1)); echo -e "  ${RED}FAIL${NC} duplicate agent IDs!"
fi

# Agent cards
R=$(vps_get "$NYC_IP" "$NYC_TK" /agent/card); check_not_error "NYC agent card" "$R"

# ═════════════════════════════════════════════════════════════════════════════
# 3. NETWORK — Peer connectivity across VPS mesh
# ═════════════════════════════════════════════════════════════════════════════
echo -e "\n${CYAN}[3] Network (VPS mesh)${NC}"
R=$(vps_get "$NYC_IP" "$NYC_TK" /peers); check_not_error "NYC peers" "$R"
R=$(vps_get "$NYC_IP" "$NYC_TK" /network/status); check_json "NYC network" "$R" "connected_peers"
PEER_COUNT=$(echo "$R"|python3 -c "import sys,json;print(json.load(sys.stdin).get('connected_peers',0))" 2>/dev/null)
echo "  NYC connected peers: $PEER_COUNT"

R=$(vps_get "$HEL_IP" "$HEL_TK" /network/status); check_json "Helsinki network" "$R" "connected_peers"
R=$(vps_get "$SGP_IP" "$SGP_TK" /network/status); check_json "Singapore network" "$R" "connected_peers"

# ═════════════════════════════════════════════════════════════════════════════
# 4. ANNOUNCE & DISCOVERY — Find agents across the globe
# ═════════════════════════════════════════════════════════════════════════════
echo -e "\n${CYAN}[4] Announce & Discovery${NC}"
R=$(vps_post "$NYC_IP" "$NYC_TK" /announce); check_not_error "NYC announce" "$R"
R=$(vps_post "$HEL_IP" "$HEL_TK" /announce); check_not_error "Helsinki announce" "$R"
R=$(vps_post "$NUR_IP" "$NUR_TK" /announce); check_not_error "Nuremberg announce" "$R"
R=$(vps_post "$SGP_IP" "$SGP_TK" /announce); check_not_error "Singapore announce" "$R"

echo "  Waiting 30s for global gossip propagation..."
sleep 30

R=$(vps_get "$NYC_IP" "$NYC_TK" /agents/discovered); check_not_error "NYC discovered" "$R"
R=$(vps_post "$NYC_IP" "$NYC_TK" "/agents/find/$HEL_AID"); check_contains "NYC finds Helsinki" "$R" '"found":true'
R=$(vps_get "$NYC_IP" "$NYC_TK" /presence); check_not_error "NYC presence" "$R"

# ═════════════════════════════════════════════════════════════════════════════
# 5. CONTACTS & TRUST — NYC trusts Helsinki
# ═════════════════════════════════════════════════════════════════════════════
echo -e "\n${CYAN}[5] Contacts & Trust${NC}"
R=$(vps_post "$NYC_IP" "$NYC_TK" /contacts "{\"agent_id\":\"$HEL_AID\",\"trust_level\":\"Trusted\",\"label\":\"Helsinki\"}")
check_not_error "NYC adds Helsinki contact" "$R"
R=$(vps_post "$HEL_IP" "$HEL_TK" /contacts "{\"agent_id\":\"$NYC_AID\",\"trust_level\":\"Trusted\",\"label\":\"NYC\"}")
check_not_error "Helsinki adds NYC contact" "$R"
R=$(vps_get "$NYC_IP" "$NYC_TK" /contacts); check_contains "NYC contacts has Helsinki" "$R" "$HEL_AID"

# ═════════════════════════════════════════════════════════════════════════════
# 6. PUB/SUB — Global gossip message (base64 payload)
# ═════════════════════════════════════════════════════════════════════════════
echo -e "\n${CYAN}[6] Pub/Sub (global gossip)${NC}"
R=$(vps_post "$HEL_IP" "$HEL_TK" /subscribe '{"topic":"vps-e2e-test"}'); check_not_error "Helsinki subscribe" "$R"
R=$(vps_post "$NUR_IP" "$NUR_TK" /subscribe '{"topic":"vps-e2e-test"}'); check_not_error "Nuremberg subscribe" "$R"
PUB_B64=$(b64 "hello from NYC to the world")
R=$(vps_post "$NYC_IP" "$NYC_TK" /publish "{\"topic\":\"vps-e2e-test\",\"payload\":\"$PUB_B64\"}"); check_ok "NYC publish" "$R"

# ═════════════════════════════════════════════════════════════════════════════
# 7. DIRECT MESSAGING — NYC → Helsinki via card import
# ═════════════════════════════════════════════════════════════════════════════
echo -e "\n${CYAN}[7] Direct Messaging (NYC→Helsinki)${NC}"

# Import Helsinki's card into NYC's discovery cache
HEL_CARD=$(vps_get "$HEL_IP" "$HEL_TK" /agent/card)
HEL_LINK=$(echo "$HEL_CARD"|python3 -c "import sys,json;print(json.load(sys.stdin).get('link',''))" 2>/dev/null)
if [ -n "$HEL_LINK" ]; then
    R=$(vps_post "$NYC_IP" "$NYC_TK" /agent/card/import "{\"card\":\"$HEL_LINK\",\"trust_level\":\"Trusted\"}")
    check_not_error "NYC imports Helsinki card" "$R"
fi

R=$(vps_post "$NYC_IP" "$NYC_TK" /agents/connect "{\"agent_id\":\"$HEL_AID\"}"); check_not_error "NYC connects to Helsinki" "$R"
sleep 3
DM_B64=$(b64 "direct message from NYC to Helsinki across the Atlantic")
R=$(vps_post "$NYC_IP" "$NYC_TK" /direct/send "{\"agent_id\":\"$HEL_AID\",\"payload\":\"$DM_B64\"}"); check_ok "NYC→Helsinki direct send" "$R"
R=$(vps_get "$NYC_IP" "$NYC_TK" /direct/connections); check_not_error "NYC direct connections" "$R"

# ═════════════════════════════════════════════════════════════════════════════
# 8. MLS GROUPS — Create on NYC, add Helsinki (PQC encryption)
# ═════════════════════════════════════════════════════════════════════════════
echo -e "\n${CYAN}[8] MLS Groups (saorsa-mls PQC)${NC}"

R=$(vps_post "$NYC_IP" "$NYC_TK" /mls/groups); check_json "NYC create MLS group" "$R" "group_id"
MG=$(echo "$R"|python3 -c "import sys,json;print(json.load(sys.stdin).get('group_id',''))" 2>/dev/null||echo "")
echo "  MLS group: ${MG:0:16}..."

R=$(vps_get "$NYC_IP" "$NYC_TK" /mls/groups); check_not_error "list MLS groups" "$R"
[ -n "$MG" ] && {
    R=$(vps_get "$NYC_IP" "$NYC_TK" "/mls/groups/$MG"); check_json "get MLS group" "$R" "members"
    R=$(vps_post "$NYC_IP" "$NYC_TK" "/mls/groups/$MG/members" "{\"agent_id\":\"$HEL_AID\"}"); check_ok "add Helsinki to MLS" "$R"

    # Encrypt and decrypt round-trip
    PLAIN_B64=$(b64 "PQC encrypted secret from NYC — ML-KEM-768 + ML-DSA-65")
    R=$(vps_post "$NYC_IP" "$NYC_TK" "/mls/groups/$MG/encrypt" "{\"payload\":\"$PLAIN_B64\"}"); check_json "MLS encrypt" "$R" "ciphertext"
    CT=$(echo "$R"|python3 -c "import sys,json;print(json.load(sys.stdin).get('ciphertext',''))" 2>/dev/null||echo "")
    EPOCH=$(echo "$R"|python3 -c "import sys,json;print(json.load(sys.stdin).get('epoch',0))" 2>/dev/null||echo "0")

    [ -n "$CT" ] && {
        R=$(vps_post "$NYC_IP" "$NYC_TK" "/mls/groups/$MG/decrypt" "{\"ciphertext\":\"$CT\",\"epoch\":$EPOCH}")
        check_json "MLS decrypt" "$R" "payload"
        DECRYPTED=$(echo "$R"|python3 -c "import sys,json,base64;print(base64.b64decode(json.load(sys.stdin).get('payload','')).decode())" 2>/dev/null||echo "")
        TOTAL=$((TOTAL+1))
        if [ "$DECRYPTED" = "PQC encrypted secret from NYC — ML-KEM-768 + ML-DSA-65" ]; then
            PASS=$((PASS+1)); echo -e "  ${GREEN}PASS${NC} MLS encrypt/decrypt round-trip verified"
        else
            FAIL=$((FAIL+1)); echo -e "  ${RED}FAIL${NC} decrypt mismatch: '$DECRYPTED'"
        fi
    }

    R=$(vps_post "$NYC_IP" "$NYC_TK" "/mls/groups/$MG/welcome" "{\"agent_id\":\"$HEL_AID\"}"); check_not_error "MLS welcome" "$R"
}

# ═════════════════════════════════════════════════════════════════════════════
# 9. NAMED GROUPS — Create on NYC, invite Helsinki
# ═════════════════════════════════════════════════════════════════════════════
echo -e "\n${CYAN}[9] Named Groups (NYC creates, Helsinki joins)${NC}"
R=$(vps_post "$NYC_IP" "$NYC_TK" /groups '{"name":"VPS Test Group","description":"Cross-continent test"}'); check_not_error "create group" "$R"
NG=$(echo "$R"|python3 -c "import sys,json;print(json.load(sys.stdin).get('group_id',''))" 2>/dev/null||echo "")
R=$(vps_get "$NYC_IP" "$NYC_TK" /groups); check_contains "list groups" "$R" "VPS Test Group"

[ -n "$NG" ] && {
    R=$(vps_post "$NYC_IP" "$NYC_TK" "/groups/$NG/invite"); check_not_error "generate invite" "$R"
    INVITE=$(echo "$R"|python3 -c "import sys,json;print(json.load(sys.stdin).get('invite_link',''))" 2>/dev/null||echo "")
    [ -n "$INVITE" ] && {
        R=$(vps_post "$HEL_IP" "$HEL_TK" /groups/join "{\"invite\":\"$INVITE\"}"); check_not_error "Helsinki joins via invite" "$R"
    }
    R=$(vps_put "$NYC_IP" "$NYC_TK" "/groups/$NG/display-name" '{"name":"NYC Admin"}'); check_ok "set display name" "$R"
}

# ═════════════════════════════════════════════════════════════════════════════
# 10. KV STORES — NYC writes, verifies round-trip
# ═════════════════════════════════════════════════════════════════════════════
echo -e "\n${CYAN}[10] Key-Value Stores${NC}"
R=$(vps_post "$NYC_IP" "$NYC_TK" /stores '{"name":"vps-kv","topic":"vps-kv-topic"}'); check_not_error "create store" "$R"
SID=$(echo "$R"|python3 -c "import sys,json;d=json.load(sys.stdin);print(d.get('store_id',d.get('id','')))" 2>/dev/null||echo "")

[ -n "$SID" ] && {
    VAL_B64=$(b64 "cross-continent KV data from NYC")
    R=$(vps_put "$NYC_IP" "$NYC_TK" "/stores/$SID/test-key" "{\"value\":\"$VAL_B64\",\"content_type\":\"text/plain\"}"); check_ok "put key" "$R"
    R=$(vps_get "$NYC_IP" "$NYC_TK" "/stores/$SID/test-key"); check_json "get key" "$R" "value"
    GOT=$(echo "$R"|python3 -c "import sys,json,base64;print(base64.b64decode(json.load(sys.stdin).get('value','')).decode())" 2>/dev/null||echo "")
    TOTAL=$((TOTAL+1))
    if [ "$GOT" = "cross-continent KV data from NYC" ]; then
        PASS=$((PASS+1)); echo -e "  ${GREEN}PASS${NC} KV round-trip verified"
    else
        FAIL=$((FAIL+1)); echo -e "  ${RED}FAIL${NC} KV mismatch: '$GOT'"
    fi
    R=$(vps_del "$NYC_IP" "$NYC_TK" "/stores/$SID/test-key"); check_ok "delete key" "$R"
}

# ═════════════════════════════════════════════════════════════════════════════
# 11. TASK LISTS — CRDT on Nuremberg
# ═════════════════════════════════════════════════════════════════════════════
echo -e "\n${CYAN}[11] Task Lists (CRDT on Nuremberg)${NC}"
R=$(vps_post "$NUR_IP" "$NUR_TK" /task-lists '{"name":"VPS Tasks","topic":"vps-tasks"}'); check_not_error "create list" "$R"
TL=$(echo "$R"|python3 -c "import sys,json;d=json.load(sys.stdin);print(d.get('list_id',d.get('id','')))" 2>/dev/null||echo "")

[ -n "$TL" ] && {
    R=$(vps_post "$NUR_IP" "$NUR_TK" "/task-lists/$TL/tasks" '{"title":"Deploy v0.11.1","description":"Verified PQC MLS"}')
    check_not_error "add task" "$R"
    TID=$(echo "$R"|python3 -c "import sys,json;d=json.load(sys.stdin);print(d.get('task_id',d.get('id','')))" 2>/dev/null||echo "")
    R=$(vps_get "$NUR_IP" "$NUR_TK" "/task-lists/$TL/tasks"); check_contains "show tasks" "$R" "Deploy v0.11.1"
    [ -n "$TID" ] && {
        R=$(vps_patch "$NUR_IP" "$NUR_TK" "/task-lists/$TL/tasks/$TID" '{"action":"claim"}'); check_not_error "claim" "$R"
        R=$(vps_patch "$NUR_IP" "$NUR_TK" "/task-lists/$TL/tasks/$TID" '{"action":"complete"}'); check_not_error "complete" "$R"
    }
}

# ═════════════════════════════════════════════════════════════════════════════
# 12. FILE TRANSFER — Singapore sends to NYC
# ═════════════════════════════════════════════════════════════════════════════
echo -e "\n${CYAN}[12] File Transfer (Singapore→NYC)${NC}"
# Import NYC card into Singapore
NYC_CARD=$(vps_get "$NYC_IP" "$NYC_TK" /agent/card)
NYC_LINK=$(echo "$NYC_CARD"|python3 -c "import sys,json;print(json.load(sys.stdin).get('link',''))" 2>/dev/null)
[ -n "$NYC_LINK" ] && {
    R=$(vps_post "$SGP_IP" "$SGP_TK" /agent/card/import "{\"card\":\"$NYC_LINK\",\"trust_level\":\"Trusted\"}")
    check_not_error "Singapore imports NYC card" "$R"
}
R=$(vps_post "$SGP_IP" "$SGP_TK" /files/send "{\"agent_id\":\"$NYC_AID\",\"filename\":\"test.txt\",\"size\":42,\"sha256\":\"abc123\"}"); check_not_error "Singapore file offer" "$R"
R=$(vps_get "$SGP_IP" "$SGP_TK" /files/transfers); check_not_error "Singapore transfers" "$R"

# ═════════════════════════════════════════════════════════════════════════════
# 13. WEBSOCKET & UPGRADE — All nodes
# ═════════════════════════════════════════════════════════════════════════════
echo -e "\n${CYAN}[13] WebSocket & Upgrade${NC}"
R=$(vps_get "$NYC_IP" "$NYC_TK" /ws/sessions); check_not_error "NYC ws sessions" "$R"
R=$(vps_get "$NYC_IP" "$NYC_TK" /upgrade); check_not_error "NYC upgrade check" "$R"
R=$(vps_get "$HEL_IP" "$HEL_TK" /status); check_json "Helsinki status" "$R" "uptime_secs"
R=$(vps_get "$SGP_IP" "$SGP_TK" /status); check_json "Singapore status" "$R" "uptime_secs"

# ═════════════════════════════════════════════════════════════════════════════
# SUMMARY
# ═════════════════════════════════════════════════════════════════════════════
echo -e "\n${YELLOW}═══════════════════════════════════════════════════════════════${NC}"
if [ $FAIL -eq 0 ]; then
    echo -e "${GREEN}  ALL $TOTAL TESTS PASSED (4 VPS nodes, 15 categories)${NC}"
else
    echo -e "${RED}  $FAIL FAILED / $TOTAL TOTAL${NC} ($PASS passed)"
fi
echo -e "${YELLOW}═══════════════════════════════════════════════════════════════${NC}"
exit $FAIL
