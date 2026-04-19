#!/bin/bash

# Boop Setup Script
# Automates installation of system dependencies for Tauri and checks for core tools.

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}Starting Boop environment setup...${NC}"

# 1. OS Detection
OS="$(uname)"
case "${OS}" in
    Linux*)     MACHINE=Linux;;
    Darwin*)    MACHINE=Mac;;
    *)          MACHINE="UNKNOWN:${OS}"
esac

echo -e "Detected OS: ${YELLOW}${MACHINE}${NC}"

# 2. Core Tool Checks (Node, npm, Rust)
check_version() {
    local cmd=$1
    local min_version=$2
    local current_version=$3
    
    if [[ "$(printf '%s\n' "$min_version" "$current_version" | sort -V | head -n1)" != "$min_version" ]]; then
        return 1
    fi
    return 0
}

# Node check
if ! command -v node &> /dev/null; then
    echo -e "${RED}Error: node is not installed.${NC}"
    echo "Please install Node.js (v18 or newer) from https://nodejs.org/"
    exit 1
else
    NODE_VER=$(node -v | sed 's/v//')
    if check_version "node" "18.0.0" "$NODE_VER"; then
        echo -e "${GREEN}✓ Node.js v${NODE_VER} detected${NC}"
    else
        echo -e "${YELLOW}Warning: Node.js v${NODE_VER} is older than the recommended v18.0.0.${NC}"
    fi
fi

# npm check
if ! command -v npm &> /dev/null; then
    echo -e "${RED}Error: npm is not installed.${NC}"
    exit 1
else
    echo -e "${GREEN}✓ npm detected${NC}"
fi

# Rust check
if ! command -v rustc &> /dev/null; then
    echo -e "${RED}Error: Rust is not installed.${NC}"
    echo "Please install Rust via rustup: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
else
    RUST_VER=$(rustc --version | awk '{print $2}')
    if check_version "rustc" "1.77.2" "$RUST_VER"; then
        echo -e "${GREEN}✓ Rust v${RUST_VER} detected${NC}"
    else
        echo -e "${YELLOW}Warning: Rust v${RUST_VER} is older than required v1.77.2.${NC}"
        echo "Please update rust: rustup update"
    fi
fi

# 3. Platform-Specific Dependencies
if [ "${MACHINE}" == "Linux" ]; then
    if command -v apt-get &> /dev/null; then
        echo -e "${GREEN}Installing Linux system dependencies...${NC}"
        sudo apt-get update
        # Minimal GStreamer set for WebM/Opus recording and playback
        sudo apt-get install -y \
            libwebkit2gtk-4.1-dev \
            build-essential \
            curl \
            wget \
            file \
            libssl-dev \
            libgtk-3-dev \
            libayatana-appindicator3-dev \
            librsvg2-dev \
            gstreamer1.0-plugins-base \
            gstreamer1.0-plugins-good \
            gstreamer1.0-pulseaudio \
            gstreamer1.0-tools
        
        # Why these GStreamer plugins?
        # - gstreamer1.0-plugins-base: Provides opusenc/opusparse (Opus audio)
        # - gstreamer1.0-plugins-good: Provides matroskamux (WebM container)
        # - gstreamer1.0-pulseaudio: Required for microphone access via PulseAudio/PipeWire
        # - gstreamer1.0-tools: Provides gst-inspect-1.0 for diagnostics
    else
        echo -e "${YELLOW}Warning: Could not find apt-get. Please manually install the following dependencies:${NC}"
        echo "libwebkit2gtk-4.1-dev, build-essential, libssl-dev, libgtk-3-dev, libayatana-appindicator3-dev, librsvg2-dev"
    fi
elif [ "${MACHINE}" == "Mac" ]; then
    echo -e "${GREEN}Checking macOS dependencies...${NC}"
    if ! xcode-select -p &> /dev/null; then
        echo -e "${YELLOW}Warning: Xcode Command Line Tools not detected.${NC}"
        echo "Please run: xcode-select --install"
    else
        echo -e "${GREEN}✓ Xcode Command Line Tools detected${NC}"
    fi
fi

# 4. Project Dependencies
echo -e "${GREEN}Installing Node dependencies...${NC}"
npm install

echo -e "${GREEN}Setup complete! You can now run the app with:${NC}"
echo -e "${YELLOW}npm run tauri dev${NC}"
