#!/bin/bash
#
# Home Registry - Automated Backup Script
#
# This script performs automated backups of the PostgreSQL database
# and optionally uploads to off-site storage (S3/Backblaze B2/Restic)
#
# Usage: ./backup.sh [--no-upload] [--retention-days N]
#
# Schedule with cron (daily at 2 AM):
#   0 2 * * * /opt/home-registry/scripts/backup.sh >> /var/log/home-registry-backup.log 2>&1
#
# Environment variables can be set in /etc/default/home-registry-backup
#

set -euo pipefail  # Exit on errors, undefined variables, pipe failures

# ============================================================================
# Configuration
# ============================================================================

# Backup directory (local storage)
BACKUP_DIR="${BACKUP_DIR:-/app/backups}"

# Retention policy (days to keep local backups)
RETENTION_DAYS="${RETENTION_DAYS:-30}"

# Docker Compose configuration
COMPOSE_FILE="${COMPOSE_FILE:-docker-compose.yml}"
DB_SERVICE="${DB_SERVICE:-db}"
DB_USER="${DB_USER:-postgres}"
DB_NAME="${DB_NAME:-home_inventory}"

# Timestamp for backup filename
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="${BACKUP_DIR}/backup_${TIMESTAMP}.sql.gz"

# Off-site backup configuration (optional)
ENABLE_OFFSITE="${ENABLE_OFFSITE:-false}"
S3_BUCKET="${S3_BUCKET:-}"                    # AWS S3 bucket name
RESTIC_REPOSITORY="${RESTIC_REPOSITORY:-}"    # Restic repository URL

# ============================================================================
# Colors for output
# ============================================================================

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# ============================================================================
# Logging functions
# ============================================================================

