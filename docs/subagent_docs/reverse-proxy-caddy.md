# Caddy Reverse Proxy Configuration

This guide covers deploying Home Registry behind Caddy with automatic HTTPS certificates from Let's Encrypt. Caddy is recommended for self-hosters and quick deployments due to its automatic SSL certificate management with zero configuration.

## Why Caddy?

**Advantages:**
- ✅ Automatic HTTPS with Let's Encrypt (zero config)
- ✅ Automatic certificate renewal (every 60 days)
- ✅ Simple, intuitive configuration file format
- ✅ HTTP/2 and HTTP/3 support out of the box
- ✅ Built-in reverse proxy with health checks
- ✅ No need for separate certbot or certificate management
- ✅ Excellent for Docker deployments

**Trade-offs:**
- Less community adoption than Nginx (but growing)
- Fewer advanced features for complex load balancing
- Higher memory usage than Nginx (but still lightweight)

**Recommended For:** Self-hosters, small to medium deployments, rapid production setup

## Installation Methods

### Method 1: Docker Compose (Recommended)

This is the easiest method and integrates seamlessly with Home Registry's Docker deployment.

Create `docker-compose-with-caddy.yml`:

```yaml
version: '3.8'

services:
  caddy:
    image: caddy:2-alpine
    container_name: home-registry-caddy
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
      - "443:443/udp"  # HTTP/3
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile
      - caddy_data:/data
      - caddy_config:/config
    environment:
      - DOMAIN=${DOMAIN}
      - ACME_EMAIL=${LETSENCRYPT_EMAIL}
    depends_on:
      - app

  app:
    image: ghcr.io/victorytek/home-registry:latest
    restart: unless-stopped
    depends_on:
      db:
        condition: service_healthy
    environment:
      DATABASE_URL: postgres://postgres:${POSTGRES_PASSWORD}@db:5432/home_inventory
      PORT: 8210
      RUST_LOG: ${RUST_LOG:-info}
    volumes:
      - appdata:/app/data
      - backups:/app/backups
      - uploads:/app/uploads

  db:
    image: postgres:17-alpine
    restart: unless-stopped
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}
      POSTGRES_DB: home_inventory
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 5s
      timeout: 5s
      retries: 5

volumes:
  caddy_data:
  caddy_config:
  pgdata:
  appdata:
  backups:
  uploads:
```

### Method 2: System Installation

Install Caddy directly on your server:

#### Ubuntu/Debian

```bash
# Install dependencies
sudo apt install -y debian-keyring debian-archive-keyring apt-transport-https

# Add Caddy repository
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | sudo tee /etc/apt/sources.list.d/caddy-stable.list

# Install Caddy
sudo apt update
sudo apt install caddy

# Verify installation
caddy version
```

#### RHEL/CentOS/Fedora

```bash
# Add Caddy repository
dnf install 'dnf-command(copr)'
dnf copr enable @caddy/caddy

# Install Caddy
dnf install caddy

# Enable and start Caddy
sudo systemctl enable caddy
sudo systemctl start caddy
```

## Basic Configuration

### Step 1: Create Caddyfile

Create `/etc/caddy/Caddyfile` (or `./Caddyfile` for Docker):

```caddyfile
# Basic configuration for Home Registry
home-registry.example.com {
    reverse_proxy localhost:8210
}
```

That's it! With just these two lines, Caddy will:
- Automatically obtain an SSL certificate from Let's Encrypt
- Configure HTTP to HTTPS redirect
- Enable HTTP/2
- Set up automatic certificate renewal

### Step 2: Start Caddy

**Docker:**
```bash
docker compose -f docker-compose-with-caddy.yml up -d
```

**System Installation:**
```bash
sudo systemctl reload caddy
```

### Step 3: Verify HTTPS

```bash
# Test HTTPS
curl -I https://home-registry.example.com/health

# Check certificate
echo | openssl s_client -servername home-registry.example.com -connect home-registry.example.com:443 2>/dev/null | openssl x509 -noout -dates
```

## Production-Ready Configuration

For production deployments with advanced features:

```caddyfile
# Home Registry - Production Caddyfile
# Automatic HTTPS with Let's Encrypt

{
    # Global settings
    email admin@example.com  # For Let's Encrypt notifications
    
    # Optional: Custom ACME server (default is Let's Encrypt)
    # acme_ca https://acme-v02.api.letsencrypt.org/directory
    
    # Admin API endpoint (for config management)
    # admin off  # Disable if not needed
}

home-registry.example.com {
    # Encode responses (gzip/zstd compression)
    encode gzip zstd
    
    # Security headers
    header {
        # HSTS with preload (2 years)
        Strict-Transport-Security "max-age=63072000; includeSubDomains; preload"
        
        # XSS Protection
        X-Content-Type-Options "nosniff"
        X-Frame-Options "DENY"
        X-XSS-Protection "1; mode=block"
        Referrer-Policy "strict-origin-when-cross-origin"
        
        # Hide server info
        -Server
    }
    
    # Request logging
    log {
        output file /var/log/caddy/home-registry-access.log {
            roll_size 100mb
            roll_keep 10
            roll_keep_for 720h
        }
        format json {
            time_format "iso8601"
            message_key "message"
        }
    }
    
    # Health check endpoint (no rate limiting)
    handle /health {
        reverse_proxy app:8210 {
            health_uri /health
            health_interval 30s
            health_timeout 5s
            health_status 200
        }
    }
    
    # Static assets (long-term caching)
    @assets {
        path /assets/*
    }
    handle @assets {
        header Cache-Control "public, max-age=31536000, immutable"
        reverse_proxy app:8210
    }
    
    # Uploaded images (moderate caching)
    @uploads {
        path /uploads/*
    }
    handle @uploads {
        header Cache-Control "public, max-age=604800"
        reverse_proxy app:8210
    }
    
    # Main reverse proxy (all other requests)
    reverse_proxy app:8210 {
        # Load balancing (for multiple instances)
        # lb_policy least_conn
        # to app2:8210
        # to app3:8210
        
        # Health check
        health_uri /health
        health_interval 30s
        health_timeout 5s
        health_status 200
        
        # Headers
        header_up Host {upstream_hostport}
        header_up X-Real-IP {remote_host}
        header_up X-Forwarded-For {remote_host}
        header_up X-Forwarded-Proto {scheme}
        
        # Timeouts
        transport http {
            dial_timeout 60s
            response_header_timeout 60s
        }
    }
}
```

## Environment Variable Configuration

For Docker deployments, use environment variables in your Caddyfile:

```caddyfile
{
    email {$ACME_EMAIL}
}

{$DOMAIN} {
    reverse_proxy app:8210 {
        health_uri /health
        health_interval {$HEALTH_CHECK_INTERVAL:30s}
    }
}
```

Then in `.env`:
```bash
DOMAIN=home-registry.example.com
ACME_EMAIL=admin@example.com
HEALTH_CHECK_INTERVAL=30s
```

## Advanced Features

### Multiple Domains

```caddyfile
home-registry.example.com, inventory.example.com {
    reverse_proxy app:8210
}

# Or with different backends
inventory-prod.example.com {
    reverse_proxy app-prod:8210
}

inventory-staging.example.com {
    reverse_proxy app-staging:8210
}
```

### Custom SSL Certificates

If you have your own certificates:

```caddyfile
home-registry.example.com {
    tls /path/to/cert.pem /path/to/key.pem
    reverse_proxy app:8210
}
```

### Rate Limiting

Caddy doesn't have built-in rate limiting, but you can use the `rate_limit` module:

```caddyfile
home-registry.example.com {
    # Install: xcaddy build --with github.com/mholt/caddy-ratelimit
    
    rate_limit {
        zone dynamic {
            key {http.request.remote.host}
            events 100
            window 1m
        }
    }
    
    reverse_proxy app:8210
}
```

### IP Whitelisting

```caddyfile
home-registry.example.com {
    # Allow specific IP ranges
    @allowed {
        remote_ip 192.168.1.0/24 10.0.0.0/8
    }
    
    handle @allowed {
        reverse_proxy app:8210
    }
    
    # Deny all others
    handle {
        abort
    }
}
```

### Basic Authentication (Admin Panel)

```caddyfile
home-registry.example.com {
    @admin {
        path /api/admin/*
    }
    
    handle @admin {
        basicauth {
            admin $2a$14$Zkx19XLiW6VYouLHR5NmfOFU0z2GTNmpkT/5qqR7hx7wHAiKAhWni  # "password"
        }
        reverse_proxy app:8210
    }
    
    # Other routes don't require auth
    handle {
        reverse_proxy app:8210
    }
}
```

Generate password hash:
```bash
caddy hash-password
```

### HTTP/3 (QUIC)

HTTP/3 is enabled by default in Caddy 2. Ensure UDP port 443 is open:

```bash
# Docker Compose
ports:
  - "443:443/udp"

# Firewall
sudo ufw allow 443/udp
```

### Automatic Certificate Renewal

Caddy automatically renews certificates. Verify renewal is working:

```bash
# Docker
docker compose logs caddy | grep -i "renew"

# System installation
sudo journalctl -u caddy | grep -i "renew"
```

Caddy renews certificates when they have 30 days or less remaining.

## High Availability Configuration

For multiple application instances:

```caddyfile
home-registry.example.com {
    reverse_proxy {
        # Load balancing strategies:
        # - round_robin (default)
        # - least_conn (route to server with fewer connections)
        # - ip_hash (sticky sessions based on client IP)
        # - random
        # - random_choose 2
        
        lb_policy least_conn
        
        to app1:8210
        to app2:8210
        to app3:8210
        
        # Health checks
        health_uri /health
        health_port 8210
        health_interval 30s
        health_timeout 5s
        health_status 200
        
        # Fail timeout
        fail_duration 30s
        max_fails 3
    }
}
```

## Monitoring

### Access Logs

View logs in JSON format:

```bash
# Docker
docker compose logs -f caddy

# System installation
sudo journalctl -u caddy -f

# View formatted JSON logs
sudo tail -f /var/log/caddy/home-registry-access.log | jq
```

### Admin API

Enable Caddy's admin API for runtime stats:

```caddyfile
{
    admin localhost:2019  # Bind to localhost only
}
```

Query metrics:

```bash
# Get configuration
curl http://localhost:2019/config/

# Get metrics (Prometheus format)
curl http://localhost:2019/metrics

# Reverse proxy info
curl http://localhost:2019/reverse_proxy/upstreams
```

### Health Checks

Monitor backend health via admin API:

```bash
curl http://localhost:2019/reverse_proxy/upstreams | jq
```

## Troubleshooting

### Certificate Issues

```bash
# View Caddy logs for ACME errors
docker compose logs caddy | grep -i "acme\|certificate"

# Common issues:
# 1. Port 80 blocked (Let's Encrypt needs HTTP challenge)
# 2. Domain not pointing to server
# 3. Rate limit hit (50 certs/week per domain)
```

### Test Configuration

```bash
# Validate Caddyfile syntax
caddy validate --config /etc/caddy/Caddyfile

# Run in adapter mode (dry run)
caddy adapt --config /etc/caddy/Caddyfile
```

### Debug Mode

Run Caddy with debug logging:

```bash
# Docker
docker compose run --rm caddy caddy run --config /etc/caddy/Caddyfile --adapter caddyfile --debug

# System installation
caddy run --config /etc/caddy/Caddyfile --debug
```

### Common Issues

**Issue: "binding to port 443: permission denied"**
- Solution: Run as root or use Linux capabilities
  ```bash
  sudo setcap cap_net_bind_service=+ep $(which caddy)
  ```

**Issue: "obtaining certificate: error; limit reached"**
- Solution: Use Let's Encrypt staging for testing
  ```caddyfile
  {
      acme_ca https://acme-staging-v02.api.letsencrypt.org/directory
  }
  ```

**Issue: Backend health check failing**
- Check backend is running: `curl http://localhost:8210/health`
- Verify network connectivity between Caddy and app containers

## Reload Configuration

Caddy reloads gracefully without downtime:

```bash
# Docker
docker compose exec caddy caddy reload --config /etc/caddy/Caddyfile

# Or restart entire stack
docker compose restart caddy

# System installation
sudo systemctl reload caddy
```

## Backup Caddy Data

Caddy stores certificates and configuration in data directory:

```bash
# Docker volumes
docker run --rm -v home-registry_caddy_data:/data -v $(pwd):/backup alpine tar czf /backup/caddy-data-backup.tar.gz -C /data .

# System installation
sudo tar czf caddy-data-backup.tar.gz -C /var/lib/caddy .
```

## Migration from Nginx

If migrating from Nginx, convert your config:

**Nginx:**
```nginx
location /api/ {
    proxy_pass http://localhost:8210;
    proxy_set_header Host $host;
}
```

**Caddy equivalent:**
```caddyfile
handle /api/* {
    reverse_proxy localhost:8210 {
        header_up Host {host}
    }
}
```

## Performance Comparison

**Caddy vs Nginx:**
- **Memory:** Caddy uses ~50-100MB vs Nginx ~20-50MB
- **CPU:** Similar performance for typical loads
- **Throughput:** Nginx slightly faster for static files
- **Ease of Use:** Caddy significantly simpler
- **HTTPS Setup:** Caddy automatic vs Nginx manual

**Verdict:** For Home Registry, the ease of use outweighs Nginx's slight performance advantage.

## Resources

- [Caddy Documentation](https://caddyserver.com/docs/)
- [Caddyfile Tutorial](https://caddyserver.com/docs/caddyfile-tutorial)
- [Caddy Community Forum](https://caddy.community/)
- [Caddy Docker Image](https://hub.docker.com/_/caddy)

## Next Steps

- Configure monitoring: [Monitoring & Logging Guide](monitoring-logging.md)
- Harden security: [Security Hardening Guide](security-hardening.md)
- Set up backups: [Database Production Guide](database-production.md)
