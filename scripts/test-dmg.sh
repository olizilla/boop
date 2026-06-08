#!/bin/bash
# scripts/test-dmg.sh
# Mounts the built macOS DMG, launches the app bundle to verify it starts up without error, and unmounts it.

set -e

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

DMG_PATH=$(find src-tauri/target/release/bundle/dmg -name "*.dmg" | head -n 1)
if [ -z "$DMG_PATH" ] || [ ! -f "$DMG_PATH" ]; then
    echo -e "${RED}Error: No DMG file found in src-tauri/target/release/bundle/dmg/${NC}"
    exit 1
fi

echo -e "Mounting DMG: $DMG_PATH"
# Mount the DMG and extract the volume path
MOUNT_INFO=$(hdiutil mount "$DMG_PATH")
echo "$MOUNT_INFO"

MOUNT_DIR=$(echo "$MOUNT_INFO" | grep -o '/Volumes/.*' | head -n 1 | xargs)

if [ -z "$MOUNT_DIR" ]; then
    echo -e "${RED}Error: Failed to find mount directory in /Volumes/${NC}"
    exit 1
fi

echo -e "Successfully mounted at: $MOUNT_DIR"

# Locate the app bundle inside the mounted volume
APP_PATH=$(find "$MOUNT_DIR" -name "*.app" -maxdepth 1 | head -n 1)
if [ -z "$APP_PATH" ]; then
    echo -e "${RED}Error: No .app bundle found in the mounted volume.${NC}"
    hdiutil detach "$MOUNT_DIR"
    exit 1
fi

echo -e "Found App Bundle: $APP_PATH"

# Get the binary executable path inside the app bundle
BINARY_NAME=$(defaults read "$APP_PATH/Contents/Info.plist" CFBundleExecutable)
EXECUTABLE_PATH="$APP_PATH/Contents/MacOS/$BINARY_NAME"

if [ ! -f "$EXECUTABLE_PATH" ]; then
    echo -e "${RED}Error: Executable not found at $EXECUTABLE_PATH${NC}"
    hdiutil detach "$MOUNT_DIR"
    exit 1
fi

echo -e "Launching app binary for smoke test..."
LOG_FILE=$(mktemp)

# Start the app in the background
"$EXECUTABLE_PATH" > "$LOG_FILE" 2>&1 &
PID=$!

# Wait 3 seconds to see if it remains running
sleep 3

# Check if the process is still running
if kill -0 "$PID" 2>/dev/null; then
    echo -e "${GREEN}✓ App launched successfully and remained active!${NC}"
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
    PASSED=true
else
    echo -e "${RED}✗ App crashed on startup!${NC}"
    echo -e "--- Crash Log ---"
    cat "$LOG_FILE"
    echo -e "-----------------"
    PASSED=false
fi

rm -f "$LOG_FILE"

# Eject/detach the mounted DMG volume
echo -e "Unmounting $MOUNT_DIR"
hdiutil detach "$MOUNT_DIR"

if [ "$PASSED" = true ]; then
    echo -e "${GREEN}DMG launch test PASSED!${NC}"
    exit 0
else
    echo -e "${RED}DMG launch test FAILED!${NC}"
    exit 1
fi
