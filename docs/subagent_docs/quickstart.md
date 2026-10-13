# Quick Start: Production Deployment (15 Minutes)

Get Home Registry running in production with HTTPS in under 15 minutes using Docker Compose and Caddy for automatic SSL certificates.

## Prerequisites

Before starting, ensure you have:

- **Linux server** (Ubuntu 22.04+ or Debian 12+ recommended)
  - Minimum: 2 CPU cores, 4GB RAM, 50GB SSD
  - Recommended: 4 CPU cores, 8GB RAM, 100GB SSD
- **Docker** and **Docker Compose** installed
- **Domain name** pointing to your server's IP address
- **Ports open:** 80 (HTTP), 443 (HTTPS)
- **Email address** for Let's Encrypt notifications

### Install Docker (if needed)

```bash
# Ubuntu/Debian
curl -fsSL https://get.docker.com -o get-docker.sh
sudo sh get-docker.sh
sudo usermod -aG docker $USER
newgrp docker

# Verify installation
docker --version
docker compose version
```

## Step 1: Download Production Configuration (2 minutes)

```bash
# Create deployment directory
mkdir -p /opt/home-registry
cd /opt/home-registry

# Download production docker-compose file
curl -o docker-compose.yml https://raw.githubusercontent.com/victorytek/home-registry/main/docs/examples/docker-compose-production.yml

# Download Caddy configuration
curl -o Caddyfile https://raw.githubusercontent.com/victorytek/home-registry/main/docs/examples/Caddyfile

# Download backup scripts
curl -o backup.sh https://raw.githubusercontent.com/victorytek/home-registry/main/docs/examples/backup.sh
chmod +x backup.sh
```

Alternatively, clone the entire repository:

```bash
git clone https://github.com/victorytek/home-registry.git
cd home-registry
cp docs/examples/docker-compose-production.yml docker-compose.yml
cp docs/examples/Caddyfile .
```

## Step 2: Configure Environment (3 minutes)

Create a `.env` file with your configuration:

```bash
cat > .env << 'EOF'
# PostgreSQL Configuration
POSTGRES_PASSWORD=REPLACE_WITH_SECURE_PASSWORD_16_CHARS_MIN

# Caddy Configuration (for automatic HTTPS)
DOMAIN=home-registry.example.com
LETSENCRYPT_EMAIL=admin@example.com

# Application Configuration
JWT_SECRET=REPLACE_WITH_RANDOM_SECRET_32_CHARS
RUST_LOG=info
RATE_LIMIT_RPS=200
RATE_LIMIT_BURST=400

# Backup Configuration
ENABLE_OFFSITE=false
RETENTION_DAYS=30
EOF
```

**IMPORTANT:** Replace the following values:
- `POSTGRES_PASSWORD`: Use a strong password (16+ characters, mixed case, numbers, symbols)
- `DOMAIN`: Your actual domain (e.g., inventory.mydomain.com)
- `LETSENCRYPT_EMAIL`: Your email for certificate notifications
- `JWT_SECRET`: Generate with `openssl rand -base64 32`

### Quick Password Generation

```bash
# Generate secure PostgreSQL password
echo "POSTGRES_PASSWORD=$(openssl rand -base64 24 | tr -d '/+=')"

# Generate JWT secret
echo "JWT_SECRET=$(openssl rand -base64 32)"
```

## Step 3: Start Services (5 minutes)

```bash
# Pull latest images
docker compose pull

# Start all services (Caddy will automatically obtain SSL certificate)
docker compose up -d

# Monitor startup (wait for migrations to complete)
docker compose logs -f app
```

**What happens during startup:**
1. PostgreSQL container starts and initializes database
2. Application container runs database migrations automatically
3. Caddy obtains Let's Encrypt certificate (may take 1-2 minutes)
4. Application becomes available at your domain

### Verify Deployment

```bash
# Check all containers are running
docker compose ps

# Verify health check
curl -f https://${DOMAIN}/health

# Expected response:
# {"status":"healthy","service":"home-registry","version":"0.1.0-beta.2"}
```

## Step 4: Create Admin Account (2 minutes)

1. Open your browser and navigate to `https://your-domain.com`
2. Click "Sign Up" to create your admin account
3. Enter your details and create a strong password
4. You're now logged in as the first admin user!

## Step 5: Configure Automated Backups (3 minutes)

```bash
# Test backup script
./backup.sh

# Verify backup was created
ls -lh backups/

# Schedule daily backups with cron (2 AM daily)
(crontab -l 2>/dev/null; echo "0 2 * * * cd /opt/home-registry && ./backup.sh >> /var/log/home-registry-backup.log 2>&1") | crontab -

# Verify cron job
crontab -l
```

## Verification Checklist

After deployment, verify everything is working:

