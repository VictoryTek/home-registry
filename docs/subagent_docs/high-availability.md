# High Availability Guide

This guide covers deploying Home Registry in a high availability (HA) configuration to minimize downtime and ensure business continuity.

## Overview

A high availability deployment includes:

- **Load balancing** - Distribute traffic across multiple app instances
- **Database replication** - Primary/replica PostgreSQL setup
- **Health monitoring** - Automatic failover for unhealthy instances
- **Zero-downtime deployments** - Rolling updates without service interruption
- **Geographic redundancy** - Optional multi-region deployment

## Architecture Patterns

### Single Server HA (Entry Level)

**Components:**
- 2x application instances on same server
- Single PostgreSQL database
- Reverse proxy with health checks

**Pros:** Simple, low cost
**Cons:** Single point of failure (server hardware)
**Availability:** 99% (~3.6 days downtime/year)

### Multi-Server HA (Recommended)

**Components:**
- 3+ application instances across 2+ servers
- PostgreSQL primary + replica
- Load balancer with health checks
- Shared storage or distributed file system

**Pros:** Survives single server failure
**Cons:** More complex, higher cost
**Availability:** 99.9% (~8.7 hours downtime/year)

### Multi-Region HA (Enterprise)

**Components:**
- Application instances in multiple regions
-  Geographic database replication
- Global load balancer (AWS Route53, Cloudflare)
- CDN for static assets

**Pros:** Survives data center outage
**Cons:** Complex, expensive, latency considerations
**Availability:** 99.99%+ (~52 minutes downtime/year)

## Multi-Instance Application Deployment

Home Registry is **stateless** (uses JWT tokens), making horizontal scaling straightforward.

### Docker Compose - Multiple Instances

```yaml
version: '3.8'

services:
  app:
    image: ghcr.io/victorytek/home-registry:latest
    deploy:
      replicas: 3  # Run 3 instances
    environment:
      DATABASE_URL: postgres://postgres:${POSTGRES_PASSWORD}@db:5432/home_inventory
      JWT_SECRET: ${JWT_SECRET}  # MUST be same across all instances
      PORT: 8210
      RUST_LOG: info
    volumes:
      - appdata:/app/data
      - uploads:/app/uploads  # Shared uploads volume
    networks:
      - app-network

volumes:
  appdata:
  uploads:
```

**Critical:** `JWT_SECRET` must be identical across all instances for token validation.

### Docker Swarm Deployment

Docker Swarm provides built-in orchestration and load balancing.

```yaml
version: '3.8'

services:
  app:
    image: ghcr.io/victorytek/home-registry:latest
    deploy:
      replicas: 3
      update_config:
        parallelism: 1
        delay: 10s
        order: start-first  # Start new before stopping old
      rollback_config:
        parallelism: 1
        delay: 5s
      restart_policy:
        condition: on-failure
        delay: 5s
        max_attempts: 3
      placement:
        max_replicas_per_node: 1  # Spread across nodes
    environment:
      DATABASE_URL: postgres://postgres:${POSTGRES_PASSWORD}@db:5432/home_inventory
      JWT_SECRET: ${JWT_SECRET}
    volumes:
      - uploads:/app/uploads
    networks:
      - app-network

  db:
    image: postgres:17
    deploy:
      replicas: 1
      placement:
        constraints:
          - node.labels.database == primary
    environment:
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}
      POSTGRES_DB: home_inventory
    volumes:
      - pgdata:/var/lib/postgresql/data
    networks:
      - app-network

volumes:
  pgdata:
  uploads:
    driver: local
    driver_opts:
      type: nfs
      o: addr=nfs-server,rw
      device: ":/exports/uploads"

networks:
  app-network:
    driver: overlay
```

**Deploy to swarm:**

```bash
# Initialize swarm
docker swarm init

# Label nodes for database placement
docker node update --label-add database=primary node1

# Deploy stack
docker stack deploy -c docker-compose.yml home-registry

# Scale services
docker service scale home-registry_app=5

# View service status
docker service ls
docker service ps home-registry_app
```

## Load Balancing

### Option 1: Nginx Load Balancer

