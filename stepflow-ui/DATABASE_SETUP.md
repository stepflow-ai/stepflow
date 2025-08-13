# Database Setup Guide

This guide explains how to set up and manage the database for the Stepflow UI server.

## Overview

The Stepflow UI uses Prisma ORM with support for both SQLite (development) and PostgreSQL (production) databases. The database stores workflow metadata, version labels, and execution tracking.

## Quick Start (Development)

### Zero-Config Setup (Recommended)

For the easiest development experience:

```bash
# Just install and run - everything else is automatic!
pnpm install
pnpm dev
```

The `pnpm dev` command automatically handles:
- Environment file creation
- Prisma client generation
- Database creation and migration
- Core server connectivity check

### Manual Setup (Alternative)

If you prefer manual control:

```bash
# 1. Copy environment file
cp .env.example .env

# 2. Install dependencies
pnpm install

# 3. Generate Prisma client
pnpm db:generate

# 4. Create/migrate database
pnpm db:migrate

# 5. Start development server
pnpm dev
```

Your SQLite database will be created at `./dev.db`.

## Environment Configuration

### Development (SQLite)
```bash
# .env file
DATABASE_URL="file:./dev.db"
STEPFLOW_SERVER_URL="http://localhost:7837/api/v1"
NODE_ENV="development"
```

### Production (PostgreSQL)
```bash
# .env file
DATABASE_URL="postgresql://username:password@localhost:5432/stepflow_ui"
STEPFLOW_SERVER_URL="http://localhost:7837/api/v1"
NODE_ENV="production"
```

## Database Schema

The database contains the following main tables:

### Workflows
- Stores workflow metadata and references to flow definitions
- Each workflow has a unique name and references a flow hash in the core server

### WorkflowLabels
- Version labels for workflows (e.g., "production", "staging", "v1.0")
- Allows multiple labels to point to different workflow versions

### WorkflowExecutions
- Tracks workflow execution metadata and results
- Links to executions in the core server for detailed step information

### FlowCache
- Caches flow definitions from the core server
- Reduces API calls and improves performance

## Available Commands

### Development Commands
```bash
# Generate Prisma client after schema changes
pnpm db:generate

# Create and apply migrations in development
pnpm db:migrate

# Push schema changes without creating migration files
pnpm db:push

# Open Prisma Studio (database browser)
pnpm db:studio

# Reset database (development only)
npx prisma migrate reset
```

### Production Commands
```bash
# Deploy migrations to production
npx prisma migrate deploy

# Generate client for production
npx prisma generate
```

## PostgreSQL Setup (Production)

### 1. Install PostgreSQL
```bash
# Ubuntu/Debian
sudo apt update
sudo apt install postgresql postgresql-contrib

# macOS with Homebrew
brew install postgresql
brew services start postgresql

# Docker
docker run --name stepflow-postgres \
  -e POSTGRES_DB=stepflow_ui \
  -e POSTGRES_USER=stepflow \
  -e POSTGRES_PASSWORD=password \
  -p 5432:5432 \
  -d postgres:15
```

### 2. Create Database and User
```sql
-- Connect as postgres user
sudo -u postgres psql

-- Create database and user
CREATE DATABASE stepflow_ui;
CREATE USER stepflow WITH ENCRYPTED PASSWORD 'your-secure-password';
GRANT ALL PRIVILEGES ON DATABASE stepflow_ui TO stepflow;

-- Exit
\q
```

### 3. Update Environment
```bash
DATABASE_URL="postgresql://stepflow:your-secure-password@localhost:5432/stepflow_ui"
```

### 4. Run Migrations
```bash
npx prisma migrate deploy
```

## Migration Management

### Creating Migrations
When you modify the Prisma schema, create a new migration:

```bash
# Create and apply migration
pnpm db:migrate

# Or create migration without applying
npx prisma migrate dev --create-only --name describe-your-change
```

### Migration History
Migrations are stored in `prisma/migrations/` and should be committed to version control.

### Schema Changes
1. Edit `prisma/schema.prisma`
2. Run `pnpm db:migrate` to create migration
3. Review the generated SQL in `prisma/migrations/`
4. Commit both schema and migration files

## Troubleshooting

### Common Issues

#### "Database does not exist"
```bash
# For SQLite, ensure the directory exists
mkdir -p $(dirname $(echo $DATABASE_URL | sed 's/file://'))

# For PostgreSQL, create the database
createdb stepflow_ui
```

#### "Prisma Client not generated"
```bash
pnpm db:generate
```

#### "Migration failed"
```bash
# Check database connection
npx prisma db pull

# Reset development database
npx prisma migrate reset
```

#### "Environment variable not found"
Ensure your `.env` file exists and contains `DATABASE_URL`.

### Logging
Enable Prisma query logging for debugging:

```bash
# .env
DATABASE_URL="..."
# Add query logging
DEBUG="prisma:query"
```

## Data Management

### Backup (PostgreSQL)
```bash
# Create backup
pg_dump stepflow_ui > backup.sql

# Restore backup
psql stepflow_ui < backup.sql
```

### Backup (SQLite)
```bash
# Create backup
cp dev.db backup.db

# Restore backup
cp backup.db dev.db
```

### Seeding Data
Create `prisma/seed.ts` for initial data:

```typescript
import { PrismaClient } from '@prisma/client'

const prisma = new PrismaClient()

async function main() {
  // Add seed data here
  console.log('Seeding complete')
}

main()
  .catch((e) => {
    console.error(e)
    process.exit(1)
  })
  .finally(async () => {
    await prisma.$disconnect()
  })
```

Run with: `npx prisma db seed`

## Performance Optimization

### Indexes
The schema includes important indexes for:
- Workflow name lookups
- Label queries
- Execution filtering by workflow and status

### Connection Pooling
For production PostgreSQL, consider connection pooling:

```bash
# Using PgBouncer
DATABASE_URL="postgresql://stepflow:password@localhost:6432/stepflow_ui"
```

### Query Optimization
- Use `select` to limit returned fields
- Use `include` instead of separate queries
- Consider `findUniqueOrThrow` vs `findUnique` for error handling

## Security Considerations

### Database Access
- Use strong passwords for database users
- Limit database user permissions to minimum required
- Use SSL connections in production
- Never commit database credentials to version control

### Environment Variables
- Use `.env.local` for local overrides (gitignored)
- Consider using a secrets management service in production
- Validate environment variables at startup

## Integration with Core Server

The UI database works alongside the Stepflow core server:

1. **Workflow Storage**: UI stores metadata, core server stores flow definitions
2. **Execution Tracking**: UI tracks high-level execution status, core server handles step details
3. **Cache Strategy**: UI caches flow definitions to reduce core server API calls
4. **Consistency**: UI validates workflow existence in core server before operations

This separation allows the UI to provide rich metadata and labeling while keeping the core server focused on workflow execution.