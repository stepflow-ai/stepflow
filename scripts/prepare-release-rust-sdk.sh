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

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
BUMP_TYPE=""
CREATE_PR=false

# Help function
show_help() {
    cat << EOF
Usage: $0 [OPTIONS] <major|minor|patch>

Prepare a Rust SDK release by bumping version.

ARGUMENTS:
    <major|minor|patch>     Type of version bump to perform

OPTIONS:
    --pr                   Create PR after preparing changes (requires clean git state)
    -h, --help             Show this help message

EXAMPLES:
    $0 patch               # Bump patch version locally (safe to run)
    $0 minor --pr          # Bump minor version and create PR

BEHAVIOR:
    By default, this script only updates local files:
    - Updates version in sdks/rust/Cargo.toml
    - Updates Cargo.lock

    With --pr flag, it also:
    - Creates release branch
    - Commits changes
    - Pushes branch and creates PR

REQUIREMENTS:
    - gh CLI (only needed with --pr flag)
    - Clean git working directory (only needed with --pr flag)

NOTE:
    The SDK version is independent of the orchestrator version.
    Dependencies on orchestrator crates (stepflow-flow, stepflow-proto)
    are pinned separately and not affected by SDK version bumps.

EOF
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        major|minor|patch)
            if [[ -n "$BUMP_TYPE" ]]; then
                echo -e "${RED}Error: Multiple bump types specified${NC}" >&2
                exit 1
            fi
            BUMP_TYPE="$1"
            shift
            ;;
        --pr)
            CREATE_PR=true
            shift
            ;;
        -h|--help)
            show_help
            exit 0
            ;;
        *)
            echo -e "${RED}Error: Unknown option '$1'${NC}" >&2
            echo "Use '$0 --help' for usage information." >&2
            exit 1
            ;;
    esac
done

# Validate bump type
if [[ -z "$BUMP_TYPE" ]]; then
    echo -e "${RED}Error: Version bump type is required${NC}" >&2
    echo "Use '$0 --help' for usage information." >&2
    exit 1
fi

# Ensure we're in the project root directory
if [[ ! -d "sdks/rust" ]] || [[ ! -f "sdks/rust/Cargo.toml" ]]; then
    echo -e "${RED}Error: Must be run from project root directory${NC}" >&2
    echo "Expected to find sdks/rust/Cargo.toml" >&2
    exit 1
fi

# Check git status only if creating PR
if [[ "$CREATE_PR" == true ]] && [[ -n "$(git status --porcelain)" ]]; then
    echo -e "${RED}Error: Working directory is not clean (required for --pr flag)${NC}" >&2
    echo "Please commit or stash your changes first, or run without --pr to prepare changes locally." >&2
    exit 1
fi

if [[ "$CREATE_PR" == true ]] && ! command -v gh >/dev/null 2>&1; then
    echo -e "${RED}Error: gh CLI is not installed (required for --pr flag)${NC}" >&2
    echo "Install from: https://cli.github.com/" >&2
    exit 1
fi

# Change to sdks/rust directory for operations
cd sdks/rust

# Get current version
CURRENT_VERSION=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
echo -e "${BLUE}Current version: ${GREEN}$CURRENT_VERSION${NC}"

# Calculate new version
IFS='.' read -ra VERSION_PARTS <<< "$CURRENT_VERSION"
MAJOR=${VERSION_PARTS[0]}
MINOR=${VERSION_PARTS[1]}
PATCH=${VERSION_PARTS[2]}

case "$BUMP_TYPE" in
    major)
        NEW_VERSION="$((MAJOR + 1)).0.0"
        ;;
    minor)
        NEW_VERSION="$MAJOR.$((MINOR + 1)).0"
        ;;
    patch)
        NEW_VERSION="$MAJOR.$MINOR.$((PATCH + 1))"
        ;;
esac

echo -e "${BLUE}New version: ${GREEN}$NEW_VERSION${NC}"

# Update version in Cargo.toml
echo -e "${BLUE}Updating version in Cargo.toml...${NC}"
if [[ "$OSTYPE" == "darwin"* ]]; then
    sed -i '' "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" Cargo.toml
else
    sed -i "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" Cargo.toml
fi

# Update Cargo.lock (workspace members only)
echo -e "${BLUE}Updating Cargo.lock...${NC}"
cargo update -w > /dev/null

echo -e "${GREEN}Release preparation complete!${NC}"
echo -e "${BLUE}Changes made:${NC}"
echo "  - Version bumped from $CURRENT_VERSION to $NEW_VERSION in sdks/rust/Cargo.toml"
echo "  - Updated Cargo.lock"

if [[ "$CREATE_PR" == false ]]; then
    echo ""
    echo -e "${YELLOW}Next steps:${NC}"
    echo "1. Review the changes with: git diff"
    echo "2. If satisfied, create PR with: $0 $BUMP_TYPE --pr"
    echo "3. Or commit manually and push as needed"
    exit 0
fi

# Create PR workflow
echo ""
echo -e "${BLUE}Creating pull request...${NC}"

# Create release branch
RELEASE_BRANCH="release/rust-sdk-$NEW_VERSION"
echo -e "${BLUE}Creating release branch: $RELEASE_BRANCH${NC}"

# Go back to repo root for git operations
cd ../..

# Check if release branch already exists
if git show-ref --verify --quiet "refs/heads/$RELEASE_BRANCH"; then
    echo -e "${YELLOW}Release branch $RELEASE_BRANCH already exists locally. Deleting and recreating...${NC}"
    git branch -D "$RELEASE_BRANCH"
fi

git checkout -b "$RELEASE_BRANCH"

# Commit changes
echo -e "${BLUE}Committing changes...${NC}"
git add sdks/rust/Cargo.toml sdks/rust/Cargo.lock
git commit -m "chore: release rust-sdk v$NEW_VERSION

- Bump version from $CURRENT_VERSION to $NEW_VERSION"

# Push branch
echo -e "${BLUE}Pushing release branch...${NC}"
git push -u origin "$RELEASE_BRANCH" --force

# Create PR
PR_BODY="## Release Rust SDK v$NEW_VERSION

This PR prepares the release of Rust SDK v$NEW_VERSION.

### Changes
- Version bump from $CURRENT_VERSION to $NEW_VERSION in sdks/rust/Cargo.toml
- Updated Cargo.lock

### Crates
- \`stepflow-client\` v$NEW_VERSION
- \`stepflow-worker\` v$NEW_VERSION

### Next Steps
1. Merge this PR
2. The release will be automatically tagged and published to crates.io"

PR_URL=$(gh pr create \
    --title "chore: release rust-sdk v$NEW_VERSION" \
    --body "$PR_BODY" \
    --label "release" \
    --label "release:rust-sdk")

echo -e "${GREEN}Release PR created successfully!${NC}"
echo -e "${BLUE}PR URL: $PR_URL${NC}"
echo ""
echo -e "${YELLOW}Next steps:${NC}"
echo "1. Review the PR"
echo "2. Merge the PR when ready"
echo "3. The release will be automatically tagged and published to crates.io"