```nginx
upstream home_registry_backend {
    least_conn;  # Route to server with fewer connections
    
    server app1:8210 max_fails=3 fail_timeout=30s weight=1;
    server app2:8210 max_fails=3 fail_timeout=30s weight=1;
    server app3:8210 max_fails=3 fail_timeout=30s weight=1;
    
    keepalive 32;
}

server {
    listen 443 ssl http2;
    server_name home-registry.example.com;
    
    # SSL configuration...
    
    location / {
        proxy_pass http://home_registry_backend;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        
        # Health check
        proxy_next_upstream error timeout http_502 http_503 http_504;
        proxy_next_upstream_tries 2;
        proxy_next_upstream_timeout 10s;
        
        # Headers
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

### Option 2: HAProxy

```haproxy
# /etc/haproxy/haproxy.cfg

global
    maxconn 4096
    log /dev/log local0
    chroot /var/lib/haproxy
    user haproxy
    group haproxy
    daemon

defaults
    log global
    mode http
    option httplog
    option dontlognull
    timeout connect 5000ms
    timeout client 50000ms
    timeout server 50000ms
    retry-on all-retryable-errors
    retries 3

frontend http_front
    bind *:80
    redirect scheme https code 301 if !{ ssl_fc }

frontend https_front
    bind *:443 ssl crt /etc/ssl/certs/home-registry.pem
    default_backend home_registry_backend
    
    # Security headers
    http-response set-header Strict-Transport-Security "max-age=63072000; includeSubDomains; preload"

backend home_registry_backend
    balance leastconn  # or roundrobin, source (IP hash)
    option httpchk GET /health
    http-check expect status 200
    
    server app1 10.0.1.10:8210 check inter 10s fall 3 rise 2
    server app2 10.0.1.11:8210 check inter 10s fall 3 rise 2
    server app3 10.0.1.12:8210 check inter 10s fall 3 rise 2
```

### Option 3: Caddy (Load Balancing)

```caddyfile
home-registry.example.com {
    reverse_proxy {
        lb_policy least_conn  # or round_robin, random, ip_hash
        
        to app1:8210
        to app2:8210
        to app3:8210
        
        health_uri /health
        health_port 8210
        health_interval 30s
        health_timeout 5s
        health_status 200
        
        fail_duration 30s
        max_fails 3
        
        header_up X-Real-IP {remote_host}
        header_up X-Forwarded-For {remote_host}
        header_up X-Forwarded-Proto {scheme}
    }
}
```

## Database High Availability

### PostgreSQL Streaming Replication

#### Primary Server Setup

```bash
# Edit postgresql.conf on primary
docker compose exec db sh -c "cat >> /var/lib/postgresql/data/postgresql.conf << EOF
wal_level = replica
max_wal_senders = 5
wal_keep_size = 1GB
hot_standby = on
EOF"

# Create replication user
docker compose exec db psql -U postgres -c "
CREATE ROLE replicator WITH REPLICATION LOGIN PASSWORD 'replica_secure_password';
"

# Allow replication in pg_hba.conf
docker compose exec db sh -c "cat >> /var/lib/postgresql/data/pg_hba.conf << EOF
host replication replicator 10.0.1.0/24 md5
EOF"

# Reload configuration
docker compose exec db psql -U postgres -c "SELECT pg_reload_conf();"
```

#### Replica Server Setup

```bash
# On replica server, perform base backup
docker compose stop db

