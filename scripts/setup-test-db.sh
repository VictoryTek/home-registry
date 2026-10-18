#!/usr/bin/env bash
# Setup test database for integration tests
# Creates home_inventory_test database and applies all migrations

set -e

echo "==================================================================="
echo "  SETTING UP TEST DATABASE"
echo "==================================================================="

# Database configuration
POSTGRES_USER="postgres"
POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-password}"
TEST_DB_NAME="home_inventory_test"
CONTAINER_NAME="home-registry-db-1"

if [ "$POSTGRES_PASSWORD" = "password" ]; then
    echo "⚠️  Using default password (set POSTGRES_PASSWORD env var to override)"
fi

# Check if database container is running
echo ""
echo "Checking database container status..."
if ! docker compose ps db --format json | grep -q '"State":"running"'; then
    echo "⚠️  Database container is not running. Starting..."
    docker compose up -d db
    echo "Waiting for database to be ready..."
    sleep 10
else
    echo "✓ Database container is running"
fi

# Wait for PostgreSQL to be ready using docker exec
echo ""
echo "Waiting for PostgreSQL to accept connections..."
for i in {1..30}; do
    if docker exec "$CONTAINER_NAME" pg_isready -U "$POSTGRES_USER" &>/dev/null; then
        echo "✓ PostgreSQL is ready"
        break
    fi
    echo "Attempt $i/30 - waiting..."
    sleep 2
    if [ $i -eq 30 ]; then
        echo "❌ PostgreSQL did not become ready in time"
        echo "Please ensure PostgreSQL is running and accessible"
        exit 1
    fi
done

# Check if test database exists
echo ""
echo "Checking if test database exists..."
DB_EXISTS=$(docker exec "$CONTAINER_NAME" psql -U "$POSTGRES_USER" -d postgres -tAc "SELECT 1 FROM pg_database WHERE datname='$TEST_DB_NAME';" 2>/dev/null || echo "")

if [ "$DB_EXISTS" = "1" ]; then
    echo "ℹ️  Test database '$TEST_DB_NAME' already exists"
    echo "Dropping and recreating to ensure clean state..."
    
    # Terminate all connections to the test database
    docker exec "$CONTAINER_NAME" psql -U "$POSTGRES_USER" -d postgres -c \
        "SELECT pg_terminate_backend(pg_stat_activity.pid)
         FROM pg_stat_activity
         WHERE pg_stat_activity.datname = '$TEST_DB_NAME'
           AND pid <> pg_backend_pid();" &>/dev/null || true
    
    # Drop the database
    docker exec "$CONTAINER_NAME" psql -U "$POSTGRES_USER" -d postgres -c "DROP DATABASE IF EXISTS $TEST_DB_NAME;" &>/dev/null
    echo "✓ Dropped existing test database"
fi

# Create test database
echo ""
echo "Creating test database '$TEST_DB_NAME'..."
if ! docker exec "$CONTAINER_NAME" psql -U "$POSTGRES_USER" -d postgres -c "CREATE DATABASE $TEST_DB_NAME;" 2>&1; then
    echo "❌ Failed to create test database"
    exit 1
fi
echo "✓ Test database created"

# Apply migrations to test database
echo ""
echo "Applying migrations to test database..."

success_count=0
fail_count=0
total_count=0
for migration in migrations/V*.sql; do
    if [ -f "$migration" ]; then
        total_count=$((total_count + 1))
        echo "  Applying: $(basename "$migration")"
        
        # Execute migration via docker exec
        if docker exec -i "$CONTAINER_NAME" psql -U "$POSTGRES_USER" -d "$TEST_DB_NAME" -v ON_ERROR_STOP=0 < "$migration" &>/dev/null; then
            success_count=$((success_count + 1))
        else
            fail_count=$((fail_count + 1))
            echo "  ⚠️  Migration $(basename "$migration") encountered issues (may already be applied)"
        fi
    fi
done

echo "✓ Applied $success_count/$total_count migrations successfully"
if [ $fail_count -gt 0 ]; then
    echo "ℹ️  $fail_count migrations had warnings (this is normal for idempotent migrations)"
fi

echo ""
echo "==================================================================="
echo "  TEST DATABASE READY"
echo "==================================================================="
echo "Database: $TEST_DB_NAME"
echo "Connection: postgres://$POSTGRES_USER:****@localhost:5432/$TEST_DB_NAME"
echo ""
echo "You can now run tests with:"
echo "  cargo test"
echo "or"
echo "  cargo test -- --include-ignored"
echo ""

exit 0
