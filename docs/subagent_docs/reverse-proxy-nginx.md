# Nginx Reverse Proxy Configuration

This guide covers deploying Home Registry behind Nginx with Let's Encrypt HTTPS certificates. Nginx is recommended for enterprise deployments requiring fine-grained control and existing infrastructure integration.

## Prerequisites

- Home Registry running on localhost:8210 (via Docker Compose)
- Domain name pointing to your server
- Root/sudo access for Nginx and Certbot installation
- Ports 80 and 443 open in firewall

## Installation

### Ubuntu/Debian

```bash
# Update package list
sudo apt update

# Install Nginx
sudo apt install nginx -y

# Install Certbot for Let's Encrypt
sudo apt install certbot python3-certbot-nginx -y

# Verify installation
nginx -v
certbot --version
```

### RHEL/CentOS/Fedora

```bash
# Install Nginx
sudo dnf install nginx -y

# Install Certbot
sudo dnf install certbot python3-certbot-nginx -y

# Enable and start Nginx
sudo systemctl enable nginx
sudo systemctl start nginx
```

## Basic Configuration

### Step 1: Create Nginx Configuration

Create a new configuration file for Home Registry:

```bash
sudo nano /etc/nginx/sites-available/home-registry
```

Add the following configuration:

```nginx
# Basic configuration without SSL (for initial setup)
server {
    listen 80;
    listen [::]:80;
    server_name home-registry.example.com;

    # Let's Encrypt verification
    location /.well-known/acme-challenge/ {
        root /var/www/html;
    }

    # Proxy to Home Registry
    location / {
        proxy_pass http://localhost:8210;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

### Step 2: Enable Configuration

```bash
# Create symbolic link to enable site
sudo ln -s /etc/nginx/sites-available/home-registry /etc/nginx/sites-enabled/

# Test configuration
sudo nginx -t

# Reload Nginx
sudo systemctl reload nginx
```

### Step 3: Obtain SSL Certificate

```bash
# Obtain certificate (Certbot will modify Nginx config automatically)
sudo certbot --nginx -d home-registry.example.com

# Follow prompts:
# - Enter email for urgent renewal notifications
# - Agree to Terms of Service
# - Choose to redirect HTTP to HTTPS (recommended: Yes)
```

Certbot will automatically:
- Obtain SSL certificate from Let's Encrypt
- Update Nginx configuration with SSL settings
- Configure automatic renewal

### Step 4: Verify HTTPS

```bash
# Test HTTPS
curl -I https://home-registry.example.com/health

# Verify auto-renewal is configured
sudo systemctl status certbot.timer
```

## Production-Ready Configuration

For production deployments, use this enhanced configuration with security headers, rate limiting, and optimizations:

```bash
sudo nano /etc/nginx/sites-available/home-registry
```

Replace with the following:

```nginx
# Home Registry - Production Nginx Configuration
# Updated: February 2026

# Rate limiting zones
limit_req_zone $binary_remote_addr zone=api_limit:10m rate=10r/s;
limit_req_zone $binary_remote_addr zone=login_limit:10m rate=5r/m;

# WebSocket upgrade map
map $http_upgrade $connection_upgrade {
    default upgrade;
    '' close;
}

# Upstream backend
upstream home_registry_backend {
    least_conn;  # Load balancing algorithm
    server localhost:8210 max_fails=3 fail_timeout=30s;
    
    # Add more instances for high availability:
    # server localhost:8211 max_fails=3 fail_timeout=30s;
    # server localhost:8212 max_fails=3 fail_timeout=30s;
    
    keepalive 32;
}

# HTTP to HTTPS redirect
server {
    listen 80;
    listen [::]:80;
    server_name home-registry.example.com;

    # Allow ACME challenge for Let's Encrypt
    location /.well-known/acme-challenge/ {
        root /var/www/html;
    }

    # Redirect all other traffic to HTTPS
    location / {
        return 301 https://$host$request_uri;
    }
}

