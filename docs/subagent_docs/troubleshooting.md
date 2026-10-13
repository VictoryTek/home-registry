# Troubleshooting Guide

This guide covers common issues, diagnostic techniques, and solutions for Home Registry production deployments.

## Quick Diagnostic Commands

```bash
# Check all services status
docker compose ps

# View logs
docker compose logs -f app
docker compose logs -f db
docker compose logs -f caddy

# Check health endpoint
curl -f https://your-domain.com/health

# Check resource usage
docker stats

# Test database connectivity
docker compose exec db psql -U postgres -c "SELECT version();"

# Check network connectivity
docker compose exec app ping db
```

## Common Issues

### 1. Application Won't Start

#### Symptoms
- Container exits immediately after starting
- "Database connection error" in logs
- Application stuck in restart loop

#### Diagnosis

```bash
# Check application logs
docker compose logs app

# Check database status
docker compose ps db

# Verify environment variables
docker compose exec app env | grep DATABASE_URL

# Check if database is ready
docker compose exec db pg_isready -U postgres
```

#### Solutions

**Database not ready:**
```yaml
# Ensure depends_on with condition in docker-compose.yml
services:
  app:
    depends_on:
      db:
        condition: service_healthy
```

**Wrong DATABASE_URL:**
```bash
# Verify password matches in .env
grep POSTGRES_PASSWORD .env

# Test connection string
docker compose exec app sh -c 'echo $DATABASE_URL'
```

**Database volume corrupted:**
```bash
# ⚠️ WARNING: This deletes all data!
docker compose down -v
docker compose up -d
```

**Port already in use:**
```bash
# Check what's using port 8210
sudo netstat -tlnp | grep 8210
# Or on Windows:
netstat -ano | findstr 8210

# Kill process or change PORT in .env
```

### 2. Cannot Access Application

#### Symptoms
- Browser shows "Connection refused" or "This site can't be reached"
- curl fails to connect

#### Diagnosis

```bash
# Check if application is running
docker compose ps

# Test locally
curl http://localhost:8210/health

# Test from outside
curl http://YOUR_SERVER_IP:8210/health

# Check firewall
sudo ufw status  # Linux
# Or check Windows Firewall settings

# Check if port is bound
sudo netstat -tlnp | grep 8210
```

#### Solutions

**Firewall blocking ports:**
```bash
# Allow ports 80 and 443
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
sudo ufw reload

# Windows Firewall
# Open Windows Defender Firewall → Advanced Settings → Inbound Rules
# New Rule → Port → TCP → 80,443 → Allow
```

**Application not binding to correct interface:**
```bash
# Check HOST env var (should be 0.0.0.0)
grep HOST .env

# If missing, add:
echo "HOST=0.0.0.0" >> .env
docker compose restart app
```

**Reverse proxy not configured:**
```bash
# Check reverse proxy status
docker compose logs caddy  # or nginx

# Verify configuration
docker compose exec caddy caddy validate --config /etc/caddy/Caddyfile
```

### 3. HTTPS Not Working

#### Symptoms
- Certificate errors in browser ("NET::ERR_CERT_AUTHORITY_INVALID")
- "Connection not secure" warnings
- Let's Encrypt certificate not obtained

#### Diagnosis

```bash
# Check reverse proxy logs
docker compose logs caddy | grep -i "certificate\|acme"

# Test SSL certificate
echo | openssl s_client -servername your-domain.com -connect your-domain.com:443 2>/dev/null | openssl x509 -noout -dates

# Check DNS resolution
dig +short your-domain.com
nslookup your-domain.com

# Test port 80 accessibility (needed for HTTP challenge)
curl -I http://your-domain.com/.well-known/acme-challenge/test
```

#### Solutions

**DNS not pointing to server:**
```bash
# Update DNS A record to point to your server IP
# Wait for propagation (5 minutes to 48 hours)

# Check propagation
nslookup your-domain.com 8.8.8.8
```

**Port 80 blocked:**
```bash
# Port 80 must be open for Let's Encrypt HTTP challenge
sudo ufw allow 80/tcp

# Test if port 80 is reachable
curl -I http://your-domain.com
```

**Let's Encrypt rate limit:**
```bash
# Check certificate transparency logs
curl "https://crt.sh/?q=%.your-domain.com&output=json" | jq

# If rate limited (50 certs/week per domain):
# 1. Use staging environment for testing
# 2. Wait 7 days
# 3. Use different subdomain
```

**Caddy staging environment (for testing):**
```caddyfile
{
    acme_ca https://acme-staging-v02.api.letsencrypt.org/directory
}
```

