#!/bin/bash
#
# Lists local branches that have been merged into the main branch.
# Detects both regular merges and squash merges.
#
# Usage: ./scripts/list-merged-branches.sh [options]
#
# Options:
#   -r, --remote     Check remote branches instead of local
#   -d, --delete     Delete the merged branches (dry-run by default)
#   -f, --force      Actually delete branches (use with -d)
#   -m, --main       Specify main branch (default: main)
#   -h, --help       Show this help message
#

set -euo pipefail

# Default values
MAIN_BRANCH="main"
CHECK_REMOTE=false
DELETE_BRANCHES=false
FORCE_DELETE=false

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

# Help text for usage output
read -r -d '' HELP_TEXT <<'EOF' || true
Lists local branches that have been merged into the main branch.
Detects both regular merges and squash merges.

Usage: ./scripts/list-merged-branches.sh [options]

Options:
  -r, --remote     Check remote branches instead of local
  -d, --delete     Delete the merged branches (dry-run by default)
  -f, --force      Actually delete branches (use with -d)
  -m, --main       Specify main branch (default: main)
  -h, --help       Show this help message
EOF

usage() {
    printf '%s\n' "$HELP_TEXT"
    exit 0
}

# Track whether we've shown the gh CLI hint
GH_HINT_SHOWN=false

show_gh_hint() {
    if [[ "$GH_HINT_SHOWN" == "false" ]] && ! command -v gh > /dev/null 2>&1; then
        echo -e "${YELLOW}Note: Install GitHub CLI (gh) for improved squash-merge detection via PR status.${NC}" >&2
        GH_HINT_SHOWN=true
    fi
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -r|--remote)
            CHECK_REMOTE=true
            shift
            ;;
        -d|--delete)
            DELETE_BRANCHES=true
            shift
            ;;
        -f|--force)
            FORCE_DELETE=true
            shift
            ;;
        -m|--main)
            MAIN_BRANCH="$2"
            shift 2
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "Unknown option: $1"
            usage
            ;;
    esac
done

# Ensure we're in a git repository
if ! git rev-parse --is-inside-work-tree > /dev/null 2>&1; then
    echo "Error: Not in a git repository" >&2
    exit 1
fi

# Fetch latest from remote to ensure we have up-to-date refs
echo "Fetching latest from remote..."
git fetch --prune origin 2>/dev/null || true

# Get the main branch ref
if ! git rev-parse "origin/$MAIN_BRANCH" > /dev/null 2>&1; then
    echo "Error: Main branch 'origin/$MAIN_BRANCH' not found" >&2
    exit 1
fi

# Show hint about gh CLI if not installed
show_gh_hint

# Function to check if a branch has been squash-merged
# This works by checking if the tree diff between the branch and main is empty
# after applying the branch's changes
is_squash_merged() {
    local branch="$1"
    local main_ref="origin/$MAIN_BRANCH"

    # Get the merge base between the branch and main
    local merge_base
    merge_base=$(git merge-base "$main_ref" "$branch" 2>/dev/null) || return 1

    # Create a temporary tree by applying the branch's changes to the merge base
    # If this tree matches something in main's history, the branch was squash-merged
    local branch_tree
    branch_tree=$(git rev-parse "$branch^{tree}" 2>/dev/null) || return 1

    # Check if the branch's changes are already in main by comparing
    # the diff of (merge_base..branch) with (merge_base..main)
    # If all changes from branch are in main, the branch is merged

    # Simpler approach: use git cherry to check if commits are already applied
    # If all commits show as "-" (applied), the branch is merged
    local unmerged_commits
    unmerged_commits=$(git cherry "$main_ref" "$branch" 2>/dev/null | grep -c "^+" || true)

    if [[ "$unmerged_commits" -eq 0 ]]; then
        return 0  # All commits are merged
    fi

    # Alternative: Check via GitHub API if the branch had a merged PR
    if command -v gh > /dev/null 2>&1; then
        local branch_name="${branch#refs/heads/}"
        branch_name="${branch_name#refs/remotes/origin/}"

        # Check if there's a merged PR for this branch
        local pr_state
        pr_state=$(gh pr list --head "$branch_name" --state merged --json state --jq '.[0].state' 2>/dev/null || echo "")

        if [[ "$pr_state" == "MERGED" ]]; then
            return 0
        fi
    fi

    return 1
}

