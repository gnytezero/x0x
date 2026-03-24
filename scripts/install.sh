#!/usr/bin/env bash
# x0x Installation Script (Unix/macOS/Linux)
#
# Interactive mode (default in terminal): prompts for confirmation when needed.
# Non-interactive mode (piped or -y flag): uses safe defaults, never blocks.
#
# Examples:
#   curl -sfL https://x0x.md | sh                        # non-interactive (piped)
#   bash install.sh                                       # interactive
#   bash install.sh -y                                    # non-interactive (explicit)
#   bash install.sh --start --health                      # install, start daemon, wait for healthy
#   curl -sfL https://x0x.md | bash -s -- --start --health  # one-liner: install + run + verify

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

REPO="saorsa-labs/x0x"
RELEASE_URL="https://github.com/$REPO/releases/latest/download"
BIN_DIR="$HOME/.local/bin"

# Detect interactive mode and flags
INTERACTIVE=true
INSTANCE_NAME=""
START_DAEMON=false
WAIT_HEALTH=false
if ! [ -t 0 ]; then
    INTERACTIVE=false
fi
SKIP_NEXT=false
for i in $(seq 1 $#); do
    arg="${!i}"
    if [ "$SKIP_NEXT" = true ]; then
        SKIP_NEXT=false
        continue
    fi
    case "$arg" in
        -y|--yes) INTERACTIVE=false ;;
        --start) START_DAEMON=true ;;
        --health) WAIT_HEALTH=true ;;
        --name)
            j=$((i + 1))
            INSTANCE_NAME="${!j}"
            SKIP_NEXT=true
            ;;
    esac
done

# Determine data directory (must match x0xd's `dirs::data_dir()` behavior)
case "$(uname -s)" in
    Darwin) DATA_BASE="${XDG_DATA_HOME:-$HOME/Library/Application Support}" ;;
    *)      DATA_BASE="${XDG_DATA_HOME:-$HOME/.local/share}" ;;
esac

if [ -n "$INSTANCE_NAME" ]; then
    INSTALL_DIR="${DATA_BASE}/x0x-${INSTANCE_NAME}"
else
    INSTALL_DIR="${DATA_BASE}/x0x"
fi

echo -e "${BLUE}x0x Installation Script${NC}"
echo -e "${BLUE}========================${NC}"
echo ""

# Check if GPG is installed
if ! command -v gpg &> /dev/null; then
    echo -e "${YELLOW}⚠ Warning: GPG not found. Signature verification will be skipped.${NC}"
    echo ""
    echo "To enable signature verification, install GPG:"
    echo "  macOS:  brew install gnupg"
    echo "  Ubuntu: sudo apt install gnupg"
    echo "  Fedora: sudo dnf install gnupg"
    echo ""
    if [ "$INTERACTIVE" = true ]; then
        read -p "Continue without verification? (y/N) " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 1
        fi
    else
        echo "Continuing without verification (non-interactive mode)."
        echo ""
    fi
    GPG_AVAILABLE=false
else
    GPG_AVAILABLE=true
fi

# Create install directory
mkdir -p "$INSTALL_DIR"
cd "$INSTALL_DIR"

echo "Downloading SKILL.md..."
if command -v curl &> /dev/null; then
    curl -sfL "$RELEASE_URL/SKILL.md" -o SKILL.md
elif command -v wget &> /dev/null; then
    wget -qO SKILL.md "$RELEASE_URL/SKILL.md"
else
    echo -e "${RED}✗ Error: Neither curl nor wget found${NC}"
    exit 1
fi

if [ "$GPG_AVAILABLE" = true ]; then
    echo "Downloading signature..."
    if command -v curl &> /dev/null; then
        curl -sfL "$RELEASE_URL/SKILL.md.sig" -o SKILL.md.sig
        curl -sfL "$RELEASE_URL/SAORSA_PUBLIC_KEY.asc" -o SAORSA_PUBLIC_KEY.asc
    else
        wget -qO SKILL.md.sig "$RELEASE_URL/SKILL.md.sig"
        wget -qO SAORSA_PUBLIC_KEY.asc "$RELEASE_URL/SAORSA_PUBLIC_KEY.asc"
    fi

    echo "Importing Saorsa Labs public key..."
    gpg --import SAORSA_PUBLIC_KEY.asc 2>&1 | grep -v "^gpg:" || true

    echo "Verifying signature..."
    if gpg --verify SKILL.md.sig SKILL.md 2>&1 | grep -q "Good signature"; then
        echo -e "${GREEN}✓ Signature verified${NC}"
    else
        echo -e "${RED}✗ Signature verification failed${NC}"
        echo ""
        echo "This file may have been tampered with."
        if [ "$INTERACTIVE" = true ]; then
            read -p "Install anyway? (y/N) " -n 1 -r
            echo
            if [[ ! $REPLY =~ ^[Yy]$ ]]; then
                exit 1
            fi
        else
            echo -e "${RED}✗ Signature verification failed in non-interactive mode. Aborting.${NC}"
            echo "  Re-run interactively or set X0X_SKIP_GPG=true to bypass."
            exit 1
        fi
    fi
