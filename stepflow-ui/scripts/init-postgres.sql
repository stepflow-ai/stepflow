-- Initialize PostgreSQL database for Stepflow UI
-- This script is run automatically when the PostgreSQL container starts

-- Ensure the database exists (should be created by POSTGRES_DB env var)
-- CREATE DATABASE IF NOT EXISTS stepflow_ui;

-- Grant all privileges to the stepflow user
GRANT ALL PRIVILEGES ON DATABASE stepflow_ui TO stepflow;

-- Optional: Create additional users or roles here if needed
-- CREATE ROLE stepflow_readonly;
-- GRANT CONNECT ON DATABASE stepflow_ui TO stepflow_readonly;
-- GRANT USAGE ON SCHEMA public TO stepflow_readonly;
-- GRANT SELECT ON ALL TABLES IN SCHEMA public TO stepflow_readonly;

-- Set up any database-specific configurations
-- ALTER DATABASE stepflow_ui SET timezone TO 'UTC';

-- Note: Tables will be created by Prisma migrations