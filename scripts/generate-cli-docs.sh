#!/bin/bash
# Generate CLI documentation from clap definitions

set -e

# Change to the stepflow-rs directory where Cargo.toml is located
cd "$(dirname "$0")/../stepflow-rs"

echo "Generating CLI documentation..."

# Run the test with the overwrite flag to generate docs
STEPFLOW_OVERWRITE_CLI_DOCS=1 cargo test -p stepflow-main cli_docs::tests::test_cli_docs_comparison --quiet

echo "CLI documentation generated successfully in docs/cli/"
echo ""
echo "Generated files:"
ls -la ../docs/cli/

echo ""
echo "To verify the docs are up to date, run:"
echo "  cargo test -p stepflow-main cli_docs::tests::test_cli_docs_comparison"