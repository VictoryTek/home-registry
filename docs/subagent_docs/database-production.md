# Database Production Guide

This guide covers PostgreSQL configuration, tuning, backup strategies, and operational procedures for production deployments of Home Registry.

## PostgreSQL Production Configuration

### Recommended Versions

- **Current:** PostgreSQL 17 (latest stable)
- **Supported:** PostgreSQL 16, 17
- **Upgrade path:** Always test migrations on staging first

### Configuration Tuning

#### Memory Settings

For a server with **8GB RAM**:

```ini
# postgresql.conf

# Memory Configuration
shared_buffers = 2GB               # 25% of RAM
effective_cache_size = 6GB         # 75% of RAM
work_mem = 20MB                    # Per-connection sort memory
maintenance_work_mem = 512MB       # For VACUUM, CREATE INDEX
```

For other RAM sizes:
- **4GB RAM:** shared_buffers=1GB, effective_cache_size=3GB
- **16GB RAM:** shared_buffers=4GB, effective_cache_size=12GB
- **32GB RAM:** shared_buffers=8GB, effective_cache_size=24GB

#### Connection Settings

```ini
max_connections = 100              # Match application pool size + admin
idle_in_transaction_session_timeout = 300000  # 5 minutes (kill idle transactions)
statement_timeout = 60000          # 60 seconds (kill long queries)
```

#### Checkpoint and WAL

```ini
# Write-Ahead Logging
wal_buffers = 16MB
checkpoint_completion_target = 0.9
max_wal_size = 4GB
min_wal_size = 1GB

# Performance
random_page_cost = 1.1             # For SSDs (default 4.0 for HDDs)
effective_io_concurrency = 200     # For SSDs (default 1 for HDDs)
```

#### Query Planning

```ini
default_statistics_target = 100    # More accurate query plans
from_collapse_limit = 8
join_collapse_limit = 8
```

#### Logging (Production)

```ini
# Logging
log_min_duration_statement = 1000  # Log queries slower than 1 second
log_connections = on
log_disconnections = on
log_lock_waits = on
log_checkpoints = on
log_autovacuum_min_duration = 0    # Log all autovacuum activity

# Log destination
logging_collector = on
log_directory = 'log'
log_filename = 'postgresql-%Y-%m-%d_%H%M%S.log'
log_rotation_age = 1d
log_rotation_size = 100MB
log_line_prefix = '%t [%p]: [%l-1] user=%u,db=%d,app=%a,client=%h '
```

### Apply Configuration

#### Docker Compose Method

Create `postgresql.conf`:

```bash
# Generate configuration from template
docker compose exec db psql -U postgres -c "SHOW config_file;"
docker compose cp db:/var/lib/postgresql/data/postgresql.conf ./postgresql.conf
```

Edit `postgresql.conf` with your tuning parameters, then mount it:

```yaml
services:
  db:
    image: postgres:17
    volumes:
      - pgdata:/var/lib/postgresql/data
      - ./postgresql.conf:/etc/postgresql/postgresql.conf
    command: postgres -c config_file=/etc/postgresql/postgresql.conf
```

Restart database:

```bash
docker compose restart db
```

#### Verify Configuration

```bash
docker compose exec db psql -U postgres -c "SHOW shared_buffers;"
docker compose exec db psql -U postgres -c "SHOW effective_cache_size;"
docker compose exec db psql -U postgres -c "SHOW max_connections;"
```

## Connection Pool Sizing

### Application Pool Configuration

Home Registry uses `deadpool-postgres` for connection pooling. Default behavior:
- **Pool size:** 10 connections (configurable)
- **Recycling:** Fast recycling (closes connections on error)
- **Timeout:** 30 seconds to acquire connection

### Optimal Pool Size Formula

```
connections = ((2 × CPU cores) + effective_spindle_count)
```

For example:
- 4-core server with SSD: ~10 connections
- 8-core server with SSD: ~18 connections

### Custom Pool Size (if needed)

Modify `src/db/mod.rs`:

```rust
pub async fn get_pool() -> Result<Pool, Box<dyn std::error::Error>> {
    let database_url = env::var("DATABASE_URL")?;
    let mut cfg = Config::from_str(&database_url)?;
    
    // Set custom pool size
    cfg.pool = Some(PoolConfig::new(20));  // Adjust based on load
    
    let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;
    Ok(pool)
}
```

### Monitor Connection Usage

