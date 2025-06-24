#!/bin/bash

# Audio Pipeline Test Script
# Usage: ./test_audio_pipeline.sh [source] [operation] [duration] [output_file] [device_name]

SOURCE=${1:-"microphone"}
OPERATION=${2:-"amplify"}
DURATION=${3:-"3.0"}
OUTPUT_FILE=${4:-"test_workflow_webcam.wav"}
DEVICE_NAME=${5:-"C922 Pro Stream Webcam"}

# Detect if we're running from examples directory or root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CURRENT_DIR="$(pwd)"

if [[ "$CURRENT_DIR" == "$SCRIPT_DIR" ]]; then
    # Running from examples directory
    INPUT_FILE="audio_input.json"
    FLOW_FILE="audio-streaming-pipeline.yaml"
    INPUT_DIR="."
else
    # Running from root directory
    INPUT_FILE="examples/audio_input.json"
    FLOW_FILE="examples/audio-streaming-pipeline.yaml"
    INPUT_DIR="examples"
fi

echo "🎵 Testing Audio Streaming Pipeline"
echo "Source: $SOURCE"
echo "Operation: $OPERATION"
echo "Duration: ${DURATION}s"
echo "Output: $OUTPUT_FILE"
echo "Device: $DEVICE_NAME"
echo "Running from: $CURRENT_DIR"
echo ""

# Run the workflow
if [[ "$CURRENT_DIR" == "$SCRIPT_DIR" ]]; then
    # Running from examples directory - run from current directory
    cargo run -- run --flow audio-streaming-pipeline.yaml --input audio_input.json
else
    # Running from root directory
    cargo run -- run --flow examples/audio-streaming-pipeline.yaml --input examples/audio_input.json
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

# Clean up input file
