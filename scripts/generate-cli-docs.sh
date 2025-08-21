#!/bin/bash
# Copyright 2025 DataStax Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License. You may obtain a copy of
# the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
# License for the specific language governing permissions and limitations under
# the License.

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