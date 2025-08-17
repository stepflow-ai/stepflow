#!/bin/bash
# Development Environment Setup Script for StepFlow
# This script sets up the development environment including pre-commit hooks and ICLA checks

set -e  # Exit on any error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# Helper functions
print_step() {
    echo -e "\n${CYAN}${BOLD}==> $1${NC}"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

print_info() {
    echo -e "${BLUE}ℹ $1${NC}"
}

# Check if we're in the right directory
if [ ! -f "ICLA.md" ] || [ ! -d ".git" ]; then
    print_error "This script must be run from the StepFlow project root directory"
    print_info "Please run: cd /path/to/stepflow && ./scripts/setup_dev.sh"
    exit 1
fi

echo -e "${BOLD}${BLUE}StepFlow Development Environment Setup${NC}"
echo -e "${BLUE}=====================================${NC}\n"

print_info "Setting up development environment for StepFlow project..."

# Check Python version
print_step "Checking Python version"
if command -v python3 &> /dev/null; then
    PYTHON_VERSION=$(python3 --version 2>&1 | cut -d' ' -f2)
    print_success "Python 3 found: $PYTHON_VERSION"
    PYTHON_CMD="python3"
elif command -v python &> /dev/null; then
    PYTHON_VERSION=$(python --version 2>&1 | cut -d' ' -f2)
    if [[ $PYTHON_VERSION == 3.* ]]; then
        print_success "Python 3 found: $PYTHON_VERSION"
        PYTHON_CMD="python"
    else
        print_error "Python 3 is required but Python $PYTHON_VERSION found"
        exit 1
    fi
else
    print_error "Python 3 is required but not found"
    print_info "Please install Python 3.6 or later"
    exit 1
fi

# Check if pip is available
print_step "Checking pip availability"
if command -v pip3 &> /dev/null; then
    PIP_CMD="pip3"
    print_success "pip3 found"
elif command -v pip &> /dev/null; then
    PIP_CMD="pip"
    print_success "pip found"
else
    print_error "pip is required but not found"
    exit 1
fi

# Install pre-commit
print_step "Installing pre-commit"
if command -v pre-commit &> /dev/null; then
    print_success "pre-commit already installed"
else
    print_info "Installing pre-commit..."
    if $PIP_CMD install pre-commit; then
        print_success "pre-commit installed successfully"
    else
        print_warning "Failed to install pre-commit via pip"
        print_info "Trying alternative installation methods..."

        # Try homebrew on macOS
        if [[ "$OSTYPE" == "darwin"* ]] && command -v brew &> /dev/null; then
            if brew install pre-commit; then
                print_success "pre-commit installed via homebrew"
            else
                print_error "Failed to install pre-commit"
                exit 1
            fi
        else
            print_error "Failed to install pre-commit"
            print_info "Please install pre-commit manually: https://pre-commit.com/#install"
            exit 1
        fi
    fi
fi

# Install pre-commit hooks
print_step "Installing pre-commit hooks"
if pre-commit install; then
    print_success "Pre-commit hooks installed"
else
    print_error "Failed to install pre-commit hooks"
    exit 1
fi

# Install pre-commit hooks for commit-msg (optional)
print_step "Installing additional git hooks"
if pre-commit install --hook-type commit-msg 2>/dev/null; then
    print_success "Commit message hooks installed"
else
    print_warning "Commit message hooks installation skipped (not critical)"
fi

# Check git configuration
print_step "Checking git configuration"
GIT_EMAIL=$(git config user.email 2>/dev/null || echo "")
GIT_NAME=$(git config user.name 2>/dev/null || echo "")
GIT_GITHUB=$(git config github.user 2>/dev/null || echo "")

if [ -z "$GIT_EMAIL" ]; then
    print_warning "Git email not configured"
    echo -e "Please configure git email: ${YELLOW}git config user.email 'your-email@example.com'${NC}"
else
    print_success "Git email configured: $GIT_EMAIL"
fi

if [ -z "$GIT_NAME" ]; then
    print_warning "Git name not configured"
    echo -e "Please configure git name: ${YELLOW}git config user.name 'Your Full Name'${NC}"
else
    print_success "Git name configured: $GIT_NAME"
fi

if [ -z "$GIT_GITHUB" ]; then
    print_info "Optional: Configure GitHub username with: git config github.user 'your-github-username'"
else
    print_success "GitHub username configured: $GIT_GITHUB"
fi

# Check ICLA status
print_step "Checking ICLA status"
if [ -n "$GIT_EMAIL" ]; then
    if $PYTHON_CMD scripts/check_icla.py; then
        print_success "ICLA already signed"
    else
        print_warning "ICLA not signed yet"
        echo ""
        echo -e "${YELLOW}${BOLD}IMPORTANT: You need to sign the Individual Contributor License Agreement (ICLA)${NC}"
        echo -e "${YELLOW}before making your first contribution.${NC}"
        echo ""
        echo -e "To sign the ICLA, run: ${CYAN}$PYTHON_CMD scripts/sign_icla.py${NC}"
        echo -e "This is a one-time process that grants the project rights to use your contributions."
        echo ""
    fi
else
    print_warning "Cannot check ICLA status without git email configuration"
fi

# Check if Rust is installed (for Rust development)
print_step "Checking Rust toolchain"
if command -v rustc &> /dev/null; then
    RUST_VERSION=$(rustc --version | cut -d' ' -f2)
    print_success "Rust found: $RUST_VERSION"

    if command -v cargo &> /dev/null; then
        CARGO_VERSION=$(cargo --version | cut -d' ' -f2)
        print_success "Cargo found: $CARGO_VERSION"
    fi
else
    print_warning "Rust not found"
    print_info "To work on Rust code, install Rust from: https://rustup.rs/"
fi

# Check if Node.js is installed (for TypeScript development)
print_step "Checking Node.js toolchain"
if command -v node &> /dev/null; then
    NODE_VERSION=$(node --version)
    print_success "Node.js found: $NODE_VERSION"

    if command -v npm &> /dev/null; then
        NPM_VERSION=$(npm --version)
        print_success "npm found: $NPM_VERSION"
    fi

    if command -v pnpm &> /dev/null; then
        PNPM_VERSION=$(pnpm --version)
        print_success "pnpm found: $PNPM_VERSION"
    else
        print_info "Optional: Install pnpm for faster package management: npm install -g pnpm"
    fi
else
    print_warning "Node.js not found"
    print_info "To work on TypeScript code, install Node.js from: https://nodejs.org/"
fi

# Run a test of the pre-commit hooks
print_step "Testing pre-commit setup"
if pre-commit run --all-files &>/dev/null; then
    print_success "Pre-commit hooks test passed"
else
    print_warning "Some pre-commit hooks failed (this is normal for a fresh setup)"
    print_info "Run 'pre-commit run --all-files' to see details and fix any issues"
fi

# Final summary
echo ""
echo -e "${GREEN}${BOLD}Development Environment Setup Complete!${NC}"
echo -e "${GREEN}=================================${NC}"
echo ""
echo -e "${BOLD}Next Steps:${NC}"
echo -e "1. ${CYAN}Configure git if not done:${NC}"
echo -e "   git config user.email 'your-email@example.com'"
echo -e "   git config user.name 'Your Full Name'"
echo -e "   git config github.user 'your-github-username'  # optional"
echo ""

if [ -z "$GIT_EMAIL" ] || ! $PYTHON_CMD scripts/check_icla.py &>/dev/null; then
    echo -e "2. ${YELLOW}${BOLD}Sign the ICLA (required):${NC}"
    echo -e "   ${CYAN}$PYTHON_CMD scripts/sign_icla.py${NC}"
    echo ""
fi

echo -e "3. ${CYAN}Start developing:${NC}"
echo -e "   - Make your changes"
echo -e "   - The pre-commit hooks will run automatically on commit"
echo -e "   - All checks must pass before commits are accepted"
echo ""
echo -e "4. ${CYAN}Useful commands:${NC}"
echo -e "   pre-commit run --all-files    # Run all hooks manually"
echo -e "   $PYTHON_CMD scripts/check_icla.py  # Check ICLA status"
echo -e "   cargo test                    # Run Rust tests"
echo -e "   cd sdks/python && uv run pytest  # Run Python tests"
echo ""
echo -e "${GREEN}Happy coding! 🚀${NC}"
