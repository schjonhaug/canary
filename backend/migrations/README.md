# Kanari Database Migrations

This directory contains SQL migration files for the Kanari database schema.

## Migration System

Kanari uses a simple, file-based migration system that automatically applies database schema changes when the application starts. This is the standard and recommended approach for database migrations.

## How It Works

1. **Migration Files**: Each migration is a separate `.sql` file with a numbered prefix (e.g., `001_initial_schema.sql`)
2. **Automatic Execution**: Migrations are automatically executed in numerical order when the application starts
3. **Tracking**: A `schema_migrations` table tracks which migrations have been applied
4. **Idempotent**: Migrations are only executed once and are safely skipped on subsequent runs

## Migration Files

### 001_initial_schema.sql
- Creates the base database schema
- Establishes all core tables: `wallets`, `transaction_events`, `contact_persons`, `wallet_contacts`, `twilio_config`, `sms_logs`

### 002_add_confirmed_amount_sats.sql  
- Adds the `confirmed_amount_sats` column to `transaction_events` table
- This was part of the original implementation

### 003_replace_confirmed_amount_with_balance_total.sql
- Replaces `confirmed_amount_sats` with `balance_total` column
- Migrates existing data by setting `balance_total` to NULL for historical records
- New events will have the wallet's total balance at the time of the event

## Adding New Migrations

To add a new migration:

1. Create a new file with the next sequential number: `004_your_migration_name.sql`
2. Write your SQL statements separated by semicolons
3. Comments starting with `--` are supported
4. The migration will be automatically applied on next application startup

## Migration Safety

- **Backup First**: Always backup your database before running migrations in production
- **Test Locally**: Test migrations thoroughly in development environment
- **Atomic**: Each migration file is applied atomically - if any statement fails, the migration is rolled back
- **One-Time**: Migrations are only applied once and tracked in the `schema_migrations` table

## Example Migration

```sql
-- Description of what this migration does
-- Multiple lines of comments are supported

CREATE TABLE IF NOT EXISTS new_table (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Add a new column to existing table
ALTER TABLE existing_table ADD COLUMN new_column TEXT;

-- Create an index
CREATE INDEX idx_new_table_name ON new_table(name);
```

## Database Schema State

After all migrations are applied, the database contains:

- `wallets`: Bitcoin wallet metadata  
- `transaction_events`: Bitcoin transaction events with `balance_total` field
- `contact_persons`: SMS notification contacts
- `wallet_contacts`: Many-to-many relationship between wallets and contacts
- `twilio_config`: Twilio SMS service configuration
- `sms_logs`: SMS delivery tracking
- `schema_migrations`: Migration tracking (automatically created)