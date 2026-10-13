# Security Hardening Guide

This guide covers essential security measures for production deployments of Home Registry. Follow this checklist to ensure your deployment meets security best practices.

## Pre-Deployment Security Checklist

### Environment Configuration

- [ ] **Strong PostgreSQL password** (minimum 16 characters, mixed complexity)
  ```bash
  # Generate secure password
  openssl rand -base64 32 | tr -d '/+='
  ```

- [ ] **Explicit JWT secret** (32+ characters, persisted across restarts)
  ```bash
  JWT_SECRET=$(openssl rand -base64 32)
  ```

- [ ] **Appropriate rate limiting** configured
  ```bash
  RATE_LIMIT_RPS=200      # Adjust based on expected traffic
  RATE_LIMIT_BURST=400    # 2x rate limit for burst traffic
  ```

- [ ] **Production logging level** (not debug)
  ```bash
  RUST_LOG=info  # Or 'warn' for less verbose logging
  ```

- [ ] **Secure .env file permissions**
  ```bash
  chmod 600 .env
  chown root:root .env  # Or app user
  ```

### Network Security

- [ ] **Firewall configured** to allow only necessary ports
  
  ```bash
  # UFW (Ubuntu/Debian)
  sudo ufw default deny incoming
  sudo ufw default allow outgoing
  sudo ufw allow 22/tcp    # SSH (consider changing port)
  sudo ufw allow 80/tcp    # HTTP (for Let's Encrypt)
  sudo ufw allow 443/tcp   # HTTPS
  sudo ufw enable
  
  # Verify rules
  sudo ufw status verbose
  ```

- [ ] **Block direct database access** from internet
  ```bash
  # Ensure PostgreSQL not exposed on 5432
  sudo netstat -tlnp | grep 5432
  
  # If exposed, check docker-compose.yml:
  # Remove: ports: - "5432:5432"  # This exposes DB to internet!
  ```

- [ ] **Use internal Docker networks** for service communication
  ```yaml
  networks:
    frontend:  # Public-facing services
    backend:   # Database, internal services
      internal: true  # No external access
  ```

- [ ] **Disable SSH password authentication** (use keys only)
  ```bash
  sudo nano /etc/ssh/sshd_config
  # Set: PasswordAuthentication no
  sudo systemctl restart sshd
  ```

### HTTPS/TLS Configuration

- [ ] **Valid SSL certificate** from Let's Encrypt or trusted CA
- [ ] **TLSv1.3 only** (disable older protocols)
- [ ] **HSTS enabled** with preload
  ```nginx
  # Nginx
  add_header Strict-Transport-Security "max-age=63072000; includeSubDomains; preload" always;
  ```

- [ ] **Test SSL configuration** with SSL Labs
  ```bash
  # Target: A+ grade
  # Visit: https://www.ssllabs.com/ssltest/analyze.html?d=your-domain.com
  ```

- [ ] **Certificate auto-renewal** configured
  ```bash
  # Certbot timer active
  sudo systemctl status certbot.timer
  
  # Caddy handles renewal automatically
  # Verify: docker compose logs caddy | grep -i renew
  ```

### Container Security

- [ ] **Run containers as non-root** (default in Home Registry)
  ```dockerfile
  # Already configured in Dockerfile
  USER appuser
  ```

- [ ] **Drop unnecessary capabilities**
  ```yaml
  services:
    app:
      cap_drop:
        - ALL
      cap_add:
        - NET_BIND_SERVICE  # Only if binding to ports <1024
  ```

- [ ] **Enable no-new-privileges**
  ```yaml
  services:
    app:
      security_opt:
        - no-new-privileges:true
  ```

- [ ] **Read-only root filesystem** (where possible)
  ```yaml
  services:
    app:
      read_only: true
      tmpfs:
        - /tmp
        - /app/data:uid=1000,gid=1000  # Writable data directory
  ```

- [ ] **Resource limits** configured
  ```yaml
  services:
    app:
      deploy:
        resources:
          limits:
            cpus: '2.0'
            memory: 2G
          reservations:
            cpus: '0.5'
            memory: 512M
  ```

### Application Security