```sql
-- Active connections by database
SELECT datname, count(*) as connections
FROM pg_stat_activity
WHERE datname = 'home_inventory'
GROUP BY datname;

-- Connection states
SELECT state, count(*)
FROM pg_stat_activity
WHERE datname = 'home_inventory'
GROUP BY state;

-- Long-running queries
SELECT pid, now() - query_start as duration, query
FROM pg_stat_activity
WHERE state = 'active'
  AND now() - query_start > interval '5 seconds'
ORDER BY duration DESC;
```

## Backup Strategies

### Automated Daily Backups

The provided `backup.sh` script performs logical backups using `pg_dump`.

**Features:**
- Compressed SQL dumps (gzip)
- Automatic retention (30 days default)
- Backup verification
- Off-site upload (S3, Restic, Backblaze B2)

**Schedule with cron:**

```bash
# Edit crontab
crontab -e

# Add daily backup at 2 AM
0 2 * * * cd /opt/home-registry && ./backup.sh >> /var/log/home-registry-backup.log 2>&1

# Add weekly off-site backup
0 3 * * 0 cd /opt/home-registry && ENABLE_OFFSITE=true ./backup.sh >> /var/log/home-registry-backup.log 2>&1
```

### Backup Types

#### 1. Logical Backups (pg_dump)

**Advantages:**
- Human-readable SQL format
- Portable across PostgreSQL versions
- Selective table backup

**Disadvantages:**
- Slower for large databases (>100GB)
- No point-in-time recovery

**Usage:**

```bash
# Full database dump
docker compose exec -T db pg_dump -U postgres home_inventory | gzip > backup.sql.gz

# Schema only
docker compose exec -T db pg_dump -U postgres --schema-only home_inventory > schema.sql

# Data only
docker compose exec -T db pg_dump -U postgres --data-only home_inventory | gzip > data.sql.gz

# Specific tables
docker compose exec -T db pg_dump -U postgres -t items -t inventories home_inventory > critical_tables.sql
```

#### 2. Physical Backups (pg_basebackup)

**Advantages:**
- Faster for large databases
- Binary format, exact copy
- Point-in-time recovery (with WAL archiving)

**Disadvantages:**
- Larger file sizes
- Must be same PostgreSQL version for restore
- Requires more setup

**Usage:**

```bash
# Base backup
docker compose exec db pg_basebackup -U postgres -D /backup/base -Ft -z -P

# With WAL archiving (for PITR)
# Requires archive_mode = on in postgresql.conf
```

#### 3. Continuous Archiving (WAL-E / pgBackRest)

For enterprise deployments needing point-in-time recovery.

**pgBackRest example:**

```bash
# Install pgBackRest
sudo apt install pgbackrest

# Configure
sudo nano /etc/pgbackrest.conf
```

```ini
[global]
repo1-path=/var/lib/pgbackrest
repo1-retention-full=7
repo1-retention-diff=4

[home-inventory]
pg1-path=/var/lib/postgresql/data
pg1-port=5432
```

```bash
# Full backup
sudo -u postgres pgbackrest --stanza=home-inventory --type=full backup

# Incremental backup
sudo -u postgres pgbackrest --stanza=home-inventory --type=incr backup

# Restore
sudo -u postgres pgbackrest --stanza=home-inventory restore
```

### Off-Site Backup Storage

#### AWS S3

```bash
# Install AWS CLI
sudo apt install awscli

# Configure credentials
aws configure

# Upload backup
aws s3 cp backup.sql.gz s3://my-bucket/home-registry/backups/backup-$(date +%Y%m%d).sql.gz

# Lifecycle policy (auto-delete after 90 days)
# Configure in AWS Console: S3 → Bucket → Management → Lifecycle rules
```

#### Backblaze B2 (Cost-Effective Alternative)

```bash
# Install B2 CLI
pip install b2

# Authorize
b2 authorize-account <application_key_id> <application_key>

# Create bucket
b2 create-bucket home-registry-backups allPrivate

# Upload
b2 upload-file --noProgress home-registry-backups backup.sql.gz backups/backup-$(date +%Y%m%d).sql.gz
```

#### Restic (Encrypted, Deduplicated)

```bash
# Install Restic
sudo apt install restic

# Initialize repository
export RESTIC_REPOSITORY="s3:s3.amazonaws.com/my-bucket/restic"
export RESTIC_PASSWORD="secure_password"
restic init

# Backup
restic backup /app/backups

# Restore
restic restore latest --target /restore

# Prune old snapshots
restic forget --keep-daily 7 --keep-weekly 4 --keep-monthly 6 --prune
```

### Backup Verification

**Always test your backups!**

