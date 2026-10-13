# Traefik Reverse Proxy Configuration

This guide covers deploying Home Registry with Traefik, a modern reverse proxy designed for Docker and microservices. Traefik is recommended for Docker Swarm, Kubernetes, or when managing multiple containerized services.

## Why Traefik?

**Advantages:**
- ✅ Docker-native with automatic service discovery
- ✅ Automatic HTTPS with Let's Encrypt
- ✅ Built-in dashboard for monitoring
- ✅ Dynamic configuration via Docker labels
- ✅ Excellent for microservices and orchestration
- ✅ Supports multiple providers (Docker, Kubernetes, Consul, etc.)
- ✅ WebSocket support out of the box

**Trade-offs:**
- Steeper learning curve than Caddy
- More complex for simple single-service deployments
- Requires understanding of providers and entry points

**Recommended For:** Docker Swarm, Kubernetes, multi-service environments, infrastructure-as-code

## Quick Start with Docker Compose

### Step 1: Create Traefik Configuration Files

Create `traefik.yml`:

```yaml
# Traefik static configuration
api:
  dashboard: true
  insecure: false  # Disable insecure dashboard (use auth)

entryPoints:
  web:
    address: ":80"
    http:
      redirections:
        entryPoint:
          to: websecure
          scheme: https
  websecure:
    address: ":443"
    http3: {}  # Enable HTTP/3
    
providers:
  docker:
    endpoint: "unix:///var/run/docker.sock"
    exposedByDefault: false  # Only expose services with traefik.enable=true
    network: traefik-public
  file:
    filename: /etc/traefik/dynamic.yml
    watch: true

certificatesResolvers:
  letsencrypt:
    acme:
      email: admin@example.com
      storage: /letsencrypt/acme.json
      tlsChallenge: {}  # Use TLS-ALPN challenge
      # httpChallenge:  # Alternative: HTTP challenge
      #   entryPoint: web

log:
  level: INFO
  filePath: /var/log/traefik/traefik.log

accessLog:
  filePath: /var/log/traefik/access.log
  format: json
```

Create `dynamic.yml` for middleware:

```yaml
# Traefik dynamic configuration
http:
  middlewares:
    # Security headers
    security-headers:
      headers:
        stsSeconds: 63072000
        stsIncludeSubdomains: true
        stsPreload: true
        forceSTSHeader: true
        frameDeny: true
        contentTypeNosniff: true
        browserXssFilter: true
        referrerPolicy: "strict-origin-when-cross-origin"
        customResponseHeaders:
          X-Frame-Options: "DENY"
    
    # Rate limiting
    rate-limit:
      rateLimit:
        average: 100
        burst: 200
        period: 1s
    
    # API rate limiting (stricter)
    api-rate-limit:
      rateLimit:
        average: 10
        burst: 20
        period: 1s
    
    # Dashboard authentication
    dashboard-auth:
      basicAuth:
        users:
          - "admin:$apr1$xyz..." # Generate with htpasswd
```

### Step 2: Create Docker Compose Configuration

