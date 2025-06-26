#!/bin/bash

# Audio Pipeline Test Script
# Usage: ./test_audio_pipeline.sh [source] [operation] [duration] [output_file] [device_name]
# Can be run from either the examples directory or the repo root directory

set -e  # Exit on any error

# Always reinstall the Python SDK in editable mode before running the test
cd "$(dirname "$0")/../sdks/python"
uv pip install -e .
cd - > /dev/null

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

# Set defaults for other parameters if not set
SAMPLE_RATE=${SAMPLE_RATE:-44100}
CHANNELS=${CHANNELS:-1}
CHUNK_SIZE=${CHUNK_SIZE:-1024}
FREQUENCY=${FREQUENCY:-440.0}

# Determine the absolute path for the output file
# The Python SDK runs from the examples directory, so it will create the file there
if [[ "$CURRENT_DIR" == "$SCRIPT_DIR" ]]; then
    # Running from examples directory
    ABSOLUTE_OUTPUT_FILE="$CURRENT_DIR/$OUTPUT_FILE"
else
    # Running from root directory
    ABSOLUTE_OUTPUT_FILE="$SCRIPT_DIR/$OUTPUT_FILE"
fi

echo "🎵 Testing Audio Streaming Pipeline"
echo "Source: $SOURCE"
echo "Operation: $OPERATION"
echo "Duration: ${DURATION}s"
echo "Output: $ABSOLUTE_OUTPUT_FILE"
echo "Device: $DEVICE_NAME"
echo ""

# Create temporary input file
TEMP_INPUT=$(mktemp --suffix=.json)
cat > "$TEMP_INPUT" << EOF
{
  "source": "$SOURCE",
  "duration": $DURATION,
  "sample_rate": $SAMPLE_RATE,
  "channels": $CHANNELS,
  "chunk_size": $CHUNK_SIZE,
  "frequency": $FREQUENCY,
  "output_file": "$OUTPUT_FILE",
  "device_name": "$DEVICE_NAME",
  "operation": "$OPERATION"
}
EOF

echo "📝 Using input configuration:"
cat "$TEMP_INPUT" | jq '.'
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
echo "📁 Output file: $ABSOLUTE_OUTPUT_FILE"

# Check if file was created
if [ -f "$ABSOLUTE_OUTPUT_FILE" ]; then
    echo "📊 File info:"
    file "$ABSOLUTE_OUTPUT_FILE"
    echo "📏 File size: $(ls -lh $ABSOLUTE_OUTPUT_FILE | awk '{print $5}')"
    echo "🎵 Duration: $(soxi -D $ABSOLUTE_OUTPUT_FILE 2>/dev/null || echo 'Unknown') seconds"
else
    # Check if file was created in examples directory (where Python SDK runs from)
    EXAMPLES_OUTPUT_FILE="examples/$OUTPUT_FILE"
    if [ -f "$EXAMPLES_OUTPUT_FILE" ]; then
        echo "📊 File found in examples directory:"
        file "$EXAMPLES_OUTPUT_FILE"
        echo "📏 File size: $(ls -lh $EXAMPLES_OUTPUT_FILE | awk '{print $5}')"
        echo "🎵 Duration: $(soxi -D $EXAMPLES_OUTPUT_FILE 2>/dev/null || echo 'Unknown') seconds"
        echo "💡 Note: File was created in examples/ directory by the Python SDK"
    else
        echo "❌ Output file not found in expected location: $ABSOLUTE_OUTPUT_FILE"
        echo "🔍 Checking for any .wav files in examples/ directory:"
        find examples/ -name "*.wav" -type f 2>/dev/null || echo "No .wav files found in examples/"
    fi
fi

# Clean up temporary input file
rm -f "$TEMP_INPUT"

echo ""
echo "🎉 Audio pipeline test finished!"