```bash
# Verify backup integrity
gunzip -t backup.sql.gz

# Test restore (on staging environment)
./restore.sh backup.sql.gz --force

# Verify data
docker compose exec db psql -U postgres home_inventory -c "SELECT count(*) FROM items;"
```

**Automated Verification:**

```bash
#!/bin/bash
# verify-backup.sh

BACKUP_FILE="$1"

# Test gzip integrity
if ! gunzip -t "$BACKUP_FILE"; then
    echo "ERROR: Backup file corrupted"
    exit 1
fi

# Test SQL validity (dry run)
if ! gunzip -c "$BACKUP_FILE" | docker compose exec -T db psql -U postgres -d postgres --set ON_ERROR_STOP=1 -o /dev/null 2>&1; then
    echo "WARNING: SQL may have issues (investigate)"
fi

echo "✓ Backup verification passed"
```

## Restore Procedures

### Full Restore

```bash
# Using provided restore.sh script
./restore.sh backups/backup_20260220_020000.sql.gz --force

# Manual restore
docker compose stop app
docker compose exec db psql -U postgres -c "DROP DATABASE home_inventory;"
docker compose exec db psql -U postgres -c "CREATE DATABASE home_inventory;"
gunzip -c backup.sql.gz | docker compose exec -T db psql -U postgres home_inventory
docker compose start app
```

### Partial Restore (Specific Tables)

```bash
# Restore only items table
docker compose exec -T db psql -U postgres home_inventory < <(gunzip -c backup.sql.gz | sed -n '/CREATE TABLE items/,/COPY items/p')
```

### Point-in-Time Recovery (WAL-based)

Requires WAL archiving to be configured:

```bash
# Stop PostgreSQL
docker compose stop db

# Restore base backup
tar -xzf base_backup.tar.gz -C /var/lib/postgresql/data

# Create recovery.signal file
touch /var/lib/postgresql/data/recovery.signal

# Configure recovery target
cat >> /var/lib/postgresql/data/postgresql.conf << EOF
restore_command = 'cp /var/lib/postgresql/archive/%f %p'
recovery_target_time = '2026-02-20 14:30:00'
EOF

# Start PostgreSQL (will replay WAL until target time)
docker compose start db
```

## Database Maintenance

### VACUUM and ANALYZE

```bash
# Auto-vacuum (enabled by default, monitor it)
docker compose exec db psql -U postgres -d home_inventory -c "
    SELECT schemaname, tablename, last_vacuum, last_autovacuum, last_analyze
    FROM pg_stat_user_tables
    ORDER BY last_autovacuum;
"

# Manual VACUUM (during low traffic)
docker compose exec db psql -U postgres -d home_inventory -c "VACUUM ANALYZE;"

# VACUUM FULL (locks table, use during maintenance window)
docker compose exec db psql -U postgres -d home_inventory -c "VACUUM FULL ANALYZE;"
```

### Index Maintenance

```bash
# Check for bloated indexes
docker compose exec db psql -U postgres -d home_inventory -c "
    SELECT
        schemaname,
        tablename,
        indexname,
        pg_size_pretty(pg_relation_size(indexrelid)) as size
    FROM pg_stat_user_indexes
    ORDER BY pg_relation_size(indexrelid) DESC;
"

# Rebuild indexes (during maintenance window)
docker compose exec db psql -U postgres -d home_inventory -c "REINDEX DATABASE home_inventory;"
```

### Statistics Update

```bash
# Update statistics for query planner
docker compose exec db psql -U postgres -d home_inventory -c "ANALYZE;"
```

### Database Size Monitoring

```bash
# Database size
docker compose exec db psql -U postgres -c "
    SELECT pg_size_pretty(pg_database_size('home_inventory'));
"

# Table sizes
docker compose exec db psql -U postgres -d home_inventory -c "
    SELECT
        tablename,
        pg_size_pretty(pg_total_relation_size(tablename::text)) as size
    FROM pg_tables
    WHERE schemaname = 'public'
    ORDER BY pg_total_relation_size(tablename::text) DESC;
"
```

## Performance Monitoring

### Enable pg_stat_statements

```sql
-- Add to postgresql.conf
shared_preload_libraries = 'pg_stat_statements'
pg_stat_statements.track = all
pg_stat_statements.max = 10000

-- Restart database
-- Then create extension
CREATE EXTENSION pg_stat_statements;
```

### Query Performance Analysis

