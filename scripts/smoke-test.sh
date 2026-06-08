#!/bin/bash
# scripts/smoke-test.sh
# Verifies that both the raw binary and the built AppImage can launch without error.

set -e

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

# Check if xvfb-run is installed
if ! command -v xvfb-run &> /dev/null; then
    echo -e "${RED}Error: xvfb-run is not installed.${NC}"
    exit 1
fi

# Locate the binary
RAW_BINARY="src-tauri/target/release/app"
if [ ! -f "$RAW_BINARY" ]; then
    echo -e "${RED}Error: Raw binary not found at $RAW_BINARY${NC}"
    exit 1
fi

# Locate the AppImage
APPIMAGE_PATH=$(find src-tauri/target/release/bundle/appimage -name "*.AppImage" | head -n 1)

# Function to run a binary headlessly for 3 seconds and verify it doesn't crash
run_smoke_test() {
    local name=$1
    local cmd=$2
    local log_file=$(mktemp)

    echo -ne "Testing launch of $name... "

    # Run the command under xvfb-run in the background.
    # We redirect stdout/stderr to a log file.
    xvfb-run -a $cmd > "$log_file" 2>&1 &
    local pid=$!

    # Wait for 3 seconds to see if it stays alive
    sleep 3

    # Check if the process is still running
    if kill -0 "$pid" 2>/dev/null; then
        # It's alive! Kill it cleanly
        kill "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
        echo -e "${GREEN}SUCCESS${NC}"
        rm -f "$log_file"
    else
        # Process died prematurely
        wait "$pid" 2>/dev/null || true
        echo -e "${RED}FAILED${NC}"
        echo -e "--- Error log for $name ---"
        cat "$log_file"
        echo -e "---------------------------"
        rm -f "$log_file"
        exit 1
    fi
}

# 1. Smoke test the raw compiled binary
run_smoke_test "Raw Binary" "./$RAW_BINARY"

# 2. Smoke test the AppImage (if present)
if [ -n "$APPIMAGE_PATH" ] && [ -f "$APPIMAGE_PATH" ]; then
    chmod +x "$APPIMAGE_PATH"
    # We must use --appimage-extract-and-run in Docker because FUSE isn't mounted
    run_smoke_test "AppImage" "./$APPIMAGE_PATH --appimage-extract-and-run"
else
    echo "Warning: No AppImage found to test."
fi

echo -e "${GREEN}All smoke tests passed successfully!${NC}"