**Wrong domain in configuration:**
```bash
# Verify domain matches DNS
grep -i "domain\|server_name" Caddyfile nginx.conf
```

### 4. Slow Performance

#### Symptoms
- Requests taking >5 seconds to complete
- High CPU or memory usage
- Database queries slow

#### Diagnosis

```bash
# Check resource usage
docker stats

# Check database connections
docker compose exec db psql -U postgres -c "
    SELECT count(*), state
    FROM pg_stat_activity
    WHERE datname = 'home_inventory'
    GROUP BY state;
"

# Check slow queries (if logging enabled)
docker compose exec db psql -U postgres -d home_inventory -c "
    SELECT query, mean_exec_time, calls
    FROM pg_stat_statements
    ORDER BY mean_exec_time DESC
    LIMIT 10;
"

# Check disk I/O
iostat -x 1 10  # Linux

# Check network latency
docker compose exec app ping db
```

#### Solutions

**Too many database connections:**
```bash
# Reduce connection pool size if configured
# Monitor with:
docker compose exec db psql -U postgres -c "
SHOW max_connections;
SELECT count(*) FROM pg_stat_activity;
"
```

**Database not tuned:**
```bash
# Apply tuning (see database-production.md)
# Key settings:
# - shared_buffers = 25% of RAM
# - effective_cache_size = 75% of RAM
# - work_mem = 20MB per connection
```

**Missing indexes:**
```sql
-- Check for missing indexes
SELECT
    schemaname,
    tablename,
    attname,
    n_distinct,
    correlation
FROM pg_stats
WHERE schemaname = 'public'
    AND n_distinct > 100
ORDER BY abs(correlation) ASC;

-- Add indexes as needed
CREATE INDEX idx_items_inventory_id ON items(inventory_id);
```

**Insufficient resources:**
```yaml
# Increase container limits
services:
  app:
    deploy:
      resources:
        limits:
          cpus: '2.0'
          memory: 4G
```

**Need to VACUUM:**
```bash
# Run VACUUM ANALYZE
docker compose exec db psql -U postgres -d home_inventory -c "VACUUM ANALYZE;"
```

### 5. High Memory Usage

#### Symptoms
- OOM killer terminating containers
- Swap usage high
- System becomes unresponsive

#### Diagnosis

```bash
# Check memory usage by container
docker stats --no-stream

# Check PostgreSQL memory
docker compose exec db psql -U postgres -c "
    SELECT pg_size_pretty(pg_database_size('home_inventory'));
"

# Check system memory
free -h  # Linux
```

#### Solutions

**PostgreSQL using too much memory:**
```ini
# Reduce shared_buffers in postgresql.conf
shared_buffers = 512MB  # Was 2GB

# Reduce work_mem
work_mem = 10MB  # Was 20MB
```

**Too many connections:**
```ini
# Reduce max_connections
max_connections = 50  # Was 100
```

**Memory leak (rare in Rust):**
```bash
# Restart application
docker compose restart app

# Monitor over time
while true; do
    docker stats --no-stream app | grep app
    sleep 60
done
```

**Add swap space:**
```bash
# Create 4GB swap file (Linux)
sudo fallocate -l 4G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile

# Make permanent
echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab
```

### 6. Database Connection Errors

#### Symptoms
- "connection refused" errors
- "too many clients already" errors
- "SSL connection error" 

#### Diagnosis

```bash
# Check if database is running
docker compose ps db

# Check database logs
docker compose logs db | tail -50

# Test connection
docker compose exec app psql -h db -U postgres -d home_inventory -c "SELECT 1;"

# Check active connections
docker compose exec db psql -U postgres -c "
    SELECT count(*), state
    FROM pg_stat_activity
    WHERE datname = 'home_inventory'
    GROUP BY state;
"
```

#### Solutions

**Database not running:**
```bash
docker compose start db
docker compose logs -f db
```

**Connection string incorrect:**
```bash
# Verify DATABASE_URL format:
# postgres://USER:PASSWORD@HOST:PORT/DATABASE

# Common issues:
# - Special characters in password (use URL encoding)
# - Wrong host (should be 'db' in Docker Compose, not 'localhost')
# - Wrong port (default: 5432)
```

**Too many connections:**
```sql
-- Kill idle connections
SELECT pg_terminate_backend(pid)
FROM pg_stat_activity
WHERE datname = 'home_inventory'
  AND state = 'idle'
  AND state_change < current_timestamp - INTERVAL '5 minutes';
```

**pg_hba.conf restrictive:**
```bash
# Check pg_hba.conf
docker compose exec db cat /var/lib/postgresql/data/pg_hba.conf

# Should have:
# host all all 0.0.0.0/0 md5
# Or more restrictive based on your network
```