```yaml
version: '3.8'

services:
  traefik:
    image: traefik:v2.11
    container_name: traefik
    restart: unless-stopped
    security_opt:
      - no-new-privileges:true
    ports:
      - "80:80"
      - "443:443"
      - "443:443/udp"  # HTTP/3
      # - "8080:8080"  # Dashboard (optional, use with auth)
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - ./traefik.yml:/etc/traefik/traefik.yml:ro
      - ./dynamic.yml:/etc/traefik/dynamic.yml:ro
      - traefik_letsencrypt:/letsencrypt
      - traefik_logs:/var/log/traefik
    networks:
      - traefik-public
    labels:
      # Dashboard
      - "traefik.enable=true"
      - "traefik.http.routers.dashboard.rule=Host(`traefik.example.com`)"
      - "traefik.http.routers.dashboard.service=api@internal"
      - "traefik.http.routers.dashboard.entrypoints=websecure"
      - "traefik.http.routers.dashboard.tls.certresolver=letsencrypt"
      - "traefik.http.routers.dashboard.middlewares=dashboard-auth@file"

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
      JWT_SECRET: ${JWT_SECRET}
    volumes:
      - appdata:/app/data
      - backups:/app/backups
      - uploads:/app/uploads
    networks:
      - traefik-public
      - backend
    labels:
      # Enable Traefik
      - "traefik.enable=true"
      
      # Router configuration
      - "traefik.http.routers.home-registry.rule=Host(`home-registry.example.com`)"
      - "traefik.http.routers.home-registry.entrypoints=websecure"
      - "traefik.http.routers.home-registry.tls.certresolver=letsencrypt"
      
      # Service configuration
      - "traefik.http.services.home-registry.loadbalancer.server.port=8210"
      
      # Middleware
      - "traefik.http.routers.home-registry.middlewares=security-headers@file,rate-limit@file"
      
      # Health check
      - "traefik.http.services.home-registry.loadbalancer.healthcheck.path=/health"
      - "traefik.http.services.home-registry.loadbalancer.healthcheck.interval=30s"
      - "traefik.http.services.home-registry.loadbalancer.healthcheck.timeout=5s"

  db:
    image: postgres:17-alpine
    restart: unless-stopped
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}
      POSTGRES_DB: home_inventory
    volumes:
      - pgdata:/var/lib/postgresql/data
    networks:
      - backend
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 5s
      timeout: 5s
      retries: 5

networks:
  traefik-public:
    external: true
  backend:
    internal: true

volumes:
  traefik_letsencrypt:
  traefik_logs:
  pgdata:
  appdata:
  backups:
  uploads:
```

### Step 3: Initialize and Start

```bash
# Create external network
docker network create traefik-public

# Create .env file
cat > .env << 'EOF'
POSTGRES_PASSWORD=your_secure_password
JWT_SECRET=your_jwt_secret
RUST_LOG=info
EOF

# Start services
docker compose up -d

# Verify Traefik is running
docker compose logs -f traefik

# Check certificates
docker compose exec traefik ls -la /letsencrypt/
```

### Step 4: Verify HTTPS

```bash
# Test health check
curl -f https://home-registry.example.com/health

# Check certificate
echo | openssl s_client -servername home-registry.example.com -connect home-registry.example.com:443 2>/dev/null | openssl x509 -noout -dates
```

## Advanced Configuration

### Multiple Instances (Load Balancing)

Traefik automatically load balances across multiple instances:

```yaml
services:
  app:
    image: ghcr.io/victorytek/home-registry:latest
    deploy:
      replicas: 3  # Run 3 instances
    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.home-registry.rule=Host(`home-registry.example.com`)"
      - "traefik.http.routers.home-registry.entrypoints=websecure"
      - "traefik.http.routers.home-registry.tls.certresolver=letsencrypt"
      - "traefik.http.services.home-registry.loadbalancer.server.port=8210"
```

### Path-Based Routing

Route different paths to different services:

```yaml
# API v1
labels:
  - "traefik.http.routers.api-v1.rule=Host(`home-registry.example.com`) && PathPrefix(`/api/v1`)"
  - "traefik.http.services.api-v1.loadbalancer.server.port=8210"

# API v2 (different service)
labels:
  - "traefik.http.routers.api-v2.rule=Host(`home-registry.example.com`) && PathPrefix(`/api/v2`)"
  - "traefik.http.services.api-v2.loadbalancer.server.port=8211"
```

### Sticky Sessions

Enable sticky sessions for stateful applications:

```yaml
labels:
  - "traefik.http.services.home-registry.loadbalancer.sticky.cookie=true"
  - "traefik.http.services.home-registry.loadbalancer.sticky.cookie.name=home_registry_session"
  - "traefik.http.services.home-registry.loadbalancer.sticky.cookie.secure=true"
  - "traefik.http.services.home-registry.loadbalancer.sticky.cookie.httpOnly=true"
```

