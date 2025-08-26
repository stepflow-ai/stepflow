#!/bin/bash

# Research Assistant Demo Runner
# This script runs the AI Research Assistant workflow with different input examples

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if we're in the right directory
if [ ! -f "workflow.yaml" ]; then
    echo -e "${RED}Error: Must run from the research-assistant directory${NC}"
    exit 1
fi

# Default values
INPUT_FILE="input_ai_workflows.json"
STEPFLOW_BIN="../../stepflow-rs/target/debug/stepflow"

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --input)
            INPUT_FILE="$2"
            shift 2
            ;;
        --release)
            STEPFLOW_BIN="../../stepflow-rs/target/release/stepflow"
            shift
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --input FILE    Input file to use (default: input_ai_workflows.json)"
            echo "  --release       Use release build of stepflow"
            echo "  --help          Show this help message"
            echo ""
            echo "Available input files:"
            echo "  - input_ai_workflows.json       (AI Workflow Orchestration)"
            echo "  - input_langchain_integration.json  (LangChain Integration Patterns)"
            echo "  - input_mcp_tools.json          (Model Context Protocol)"
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

# Check if stepflow binary exists
if [ ! -f "$STEPFLOW_BIN" ]; then
    echo -e "${YELLOW}Stepflow binary not found. Building...${NC}"
    cd ../../stepflow-rs
    if [[ "$STEPFLOW_BIN" == *"release"* ]]; then
        cargo build --release
    else
        cargo build
    fi
    cd - > /dev/null
fi

# Extract topic from input file for display
TOPIC=$(grep '"topic"' "$INPUT_FILE" | head -1 | cut -d'"' -f4)

echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}       AI Research Assistant - Stepflow Demo${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo ""
echo -e "${YELLOW}Topic:${NC} $TOPIC"
echo -e "${YELLOW}Input:${NC} $INPUT_FILE"
echo -e "${YELLOW}Config:${NC} stepflow-config.yml"
echo ""
echo -e "${BLUE}───────────────────────────────────────────────────────────────${NC}"

# Run the workflow
echo -e "${GREEN}Starting workflow execution...${NC}"
echo ""

$STEPFLOW_BIN run \
    --flow=workflow.yaml \
    --input="$INPUT_FILE" \
    --config=stepflow-config.yml

echo ""
echo -e "${BLUE}───────────────────────────────────────────────────────────────${NC}"

# Show output location
OUTPUT_DIR=$(grep '"output_dir"' "$INPUT_FILE" | head -1 | cut -d'"' -f4)
if [ -d "$OUTPUT_DIR" ]; then
    echo -e "${GREEN}✓ Research files created successfully!${NC}"
    echo ""
    echo -e "${YELLOW}Output files:${NC}"
    ls -la "$OUTPUT_DIR"
    echo ""
    echo -e "${YELLOW}View the results:${NC}"
    echo "  cat $OUTPUT_DIR/research_questions.md"
    echo "  cat $OUTPUT_DIR/research_notes.md"
    echo "  cat $OUTPUT_DIR/research_report.json | jq ."
else
    echo -e "${YELLOW}Note: Output directory not found. Check workflow execution above.${NC}"
fi

echo ""
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}Research Assistant demo complete!${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"