### 7. Backup Failures

#### Symptoms
- Backup script exits with error
- Empty or corrupt backup files
- pg_dump hangs

#### Diagnosis

```bash
# Test pg_dump manually
docker compose exec -T db pg_dump -U postgres home_inventory | head -20

# Check disk space
df -h

# Check permissions
ls -la /app/backups

# Test gzip
echo "test" | gzip > /tmp/test.gz
gunzip -t /tmp/test.gz
```

#### Solutions

**Disk full:**
```bash
# Clean old backups
find /app/backups -name "backup_*.sql.gz" -mtime +30 -delete

# Clean Docker volumes
docker system prune -a --volumes

# Increase disk space
```

**Permissions error:**
```bash
# Fix backup directory ownership
docker compose exec app chown -R appuser:appgroup /app/backups
docker compose exec app chmod -R 755 /app/backups
```

**Database locked:**
```bash
# Check for long-running queries
docker compose exec db psql -U postgres -d home_inventory -c "
    SELECT pid, now() - query_start as duration, query
    FROM pg_stat_activity
    WHERE state = 'active'
    ORDER BY duration DESC;
"

# Kill blocking query if needed
# SELECT pg_terminate_backend(PID);
```

**pg_dump version mismatch:**
```bash
# Check versions match
docker compose exec db psql -U postgres -c "SELECT version();"
pg_dump --version

# Use pg_dump from container
docker compose exec -T db pg_dump -U postgres home_inventory > backup.sql
```

### 8. Rate Limiting Issues

#### Symptoms
- Users receiving 429 "Too Many Requests" errors
- Legitimate traffic being blocked
- API clients failing

#### Diagnosis

```bash
# Check rate limit logs
docker compose logs app | grep "429"

# Count 429 responses
docker compose logs app | grep "429" | wc -l

# Check current rate limit settings
grep RATE_LIMIT .env
```

#### Solutions

**Increase rate limits:**
```bash
# Edit .env
RATE_LIMIT_RPS=200      # Was 50
RATE_LIMIT_BURST=400    # Was 100

# Restart application
docker compose restart app
```

**Whitelist trusted IPs (Nginx):**
```nginx
geo $limit {
    default 1;
    192.168.1.0/24 0;  # Your office network
    10.0.0.0/8 0;      # Internal network
}

map $limit $limit_key {
    0 "";
    1 $binary_remote_addr;
}

limit_req_zone $limit_key zone=api_limit:10m rate=10r/s;
```

**Per-user rate limiting (future feature):**
```rust
// Implement per-user rate limiting based on JWT token
// Instead of per-IP rate limiting
```

### 9. SSL Certificate Renewal Failed

#### Symptoms
- Certificate expired
- Browser warnings after 90 days
- Certbot renewal errors

#### Diagnosis

```bash
# Check certificate expiry
echo | openssl s_client -servername your-domain.com -connect your-domain.com:443 2>/dev/null | openssl x509 -noout -dates

# Check Certbot logs
sudo tail -f /var/log/letsencrypt/letsencrypt.log

# Check Caddy logs
docker compose logs caddy | grep -i "renew\|certificate"

# Check renewal timer (Certbot)
sudo systemctl status certbot.timer
```

#### Solutions

**Certbot renewal failed:**
```bash
# Test renewal
sudo certbot renew --dry-run

# Force renewal
sudo certbot renew --force-renewal

# Check timer
sudo systemctl enable certbot.timer
sudo systemctl start certbot.timer
```

**Caddy renewal failed:**
```bash
# Caddy renews automatically
# Check for errors in logs
docker compose logs caddy | grep -i "error\|fail"

# Restart Caddy
docker compose restart caddy

# If persistent, delete acme.json and restart
docker compose down
docker volume rm home-registry_caddy_data
docker compose up -d
```

**Port 80 blocked during renewal:**
```bash
# Ensure port 80 is accessible
sudo ufw allow 80/tcp

# Test HTTP challenge endpoint
curl http://your-domain.com/.well-known/acme-challenge/
```

### 10. Docker Issues

#### Symptoms
- "Cannot connect to Docker daemon" errors
- Containers won't start
- Docker commands hang

#### Diagnosis

```bash
# Check Docker service
sudo systemctl status docker

# Check Docker disk usage
docker system df

# Check Docker logs
sudo journalctl -u docker -n 100

# Test Docker
docker run hello-world
```

#### Solutions

**Docker not running:**
```bash
sudo systemctl start docker
sudo systemctl enable docker
```

**Docker out of disk space:**
```bash
# Clean up
docker system prune -a --volumes

# Check disk usage
docker system df -v
```

