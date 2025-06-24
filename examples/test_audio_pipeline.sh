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
    INPUT_FILE="examples/input.json"
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

# Create input file
if [[ "$CURRENT_DIR" == "$SCRIPT_DIR" ]]; then
    # Running from examples directory
    cat > "$INPUT_FILE" << EOF
{
  "operation": "$OPERATION",
  "sample_rate": 16000,
  "channels": 1,
  "chunk_size": 1024,
  "frequency": 440.0,
  "source": "$SOURCE",
  "duration": $DURATION,
  "output_file": "$OUTPUT_FILE",
  "device_name": "$DEVICE_NAME",
  "play_audio": true
}
EOF
else
    # Running from root directory
    cat > "$INPUT_FILE" << EOF
{
  "operation": "$OPERATION",
  "sample_rate": 16000,
  "channels": 1,
  "chunk_size": 1024,
  "frequency": 440.0,
  "source": "$SOURCE",
  "duration": $DURATION,
  "output_file": "$OUTPUT_FILE",
  "device_name": "$DEVICE_NAME",
  "play_audio": true
}
EOF
fi

# Run the workflow
cd examples
cargo run --bin stepflow -- run \
  --flow audio-streaming-pipeline.yaml \
  --input input.json
cd ..

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
rm -f "$INPUT_FILE" 
