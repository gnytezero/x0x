#!/usr/bin/env sh
# x0x quick installer — one command, running daemon.
#
# Usage:
#   curl -sfL https://x0x.md | sh
#   curl -sfL https://x0x.md | sh -s -- --name alice
#
# What it does:
#   1. Detects your platform (Linux/macOS, x64/arm64)
#   2. Downloads the latest release from GitHub
#   3. Installs x0xd + x0x CLI to ~/.local/bin
#   4. Starts the daemon
#   5. Waits for healthy
#   6. Prints your agent ID
#
# Requirements: curl or wget, tar, sh
# No root/sudo required.

set -e

REPO="saorsa-labs/x0x"
URL="https://github.com/$REPO/releases/latest/download"
BIN="$HOME/.local/bin"
NAME=""

# Parse args
for arg in "$@"; do
    case "$arg" in
        --name) shift; NAME="$1"; shift ;;
        --name=*) NAME="${arg#*=}" ;;
    esac
done

# Detect platform
OS=$(uname -s)
ARCH=$(uname -m)
case "$OS" in
    Linux)
        case "$ARCH" in
            x86_64)  PLATFORM="linux-x64-gnu" ;;
            aarch64) PLATFORM="linux-arm64-gnu" ;;
            *) echo "Unsupported: $OS $ARCH"; exit 1 ;;
        esac ;;
    Darwin)
        case "$ARCH" in
            arm64)  PLATFORM="macos-arm64" ;;
            x86_64) PLATFORM="macos-x64" ;;
            *) echo "Unsupported: $OS $ARCH"; exit 1 ;;
        esac ;;
    *) echo "Unsupported: $OS"; exit 1 ;;
esac

echo "x0x installer"
echo "Platform: $PLATFORM"

# Download and extract
ARCHIVE="x0x-${PLATFORM}.tar.gz"
TMP=$(mktemp -d)
echo "Downloading $ARCHIVE..."
if command -v curl >/dev/null 2>&1; then
    curl -sfL "$URL/$ARCHIVE" -o "$TMP/$ARCHIVE"
elif command -v wget >/dev/null 2>&1; then
    wget -qO "$TMP/$ARCHIVE" "$URL/$ARCHIVE"
else
    echo "Error: need curl or wget"; exit 1
fi

echo "Installing to $BIN..."
mkdir -p "$BIN"
tar -xzf "$TMP/$ARCHIVE" -C "$TMP"
for bin in x0xd x0x x0x-bootstrap; do
    if [ -f "$TMP/x0x-${PLATFORM}/$bin" ]; then
        cp "$TMP/x0x-${PLATFORM}/$bin" "$BIN/$bin"
        chmod +x "$BIN/$bin"
    fi
done
rm -rf "$TMP"

# Check PATH
case ":$PATH:" in
    *":$BIN:"*) ;;
    *) echo "Note: add $BIN to your PATH"
       echo "  export PATH=\"\$HOME/.local/bin:\$PATH\"" ;;
esac

# Determine x0xd command
if command -v x0xd >/dev/null 2>&1; then
    XOXD="x0xd"
else
    XOXD="$BIN/x0xd"
fi

# Start daemon
CMD="$XOXD"
if [ -n "$NAME" ]; then
    CMD="$XOXD --name $NAME"
fi

echo "Starting: $CMD"

# Determine data dir for port file
case "$OS" in
    Darwin) DATADIR="$HOME/Library/Application Support" ;;
    *)      DATADIR="${XDG_DATA_HOME:-$HOME/.local/share}" ;;
esac
if [ -n "$NAME" ]; then
    DATADIR="$DATADIR/x0x-$NAME"
else
    DATADIR="$DATADIR/x0x"
fi

# Ensure data dir exists before starting (daemon creates it too, but we need it for the log)
mkdir -p "$DATADIR"

# Start in background
nohup $CMD >> "$DATADIR/x0xd.log" 2>&1 &
PID=$!

# Wait for port file
PORTFILE="$DATADIR/api.port"
echo "Waiting for daemon (PID $PID)..."
TRIES=0
while [ ! -f "$PORTFILE" ] && [ $TRIES -lt 30 ]; do
    sleep 1
    TRIES=$((TRIES + 1))
done

if [ ! -f "$PORTFILE" ]; then
    echo "Timeout waiting for daemon. Check: cat $DATADIR/x0xd.log"
    exit 1
fi

API=$(cat "$PORTFILE")

# Wait for healthy
TRIES=0
while [ $TRIES -lt 15 ]; do
    if curl -sf "http://$API/health" >/dev/null 2>&1; then
        break
    fi
    sleep 1
    TRIES=$((TRIES + 1))
done

# Print result
HEALTH=$(curl -sf "http://$API/health" 2>/dev/null || echo '{"ok":false}')
AGENT=$(curl -sf "http://$API/agent" 2>/dev/null || echo '{}')

echo ""
echo "x0x is running"
echo "  API:      http://$API"
echo "  Health:   $HEALTH"
echo "  Agent:    $AGENT"
echo "  Log:      $DATADIR/x0xd.log"
echo "  PID:      $PID"
echo ""
echo "Try:"
echo "  curl http://$API/health"
echo "  curl http://$API/agents/discovered"
echo "  curl -X POST http://$API/publish -H 'Content-Type: application/json' \\"
echo "    -d '{\"topic\":\"hello\",\"payload\":\"'$(echo -n hello | base64)'\"}'"