- [ ] **Security headers** configured (automatic in Home Registry)
  - X-Frame-Options: DENY
  - X-Content-Type-Options: nosniff
  - X-XSS-Protection: 1; mode=block
  - Referrer-Policy: strict-origin-when-cross-origin

- [ ] **CORS configured** for production domains
  ```rust
  // Modify src/main.rs if needed
  // Default: localhost/127.0.0.1
  // Add your production domain
  ```

- [ ] **Rate limiting** tested and working
  ```bash
  # Test rate limit by sending rapid requests
  for i in {1..200}; do
    curl -s -o /dev/null -w "%{http_code}\n" https://your-domain/api/items
  done
  # Should see 429 responses after exceeding limit
  ```

### Database Security

- [ ] **Strong PostgreSQL password** (different from default)
- [ ] **Database user permissions** limited (principle of least privilege)
  ```sql
  -- If using dedicated DB user (not postgres):
  CREATE USER homeregistry WITH PASSWORD 'secure_password';
  GRANT CONNECT ON DATABASE home_inventory TO homeregistry;
  GRANT ALL PRIVILEGES ON DATABASE home_inventory TO homeregistry;
  ```

- [ ] **PostgreSQL not exposed** to internet (internal network only)
- [ ] **SSL/TLS for database connections** (for remote DB)
  ```bash
  DATABASE_URL=postgres://user:pass@host:5432/db?sslmode=require
  ```

- [ ] **Regular security updates** for PostgreSQL
  ```bash
  # Update PostgreSQL Docker image
  docker compose pull db
  docker compose up -d db
  ```

### Backup Security

- [ ] **Backups encrypted** at rest and in transit
  ```bash
  # Encrypt backup with GPG
  pg_dump ... | gzip | gpg --encrypt --recipient your-key > backup.sql.gz.gpg
  ```

- [ ] **Off-site backup** storage secured
  ```bash
  # Use encrypted S3/B2 with access keys rotated regularly
  ```

- [ ] **Backup access restricted** (file permissions)
  ```bash
  chmod 600 /app/backups/*.sql.gz
  chown appuser:appgroup /app/backups/
  ```

- [ ] **Backup retention policy** documented and enforced
  ```bash
  # Example: 30 days local, 90 days off-site
  RETENTION_DAYS=30
  ```

## Post-Deployment Security Checklist

### Verification

- [ ] **HTTPS working** (no certificate warnings)
  ```bash
  curl -I https://your-domain.com/health
  ```

- [ ] **HTTP redirects to HTTPS** automatically
  ```bash
  curl -I http://your-domain.com
  # Should return 301 redirect to https://
  ```

- [ ] **Health check accessible**
  ```bash
  curl https://your-domain.com/health
  # Should return: {"status":"healthy",...}
  ```

- [ ] **Security headers present**
  ```bash
  curl -I https://your-domain.com | grep -i "strict-transport"
  ```

- [ ] **Rate limiting functional**
  ```bash
  # Test with rapid requests (see above)
  ```

- [ ] **No sensitive data in logs**
  ```bash
  docker compose logs app | grep -i "password\|secret\|token"
  # Should not show actual credentials
  ```

### Monitoring & Alerts

- [ ] **Uptime monitoring** configured
  ```bash
  # Uptime Kuma, Healthchecks.io, Pingdom, etc.
  ```

- [ ] **Log aggregation** enabled (Loki, CloudWatch, etc.)
- [ ] **Alerts for critical events** configured
  - Application down
  - Database down
  - SSL certificate expiring
  - High error rate
  - Disk space low

- [ ] **Security scanning** enabled
  ```bash
  # Trivy scan for vulnerabilities
  trivy image ghcr.io/victorytek/home-registry:latest
  ```

### Access Control

- [ ] **Strong admin password** created (first user)
- [ ] **Multi-factor authentication** considered (future feature)
- [ ] **SSH key-based authentication** only
- [ ] **Sudo access** limited to necessary users
  ```bash
  sudo visudo
  # Review sudo access
  ```

- [ ] **Docker socket access** restricted
  ```bash
  # Only docker group can access socket
  ls -la /var/run/docker.sock
  ```

## Advanced Security Hardening

### fail2ban Configuration

Protect against brute-force attacks:

