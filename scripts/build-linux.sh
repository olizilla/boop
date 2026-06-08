#!/bin/bash
# scripts/build-linux.sh
# Builds the Linux packages (deb, AppImage) inside a Docker container on macOS
# and extracts them to the host's dist/linux/ directory.

set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}Building Linux packages inside Docker container...${NC}"

# Build the Docker image
docker build -t boop-builder -f Dockerfile.linux .

# Create a temporary container (without running it) to extract files
echo -e "${GREEN}Extracting built packages...${NC}"
CONTAINER_ID=$(docker create boop-builder)

# Create output directories on host
mkdir -p dist/linux

# Copy the bundles out of the container
docker cp "${CONTAINER_ID}:/workspace/src-tauri/target/release/bundle/deb" dist/linux/ || true
docker cp "${CONTAINER_ID}:/workspace/src-tauri/target/release/bundle/appimage" dist/linux/ || true

# Clean up container
docker rm "${CONTAINER_ID}" >/dev/null

echo -e "${GREEN}Done! Built packages are available in:${NC}"
echo -e "${YELLOW}  - dist/linux/deb/${NC}"
echo -e "${YELLOW}  - dist/linux/appimage/${NC}"
