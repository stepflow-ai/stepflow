#!/bin/bash

# StepFlow UI Development Environment Setup
# This script helps set up a complete development environment

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

print_status() { echo -e "${BLUE}[INFO]${NC} $1"; }
print_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
print_warning() { echo -e "${YELLOW}[WARNING]${NC} $1"; }
print_error() { echo -e "${RED}[ERROR]${NC} $1"; }

command_exists() { command -v "$1" >/dev/null 2>&1; }

# Check prerequisites
check_prerequisites() {
    print_status "Checking prerequisites..."

    local missing_tools=()

    if ! command_exists node; then
        missing_tools+=("Node.js")
    fi

    if ! command_exists pnpm && ! command_exists npm; then
        missing_tools+=("pnpm or npm")
    fi

    if ! command_exists docker && [[ "${1:-}" == "postgres" ]]; then
        missing_tools+=("Docker (for PostgreSQL setup)")
    fi

    if [ ${#missing_tools[@]} -ne 0 ]; then
        print_error "Missing required tools:"
        printf '%s\n' "${missing_tools[@]}"
        echo ""
        print_status "Please install the missing tools and try again."
        echo ""
        print_status "Installation guides:"
        echo "  Node.js:  https://nodejs.org/"
        echo "  pnpm:     https://pnpm.io/"
        echo "  Docker:   https://docker.com/"
        exit 1
    fi

    print_success "All prerequisites satisfied"
}

# Quick setup for existing installations (used by pnpm predev)
quick_setup() {
    print_status "Quick development setup..."

    # Copy environment file if missing
    if [ ! -f .env ]; then
        if [ -f .env.example ]; then
            cp .env.example .env
            print_success "Created .env file from .env.example"
        else
            # Create basic .env if .env.example is missing
            cat > .env << 'EOF'
# Database - SQLite for development
DATABASE_URL="file:./dev.db"

# StepFlow Core Server URL
STEPFLOW_SERVER_URL="http://localhost:7837/api/v1"

# Environment
NODE_ENV="development"
EOF
            print_success "Created basic .env file"
        fi
    fi

    # Check if Prisma client needs generation
    if [ ! -d "node_modules/.prisma/client" ] || [ "prisma/schema.prisma" -nt "node_modules/.prisma/client/index.js" ]; then
        print_status "Generating Prisma client..."
        if command_exists pnpm; then
            pnpm db:generate
        else
            npx prisma generate
        fi
    fi

    # Setup database if needed
    if [ ! -d "prisma/migrations" ] || [ -z "$(ls -A prisma/migrations 2>/dev/null)" ]; then
        # No migrations exist, create the initial migration
        print_status "Creating initial database migration..."
        npx prisma migrate dev --name init
    elif [ ! -f "dev.db" ]; then
        # Migrations exist but no database, apply existing migrations
        print_status "Applying existing migrations to new database..."
        npx prisma migrate deploy
    else
        # Check if migrations are needed
        local migration_status
        migration_status=$(npx prisma migrate status 2>&1 || true)
        if echo "$migration_status" | grep -q "pending migrations\|not in sync"; then
            print_status "Applying pending migrations..."
            npx prisma migrate deploy
        fi
    fi

    # Optional: Check core server connection
    STEPFLOW_URL="${STEPFLOW_SERVER_URL:-http://localhost:7837}/api/v1"
    if command_exists curl && curl -s --connect-timeout 2 "$STEPFLOW_URL/health" >/dev/null 2>&1; then
        print_success "Connected to StepFlow core server at $STEPFLOW_URL"
    else
        print_warning "Cannot connect to StepFlow core server at $STEPFLOW_URL"
        print_warning "Make sure the core server is running for full functionality"
    fi

    print_success "Quick setup complete!"
}

# Setup with SQLite (default)
setup_sqlite() {
    print_status "Setting up development environment with SQLite..."

    # Copy environment file
    if [ ! -f .env ]; then
        cp .env.example .env
        print_success "Created .env file"
    fi

    # Install dependencies
    print_status "Installing dependencies..."
    if command_exists pnpm; then
        pnpm install
    else
        npm install
    fi

    # Setup database
    print_status "Setting up SQLite database..."
    ./scripts/setup-database.sh dev

    print_success "SQLite development environment ready!"
    print_status "Run 'pnpm dev' to start the development server"
}

# Setup with PostgreSQL
setup_postgres() {
    print_status "Setting up development environment with PostgreSQL..."

    # Start PostgreSQL with Docker
    print_status "Starting PostgreSQL container..."
    docker compose -f docker-compose.dev.yml up -d postgres

    # Wait for PostgreSQL to be ready
    print_status "Waiting for PostgreSQL to be ready..."
    local retries=30
    while [ $retries -gt 0 ]; do
        if docker compose -f docker-compose.dev.yml exec postgres pg_isready -U stepflow -d stepflow_ui >/dev/null 2>&1; then
            break
        fi
        sleep 1
        retries=$((retries - 1))
    done

    if [ $retries -eq 0 ]; then
        print_error "PostgreSQL failed to start within 30 seconds"
        exit 1
    fi

    print_success "PostgreSQL is ready"

    # Update environment file
    if [ ! -f .env ]; then
        cp .env.example .env
    fi

    # Update DATABASE_URL for PostgreSQL
    if command_exists sed; then
        sed -i.bak 's|^DATABASE_URL=.*|DATABASE_URL="postgresql://stepflow:password@localhost:5432/stepflow_ui"|' .env
        rm .env.bak 2>/dev/null || true
    else
        print_warning "Please manually update DATABASE_URL in .env to: postgresql://stepflow:password@localhost:5432/stepflow_ui"
    fi

    # Install dependencies
    print_status "Installing dependencies..."
    if command_exists pnpm; then
        pnpm install
    else
        npm install
    fi

    # Setup database
    print_status "Setting up PostgreSQL database..."
    DATABASE_URL="postgresql://stepflow:password@localhost:5432/stepflow_ui" ./scripts/setup-database.sh prod

    print_success "PostgreSQL development environment ready!"
    print_status "PostgreSQL: localhost:5432 (user: stepflow, password: password)"
    print_status "PgAdmin:    http://localhost:8081 (admin@stepflow.dev / admin)"
    print_status "Run 'pnpm dev' to start the development server"
}

# Clean up development environment
cleanup() {
    print_status "Cleaning up development environment..."

    # Stop Docker containers
    if [ -f docker-compose.dev.yml ]; then
        docker compose -f docker-compose.dev.yml down -v
        print_success "Stopped Docker containers"
    fi

    # Remove SQLite database
    if [ -f dev.db ]; then
        rm dev.db
        print_success "Removed SQLite database"
    fi

    # Remove generated files
    if [ -d node_modules ]; then
        rm -rf node_modules
        print_success "Removed node_modules"
    fi

    if [ -f .env ]; then
        read -p "Remove .env file? (y/N): " -n 1 -r
        echo
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            rm .env
            print_success "Removed .env file"
        fi
    fi

    print_success "Cleanup complete"
}

# Show help
show_help() {
    echo "StepFlow UI Development Environment Setup"
    echo ""
    echo "Usage: $0 [COMMAND]"
    echo ""
    echo "Commands:"
    echo "  quick      Quick setup (env + database only, for existing installs)"
    echo "  sqlite     Setup with SQLite (default, recommended for most development)"
    echo "  postgres   Setup with PostgreSQL (Docker required)"
    echo "  cleanup    Remove all development files and containers"
    echo "  help       Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0              # Setup with SQLite"
    echo "  $0 quick        # Quick setup for pnpm dev"
    echo "  $0 sqlite       # Setup with SQLite (explicit)"
    echo "  $0 postgres     # Setup with PostgreSQL"
    echo "  $0 cleanup      # Clean up everything"
    echo ""
    echo "After setup, run:"
    echo "  pnpm dev        # Start development server"
    echo "  pnpm db:studio  # Open database browser"
    echo ""
}

# Main script
case "${1:-sqlite}" in
    "quick")
        quick_setup
        ;;
    "sqlite"|"")
        check_prerequisites
        setup_sqlite
        ;;
    "postgres"|"postgresql")
        check_prerequisites postgres
        setup_postgres
        ;;
    "cleanup"|"clean")
        cleanup
        ;;
    "help"|"-h"|"--help")
        show_help
        ;;
    *)
        print_error "Unknown command: $1"
        show_help
        exit 1
        ;;
esac