#!/bin/bash

# Stepflow UI Database Setup Script
# This script sets up the database for the Stepflow UI server

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Function to setup development environment
setup_development() {
    print_status "Setting up development database (SQLite)..."

    # Copy environment file if it doesn't exist
    if [ ! -f .env ]; then
        if [ -f .env.example ]; then
            cp .env.example .env
            print_success "Created .env file from .env.example"
        else
            print_error ".env.example file not found!"
            exit 1
        fi
    else
        print_warning ".env file already exists, skipping copy"
    fi

    # Install dependencies
    print_status "Installing dependencies..."
    if command_exists pnpm; then
        pnpm install
    elif command_exists npm; then
        npm install
    else
        print_error "Neither pnpm nor npm found! Please install Node.js and pnpm."
        exit 1
    fi

    # Generate Prisma client
    print_status "Generating Prisma client..."
    if command_exists pnpm; then
        pnpm db:generate
    else
        npx prisma generate
    fi

    # Run migrations
    print_status "Running database migrations..."
    if [ ! -d "prisma/migrations" ] || [ -z "$(ls -A prisma/migrations 2>/dev/null)" ]; then
        # No migrations exist, create the initial migration
        npx prisma migrate dev --name init
    else
        # Migrations exist, apply them
        npx prisma migrate deploy
    fi

    print_success "Development database setup complete!"
    print_status "SQLite database created at: ./dev.db"
    print_status "You can now run 'pnpm dev' to start the development server"
}

# Function to setup production environment
setup_production() {
    print_status "Setting up production database (PostgreSQL)..."

    # Check if DATABASE_URL is set
    if [ -z "$DATABASE_URL" ]; then
        print_error "DATABASE_URL environment variable is not set!"
        print_status "Please set DATABASE_URL to your PostgreSQL connection string:"
        print_status "export DATABASE_URL=\"postgresql://user:password@localhost:5432/stepflow_ui\""
        exit 1
    fi

    # Check if PostgreSQL is accessible
    print_status "Testing database connection..."
    if ! npx prisma db pull >/dev/null 2>&1; then
        print_error "Cannot connect to database!"
        print_status "Please ensure:"
        print_status "1. PostgreSQL is running"
        print_status "2. Database exists"
        print_status "3. Credentials are correct"
        print_status "4. Network connectivity is available"
        exit 1
    fi

    print_success "Database connection successful"

    # Generate Prisma client
    print_status "Generating Prisma client..."
    npx prisma generate

    # Deploy migrations
    print_status "Deploying migrations..."
    npx prisma migrate deploy

    print_success "Production database setup complete!"
}

# Function to reset database (development only)
reset_database() {
    print_warning "This will reset your database and delete ALL data!"
    read -p "Are you sure you want to continue? (y/N): " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        print_status "Resetting database..."
        if command_exists pnpm; then
            npx prisma migrate reset --force
        else
            npx prisma migrate reset --force
        fi
        print_success "Database reset complete!"
    else
        print_status "Database reset cancelled"
    fi
}

# Function to show help
show_help() {
    echo "Stepflow UI Database Setup Script"
    echo ""
    echo "Usage: $0 [COMMAND]"
    echo ""
    echo "Commands:"
    echo "  dev       Setup development database (SQLite)"
    echo "  prod      Setup production database (PostgreSQL)"
    echo "  reset     Reset development database (DESTRUCTIVE)"
    echo "  help      Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0 dev              # Setup SQLite for development"
    echo "  $0 prod             # Deploy to PostgreSQL"
    echo "  $0 reset            # Reset development database"
    echo ""
    echo "Environment Variables:"
    echo "  DATABASE_URL        PostgreSQL connection string (required for prod)"
    echo "  STEPFLOW_SERVER_URL Stepflow core server URL (default: http://localhost:7837)"
    echo ""
}

# Main script logic
case "${1:-}" in
    "dev"|"development")
        setup_development
        ;;
    "prod"|"production")
        setup_production
        ;;
    "reset")
        reset_database
        ;;
    "help"|"-h"|"--help")
        show_help
        ;;
    "")
        print_status "No command specified, setting up development environment..."
        setup_development
        ;;
    *)
        print_error "Unknown command: $1"
        show_help
        exit 1
        ;;
esac