# Remove existing data
rm -rf /var/lib/postgresql/data/*

# Create base backup from primary
pg_basebackup -h primary-server -D /var/lib/postgresql/data -U replicator -P -v -R -X stream -C -S replica1

# Create standby.signal file (PostgreSQL 12+)
touch /var/lib/postgresql/data/standby.signal

# Start replica
docker compose start db
```

#### Monitoring Replication

```sql
-- On primary: View replication status
SELECT * FROM pg_stat_replication;

-- On replica: Check replication lag
SELECT now() - pg_last_xact_replay_timestamp() AS replication_delay;

-- On replica: Check if in recovery mode
SELECT pg_is_in_recovery();  -- Should return true
```

### Automatic Failover with Patroni

Patroni provides automatic failover using distributed consensus (etcd/Consul).

**docker-compose.yml with Patroni:**

```yaml
version: '3.8'

services:
  etcd:
    image: quay.io/coreos/etcd:latest
    environment:
      ETCD_LISTEN_CLIENT_URLS: http://0.0.0.0:2379
      ETCD_ADVERTISE_CLIENT_URLS: http://etcd:2379
    networks:
      - db-network

  patroni-primary:
    image: patroni/patroni:latest
    environment:
      PATRONI_NAME: node1
      PATRONI_SCOPE: home-registry-cluster
      PATRONI_ETCD_HOSTS: etcd:2379
      PATRONI_POSTGRESQL_CONNECT_ADDRESS: patroni-primary:5432
      PATRONI_POSTGRESQL_DATA_DIR: /data/patroni
    volumes:
      - patroni_primary_data:/data/patroni
    networks:
      - db-network

  patroni-replica:
    image: patroni/patroni:latest
    environment:
      PATRONI_NAME: node2
      PATRONI_SCOPE: home-registry-cluster
      PATRONI_ETCD_HOSTS: etcd:2379
      PATRONI_POSTGRESQL_CONNECT_ADDRESS: patroni-replica:5432
      PATRONI_POSTGRESQL_DATA_DIR: /data/patroni
    volumes:
      - patroni_replica_data:/data/patroni
    networks:
      - db-network

volumes:
  patroni_primary_data:
  patroni_replica_data:

networks:
  db-network:
```

Patroni automatically handles:
- Leader election
- Failover when primary fails
- Replica promotion
- Configuration synchronization

## Shared Storage for Uploads

Multiple app instances need shared access to uploaded files.

### Option 1: NFS (Network File System)

```bash
# On NFS server
sudo apt install nfs-kernel-server
sudo mkdir -p /exports/home-registry/uploads
sudo chown nobody:nogroup /exports/home-registry/uploads
sudo chmod 777 /exports/home-registry/uploads

# Configure exports
echo "/exports/home-registry/uploads *(rw,sync,no_subtree_check,no_root_squash)" | sudo tee -a /etc/exports
sudo exportfs -ra
```

**Mount in Docker Compose:**

```yaml
volumes:
  uploads:
    driver: local
    driver_opts:
      type: nfs
      o: addr=nfs-server-ip,rw,nfsvers=4
      device: ":/exports/home-registry/uploads"
```

### Option 2: S3-Compatible Object Storage

Use AWS S3, MinIO, or Backblaze B2 for scalable file storage.

**MinIO (self-hosted):**

```yaml
services:
  minio:
    image: minio/minio:latest
    command: server /data --console-address ":9001"
    environment:
      MINIO_ROOT_USER: admin
      MINIO_ROOT_PASSWORD: ${MINIO_PASSWORD}
    volumes:
      - minio_data:/data
    ports:
      - "9000:9000"
      - "9001:9001"

volumes:
  minio_data:
```

**Application integration** (requires code changes to use S3 SDK instead of local filesystem).

### Option 3: GlusterFS (Distributed Filesystem)

For larger deployments with multiple servers.

## Zero-Downtime Deployments

### Rolling Updates

**Docker Swarm (automatic):**

```bash
# Update image
docker service update --image ghcr.io/victorytek/home-registry:v1.1.0 home-registry_app

# Swarm will:
# 1. Start new container (v1.1.0)
# 2. Wait for health check
# 3. Stop old container (v1.0.0)
# 4. Repeat for each replica
```

**Manual rolling update:**

```bash
# Scale up with new version
docker compose -f docker-compose-new.yml up -d --scale app=6

# Wait for health checks
sleep 30

# Scale down old version
docker compose -f docker-compose-old.yml down
```

### Blue-Green Deployment

Maintain two complete environments:

```bash
# Deploy to "green" environment
docker compose -f docker-compose-green.yml up -d

# Test green environment
curl https://green.home-registry.example.com/health

# Switch load balancer to green
# Update DNS or load balancer config

# Keep blue environment for quick rollback (24-48 hours)
```

### Database Migrations

For database schema changes during deployments:

```bash
# 1. Ensure migrations are backward compatible
# 2. Deploy new application version (compatible with old schema)
# 3. Run migrations
# 4. Deploy final application version

# Example:
# Version 1.0: column "name" VARCHAR(100)
# Version 1.1: column "name" VARCHAR(255) -- backward compatible!
# Migration: ALTER TABLE items ALTER COLUMN name TYPE VARCHAR(255);
```

## Health Checks

### Application Health Checks

Home Registry provides `/health` endpoint:

```json
{
  "status": "healthy",
  "service": "home-registry",
  "version": "0.1.0-beta.2",
  "timestamp": "2026-02-20T12:00:00Z"
}
```

**Enhanced health check** (can be added):

```rust
// Check database connectivity
// Check disk space
// Check critical dependencies

{
  "status": "healthy",
  "service": "home-registry",
  "version": "0.1.0-beta.2",
  "timestamp": "2026-02-20T12:00:00Z",
  "checks": {
    "database": "healthy",
    "disk_space": "healthy",
    "memory": "healthy"
  }
}
```

### Load Balancer Health Checks

Configure health checks in your load balancer:

```nginx
# Nginx
location /health {
    proxy_pass http://backend;
    # If health check fails, mark server down
}
```

```haproxy
# HAProxy
option httpchk GET /health
http-check expect status 200
```

## Disaster Recovery

### RTO/RPO Targets

- **RTO (Recovery Time Objective):** < 1 hour
- **RPO (Recovery Point Objective):** < 24 hours (daily backups)
- **Ideal RPO:** < 1 hour (with WAL archiving)

### DR Runbook

1. **Detect outage** (monitoring alerts)
2. **Assess scope** (application, database, infrastructure)
3. **Notify team** (incident response)
4. **Failover checklist:**
   - Promote replica to primary (if database issue)
   - Spin up new application instances
   - Update DNS/load balancer
   - Restore from backup (if needed)
5. **Verify functionality** (smoke tests)
6. **Monitor closely** (watch for issues)
7. **Post-mortem** (root cause analysis)

### Backup Strategies for HA

- **Application state:** Stateless (no backup needed)
- **Database:** Daily logical backups + WAL archiving
- **Uploads:** Replicated to S3/NFS with versioning
- **Configuration:** Version controlled in Git
- **Secrets:** Backed up to secure vault

## Testing HA Configuration

### Chaos Engineering

Deliberately induce failures to test resilience:

```bash
# Kill random app instance
docker kill $(docker ps -q -f name=app | shuf -n 1)
# Verify: System continues serving requests

# Simulate network partition
docker network disconnect app-network app-1
# Verify: Load balancer routes around failed instance

# Simulate database failure
docker stop db
# Verify: Application enters degraded state gracefully

# Fill disk space
dd if=/dev/zero of=/tmp/fill bs=1M count=1000
# Verify: Monitoring alerts fire
```

### Load Testing

Test system under high load:

```bash
# Install Apache Bench
sudo apt install apache2-utils

# Run load test (1000 requests, 100 concurrent)
ab -n 1000 -c 100 https://home-registry.example.com/health

# Or use k6 for more advanced testing
k6 run loadtest.js
```

## Multi-Region Deployment (Advanced)

### Architecture

```
            [Global Load Balancer - Route53/Cloudflare]
                              |
                              |
         +--------------------+--------------------+
         |                                         |
    [US-East]                                 [EU-West]
         |                                         |
    [App Instances x3]                        [App Instances x3]
         |                                         |
    [PostgreSQL Primary]                     [PostgreSQL Replica]
         |                                         |
    [Uploads - S3]                           [Uploads - S3 replicated]
```

### Database Multi-Region Replication

Use PostgreSQL logical replication for cross-region:

```sql
-- On primary (US-East)
CREATE PUBLICATION home_registry_pub FOR ALL TABLES;

-- On replica (EU-West)
CREATE SUBSCRIPTION home_registry_sub
    CONNECTION 'host=us-east-primary dbname=home_inventory user=replicator password=xxx'
    PUBLICATION home_registry_pub;
```

### Considerations

- **Latency:** Cross-region replication adds latency
- **Consistency:** Eventual consistency model
- **Cost:** Data transfer between regions is expensive
- **Complexity:** Significantly more complex to operate

## Resources

- [Docker Swarm Documentation](https://docs.docker.com/engine/swarm/)
- [PostgreSQL Replication](https://www.postgresql.org/docs/current/high-availability.html)
- [Patroni Documentation](https://patroni.readthedocs.io/)
- [HAProxy Documentation](http://www.haproxy.org/#docs)

## Next Steps

- Configure monitoring: [Monitoring & Logging Guide](monitoring-logging.md)
- Harden security: [Security Hardening Guide](security-hardening.md)
- Configure backups: [Database Production Guide](database-production.md)