# Function to check if a branch is merged (regular merge)
is_regular_merged() {
    local branch="$1"
    local main_ref="origin/$MAIN_BRANCH"

    # Check if the branch commit is an ancestor of main
    git merge-base --is-ancestor "$branch" "$main_ref" 2>/dev/null
}

# Collect branches to check
declare -a merged_branches=()
declare -a branches_to_check=()

if [[ "$CHECK_REMOTE" == "true" ]]; then
    # Get remote branches (excluding main and HEAD)
    while IFS= read -r branch; do
        branch="${branch#  }"  # Remove leading spaces
        branch="${branch#origin/}"
        [[ "$branch" == "$MAIN_BRANCH" ]] && continue
        [[ "$branch" == "HEAD"* ]] && continue
        branches_to_check+=("origin/$branch")
    done < <(git branch -r 2>/dev/null | grep -v '\->')
else
    # Get local branches (excluding main and current)
    current_branch=$(git branch --show-current 2>/dev/null || echo "")
    while IFS= read -r branch; do
        branch="${branch#  }"  # Remove leading spaces
        branch="${branch#\* }"  # Remove current branch marker
        [[ "$branch" == "$MAIN_BRANCH" ]] && continue
        [[ "$branch" == "$current_branch" ]] && continue
        branches_to_check+=("$branch")
    done < <(git branch 2>/dev/null)
fi

echo ""
echo "Checking ${#branches_to_check[@]} branches against $MAIN_BRANCH..."
echo ""

# Check each branch
for branch in "${branches_to_check[@]}"; do
    branch_display="${branch#origin/}"

    if is_regular_merged "$branch"; then
        echo -e "${GREEN}[merged]${NC} $branch_display (regular merge)"
        merged_branches+=("$branch")
    elif is_squash_merged "$branch"; then
        echo -e "${GREEN}[merged]${NC} $branch_display (squash merge)"
        merged_branches+=("$branch")
    fi
done

echo ""
echo "Found ${#merged_branches[@]} merged branch(es)."

# Handle deletion if requested
if [[ "$DELETE_BRANCHES" == "true" && ${#merged_branches[@]} -gt 0 ]]; then
    echo ""

    # Build list of branch names for deletion
    if [[ "$CHECK_REMOTE" == "true" ]]; then
        branch_names=()
        for branch in "${merged_branches[@]}"; do
            branch_names+=("${branch#origin/}")
        done
    else
        branch_names=("${merged_branches[@]}")
    fi

    if [[ "$FORCE_DELETE" == "true" ]]; then
        echo -e "${RED}Deleting ${#branch_names[@]} merged branches...${NC}"
        if [[ "$CHECK_REMOTE" == "true" ]]; then
            printf '%s\n' "${branch_names[@]}" | xargs git push origin --delete
        else
            printf '%s\n' "${branch_names[@]}" | xargs git branch -D
        fi
    else
        echo -e "${YELLOW}Dry run - command that would be run:${NC}"
        if [[ "$CHECK_REMOTE" == "true" ]]; then
            echo "  git push origin --delete \\"
            printf '    %s \\\n' "${branch_names[@]}" | sed '$ s/ \\$//'
        else
            echo "  git branch -D \\"
            printf '    %s \\\n' "${branch_names[@]}" | sed '$ s/ \\$//'
        fi
        echo ""
        echo "Run with -f/--force to actually delete these branches."
    fi
fi
