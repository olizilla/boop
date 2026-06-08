#!/bin/bash
# scripts/smoke-test-all.sh
# Builds and verifies release packages for both macOS and Linux before publishing a new version.

set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}===============================================${NC}"
echo -e "${GREEN}    Starting Cross-Platform Smoke Test Suite   ${NC}"
echo -e "${GREEN}===============================================${NC}"

# 1. Build and test local macOS DMG
echo -e "\n${YELLOW}[1/2] Building and testing macOS DMG package...${NC}"
npm run build
npx tauri build --bundles dmg
./scripts/test-dmg.sh

# 2. Build and test Linux packages in Docker
echo -e "\n${YELLOW}[2/2] Building and testing Linux packages (Debian/AppImage) in Docker...${NC}"
./scripts/build-linux.sh

echo -e "\n${GREEN}===============================================${NC}"
echo -e "${GREEN}✓ All pre-release smoke tests passed successfully!${NC}"
echo -e "${GREEN}===============================================${NC}"
