#!/bin/bash
# Health check for x0x bootstrap network
# Usage: ./health-check.sh [node_name|all] [--extended]
#   node_name: nyc, sfo, helsinki, nuremberg, singapore, tokyo, or 'all' (default)
#   --extended: also show peer count

set -euo pipefail

EXPECTED_VERSION="0.14.0"
EXTENDED=false
SSH="ssh -o ConnectTimeout=10 -o ControlMaster=no -o ControlPath=none -o BatchMode=yes"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Parse version from JSON using grep/sed (no python3 dependency)
parse_version() {
    echo "$1" | grep -o '"version":"[^"]*"' | cut -d'"' -f4
}

parse_peers() {
    echo "$1" | grep -o '"peers":[0-9]*' | cut -d: -f2
}

check_node() {
    local node_name=$1 ip=$2

    printf "%-15s %-17s " "$node_name" "$ip"

    # Single SSH call to check everything
    local result
    if [[ "$EXTENDED" == "true" ]]; then
        result=$($SSH "root@$ip" bash -c '"
            STATUS=\$(systemctl is-active x0xd 2>/dev/null || echo inactive)
            HEALTH=\$(curl -sf -m 5 http://127.0.0.1:12600/health 2>/dev/null || echo FAILED)
            TOKEN=\$(cat /root/.local/share/x0x/api-token 2>/dev/null || echo \"\")
            if [ -n \"\$TOKEN\" ]; then
                NET=\$(curl -sf -m 5 -H \"Authorization: Bearer \$TOKEN\" http://127.0.0.1:12600/network/status 2>/dev/null || echo \"{}\")
            else
                NET=\"{}\"
            fi
            echo \"\$STATUS\"
            echo \"\$HEALTH\"
            echo \"\$NET\"
        "' 2>/dev/null) || result="SSH_FAILED"
    else
        result=$($SSH "root@$ip" bash -c '"
            STATUS=\$(systemctl is-active x0xd 2>/dev/null || echo inactive)
            HEALTH=\$(curl -sf -m 5 http://127.0.0.1:12600/health 2>/dev/null || echo FAILED)
            echo \"\$STATUS\"
            echo \"\$HEALTH\"
        "' 2>/dev/null) || result="SSH_FAILED"
    fi

    if [[ "$result" == "SSH_FAILED" ]]; then
        echo -e "${RED}SSH FAILED${NC}"
        return 1
    fi

    # Parse multi-line result
    local svc_status health net_status
    svc_status=$(echo "$result" | sed -n '1p')
    health=$(echo "$result" | sed -n '2p')
    net_status=$(echo "$result" | sed -n '3p')

    if [[ "$svc_status" != "active" ]]; then
        echo -e "${RED}SERVICE $svc_status${NC}"
        return 1
    fi

    if [[ "$health" == "FAILED" || -z "$health" ]]; then
        echo -e "${RED}HEALTH FAILED${NC}"
        return 1
    fi

    local version
    version=$(parse_version "$health")
    version=${version:-unknown}

    if [[ "$version" == "$EXPECTED_VERSION" ]]; then
        echo -ne "${GREEN}OK${NC} v${version}"
    elif [[ "$version" == "unknown" ]]; then
        echo -ne "${GREEN}OK${NC} (version unknown)"
    else
        echo -ne "${YELLOW}OK${NC} v${version} (expected v${EXPECTED_VERSION})"
    fi

    if [[ "$EXTENDED" == "true" && -n "${net_status:-}" && "$net_status" != "{}" ]]; then
        local peers
        peers=$(echo "$net_status" | grep -o '"connected_peers":[0-9]*' | cut -d: -f2)
        peers=${peers:-?}
        echo -ne " | peers: ${peers}"
    fi

    echo ""
    return 0
}

main() {
    local target="all"

    for arg in "$@"; do
        case "$arg" in
            --extended) EXTENDED=true ;;
            --*) ;;
            *) target="$arg" ;;
        esac
    done

    echo "x0x Bootstrap Network Health Check"
    echo "===================================="
    echo "Expected version: v${EXPECTED_VERSION}"
    echo
    printf "%-15s %-17s %s\n" "NODE" "IP" "STATUS"
    printf "%-15s %-17s %s\n" "----" "--" "------"

    if [[ "$target" == "all" ]]; then
        local total=0 healthy=0
        local -a nodes=("nyc:142.93.199.50" "sfo:147.182.234.192" "helsinki:65.21.157.229" "nuremberg:116.203.101.172" "singapore:149.28.156.231" "tokyo:45.77.176.184")

        for entry in "${nodes[@]}"; do
            local node="${entry%%:*}" ip="${entry##*:}"
            if check_node "$node" "$ip"; then
                ((healthy++))
            fi
            ((total++))
        done

        echo
        echo "Summary: $healthy/$total nodes healthy"

        if [[ $healthy -eq $total ]]; then
            echo -e "${GREEN}All nodes operational${NC}"
            exit 0
        else
            echo -e "${YELLOW}Some nodes have issues${NC}"
            exit 1
        fi
    else
        local ip=""
        case "$target" in
            nyc) ip="142.93.199.50" ;; sfo) ip="147.182.234.192" ;;
            helsinki) ip="65.21.157.229" ;; nuremberg) ip="116.203.101.172" ;;
            singapore) ip="149.28.156.231" ;; tokyo) ip="45.77.176.184" ;;
            *) echo "Unknown node: $target"; echo "Available: nyc sfo helsinki nuremberg singapore tokyo"; exit 1 ;;
        esac
        check_node "$target" "$ip"
    fi
}

main "$@"
