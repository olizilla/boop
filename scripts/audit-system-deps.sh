#!/bin/bash
# scripts/audit-system-deps.sh
# Audits the compiled binary to find the exact Debian package dependencies.

set -e

# Find the binary
BINARY_PATH="src-tauri/target/release/app"
if [ ! -f "$BINARY_PATH" ]; then
    BINARY_PATH="src-tauri/target/debug/app"
fi

if [ ! -f "$BINARY_PATH" ]; then
    echo "Error: Binary not found at $BINARY_PATH."
    echo "Please build the app first (e.g., 'npm run tauri build' or 'npm run tauri dev')."
    exit 1
fi

if ! command -v dpkg &> /dev/null; then
    echo "Error: This script must be run on a Debian/Ubuntu system (requires 'dpkg' and 'ldd')."
    exit 1
fi

echo "=========================================================="
echo "Analyzing binary: $BINARY_PATH"
echo "=========================================================="

# Create a temporary file to store packages
PKGS_FILE=$(mktemp)

# Extract shared libraries from ldd and query their package providers via dpkg
ldd "$BINARY_PATH" | grep "=> /" | awk '{print $3}' | while read -r lib; do
    if [ -f "$lib" ]; then
        # Try finding package using the raw path
        pkg=$(dpkg -S "$lib" 2>/dev/null | cut -d: -f1)
        
        # If that fails and it starts with /lib, try /usr/lib (due to UsrMerge)
        if [ -z "$pkg" ] && [[ "$lib" =~ ^/lib/ ]]; then
            usr_lib=$(echo "$lib" | sed 's|^\(/lib/\|/lib32/\|/lib64/\|/libx32/\)|/usr\1|')
            pkg=$(dpkg -S "$usr_lib" 2>/dev/null | cut -d: -f1)
        fi
        
        # If still not found, try resolving canonical path via realpath
        if [ -z "$pkg" ]; then
            real_lib=$(realpath "$lib" 2>/dev/null || true)
            if [ -n "$real_lib" ] && [ "$real_lib" != "$lib" ]; then
                pkg=$(dpkg -S "$real_lib" 2>/dev/null | cut -d: -f1)
            fi
        fi

        if [ -n "$pkg" ]; then
            echo "$pkg" >> "$PKGS_FILE"
        fi
    fi
done

# Filter out standard base system packages to highlight what we actually care about
EXCLUDE_PATTERN="^(libc6|libgcc-s1|libstdc\+\+6|libselinux1|libpcre2-8-0|libffi8|libmount1|libblkid1|libuuid1|zlib1g|libdbus-1-3|libsystemd0|liblzma5|libgcrypt20|libgpg-error0|libzstd1|liblz4-1|libcap2)$"

echo "Detected linked system packages:"
sort -u "$PKGS_FILE" | grep -Ev "$EXCLUDE_PATTERN" | sed 's/^/  - /'

rm -f "$PKGS_FILE"

echo ""
echo "=========================================================="
echo "Runtime & Dynamic Plugin Dependencies"
echo "=========================================================="
echo "Important: GStreamer plugins (gstreamer1.0-plugins-*) and ALSA drivers"
echo "are loaded dynamically at runtime via dlopen() and do NOT show up in ldd."
echo ""
echo "To audit what plugins are loaded while the app is active, run:"
echo "  strace -e trace=open,openat -f $BINARY_PATH 2>&1 | grep -E '\.so' | grep -v 'ENOENT'"
echo ""