fi

# ── x0xd daemon binary ────────────────────────────────────────────────────────

echo ""
echo "Detecting platform..."

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)
        case "$ARCH" in
            x86_64)  PLATFORM="linux-x64-gnu" ;;
            aarch64) PLATFORM="linux-arm64-gnu" ;;
            *)
                echo -e "${YELLOW}⚠ Unsupported Linux architecture: $ARCH${NC}"
                echo "  x0xd daemon installation skipped."
                PLATFORM=""
                ;;
        esac
        ;;
    Darwin)
        case "$ARCH" in
            arm64)   PLATFORM="macos-arm64" ;;
            x86_64)  PLATFORM="macos-x64" ;;
            *)
                echo -e "${YELLOW}⚠ Unsupported macOS architecture: $ARCH${NC}"
                echo "  x0xd daemon installation skipped."
                PLATFORM=""
                ;;
        esac
        ;;
    *)
        echo -e "${YELLOW}⚠ Unsupported operating system: $OS${NC}"
        echo "  x0xd daemon installation is only supported on Linux and macOS."
        echo "  Skipping daemon installation."
        PLATFORM=""
        ;;
esac

if [ -n "$PLATFORM" ]; then
    ARCHIVE="x0x-${PLATFORM}.tar.gz"
    ARCHIVE_URL="$RELEASE_URL/$ARCHIVE"
    TMPDIR="$(mktemp -d)"

    echo "Downloading x0xd ($PLATFORM)..."
    if command -v curl &> /dev/null; then
        curl -sfL "$ARCHIVE_URL" -o "$TMPDIR/$ARCHIVE"
    else
        wget -qO "$TMPDIR/$ARCHIVE" "$ARCHIVE_URL"
    fi

    echo "Extracting x0xd..."
    tar -xzf "$TMPDIR/$ARCHIVE" -C "$TMPDIR" "x0x-${PLATFORM}/x0xd"

    mkdir -p "$BIN_DIR"
    mv "$TMPDIR/x0x-${PLATFORM}/x0xd" "$BIN_DIR/x0xd"
    chmod +x "$BIN_DIR/x0xd"

    rm -rf "$TMPDIR"

    echo -e "${GREEN}✓ x0xd installed to: $BIN_DIR/x0xd${NC}"

    # Warn if ~/.local/bin is not in PATH
    case ":$PATH:" in
        *":$BIN_DIR:"*) ;;
        *)
            echo ""
            echo -e "${YELLOW}⚠ $BIN_DIR is not in your PATH.${NC}"
            echo "  Add it by appending one of the following to your shell profile:"
            echo ""
            echo "    # bash (~/.bashrc or ~/.bash_profile)"
            echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
            echo ""
            echo "    # zsh (~/.zshrc)"
            echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
            echo ""
            echo "  Then reload your shell: source ~/.bashrc  (or ~/.zshrc)"
            ;;
    esac
fi

# ── Summary ───────────────────────────────────────────────────────────────────

# Generate per-instance config if --name was provided
if [ -n "$INSTANCE_NAME" ]; then
    CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/x0x-${INSTANCE_NAME}"
    mkdir -p "$CONFIG_DIR"
    if [ ! -f "$CONFIG_DIR/config.toml" ]; then
        cat > "$CONFIG_DIR/config.toml" <<TOML
# x0x instance configuration for: $INSTANCE_NAME
instance_name = "$INSTANCE_NAME"
TOML
        echo -e "${GREEN}✓ Config created: $CONFIG_DIR/config.toml${NC}"
    fi
fi

echo ""
echo -e "${GREEN}✓ Installation complete${NC}"
echo ""
echo "SKILL.md installed to: $INSTALL_DIR/SKILL.md"
if [ -n "$PLATFORM" ]; then
    echo "x0xd installed to:     $BIN_DIR/x0xd"
fi

# ── Start daemon if --start was passed ────────────────────────────────────────