```sql
-- Top 10 slowest queries
SELECT
    mean_exec_time,
    calls,
    query
FROM pg_stat_statements
ORDER BY mean_exec_time DESC
LIMIT 10;

-- Most frequently executed queries
SELECT
    calls,
    mean_exec_time,
    query
FROM pg_stat_statements
ORDER BY calls DESC
LIMIT 10;

-- Queries with highest total time
SELECT
    total_exec_time,
    calls,
    mean_exec_time,
    query
FROM pg_stat_statements
ORDER BY total_exec_time DESC
LIMIT 10;
```

### Index Usage

```sql
-- Unused indexes (consider dropping)
SELECT
    schemaname,
    tablename,
    indexname,
    idx_scan as index_scans
FROM pg_stat_user_indexes
WHERE idx_scan = 0
  AND indexrelname NOT LIKE '%_pkey'
ORDER BY pg_relation_size(indexrelid) DESC;

-- Index hit ratio (should be >99%)
SELECT
    sum(idx_blks_hit) / nullif(sum(idx_blks_hit + idx_blks_read), 0) * 100 as index_hit_ratio
FROM pg_statio_user_indexes;
```

### Cache Hit Ratio

```sql
-- Should be >99% for production
SELECT
    sum(heap_blks_hit) / nullif(sum(heap_blks_hit + heap_blks_read), 0) * 100 as cache_hit_ratio
FROM pg_statio_user_tables;
```

## Replication (High Availability)

### Streaming Replication Setup

#### Primary Server Configuration

```ini
# postgresql.conf on primary
wal_level = replica
max_wal_senders = 3
wal_keep_size = 1GB
```

```bash
# Create replication user
docker compose exec db psql -U postgres -c "
    CREATE ROLE replicator WITH REPLICATION LOGIN PASSWORD 'replica_password';
"

# Allow replication connections in pg_hba.conf
echo "host replication replicator 10.0.1.0/24 md5" >> /var/lib/postgresql/data/pg_hba.conf

# Reload configuration
docker compose exec db psql -U postgres -c "SELECT pg_reload_conf();"
```

#### Replica Server Setup

```bash
# On replica server
pg_basebackup -h primary_server -D /var/lib/postgresql/data -U replicator -P -v -R -X stream -C -S replica1

# Start replica
docker compose up -d db
```

### Monitor Replication

```sql
-- On primary: Check replication status
SELECT * FROM pg_stat_replication;

-- On replica: Check replication delay
SELECT now() - pg_last_xact_replay_timestamp() AS replication_delay;
```

## Upgrade Procedures

### Minor Version Upgrade (e.g., 17.0 → 17.1)

```bash
# Backup first!
./backup.sh

# Update image
docker compose pull db

# Restart database
docker compose up -d db

# Verify version
docker compose exec db psql -U postgres -c "SELECT version();"
```

### Major Version Upgrade (e.g., 16 → 17)

```bash
# 1. Backup
./backup.sh

# 2. Export schema and data
docker compose exec -T db pg_dumpall -U postgres > full_backup.sql

# 3. Stop services
docker compose down

# 4. Update docker-compose.yml to new PostgreSQL version
# image: postgres:17

# 5. Remove old data volume (⚠️ DANGEROUS - ensure backup is safe!)
docker volume rm home-registry_pgdata

# 6. Start new version
docker compose up -d db

# 7. Restore data
cat full_backup.sql | docker compose exec -T db psql -U postgres

# 8. Verify and start application
docker compose up -d app
```

## Disaster Recovery

### Recovery Time Objective (RTO)

**Target:** < 1 hour from catastrophic failure to service restoration

### Recovery Point Objective (RPO)

**Target:** < 24 hours of data loss (daily backups)
**Ideal:** < 1 hour (with WAL archiving)

### DR Runbook

1. **Detect failure** (monitoring alerts)
2. **Assess damage** (database corruption, data loss, etc.)
3. **Stop application** (`docker compose stop app`)
4. **Restore from latest backup** (`./restore.sh`)
5. **Verify data integrity** (spot checks, row counts)
6. **Restart application** (`docker compose start app`)
7. **Monitor for issues** (check logs, health endpoint)
8. **Document incident** (timeline, root cause, improvements)

## Resources

- [PostgreSQL Documentation](https://www.postgresql.org/docs/)
- [PostgreSQL Performance Tuning](https://wiki.postgresql.org/wiki/Performance_Optimization)
- [PGTune](https://pgtune.leopard.in.ua/) - Configuration calculator
- [pgBadger](https://github.com/darold/pgbadger) - Log analyzer

## Next Steps

- Configure monitoring: [Monitoring & Logging Guide](monitoring-logging.md)
- Set up high availability: [High Availability Guide](high-availability.md)
- Harden security: [Security Hardening Guide](security-hardening.md)
