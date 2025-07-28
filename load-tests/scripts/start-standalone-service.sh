#!/bin/bash

# Start Stepflow service for standalone load testing (no OpenAI)
# This script should be run from the load-tests directory

set -e

PORT=7837

# Check if port is already in use
if lsof -Pi :$PORT -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo "Error: Port $PORT is already in use"
    echo "Please stop the existing service or use a different port"
    echo "To find what's using the port: lsof -i :$PORT"
    exit 1
fi

# Get the script directory and resolve config path
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CONFIG_PATH="$SCRIPT_DIR/../workflows/stepflow-standalone-config.yml"

# Verify config file exists
if [ ! -f "$CONFIG_PATH" ]; then
    echo "Error: Config file not found at $CONFIG_PATH"
    exit 1
fi

# Navigate to the stepflow-rs directory
cd "$SCRIPT_DIR/../../stepflow-rs"

# Build the project
echo "Building Stepflow..."
cargo build --release

# Start the service
echo "Starting Stepflow service on port $PORT for standalone testing..."
echo "Config: $CONFIG_PATH"
echo ""
echo "This service configuration supports:"
echo "  - Rust built-in components (create_messages, etc.)"
echo "  - Python UDF components via Python SDK"
echo "  - Python custom components via message_custom_server.py"
echo ""
echo "No OpenAI API key required for standalone tests."
echo ""

./target/release/stepflow serve \
    --port=$PORT \
    --config="$CONFIG_PATH" \
    --log-level=warn \
    --log-file "stepflow-standalone.log"