**Docker daemon unresponsive:**
```bash
# Restart Docker
sudo systemctl restart docker

# If unable to restart, check logs
sudo journalctl -u docker -xe
```

**Corrupted Docker data:**
```bash
# ⚠️ Nuclear option - destroys all containers/volumes
sudo systemctl stop docker
sudo rm -rf /var/lib/docker
sudo systemctl start docker
```

## Performance Tuning

### Application Tuning

```bash
# Increase file descriptor limits
ulimit -n 65535

# Tune kernel parameters
sudo sysctl -w net.core.somaxconn=4096
sudo sysctl -w net.ipv4.tcp_max_syn_backlog=4096
```

### Database Tuning

```ini
# postgresql.conf optimizations
shared_buffers = 2GB              # 25% of RAM
effective_cache_size = 6GB        # 75% of RAM
work_mem = 20MB
maintenance_work_mem = 512MB
checkpoint_completion_target = 0.9
wal_buffers = 16MB
random_page_cost = 1.1            # For SSDs
effective_io_concurrency = 200    # For SSDs
```

### Network Tuning

```bash
# Increase TCP buffer sizes
sudo sysctl -w net.core.rmem_max=16777216
sudo sysctl -w net.core.wmem_max=16777216
sudo sysctl -w net.ipv4.tcp_rmem="4096 87380 16777216"
sudo sysctl -w net.ipv4.tcp_wmem="4096 65536 16777216"
```

## Debugging Tools

### Docker Debugging

```bash
# Exec into container
docker compose exec app sh

# View container details
docker inspect app

# Check container resources
docker stats app

# View container processes
docker top app
```

### Network Debugging

```bash
# Test connectivity between containers
docker compose exec app ping db

# Check DNS resolution
docker compose exec app nslookup db

# Trace network route
docker compose exec app traceroute db

# Check open ports
docker compose exec app netstat -tlnp
```

### Database Debugging

```sql
-- Check active queries
SELECT pid, now() - query_start as duration, query, state
FROM pg_stat_activity
WHERE datname = 'home_inventory'
ORDER BY duration DESC;

-- Check table sizes
SELECT
    tablename,
    pg_size_pretty(pg_total_relation_size(tablename::text)) as size
FROM pg_tables
WHERE schemaname = 'public'
ORDER BY pg_total_relation_size(tablename::text) DESC;

-- Check index usage
SELECT
    schemaname,
    tablename,
    indexname,
    idx_scan,
    pg_size_pretty(pg_relation_size(indexrelid)) as size
FROM pg_stat_user_indexes
ORDER BY idx_scan ASC;

-- Check cache hit ratio
SELECT
    sum(heap_blks_hit) / nullif(sum(heap_blks_hit + heap_blks_read), 0) * 100 as cache_hit_ratio
FROM pg_statio_user_tables;
```

## Getting Help

### Collecting Diagnostic Information

When reporting issues, collect:

```bash
# System information
uname -a
docker --version
docker compose version

# Service status
docker compose ps

# Recent logs (last 100 lines)
docker compose logs --tail=100 > logs.txt

# Configuration (sanitize passwords!)
cat docker-compose.yml > config.txt
cat .env | sed 's/PASSWORD=.*/PASSWORD=REDACTED/' >> config.txt

# Resource usage
docker stats --no-stream >> diag.txt
df -h >> diag.txt
free -h >> diag.txt
```

### Support Channels

- **GitHub Issues:** https://github.com/victorytek/home-registry/issues
- **Documentation:** https://github.com/victorytek/home-registry/docs
- **Security Issues:** Email security@victorytek.com

## Advanced Debugging

### Enable Debug Logging

```bash
# Set debug logging
echo "RUST_LOG=debug" >> .env
docker compose restart app

# View debug logs
docker compose logs -f app
```

### Profiling

```bash
# CPU profiling (if enabled in build)
# Use perf or flamegraph tools

# Memory profiling
# Use valgrind or heaptrack
```

### Distributed Tracing

For microservices deployments, consider adding OpenTelemetry for distributed tracing.

## Resources

- [Docker Documentation](https://docs.docker.com/)
- [PostgreSQL Troubleshooting](https://wiki.postgresql.org/wiki/Troubleshooting)
- [Nginx Troubleshooting](https://nginx.org/en/docs/debugging_log.html)
- [Let's Encrypt Rate Limits](https://letsencrypt.org/docs/rate-limits/)

## Next Steps

- Configure monitoring: [Monitoring & Logging Guide](monitoring-logging.md)
- Review security: [Security Hardening Guide](security-hardening.md)
- Optimize database: [Database Production Guide](database-production.md)
