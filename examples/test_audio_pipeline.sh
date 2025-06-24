#!/bin/bash

# Audio Pipeline Test Script
# Usage: ./test_audio_pipeline.sh [source] [operation] [duration] [output_file] [device_name]
# Can be run from either the examples directory or the repo root directory

set -e  # Exit on any error

# Get script directory and current working directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CURRENT_DIR="$(pwd)"

# Determine if we're running from examples directory or root
if [[ "$CURRENT_DIR" == "$SCRIPT_DIR" ]]; then
    # Running from examples directory
    INPUT_FILE="audio_input.json"
    FLOW_FILE="audio-streaming-pipeline.yaml"
    CONFIG_FILE="stepflow-config.yml"
    INPUT_DIR="."
    echo "📍 Running from examples directory"
else
    # Running from root directory
    INPUT_FILE="examples/audio_input.json"
    FLOW_FILE="examples/audio-streaming-pipeline.yaml"
    CONFIG_FILE="examples/stepflow-config.yml"
    INPUT_DIR="examples"
    echo "📍 Running from repo root directory"
fi

# Check if required files exist
if [[ ! -f "$INPUT_FILE" ]]; then
    echo "❌ Error: Input file not found: $INPUT_FILE"
    exit 1
fi

if [[ ! -f "$FLOW_FILE" ]]; then
    echo "❌ Error: Flow file not found: $FLOW_FILE"
    exit 1
fi

if [[ ! -f "$CONFIG_FILE" ]]; then
    echo "❌ Error: Config file not found: $CONFIG_FILE"
    exit 1
fi

# Parse command line arguments (all optional)
SOURCE=${1:-"microphone"}
OPERATION=${2:-"amplify"}
DURATION=${3:-"3.0"}
OUTPUT_FILE=${4:-"test_workflow_webcam.wav"}
DEVICE_NAME=${5:-"C922 Pro Stream Webcam"}

echo "🎵 Testing Audio Streaming Pipeline"
echo "Source: $SOURCE"
echo "Operation: $OPERATION"
echo "Duration: ${DURATION}s"
echo "Output: $OUTPUT_FILE"
echo "Device: $DEVICE_NAME"
echo ""

# Create a temporary input file with the provided parameters
TEMP_INPUT=$(mktemp --suffix=.json)
cat > "$TEMP_INPUT" << EOF
{
  "source": "$SOURCE",
  "operation": "$OPERATION",
  "sample_rate": 44100,
  "channels": 1,
  "chunk_size": 1024,
  "frequency": 440.0,
  "duration": $DURATION,
  "output_file": "$OUTPUT_FILE",
  "device_name": "$DEVICE_NAME"
}
EOF

echo "📝 Using input configuration:"
cat "$TEMP_INPUT"
echo ""

# Run the workflow
echo "🚀 Starting workflow execution..."
if [[ "$CURRENT_DIR" == "$SCRIPT_DIR" ]]; then
    # Running from examples directory - run from current directory
    cargo run -- run --flow "$FLOW_FILE" --input "$TEMP_INPUT"
else
    # Running from root directory
    cargo run -- run --flow "$FLOW_FILE" --input "$TEMP_INPUT"
fi

echo ""
echo "✅ Test completed!"
echo "📁 Output file: $OUTPUT_FILE"

# Check if file was created
if [ -f "$OUTPUT_FILE" ]; then
    echo "📊 File info:"
    file "$OUTPUT_FILE"
    echo "📏 File size: $(ls -lh $OUTPUT_FILE | awk '{print $5}')"
    echo "🎵 Duration: $(soxi -D $OUTPUT_FILE 2>/dev/null || echo 'Unknown') seconds"
else
    echo "❌ Output file not found"
fi

# Clean up temporary input file
rm -f "$TEMP_INPUT"

echo ""
echo "🎉 Audio pipeline test finished!"