Note: Home Registry uses JWT tokens, so sticky sessions are not required.

### Custom Middleware

Create custom rate limiting for specific endpoints:

```yaml
# dynamic.yml
http:
  middlewares:
    login-rate-limit:
      rateLimit:
        average: 5
        burst: 10
        period: 60s
    
    api-compression:
      compress: {}
```

Apply to routers:

```yaml
labels:
  - "traefik.http.routers.login.middlewares=login-rate-limit@file"
```

### Wildcard Certificates

Use DNS challenge for wildcard certificates:

```yaml
# traefik.yml
certificatesResolvers:
  letsencrypt:
    acme:
      email: admin@example.com
      storage: /letsencrypt/acme.json
      dnsChallenge:
        provider: cloudflare
        delayBeforeCheck: 0
```

Environment variables for Cloudflare:

```bash
CF_API_EMAIL=your-email@example.com
CF_API_KEY=your-api-key
```

Supported DNS providers: Cloudflare, Route53, Google Cloud DNS, DigitalOcean, and 100+ more.

## Docker Swarm Deployment

Traefik excels in Docker Swarm for high availability:

```yaml
version: '3.8'

services:
  traefik:
    image: traefik:v2.11
    command:
      - "--providers.docker.endpoint=unix:///var/run/docker.sock"
      - "--providers.docker.swarmMode=true"
      - "--providers.docker.exposedbydefault=false"
      - "--entrypoints.websecure.address=:443"
      - "--certificatesresolvers.letsencrypt.acme.tlschallenge=true"
      - "--certificatesresolvers.letsencrypt.acme.email=admin@example.com"
      - "--certificatesresolvers.letsencrypt.acme.storage=/letsencrypt/acme.json"
    ports:
      - target: 443
        published: 443
        protocol: tcp
        mode: host
    volumes:
      - "/var/run/docker.sock:/var/run/docker.sock:ro"
      - "traefik_letsencrypt:/letsencrypt"
    deploy:
      mode: global
      placement:
        constraints:
          - node.role == manager
      update_config:
        parallelism: 1
        delay: 10s
      restart_policy:
        condition: on-failure
    networks:
      - traefik-public

  app:
    image: ghcr.io/victorytek/home-registry:latest
    environment:
      DATABASE_URL: postgres://postgres:${POSTGRES_PASSWORD}@db:5432/home_inventory
    networks:
      - traefik-public
      - backend
    deploy:
      replicas: 3
      update_config:
        parallelism: 1
        delay: 10s
        order: start-first
      rollback_config:
        parallelism: 1
        delay: 5s
      labels:
        - "traefik.enable=true"
        - "traefik.http.routers.home-registry.rule=Host(`home-registry.example.com`)"
        - "traefik.http.routers.home-registry.entrypoints=websecure"
        - "traefik.http.routers.home-registry.tls.certresolver=letsencrypt"
        - "traefik.http.services.home-registry.loadbalancer.server.port=8210"

networks:
  traefik-public:
    driver: overlay
    attachable: true
  backend:
    driver: overlay
    internal: true

volumes:
  traefik_letsencrypt:
```

Deploy to swarm:

```bash
docker stack deploy -c docker-compose.yml home-registry
```

## Kubernetes Integration

For Kubernetes deployments:

```yaml
apiVersion: v1
kind: Service
metadata:
  name: home-registry
  labels:
    app: home-registry
spec:
  ports:
    - port: 80
      targetPort: 8210
  selector:
    app: home-registry
---
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: home-registry
  annotations:
    kubernetes.io/ingress.class: traefik
    cert-manager.io/cluster-issuer: letsencrypt-prod
    traefik.ingress.kubernetes.io/router.middlewares: default-security-headers@kubernetescrd
spec:
  tls:
    - hosts:
        - home-registry.example.com
      secretName: home-registry-tls
  rules:
    - host: home-registry.example.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: home-registry
                port:
                  number: 80
```