```bash
# Install fail2ban
sudo apt install fail2ban

# Create Home Registry jail
sudo nano /etc/fail2ban/jail.d/home-registry.conf
```

Add configuration:

```ini
[home-registry-auth]
enabled = true
port = 80,443
filter = home-registry-auth
logpath = /var/log/nginx/home-registry-access.log  # Or Caddy logs
maxretry = 5
findtime = 600
bantime = 3600

[home-registry-rate-limit]
enabled = true
port = 80,443
filter = home-registry-rate-limit
logpath = /var/log/nginx/home-registry-access.log
maxretry = 10
findtime = 60
bantime = 600
```

Create filter:

```bash
sudo nano /etc/fail2ban/filter.d/home-registry-auth.conf
```

```ini
[Definition]
failregex = ^<HOST> .* "(POST|PUT) /api/auth/login.*" (401|403) 
ignoreregex =
```

```bash
sudo nano /etc/fail2ban/filter.d/home-registry-rate-limit.conf
```

```ini
[Definition]
failregex = ^<HOST> .* ".* /api/.*" 429 
ignoreregex =
```

Restart fail2ban:

```bash
sudo systemctl restart fail2ban
sudo fail2ban-client status
```

### SELinux/AppArmor

**AppArmor (Ubuntu/Debian):**

```bash
# Check AppArmor status
sudo aa-status

# Create Docker profile
sudo nano /etc/apparmor.d/docker-home-registry
```

```text
#include <tunables/global>

profile docker-home-registry flags=(attach_disconnected,mediate_deleted) {
  #include <abstractions/base>

  network inet tcp,
  network inet udp,
  network inet icmp,

  deny @{PROC}/* w,   # Deny write to /proc
  deny /sys/[^f]** wklx,  # Deny write to /sys

  /app/** r,
  /app/data/** rw,
  /app/backups/** rw,
  /app/uploads/** rw,
}
```

Enable profile:

```bash
sudo apparmor_parser -r -W /etc/apparmor.d/docker-home-registry
```

Apply to container:

```yaml
services:
  app:
    security_opt:
      - apparmor=docker-home-registry
```

### Secret Management

#### Docker Secrets (Docker Swarm)

```bash
# Create secrets
echo "my_postgres_password" | docker secret create postgres_password -
echo "my_jwt_secret" | docker secret create jwt_secret -
```

```yaml
services:
  app:
    secrets:
      - postgres_password
      - jwt_secret
    environment:
      DATABASE_URL: postgres://postgres:$(cat /run/secrets/postgres_password)@db:5432/home_inventory
      JWT_SECRET_FILE: /run/secrets/jwt_secret

secrets:
  postgres_password:
    external: true
  jwt_secret:
    external: true
```

#### HashiCorp Vault

For enterprise deployments:

```bash
# Install Vault
sudo apt install vault

# Initialize Vault
vault operator init

# Store secrets
vault kv put secret/home-registry \
  postgres_password="secure_password" \
  jwt_secret="secure_jwt_secret"

# Retrieve in application
export POSTGRES_PASSWORD=$(vault kv get -field=postgres_password secret/home-registry)
```

### Network Segmentation

Implement defense in depth:

```yaml
networks:
  dmz:          # Public-facing reverse proxy
    driver: bridge
  app-network:  # Application tier
    driver: bridge
    internal: false
  db-network:   # Database tier
    driver: bridge
    internal: true  # No external access

services:
  caddy:
    networks:
      - dmz
      - app-network
  
  app:
    networks:
      - app-network
      - db-network
  
  db:
    networks:
      - db-network  # Only accessible by app
```

### Regular Security Scanning

```bash
# Trivy: Comprehensive vulnerability scanner
trivy image --severity HIGH,CRITICAL ghcr.io/victorytek/home-registry:latest

# Grype: Alternative scanner
grype ghcr.io/victorytek/home-registry:latest

# Docker Scout (if using Docker Hub)
docker scout cves ghcr.io/victorytek/home-registry:latest

# Schedule regular scans
crontab -e
# Add: 0 3 * * * trivy image --severity HIGH,CRITICAL ghcr.io/victorytek/home-registry:latest | mail -s "Security Scan Report" admin@example.com
```

### Intrusion Detection