log_info() {
    echo -e "${GREEN}[INFO]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

log_debug() {
    if [ "${DEBUG:-false}" = "true" ]; then
        echo -e "${BLUE}[DEBUG]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
    fi
}

# ============================================================================
# Error handling
# ============================================================================

cleanup() {
    if [ $? -ne 0 ]; then
        log_error "Backup script failed!"
        if [ -f "${BACKUP_FILE}" ]; then
            log_warn "Removing incomplete backup file: ${BACKUP_FILE}"
            rm -f "${BACKUP_FILE}"
        fi
    fi
}

trap cleanup EXIT

# ============================================================================
# Pre-flight checks
# ============================================================================

log_info "Home Registry Database Backup Script"
log_info "========================================"

# Ensure backup directory exists
mkdir -p "${BACKUP_DIR}"
log_debug "Backup directory: ${BACKUP_DIR}"

# Check if Docker Compose is available
if ! command -v docker &> /dev/null; then
    log_error "Docker is not installed or not in PATH!"
    exit 1
fi

if ! command -v docker compose &> /dev/null && ! command -v docker-compose &> /dev/null; then
    log_error "Docker Compose is not installed or not in PATH!"
    exit 1
fi

# Check if database container is running
log_info "Checking database container status..."
if ! docker compose -f "${COMPOSE_FILE}" ps | grep -q "${DB_SERVICE}.*running"; then
    log_error "Database container '${DB_SERVICE}' is not running!"
    log_info "Start the database with: docker compose -f ${COMPOSE_FILE} up -d ${DB_SERVICE}"
    exit 1
fi
log_info "Database container is running"

# ============================================================================
# Create database backup
# ============================================================================

log_info "Starting database backup..."
log_info "  Database: ${DB_NAME}"
log_info "  User: ${DB_USER}"
log_info "  Output: ${BACKUP_FILE}"

# Create backup using pg_dump
log_debug "Executing pg_dump command..."
START_TIME=$(date +%s)

if docker compose -f "${COMPOSE_FILE}" exec -T "${DB_SERVICE}" \
    pg_dump -U "${DB_USER}" --no-owner --no-acl --clean --if-exists "${DB_NAME}" | \
    gzip -9 > "${BACKUP_FILE}"; then
    
    END_TIME=$(date +%s)
    DURATION=$((END_TIME - START_TIME))
    log_info "Database backup completed in ${DURATION} seconds"
else
    log_error "Database backup failed!"
    exit 1
fi

# ============================================================================
# Verify backup
# ============================================================================

log_info "Verifying backup file..."

# Check if backup file exists
if [ ! -f "${BACKUP_FILE}" ]; then
    log_error "Backup file was not created: ${BACKUP_FILE}"
    exit 1
fi

# Check if backup file is not empty
if [ ! -s "${BACKUP_FILE}" ]; then
    log_error "Backup file is empty!"
    rm -f "${BACKUP_FILE}"
    exit 1
fi

# Get backup file size
BACKUP_SIZE=$(du -h "${BACKUP_FILE}" | cut -f1)
BACKUP_SIZE_BYTES=$(stat -c%s "${BACKUP_FILE}" 2>/dev/null || stat -f%z "${BACKUP_FILE}" 2>/dev/null)

log_info "Backup file created: ${BACKUP_FILE} (${BACKUP_SIZE})"

# Verify gzip integrity
log_debug "Testing gzip integrity..."
if gunzip -t "${BACKUP_FILE}" 2>/dev/null; then
    log_info "Backup file integrity verified (gzip valid)"
else
    log_error "Backup file is corrupted! (gzip test failed)"
    exit 1
fi

# Optional: Quick check for SQL content
if gunzip -c "${BACKUP_FILE}" | head -n 1 | grep -q "^--"; then
    log_debug "Backup file contains valid SQL content"
else
    log_warn "Backup file may not contain valid SQL (header check failed)"
fi

# ============================================================================
# Off-site backup (optional)
# ============================================================================

if [ "${ENABLE_OFFSITE}" = "true" ]; then
    log_info "Off-site backup is enabled"
    
    # AWS S3 upload
    if [ -n "${S3_BUCKET}" ]; then
        log_info "Uploading to AWS S3: s3://${S3_BUCKET}/backups/"
        
        if command -v aws &> /dev/null; then
            if aws s3 cp "${BACKUP_FILE}" "s3://${S3_BUCKET}/backups/" --storage-class STANDARD_IA; then
                log_info "✓ S3 upload completed successfully"
                
                # Set lifecycle policy (optional - configure in AWS Console or with aws s3api)
                log_debug "S3 lifecycle policies should be configured in AWS Console for automatic cleanup"
            else
                log_warn "S3 upload failed (continuing with local backup)"
            fi
        else
            log_warn "AWS CLI not installed, skipping S3 upload"
            log_info "Install with: pip install awscli"
        fi
    fi
    
    # Backblaze B2 upload (alternative to S3)
    if [ -n "${B2_BUCKET:-}" ]; then
        log_info "Uploading to Backblaze B2: ${B2_BUCKET}"
        
        if command -v b2 &> /dev/null; then
            if b2 upload-file "${B2_BUCKET}" "${BACKUP_FILE}" "backups/$(basename "${BACKUP_FILE}")"; then
                log_info "✓ B2 upload completed successfully"
            else
                log_warn "B2 upload failed (continuing with local backup)"
            fi
        else
            log_warn "B2 CLI not installed, skipping B2 upload"
            log_info "Install with: pip install b2"
        fi
    fi
    
    # Restic backup (encrypted, deduplicated backups)
    if [ -n "${RESTIC_REPOSITORY}" ]; then
        log_info "Backing up to Restic repository: ${RESTIC_REPOSITORY}"
        
        if command -v restic &> /dev/null; then
            # Backup entire backup directory
            if restic -r "${RESTIC_REPOSITORY}" backup "${BACKUP_DIR}" \
                --tag "home-registry" \
                --tag "database" \
                --tag "automated"; then
                
                log_info "✓ Restic backup completed successfully"
                
                # Prune old Restic snapshots according to retention policy
                log_info "Pruning old Restic snapshots..."
                if restic -r "${RESTIC_REPOSITORY}" forget \
                    --tag "home-registry" \
                    --keep-daily 7 \
                    --keep-weekly 4 \
                    --keep-monthly 6 \
                    --prune; then
                    
                    log_info "✓ Restic pruning completed"
                else
                    log_warn "Restic pruning failed"
                fi
                
                # Optional: Check repository integrity (can be slow)
                if [ "${RESTIC_CHECK:-false}" = "true" ]; then
                    log_info "Checking Restic repository integrity..."
                    restic -r "${RESTIC_REPOSITORY}" check
                fi
            else
                log_warn "Restic backup failed (continuing with local backup)"
            fi
        else
            log_warn "Restic not installed, skipping Restic backup"
            log_info "Install from: https://restic.net/"
        fi
    fi
    
else
    log_info "Off-site backup is disabled (set ENABLE_OFFSITE=true to enable)"
fi

# ============================================================================
# Cleanup old local backups
# ============================================================================

log_info "Cleaning up backups older than ${RETENTION_DAYS} days..."

# Find and delete old backups
OLD_BACKUPS=$(find "${BACKUP_DIR}" -name "backup_*.sql.gz" -type f -mtime +${RETENTION_DAYS} 2>/dev/null || true)
DELETED_COUNT=0

if [ -n "${OLD_BACKUPS}" ]; then
    while IFS= read -r old_backup; do
        if [ -f "${old_backup}" ]; then
            OLD_SIZE=$(du -h "${old_backup}" | cut -f1)
            log_debug "Deleting old backup: ${old_backup} (${OLD_SIZE})"
            rm -f "${old_backup}"
            DELETED_COUNT=$((DELETED_COUNT + 1))
        fi
    done <<< "${OLD_BACKUPS}"
    
    if [ ${DELETED_COUNT} -gt 0 ]; then
        log_info "Deleted ${DELETED_COUNT} old backup(s)"
    fi
else
    log_info "No old backups to delete"
fi

# ============================================================================
# Backup statistics
# ============================================================================

BACKUP_COUNT=$(find "${BACKUP_DIR}" -name "backup_*.sql.gz" -type f 2>/dev/null | wc -l)
TOTAL_SIZE=$(du -sh "${BACKUP_DIR}" 2>/dev/null | cut -f1)

log_info "Backup statistics:"
log_info "  Total backups: ${BACKUP_COUNT} files"
log_info "  Total size: ${TOTAL_SIZE}"
log_info "  Retention policy: ${RETENTION_DAYS} days"

# List recent backups
log_debug "Recent backups:"
find "${BACKUP_DIR}" -name "backup_*.sql.gz" -type f -printf "%T@ %p\n" 2>/dev/null | \
    sort -rn | head -5 | while read -r timestamp filepath; do
    BACKUP_DATE=$(date -d "@${timestamp}" '+%Y-%m-%d %H:%M:%S' 2>/dev/null || date -r "${timestamp}" '+%Y-%m-%d %H:%M:%S' 2>/dev/null)
    BACKUP_SIZE=$(du -h "${filepath}" | cut -f1)
    log_debug "  ${BACKUP_DATE} - $(basename "${filepath}") (${BACKUP_SIZE})"
done

# ============================================================================
# Final summary
# ============================================================================

echo ""
log_info "════════════════════════════════════════════════════════════════"
log_info "Backup completed successfully!"
log_info "════════════════════════════════════════════════════════════════"
echo ""
echo "Summary:"
echo "  Backup File:     ${BACKUP_FILE}"
echo "  File Size:       ${BACKUP_SIZE} (${BACKUP_SIZE_BYTES} bytes)"
echo "  Retention Days:  ${RETENTION_DAYS}"
echo "  Total Backups:   ${BACKUP_COUNT}"
echo "  Total Size:      ${TOTAL_SIZE}"

if [ "${ENABLE_OFFSITE}" = "true" ]; then
    echo "  Off-site Backup: Enabled"
    [ -n "${S3_BUCKET}" ] && echo "    - AWS S3: s3://${S3_BUCKET}/backups/"
    [ -n "${B2_BUCKET:-}" ] && echo "    - Backblaze B2: ${B2_BUCKET}"
    [ -n "${RESTIC_REPOSITORY}" ] && echo "    - Restic: ${RESTIC_REPOSITORY}"
else
    echo "  Off-site Backup: Disabled"
fi

echo ""
echo "Next steps:"
echo "  - Test restore procedure: ./restore.sh ${BACKUP_FILE}"
echo "  - Verify off-site backups are accessible"
echo "  - Review log file for any warnings"
echo ""

exit 0

# ============================================================================
# Configuration Examples
# ============================================================================
#
# Example 1: Basic local backup (default)
#   BACKUP_DIR=/app/backups
#   RETENTION_DAYS=30
#   ENABLE_OFFSITE=false
#
# Example 2: Backup with AWS S3 off-site storage
#   ENABLE_OFFSITE=true
#   S3_BUCKET=my-backups-bucket
#   AWS_ACCESS_KEY_ID=<your-key>
#   AWS_SECRET_ACCESS_KEY=<your-secret>
#   AWS_DEFAULT_REGION=us-east-1
#
# Example 3: Backup with Restic (encrypted, deduplicated)
#   ENABLE_OFFSITE=true
#   RESTIC_REPOSITORY=s3:s3.amazonaws.com/my-restic-repo
#   RESTIC_PASSWORD=<secure-password>
#   AWS_ACCESS_KEY_ID=<your-key>
#   AWS_SECRET_ACCESS_KEY=<your-secret>
#
# Example 4: Backup with Backblaze B2
#   ENABLE_OFFSITE=true
#   B2_BUCKET=my-b2-bucket
#   B2_APPLICATION_KEY_ID=<key-id>
#   B2_APPLICATION_KEY=<key>
#
# ============================================================================
# Cron Schedule Examples
# ============================================================================
#
# Daily at 2 AM:
#   0 2 * * * /opt/home-registry/scripts/backup.sh
#
# Every 6 hours:
#   0 */6 * * * /opt/home-registry/scripts/backup.sh
#
# Weekly on Sunday at 3 AM:
#   0 3 * * 0 /opt/home-registry/scripts/backup.sh
#
# Store environment variables in /etc/default/home-registry-backup:
#   0 2 * * * . /etc/default/home-registry-backup && /opt/home-registry/scripts/backup.sh
#