# HTTPS server
server {
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name home-registry.example.com;

    # SSL certificates (managed by Certbot)
    ssl_certificate /etc/letsencrypt/live/home-registry.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/home-registry.example.com/privkey.pem;

    # SSL configuration (Mozilla Modern profile - 2024)
    ssl_protocols TLSv1.3;
    ssl_prefer_server_ciphers off;
    ssl_ciphers 'TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256';
    ssl_session_cache shared:SSL:10m;
    ssl_session_timeout 10m;
    ssl_session_tickets off;

    # OCSP stapling
    ssl_stapling on;
    ssl_stapling_verify on;
    ssl_trusted_certificate /etc/letsencrypt/live/home-registry.example.com/chain.pem;
    resolver 8.8.8.8 8.8.4.4 valid=300s;
    resolver_timeout 5s;

    # Security headers
    add_header Strict-Transport-Security "max-age=63072000; includeSubDomains; preload" always;
    add_header X-Frame-Options "DENY" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-XSS-Protection "1; mode=block" always;
    add_header Referrer-Policy "strict-origin-when-cross-origin" always;

    # Logging with timing information
    access_log /var/log/nginx/home-registry-access.log combined;
    error_log /var/log/nginx/home-registry-error.log warn;

    # File upload size (matches application limit)
    client_max_body_size 25M;

    # Timeouts
    proxy_connect_timeout 60s;
    proxy_send_timeout 60s;
    proxy_read_timeout 60s;

    # Health check endpoint (no rate limiting, no auth)
    location = /health {
        proxy_pass http://home_registry_backend;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        access_log off;  # Don't log health checks
    }

    # API endpoints (rate limited)
    location /api/ {
        limit_req zone=api_limit burst=20 nodelay;
        limit_req_status 429;

        proxy_pass http://home_registry_backend;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Connection "";

        # WebSocket support (future-proofing)
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection $connection_upgrade;
    }

    # Login endpoint (stricter rate limiting)
    location = /api/auth/login {
        limit_req zone=login_limit burst=1 nodelay;
        limit_req_status 429;

        proxy_pass http://home_registry_backend;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # Static assets (long-term caching)
    location /assets/ {
        proxy_pass http://home_registry_backend;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        
        # Aggressive caching for versioned assets
        add_header Cache-Control "public, max-age=31536000, immutable";
        expires 1y;
    }

    # Uploaded images (moderate caching)
    location /uploads/ {
        proxy_pass http://home_registry_backend;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        
        add_header Cache-Control "public, max-age=604800";
        expires 7d;
    }

    # All other requests
    location / {
        proxy_pass http://home_registry_backend;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Connection "";
    }
}
```

Apply the configuration:

```bash
# Test configuration
sudo nginx -t

# Reload Nginx
sudo systemctl reload nginx
```

## Performance Tuning

Edit the main Nginx configuration for better performance:

```bash
sudo nano /etc/nginx/nginx.conf
```

Key settings to optimize:

```nginx
user www-data;
worker_processes auto;  # Auto-detect CPU cores
worker_rlimit_nofile 65535;

events {
    worker_connections 2048;
    use epoll;  # Efficient for Linux
    multi_accept on;
}

http {
    # Enable sendfile for better performance
    sendfile on;
    tcp_nopush on;
    tcp_nodelay on;
    
    # Keepalive settings
    keepalive_timeout 65;
    keepalive_requests 100;
    
    # Types hash settings
    types_hash_max_size 2048;
    server_names_hash_bucket_size 64;
    
    # Gzip compression
    gzip on;
    gzip_vary on;
    gzip_proxied any;
    gzip_comp_level 6;
    gzip_types
        text/plain
        text/css
        text/xml
        text/javascript
        application/json
        application/javascript
        application/xml+rss
        application/rss+xml
        application/atom+xml
        image/svg+xml;
    gzip_disable "msie6";
    
    # Include other configs
    include /etc/nginx/mime.types;
    include /etc/nginx/conf.d/*.conf;
    include /etc/nginx/sites-enabled/*;
}
```

Restart Nginx:

```bash
sudo systemctl restart nginx
```

## Certificate Renewal

Certbot automatically configures renewal. Verify it's working:

```bash
# Check renewal timer status
sudo systemctl status certbot.timer

# Test renewal (dry run)
sudo certbot renew --dry-run

# Manual renewal (if needed)
sudo certbot renew
```

Certificates are renewed automatically when they have 30 days or less remaining.

## High Availability Configuration

For multiple application instances, update the upstream block:

```nginx
upstream home_registry_backend {
    least_conn;  # Route to server with fewer connections
    
    server 10.0.1.10:8210 max_fails=3 fail_timeout=30s weight=1;
    server 10.0.1.11:8210 max_fails=3 fail_timeout=30s weight=1;
    server 10.0.1.12:8210 max_fails=3 fail_timeout=30s weight=1;
    
    keepalive 32;
}
```

Add health checks (requires nginx-plus or compile with health check module):

```nginx
upstream home_registry_backend {
    server 10.0.1.10:8210 max_fails=3 fail_timeout=30s;
    server 10.0.1.11:8210 max_fails=3 fail_timeout=30s;
    
    # Health check (nginx-plus only)
    # check interval=3000 rise=2 fall=3 timeout=1000 type=http;
    # check_http_send "GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n";
    # check_http_expect_alive http_2xx;
}
```

For open-source Nginx, use passive health checks (automatic with `max_fails` and `fail_timeout`).

## Monitoring

### Access Logs

View real-time requests:

```bash
# Follow access log
sudo tail -f /var/log/nginx/home-registry-access.log

# View only errors
sudo tail -f /var/log/nginx/home-registry-error.log

# Count requests by status code
sudo awk '{print $9}' /var/log/nginx/home-registry-access.log | sort | uniq -c | sort -rn
```

### Log Rotation

Nginx logs rotate automatically via logrotate. Verify configuration:

```bash
cat /etc/logrotate.d/nginx
```

### Status Module

Enable Nginx status module for monitoring:

```bash
sudo nano /etc/nginx/sites-available/home-registry
```

Add status location:

```nginx
# Status endpoint (restrict to localhost)
location /nginx_status {
    stub_status on;
    access_log off;
    allow 127.0.0.1;
    deny all;
}
```

View status:

```bash
curl http://localhost/nginx_status
```

## Troubleshooting

### Certificate Issues

```bash
# Check certificate expiry
sudo certbot certificates

# Force renewal
sudo certbot renew --force-renewal

# View Certbot logs
sudo tail -f /var/log/letsencrypt/letsencrypt.log
```

### Test Configuration

```bash
# Syntax check
sudo nginx -t

# Verbose test
sudo nginx -T | less
```

### Debug Mode

Enable debug logging temporarily:

```nginx
error_log /var/log/nginx/home-registry-error.log debug;
```

Reload Nginx and check logs:

```bash
sudo systemctl reload nginx
sudo tail -f /var/log/nginx/home-registry-error.log
```

### Common Issues

**Issue: 502 Bad Gateway**
- Check backend is running: `curl http://localhost:8210/health`
- Verify upstream servers: `sudo systemctl status docker`
- Check Nginx error logs for connection refused errors

**Issue: 413 Request Entity Too Large**
- Increase `client_max_body_size` in Nginx config (default: 25M)

**Issue: 504 Gateway Timeout**
- Increase proxy timeouts in Nginx config
- Check application performance and database queries

## Security Best Practices

1. **Keep Nginx Updated**
   ```bash
   sudo apt update && sudo apt upgrade nginx
   ```

2. **Disable Server Tokens**
   ```nginx
   http {
       server_tokens off;
   }
   ```

3. **Limit Request Methods**
   ```nginx
   if ($request_method !~ ^(GET|POST|PUT|DELETE|HEAD|OPTIONS)$ ) {
       return 405;
   }
   ```

4. **IP Whitelisting (optional)**
   ```nginx
   location /api/admin/ {
       allow 192.168.1.0/24;  # Your office network
       deny all;
       
       proxy_pass http://home_registry_backend;
   }
   ```

5. **DDoS Protection with fail2ban**
   ```bash
   sudo apt install fail2ban
   sudo nano /etc/fail2ban/jail.local
   ```

   Add:
   ```ini
   [nginx-http-auth]
   enabled = true
   
   [nginx-limit-req]
   enabled = true
   ```

## References

- [Nginx Official Documentation](https://nginx.org/en/docs/)
- [Mozilla SSL Configuration Generator](https://ssl-config.mozilla.org/)
- [Let's Encrypt Documentation](https://letsencrypt.org/docs/)
- [Certbot Documentation](https://eff-certbot.readthedocs.io/)

## Next Steps

- Configure monitoring: [Monitoring & Logging Guide](monitoring-logging.md)
- Harden security: [Security Hardening Guide](security-hardening.md)
- Set up high availability: [High Availability Guide](high-availability.md)