**OSSEC (Host-based IDS):**

```bash
# Install OSSEC
sudo apt install ossec-hids

# Configure alerts
sudo nano /var/ossec/etc/ossec.conf
```

**Wazuh (Modern alternative):**

```bash
# More user-friendly, includes dashboard
# See: https://wazuh.com/
```

## Security Monitoring

### Log Analysis

Monitor for suspicious activity:

```bash
# Failed authentication attempts
docker compose logs app | grep -i "authentication failed\|unauthorized"

# Unusual access patterns
docker compose logs caddy | jq 'select(.status == 403 or .status == 401)'

# Rate limit violations
docker compose logs app | grep "429"

# SQL injection attempts (should be blocked by Rust safety)
docker compose logs app | grep -i "select.*from\|union.*select\|drop.*table"
```

### Security Metrics

Track security-related metrics:

- Failed login attempts over time
- Rate limit violations
- 4xx/5xx error rates
- Certificate expiration dates
- Disk space on backups volume

## Incident Response Plan

### 1. Detection

- Monitor alerts from uptime monitoring
- Review security scan results
- Analyze access logs for anomalies

### 2. Containment

```bash
# Immediate response to compromise:

# 1. Block attacker IP (if identified)
sudo ufw deny from <attacker_ip>

# 2. Stop affected services
docker compose stop app

# 3. Isolate system (if needed)
sudo ufw default deny incoming
```

### 3. Investigation

```bash
# Collect evidence
docker compose logs > incident-logs-$(date +%Y%m%d).txt
cp /var/log/nginx/access.log incident-access-$(date +%Y%m%d).log

# Check for unauthorized changes
docker exec app find /app -type f -mtime -1  # Files modified in last 24h
```

### 4. Recovery

```bash
# Restore from backup
./restore.sh backups/backup_YYYYMMDD_HHMMSS.sql.gz --force

# Rotate secrets
./rotate-secrets.sh

# Update and restart
docker compose pull
docker compose up -d
```

### 5. Post-Incident

- Document timeline and root cause
- Update security measures to prevent recurrence
- Review and update incident response plan
- Notify users if data breach occurred (GDPR compliance)

## Compliance Considerations

### GDPR (EU)

- [ ] Data processing agreement in place
- [ ] Privacy policy published
- [ ] User consent mechanism for data collection
- [ ] Data export functionality (user can download their data)
- [ ] Data deletion functionality (right to be forgotten)
- [ ] Breach notification procedures (72 hours)
- [ ] Data retention policies documented

### CCPA (California)

- [ ] Privacy notice provided
- [ ] Opt-out mechanism for data selling (if applicable)
- [ ] Data access request process
- [ ] Data deletion request process

### HIPAA (Healthcare)

If storing health-related inventory:

- [ ] Encryption at rest and in transit
- [ ] Access logs maintained
- [ ] Business associate agreements
- [ ] Regular security risk assessments
- [ ] Audit controls implemented

## Security Update Policy

### Application Updates

```bash
# Check for updates
docker compose pull

# Review changelog
curl https://api.github.com/repos/victorytek/home-registry/releases/latest

# Apply updates
docker compose up -d

# Verify health
curl https://your-domain/health
```

### Database Updates

```bash
# Pull latest PostgreSQL image
docker compose pull db

# Backup before upgrade
./backup.sh

# Apply update
docker compose up -d db

# Verify database connectivity
docker compose exec db psql -U postgres -c "SELECT version();"
```

### Operating System Updates

```bash
# Ubuntu/Debian
sudo apt update && sudo apt upgrade -y

# Reboot if kernel updated
sudo reboot

# Verify services restart
docker compose ps
```

## Resources

- [OWASP Top 10](https://owasp.org/www-project-top-ten/)
- [CIS Docker Benchmark](https://www.cisecurity.org/benchmark/docker)
- [Mozilla SSL Configuration Generator](https://ssl-config.mozilla.org/)
- [Docker Security Best Practices](https://docs.docker.com/engine/security/)

## Next Steps

- Set up monitoring: [Monitoring & Logging Guide](monitoring-logging.md)
- Configure backups: [Database Production Guide](database-production.md)
- Plan high availability: [High Availability Guide](high-availability.md)
