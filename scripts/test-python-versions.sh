#!/bin/bash
# Test script to verify Python SDK works with Python 3.11, 3.12, and 3.13

set -e

# Change to the Python SDK directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/../sdks/python"

echo "Testing Python SDK compatibility across Python versions..."

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

test_python_version() {
    local version=$1
    echo -e "\n${YELLOW}Testing Python ${version}...${NC}"

    # Install the specific Python version
    echo "Installing Python ${version}..."
    if ! uv python install "$version"; then
        echo -e "${RED}Failed to install Python ${version}${NC}"
        return 1
    fi

    # Use the specific Python version
    echo "Using Python ${version}..."
    if ! uv python pin "$version"; then
        echo -e "${RED}Failed to pin Python ${version}${NC}"
        return 1
    fi

    # Install dependencies
    echo "Installing dependencies for Python ${version}..."
    if ! uv sync --all-extras; then
        echo -e "${RED}Failed to install dependencies for Python ${version}${NC}"
        return 1
    fi

    # Run tests
    echo "Running tests with Python ${version}..."
    if ! uv run poe test; then
        echo -e "${RED}Tests failed for Python ${version}${NC}"
        return 1
    fi

    # Run type checking
    echo "Running type checking with Python ${version}..."
    if ! uv run poe typecheck; then
        echo -e "${RED}Type checking failed for Python ${version}${NC}"
        return 1
    fi

    echo -e "${GREEN}Python ${version} tests passed!${NC}"
}

# Test each Python version
versions=("3.11" "3.12" "3.13")
failed_versions=()

for version in "${versions[@]}"; do
    if ! test_python_version "$version"; then
        failed_versions+=("$version")
    fi
done

# Summary
echo -e "\n${YELLOW}Summary:${NC}"
if [ ${#failed_versions[@]} -eq 0 ]; then
    echo -e "${GREEN}✅ All Python versions (${versions[*]}) passed tests!${NC}"
    exit 0
else
    echo -e "${RED}❌ Failed versions: ${failed_versions[*]}${NC}"
    echo -e "${GREEN}✅ Passed versions: $(printf '%s ' "${versions[@]}" | sed "s/${failed_versions[*]//,/\\|}//" | xargs)${NC}"
    exit 1
fi