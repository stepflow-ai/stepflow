# Stepflow UI

A modern web interface for the Stepflow workflow execution engine. Built with Next.js, React, and TypeScript.

## Features

- 🔄 **Workflow Management**: Create, store, and version workflows with labels
- 🚀 **Execution Tracking**: Monitor workflow executions with real-time status
- 📊 **Visual Workflow Editor**: Interactive workflow visualization and editing
- ⚡ **Ad-hoc Execution**: Execute workflows directly without storing (great for testing)
- 🏷️ **Version Labels**: Tag workflows with semantic versions (production, staging, etc.)
- 💾 **Persistent Storage**: SQLite for development, PostgreSQL for production
- 🔌 **Plugin Integration**: Seamless integration with Stepflow components and plugins

## Quick Start

### Prerequisites

- Node.js 18+
- pnpm (recommended) or npm
- Stepflow core server running on localhost:7837

### Quick Start (Recommended)

For zero-config development:

```bash
# Clone and start - automated setup! 🚀
git clone <repository>
cd stepflow-ui
pnpm install
pnpm dev
```

The `pnpm dev` command automatically:
- Creates `.env` file if missing
- Generates Prisma client if needed
- Creates and migrates SQLite database (non-interactive)
- Checks core server connection
- Starts the development server

No prompts or user input required!

Open [http://localhost:3000](http://localhost:3000) to access the UI.

### Manual Setup (Alternative)

For complete control over the setup process:

```bash
# 1. Install dependencies
pnpm install

# 2. Full setup with database choice
./scripts/dev-setup.sh sqlite    # SQLite (default)
./scripts/dev-setup.sh postgres  # PostgreSQL with Docker

# 3. Start development server
pnpm dev
```


## Database Setup

Stepflow UI supports both SQLite (development) and PostgreSQL (production).

### SQLite (Development)
```bash
# Quick setup
./scripts/setup-database.sh dev

# Or manually
pnpm db:migrate
```

### PostgreSQL (Production)
```bash
# With Docker
docker compose -f docker-compose.dev.yml up -d postgres

# Setup database
export DATABASE_URL="postgresql://stepflow:password@localhost:5432/stepflow_ui"
./scripts/setup-database.sh prod
```

See [DATABASE_SETUP.md](DATABASE_SETUP.md) for detailed instructions.

## Development

### Available Scripts

```bash
# Development
pnpm dev              # Start development server
pnpm build            # Build for production
pnpm start            # Start production server

# Testing
pnpm test             # Run tests
pnpm test:watch       # Run tests in watch mode
pnpm lint             # Run linting

# Database
pnpm db:generate      # Generate Prisma client
pnpm db:migrate       # Run migrations
pnpm db:push          # Push schema changes
pnpm db:studio        # Open database browser
pnpm db:seed          # Seed database with sample data
pnpm db:reset         # Reset database (development only)

# API Client
pnpm generate:api-client  # Regenerate Stepflow API client
```

### Project Structure

```
stepflow-ui/
├── app/                 # Next.js app router pages
├── components/          # Reusable React components
├── lib/                 # Utilities and API clients
│   ├── hooks/          # React Query hooks
│   ├── api-types.ts    # Shared API types
│   └── ui-api-client.ts # UI server API client
├── prisma/             # Database schema and migrations
│   ├── schema.prisma   # Database schema
│   └── seed.ts         # Database seeding
├── scripts/            # Setup and utility scripts
└── __tests__/          # Test files
```

## Architecture

Stepflow UI uses a three-tier architecture:

```
Browser → UI Server → Stepflow Core Server
```

1. **Browser**: React frontend with Next.js
2. **UI Server**: Next.js API routes providing business logic
3. **Core Server**: Stepflow execution engine

### Key Components

- **Workflow Management**: Store workflow metadata and version labels
- **Execution Tracking**: Track execution status and results
- **Flow Cache**: Cache workflow definitions for performance
- **API Gateway**: Higher-level API wrapping core server operations

## Configuration

### Environment Variables

```bash
# Database
DATABASE_URL="file:./dev.db"                              # SQLite
# DATABASE_URL="postgresql://user:pass@localhost:5432/db" # PostgreSQL

# Stepflow Core Server
STEPFLOW_SERVER_URL="http://localhost:7837/api/v1"

# Environment
NODE_ENV="development"
```

### Database Schema

The UI maintains metadata about workflows while the core server handles execution:

- **Workflows**: Metadata, descriptions, flow references
- **WorkflowLabels**: Version tags (production, staging, v1.0)
- **WorkflowExecutions**: Execution metadata and status
- **FlowCache**: Cached flow definitions from core server

## API Documentation

### UI Server Endpoints

The UI server provides a higher-level API:

```bash
# Workflows
GET    /api/workflows                    # List all workflows
POST   /api/workflows                    # Store new workflow
GET    /api/workflows/{name}             # Get workflow details
PUT    /api/workflows/{name}             # Update workflow
DELETE /api/workflows/{name}             # Delete workflow

# Execution
POST   /api/workflows/{name}/execute     # Execute named workflow
POST   /api/workflows/{name}/labels/{label}/execute  # Execute by label
POST   /api/runs                         # Execute ad-hoc workflow (provide definition directly)

# Labels
GET    /api/workflows/{name}/labels      # List workflow labels
POST   /api/workflows/{name}/labels/{label}  # Create/update label
DELETE /api/workflows/{name}/labels/{label}  # Delete label

# Runs (proxy to core)
GET    /api/runs/{id}                    # Get run details
POST   /api/runs/{id}/cancel             # Cancel run
DELETE /api/runs/{id}                    # Delete run

# Components (proxy to core)
GET    /api/components                   # List available components
```

## Testing

```bash
# Run all tests
pnpm test

# Run tests with coverage
pnpm test:coverage

# Run specific test file
pnpm test __tests__/api.test.ts

# Watch mode for development
pnpm test:watch
```

### Test Structure

- **Unit Tests**: Component and utility testing
- **Integration Tests**: API endpoint testing
- **Hooks Tests**: React Query hook testing

## Deployment

### Development
```bash
pnpm dev
```

### Production Build
```bash
# Build application
pnpm build

# Start production server
pnpm start
```

### Database Migration (Production)
```bash
# Deploy migrations
npx prisma migrate deploy

# Or use setup script
DATABASE_URL="postgresql://..." ./scripts/setup-database.sh prod
```

### Docker Deployment
```bash
# PostgreSQL for production
docker compose -f docker-compose.dev.yml up -d postgres

# Update DATABASE_URL in production environment
export DATABASE_URL="postgresql://stepflow:password@localhost:5432/stepflow_ui"
```

## Troubleshooting

### Common Issues

**Database Connection Failed**
```bash
# Check if core server is running
curl http://localhost:7837/api/v1/health

# Verify database setup
pnpm db:studio
```

**API Client Issues**
```bash
# Regenerate API client
pnpm generate:api-client

# Check core server OpenAPI spec
curl http://localhost:7837/openapi.json
```

**Build Errors**
```bash
# Clear Next.js cache
rm -rf .next

# Clear dependencies
rm -rf node_modules pnpm-lock.yaml
pnpm install
```

For detailed troubleshooting, see [DATABASE_SETUP.md](DATABASE_SETUP.md).

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests for new functionality
5. Run the test suite: `pnpm test`
6. Run linting: `pnpm lint`
7. Submit a pull request

## License

This project is part of the Stepflow workflow engine. See the main repository for license information.