## Monitoring

### Traefik Dashboard

Access dashboard (with authentication):

```bash
# Generate password hash
htpasswd -nb admin your_password

# Add to dynamic.yml middleware
# Then access: https://traefik.example.com/dashboard/
```

### Metrics (Prometheus)

Enable Prometheus metrics:

```yaml
# traefik.yml
metrics:
  prometheus:
    entryPoint: metrics
    addEntryPointsLabels: true
    addServicesLabels: true

entryPoints:
  metrics:
    address: ":8082"
```

Scrape configuration:

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'traefik'
    static_configs:
      - targets: ['traefik:8082']
```

### Access Logs

View access logs:

```bash
# Docker Compose
docker compose exec traefik cat /var/log/traefik/access.log | jq

# Docker Swarm
docker service logs home-registry_traefik

# Filter by status code
docker compose exec traefik cat /var/log/traefik/access.log | jq 'select(.DownstreamStatus == 500)'
```

## Troubleshooting

### Certificate Issues

```bash
# Check ACME account
docker compose exec traefik cat /letsencrypt/acme.json | jq

# Verify DNS resolution
dig +short home-registry.example.com

# Check Traefik logs for ACME errors
docker compose logs traefik | grep -i acme
```

### Service Discovery Issues

```bash
# Verify Docker socket is mounted
docker compose exec traefik ls -la /var/run/docker.sock

# Check Traefik can see containers
docker compose exec traefik cat /etc/traefik/traefik.yml

# Verify labels are set
docker inspect home-registry_app_1 | jq '.[0].Config.Labels'
```

### Debug Mode

Enable debug logging:

```yaml
# traefik.yml
log:
  level: DEBUG
```

Verify configuration:

```bash
# Test configuration
docker compose config

# View all registered routes
curl http://localhost:8080/api/http/routers | jq
```

### Common Issues

**Issue: 404 Not Found**
- Check `traefik.enable=true` label is set
- Verify `exposedByDefault: false` in traefik.yml
- Confirm `Host()` rule matches your domain

**Issue: Certificate not obtained**
- Ensure ports 80/443 are open
- Verify email in acme configuration
- Check Let's Encrypt rate limits

**Issue: Backend unreachable**
- Verify containers are on same network
- Check health check endpoint returns 200
- Confirm port in label matches container port

## Performance Tuning

```yaml
# traefik.yml
global:
  checkNewVersion: false
  sendAnonymousUsage: false

serversTransport:
  maxIdleConnsPerHost: 200
  
entryPoints:
  websecure:
    transport:
      respondingTimeouts:
        readTimeout: 60s
        writeTimeout: 60s
        idleTimeout: 180s
```

## Security Best Practices

1. **Disable dashboard in production** or secure with strong auth
2. **Use read-only Docker socket** when possible
3. **Enable security headers middleware** on all routes
4. **Implement rate limiting** to prevent abuse
5. **Use internal networks** for database connections
6. **Keep Traefik updated** for security patches

## Migration from Nginx/Caddy

Key differences:

- Configuration via Docker labels (not files)
- Automatic service discovery
- No manual reload needed (watches for changes)
- Certificate management per domain

## Resources

- [Traefik Documentation](https://doc.traefik.io/traefik/)
- [Traefik Docker Provider](https://doc.traefik.io/traefik/providers/docker/)
- [Let's Encrypt Configuration](https://doc.traefik.io/traefik/https/acme/)
- [Traefik Community](https://community.traefik.io/)

## Next Steps

- Configure monitoring: [Monitoring & Logging Guide](monitoring-logging.md)
- Harden security: [Security Hardening Guide](security-hardening.md)
- Set up high availability: [High Availability Guide](high-availability.md)