if [ "$START_DAEMON" = true ] && [ -n "$PLATFORM" ]; then
    echo ""

    # Resolve the x0xd binary path
    X0XD_BIN=""
    if command -v x0xd &> /dev/null; then
        X0XD_BIN="x0xd"
    elif [ -x "$BIN_DIR/x0xd" ]; then
        X0XD_BIN="$BIN_DIR/x0xd"
    fi

    if [ -z "$X0XD_BIN" ]; then
        echo -e "${RED}✗ Cannot start daemon: x0xd not found in PATH or $BIN_DIR${NC}"
        echo "  Add $BIN_DIR to your PATH and try again."
    else
        # Build the command
        X0XD_CMD="$X0XD_BIN"
        if [ -n "$INSTANCE_NAME" ]; then
            X0XD_CMD="$X0XD_BIN --name $INSTANCE_NAME"
        fi

        echo "Starting daemon: $X0XD_CMD"
        # Start in background with nohup so it survives the install script exiting
        X0XD_LOG="$INSTALL_DIR/x0xd.log"
        nohup $X0XD_CMD >> "$X0XD_LOG" 2>&1 &
        X0XD_PID=$!
        echo -e "${GREEN}✓ x0xd started (PID $X0XD_PID, log: $X0XD_LOG)${NC}"

        # ── Wait for health if --health was passed ────────────────────────────

        if [ "$WAIT_HEALTH" = true ]; then
            # Determine the API address to poll
            if [ -n "$INSTANCE_NAME" ]; then
                # Named instances use a random port — wait for the port file
                PORT_FILE="$INSTALL_DIR/api.port"
                echo "Waiting for port file ($PORT_FILE)..."
                HEALTH_TIMEOUT=30
                ELAPSED=0
                while [ ! -f "$PORT_FILE" ] && [ $ELAPSED -lt $HEALTH_TIMEOUT ]; do
                    sleep 1
                    ELAPSED=$((ELAPSED + 1))
                done
                if [ -f "$PORT_FILE" ]; then
                    API_ADDR=$(cat "$PORT_FILE")
                else
                    echo -e "${RED}✗ Timed out waiting for port file after ${HEALTH_TIMEOUT}s${NC}"
                    echo "  Check logs: cat $X0XD_LOG"
                    API_ADDR=""
                fi
            else
                API_ADDR="127.0.0.1:12700"
                sleep 2  # give it a moment to start
            fi

            if [ -n "$API_ADDR" ]; then
                echo "Waiting for health at http://${API_ADDR}/health ..."
                HEALTH_TIMEOUT=30
                ELAPSED=0
                HEALTHY=false
                while [ $ELAPSED -lt $HEALTH_TIMEOUT ]; do
                    if curl -sf "http://${API_ADDR}/health" > /dev/null 2>&1; then
                        HEALTHY=true
                        break
                    fi
                    sleep 1
                    ELAPSED=$((ELAPSED + 1))
                done

                if [ "$HEALTHY" = true ]; then
                    HEALTH_JSON=$(curl -sf "http://${API_ADDR}/health" 2>/dev/null)
                    echo -e "${GREEN}✓ Daemon is healthy${NC}"
                    echo "  $HEALTH_JSON"
                    echo ""
                    echo "API: http://${API_ADDR}"
                else
                    echo -e "${RED}✗ Timed out waiting for healthy daemon after ${HEALTH_TIMEOUT}s${NC}"
                    echo "  Check logs: cat $X0XD_LOG"
                fi
            fi
        fi
    fi
elif [ "$START_DAEMON" = true ] && [ -z "$PLATFORM" ]; then
    echo ""
    echo -e "${YELLOW}⚠ --start ignored: no daemon binary was installed for this platform${NC}"
fi

# ── Next steps ────────────────────────────────────────────────────────────────

echo ""
if [ "$START_DAEMON" != true ] && [ -n "$PLATFORM" ]; then
    echo "Next steps:"
    if [ -n "$INSTANCE_NAME" ]; then
        echo "  1. Run x0xd:"
        echo "       x0xd --name $INSTANCE_NAME"
    else
        echo "  1. Run x0xd:"
        echo "       x0xd"
    fi
    echo "     (x0xd creates your identity on first run and joins the global network)"
    echo "     (If x0xd is not found, ensure $BIN_DIR is in your PATH — see above)"
    echo ""
    echo "  2. Manage contacts:"
    if [ -n "$INSTANCE_NAME" ]; then
        echo "       # Port is auto-assigned — check the port file:"
        echo "       cat $INSTALL_DIR/api.port"
    else
        echo "       curl http://127.0.0.1:12700/contacts"
    fi
    echo ""
    echo "  3. Review SKILL.md: cat $INSTALL_DIR/SKILL.md"
    echo ""
    echo "  4. Install SDK:"
elif [ -z "$PLATFORM" ]; then
    echo "Next steps:"
    echo "  1. Review SKILL.md: cat $INSTALL_DIR/SKILL.md"
    echo ""
    echo "  2. Install SDK:"
fi
if [ "$START_DAEMON" != true ] || [ -z "$PLATFORM" ]; then
    echo "     - Rust:       cargo add x0x"
    echo "     - TypeScript: npm install x0x"
    echo "     - Python:     pip install agent-x0x"
fi
echo ""
echo "Learn more: https://github.com/$REPO"