- [ ] Application accessible at `https://your-domain.com`
- [ ] HTTPS certificate valid (no browser warnings)
- [ ] HTTP automatically redirects to HTTPS
- [ ] Health check returns 200: `curl https://your-domain.com/health`
- [ ] Can create admin account and login
- [ ] Backup script runs successfully
- [ ] All containers running: `docker compose ps`

## Quick SSL Verification

```bash
# Test SSL certificate
echo | openssl s_client -servername ${DOMAIN} -connect ${DOMAIN}:443 2>/dev/null | openssl x509 -noout -dates

# Check SSL grade (A+ is target)
# Visit: https://www.ssllabs.com/ssltest/analyze.html?d=your-domain.com
```

## What You Just Deployed

Your production environment now includes:

- **Home Registry Application**: Rust-based inventory management system
- **PostgreSQL 17**: Production database with automatic migrations
- **Caddy Reverse Proxy**: 
  - Automatic HTTPS with Let's Encrypt
  - Auto-renewal every 60 days
  - HTTP/2 and HTTP/3 support
  - Security headers configured
- **Automated Backups**: Daily at 2 AM with 30-day retention
- **Health Monitoring**: `/health` endpoint for uptime monitoring

## Next Steps

Now that you have a basic production deployment:

1. **Set up monitoring**: [Monitoring & Logging Guide](monitoring-logging.md)
2. **Harden security**: [Security Hardening Checklist](security-hardening.md)
3. **Configure advanced features**: [Database Production Guide](database-production.md)
4. **Set up high availability**: [High Availability Guide](high-availability.md)

## Common Quick Start Issues

### Certificate Provisioning Failed

**Symptoms:** Caddy logs show "failed to obtain certificate"

**Solutions:**
1. Verify domain DNS: `dig +short ${DOMAIN}` (should show your server IP)
2. Check ports 80/443 are open: `sudo netcat -l 80` and access from browser
3. Verify email in Caddyfile is valid
4. Check Let's Encrypt rate limits: https://crt.sh/?q=%.yourdomain.com

### Application Won't Start

**Symptoms:** App container exits immediately

**Solutions:**
```bash
# Check logs for errors
docker compose logs app

# Common issue: Database not ready
# Wait for PostgreSQL health check (15-30 seconds)
docker compose ps db

# Verify DATABASE_URL is correct
docker compose exec app env | grep DATABASE_URL
```

### Cannot Access Application

**Symptoms:** Browser shows "connection refused"

**Solutions:**
1. Verify firewall allows ports 80/443:
   ```bash
   sudo ufw status
   sudo ufw allow 80/tcp
   sudo ufw allow 443/tcp
   ```

2. Check Caddy is binding to correct ports:
   ```bash
   docker compose logs caddy
   sudo netstat -tlnp | grep 443
   ```

## Manual Deployment (Without Automatic Scripts)

If you prefer to configure everything manually:

```bash
# 1. Create directory structure
mkdir -p /opt/home-registry/{backups,data,uploads}

# 2. Create docker-compose.yml (see docs/examples/)

# 3. Create Caddyfile (see docs/examples/)

# 4. Create .env with configuration

# 5. Start services
docker compose up -d
```

## Upgrading to Latest Version

```bash
# Pull latest images
docker compose pull

# Restart with zero downtime (if using multiple instances)
docker compose up -d --no-deps app

# Or restart all services
docker compose down
docker compose up -d

# Migrations run automatically on startup
docker compose logs -f app
```

## Emergency Rollback

```bash
# Stop current version
docker compose down

# Edit docker-compose.yml to use previous version
# Change: ghcr.io/victorytek/home-registry:latest
# To:     ghcr.io/victorytek/home-registry:v0.1.0-beta.1

# Restore from backup if needed
./restore.sh backups/backup_YYYYMMDD_HHMMSS.sql.gz --force

# Start previous version
docker compose up -d
```

## Estimated Costs

**Self-Hosting (VPS):**
- Small deployment (1-10 users): $5-15/month
- Medium deployment (10-100 users): $20-50/month
- Domain name: $10-15/year
- Total: ~$75-600/year

**Cloud Providers:**
- AWS t3.medium: ~$30/month + $5 database
- DigitalOcean Droplet (4GB): $24/month
- Linode (4GB): $24/month
- Hetzner Cloud (CX21): €5.83/month (~$6)

**Recommended Providers for Self-Hosters:**
1. **Hetzner Cloud** - Best value, excellent performance
2. **DigitalOcean** - Simple, well-documented
3. **Linode** - Reliable, good support
4. **Vultr** - Competitive pricing

## Support & Documentation

- **Full Documentation**: See [docs/deployment/](../deployment/)
- **Troubleshooting**: [troubleshooting.md](troubleshooting.md)
- **GitHub Issues**: https://github.com/victorytek/home-registry/issues
- **Security Issues**: Email security@victorytek.com

---

**Deployment completed! 🎉**

You now have a production-ready Home Registry deployment with HTTPS, automated backups, and security best practices.
