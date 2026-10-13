# Monitoring & Logging Guide

This guide covers setting up comprehensive monitoring, logging, and alerting for production Home Registry deployments.

## Overview

A production monitoring stack should provide:

- **Real-time metrics** - System and application performance
- **Log aggregation** - Centralized log collection and search
- **Health checks** - Automated uptime monitoring
- **Alerting** - Notifications for critical events
- **Dashboards** - Visual representation of system state

## Quick Start: Lightweight Monitoring

For self-hosters and small deployments, start with these lightweight tools:

### Uptime Kuma (Uptime Monitoring)

Simple, beautiful uptime monitoring with notifications.

```yaml
# Add to docker-compose.yml
services:
  uptime-kuma:
    image: louislam/uptime-kuma:latest
    container_name: uptime-kuma
    restart: unless-stopped
    ports:
      - "3001:3001"
    volumes:
      - uptime_kuma_data:/app/data

volumes:
  uptime_kuma_data:
```

```bash
# Start Uptime Kuma
docker compose up -d uptime-kuma

# Access dashboard
# Open: http://your-server:3001
```

**Setup monitors:**
1. Create HTTP monitor for `https://your-domain.com/health`
2. Set check interval: 60 seconds
3. Configure notifications (email, Slack, Discord, etc.)
4. Add keyword check: expect `"status":"healthy"`

### Netdata (Real-Time System Metrics)

Zero-configuration monitoring with auto-detection.

```bash
# Install Netdata
bash <(curl -Ss https://my-netdata.io/kickstart.sh) --dont-wait

# Access dashboard
# Open: http://your-server:19999
```

**What Netdata monitors:**
- CPU, RAM, disk I/O
- Network traffic
- Docker containers
- PostgreSQL (auto-detected)
- System services

**Secure Netdata:**

```bash
sudo nano /etc/netdata/netdata.conf
```

```ini
[web]
    bind to = localhost  # Only accessible via reverse proxy

[registry]
    enabled = no
```

Add to Caddy/Nginx reverse proxy:

```caddyfile
# Caddy
netdata.example.com {
    reverse_proxy localhost:19999
    basicauth {
        admin $2a$14$...  # htpasswd generated
    }
}
```

## Full Observability Stack (Prometheus + Grafana + Loki)

For production deployments requiring comprehensive monitoring.

### Architecture

```
Application → Prometheus (metrics) → Grafana (visualization)
           → Loki (logs)         ↗
           → Alertmanager (alerts)
```

### Docker Compose Configuration

Create `docker-compose-monitoring.yml`:

```yaml
version: '3.8'

services:
  prometheus:
    image: prom/prometheus:latest
    container_name: prometheus
    restart: unless-stopped
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
      - '--storage.tsdb.retention.time=30d'
      - '--web.console.libraries=/usr/share/prometheus/console_libraries'
      - '--web.console.templates=/usr/share/prometheus/consoles'
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml
      - ./alerts.yml:/etc/prometheus/alerts.yml
      - prometheus_data:/prometheus
    networks:
      - monitoring

  grafana:
    image: grafana/grafana:latest
    container_name: grafana
    restart: unless-stopped
    ports:
      - "3000:3000"
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=${GRAFANA_PASSWORD}
      - GF_SERVER_ROOT_URL=https://grafana.example.com
      - GF_INSTALL_PLUGINS=grafana-piechart-panel
    volumes:
      - grafana_data:/var/lib/grafana
      - ./grafana-dashboards:/etc/grafana/provisioning/dashboards
      - ./grafana-datasources:/etc/grafana/provisioning/datasources
    networks:
      - monitoring
    depends_on:
      - prometheus

  loki:
    image: grafana/loki:latest
    container_name: loki
    restart: unless-stopped
    ports:
      - "3100:3100"
    volumes:
      - ./loki-config.yml:/etc/loki/local-config.yaml
      - loki_data:/loki
    networks:
      - monitoring
    command: -config.file=/etc/loki/local-config.yaml

  promtail:
    image: grafana/promtail:latest
    container_name: promtail
    restart: unless-stopped
    volumes:
      - /var/log:/var/log:ro
      - /var/lib/docker/containers:/var/lib/docker/containers:ro
      - ./promtail-config.yml:/etc/promtail/config.yml
    networks:
      - monitoring
    command: -config.file=/etc/promtail/config.yml
    depends_on:
      - loki

  node-exporter:
    image: prom/node-exporter:latest
    container_name: node-exporter
    restart: unless-stopped
    ports:
      - "9100:9100"
    volumes:
      - /proc:/host/proc:ro
      - /sys:/host/sys:ro
      - /:/rootfs:ro
    command:
      - '--path.procfs=/host/proc'
      - '--path.sysfs=/host/sys'
      - '--collector.filesystem.mount-points-exclude=^/(sys|proc|dev|host|etc)($$|/)'
    networks:
      - monitoring

  postgres-exporter:
    image: prometheuscommunity/postgres-exporter:latest
    container_name: postgres-exporter
    restart: unless-stopped
    ports:
      - "9187:9187"
    environment:
      DATA_SOURCE_NAME: "postgresql://postgres:${POSTGRES_PASSWORD}@db:5432/home_inventory?sslmode=disable"
    networks:
      - monitoring
      - backend

  alertmanager:
    image: prom/alertmanager:latest
    container_name: alertmanager
    restart: unless-stopped
    ports:
      - "9093:9093"
    volumes:
      - ./alertmanager.yml:/etc/alertmanager/alertmanager.yml
      - alertmanager_data:/alertmanager
    networks:
      - monitoring
    command:
      - '--config.file=/etc/alertmanager/alertmanager.yml'
      - '--storage.path=/alertmanager'

networks:
  monitoring:
    external: true
  backend:
    external: true

volumes:
  prometheus_data:
  grafana_data:
  loki_data:
  alertmanager_data:
```

### Prometheus Configuration

Create `prometheus.yml`:

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s
  external_labels:
    cluster: 'home-registry-prod'
    environment: 'production'

# Alertmanager configuration
alerting:
  alertmanagers:
    - static_configs:
        - targets:
            - 'alertmanager:9093'

# Load alert rules
rule_files:
  - 'alerts.yml'

# Scrape configurations
scrape_configs:
  # Home Registry application (if metrics endpoint added)
  - job_name: 'home-registry-app'
    static_configs:
      - targets: ['app:8210']
        labels:
          service: 'home-registry'
          component: 'backend'

  # PostgreSQL metrics
  - job_name: 'postgresql'
    static_configs:
      - targets: ['postgres-exporter:9187']
        labels:
          service: 'home-registry'
          component: 'database'

  # System metrics (OS level)
  - job_name: 'node-exporter'
    static_configs:
      - targets: ['node-exporter:9100']
        labels:
          instance: 'prod-server-01'

  # Prometheus self-monitoring
  - job_name: 'prometheus'
    static_configs:
      - targets: ['localhost:9090']
```

### Alert Rules

Create `alerts.yml`:

```yaml
groups:
  - name: home-registry-alerts
    interval: 30s
    rules:
      # Application down
      - alert: ApplicationDown
        expr: up{job="home-registry-app"} == 0
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "Home Registry application is down"
          description: "Application has been unreachable for more than 2 minutes"

      # High error rate
      - alert: HighErrorRate
        expr: rate(http_requests_total{status=~"5.."}[5m]) > 0.05
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High 5xx error rate detected"
          description: "Error rate is {{ $value }} errors/sec over last 5 minutes"

      # PostgreSQL down
      - alert: PostgreSQLDown
        expr: up{job="postgresql"} == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "PostgreSQL database is down"
          description: "Database has been unreachable for more than 1 minute"

      # High database connections
      - alert: HighDatabaseConnections
        expr: pg_stat_database_numbackends{datname="home_inventory"} > 80
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High number of database connections"
          description: "Database has {{ $value }} active connections (threshold: 80)"

      # Disk space low
      - alert: DiskSpaceLow
        expr: (node_filesystem_avail_bytes{mountpoint="/"} / node_filesystem_size_bytes{mountpoint="/"}) * 100 < 10
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "Disk space critically low"
          description: "Root filesystem has less than 10% free space ({{ $value }}%)"

      # High memory usage
      - alert: HighMemoryUsage
        expr: (1 - (node_memory_MemAvailable_bytes / node_memory_MemTotal_bytes)) * 100 > 90
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Memory usage is high"
          description: "Memory usage is {{ $value }}% (threshold: 90%)"

      # High CPU usage
      - alert: HighCPUUsage
        expr: 100 - (avg by(instance) (rate(node_cpu_seconds_total{mode="idle"}[5m])) * 100) > 80
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "High CPU usage detected"
          description: "CPU usage is {{ $value }}% (threshold: 80%)"

      # Database replication lag (if using replication)
      - alert: DatabaseReplicationLag
        expr: pg_replication_lag_seconds > 300
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "PostgreSQL replication lag is high"
          description: "Replication lag is {{ $value }} seconds (threshold: 300s)"

      # Certificate expiring soon
      - alert: SSLCertificateExpiringSoon
        expr: (probe_ssl_earliest_cert_expiry - time()) / 86400 < 7
        for: 1h
        labels:
          severity: warning
        annotations:
          summary: "SSL certificate expiring soon"
          description: "Certificate will expire in {{ $value }} days"
```

### Alertmanager Configuration

Create `alertmanager.yml`:

```yaml
global:
  resolve_timeout: 5m

# Notification templates
templates:
  - '/etc/alertmanager/templates/*.tmpl'

# Route tree
route:
  group_by: ['alertname', 'cluster', 'service']
  group_wait: 10s
  group_interval: 10s
  repeat_interval: 12h
  receiver: 'default'
  routes:
    - match:
        severity: critical
      receiver: 'critical-alerts'
      continue: true
    
    - match:
        severity: warning
      receiver: 'warning-alerts'

# Notification receivers
receivers:
  - name: 'default'
    email_configs:
      - to: 'admin@example.com'
        from: 'alerts@example.com'
        smarthost: 'smtp.gmail.com:587'
        auth_username: 'alerts@example.com'
        auth_password: 'app_password'
        headers:
          Subject: '[{{ .Status | toUpper }}] {{ .GroupLabels.alertname }}'

  - name: 'critical-alerts'
    email_configs:
      - to: 'oncall@example.com'
        from: 'alerts@example.com'
        smarthost: 'smtp.gmail.com:587'
        auth_username: 'alerts@example.com'
        auth_password: 'app_password'
    slack_configs:
      - api_url: 'https://hooks.slack.com/services/YOUR/SLACK/WEBHOOK'
        channel: '#alerts-critical'
        title: '{{ .GroupLabels.alertname }}'
        text: '{{ range .Alerts }}{{ .Annotations.description }}{{ end }}'

  - name: 'warning-alerts'
    slack_configs:
      - api_url: 'https://hooks.slack.com/services/YOUR/SLACK/WEBHOOK'
        channel: '#alerts-warning'
        title: '{{ .GroupLabels.alertname }}'
        text: '{{ range .Alerts }}{{ .Annotations.description }}{{ end }}'

# Inhibition rules (suppress alerts when other alerts fire)
inhibit_rules:
  - source_match:
      severity: 'critical'
    target_match:
      severity: 'warning'
    equal: ['alertname', 'instance']
```

### Loki Configuration

Create `loki-config.yml`:

```yaml
auth_enabled: false

server:
  http_listen_port: 3100

ingester:
  lifecycler:
    address: 127.0.0.1
    ring:
      kvstore:
        store: inmemory
      replication_factor: 1
    final_sleep: 0s
  chunk_idle_period: 5m
  chunk_retain_period: 30s

schema_config:
  configs:
    - from: 2024-01-01
      store: boltdb-shipper
      object_store: filesystem
      schema: v11
      index:
        prefix: index_
        period: 24h

storage_config:
  boltdb_shipper:
    active_index_directory: /loki/index
    cache_location: /loki/cache
    shared_store: filesystem
  filesystem:
    directory: /loki/chunks

limits_config:
  enforce_metric_name: false
  reject_old_samples: true
  reject_old_samples_max_age: 168h
  retention_period: 30d

chunk_store_config:
  max_look_back_period: 30d

table_manager:
  retention_deletes_enabled: true
  retention_period: 30d
```

### Promtail Configuration

Create `promtail-config.yml`:

```yaml
server:
  http_listen_port: 9080
  grpc_listen_port: 0

positions:
  filename: /tmp/positions.yaml

clients:
  - url: http://loki:3100/loki/api/v1/push

scrape_configs:
  # Docker container logs
  - job_name: docker
    docker_sd_configs:
      - host: unix:///var/run/docker.sock
        refresh_interval: 5s
    relabel_configs:
      - source_labels: ['__meta_docker_container_name']
        regex: '/(.*)'
        target_label: 'container'
      - source_labels: ['__meta_docker_container_log_stream']
        target_label: 'stream'
      - source_labels: ['__meta_docker_container_label_com_docker_compose_service']
        target_label: 'service'
    pipeline_stages:
      - json:
          expressions:
            level: level
            msg: msg
      - labels:
          level:

  # System logs
  - job_name: system
    static_configs:
      - targets:
          - localhost
        labels:
          job: system
          __path__: /var/log/syslog
```

### Grafana Dashboards

**Provisioning datasources** - create `grafana-datasources/datasources.yml`:

```yaml
apiVersion: 1

datasources:
  - name: Prometheus
    type: prometheus
    access: proxy
    url: http://prometheus:9090
    isDefault: true
    editable: false

  - name: Loki
    type: loki
    access: proxy
    url: http://loki:3100
    editable: false
```

**Import pre-built dashboards:**

1. **Node Exporter Full** (ID: 1860)
2. **PostgreSQL Database** (ID: 9628)
3. **Docker and System Monitoring** (ID: 893)

Access Grafana at `http://your-server:3000`, then:
- Go to Dashboards → Import
- Enter dashboard ID
- Select Prometheus datasource
- Click Import

### Start Monitoring Stack

```bash
# Create networks
docker network create monitoring
docker network create backend

# Start monitoring services
docker compose -f docker-compose-monitoring.yml up -d

# Verify all services running
docker compose -f docker-compose-monitoring.yml ps

# Check Prometheus targets
# Open: http://your-server:9090/targets

# Access Grafana
# Open: http://your-server:3000
# Default login: admin / <GRAFANA_PASSWORD from .env>
```

## Application Metrics (Optional Enhancement)

To add application-level metrics to Home Registry:

### Add actix-web-prom Dependency

```toml
# Cargo.toml
[dependencies]
actix-web-prom = "0.8"
```

### Instrument Application

```rust
// src/main.rs
use actix_web_prom::PrometheusMetricsBuilder;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // ... existing code ...
    
    // Create Prometheus metrics middleware
    let prometheus = PrometheusMetricsBuilder::new("home_registry")
        .endpoint("/metrics")
        .build()
        .unwrap();
    
    HttpServer::new(move || {
        App::new()
            .wrap(prometheus.clone())  // Add metrics middleware
            // ... rest of configuration ...
    })
    .bind(("0.0.0.0", 8210))?
    .run()
    .await
}
```

### Access Metrics

```bash
curl http://localhost:8210/metrics
```

**Metrics exposed:**
- HTTP request duration
- Request count by endpoint and status code
- Active connections
- Response sizes

## Log Management

### Viewing Logs

```bash
# Application logs
docker compose logs -f app

# Database logs
docker compose logs -f db

# All services
docker compose logs -f

# Last 100 lines
docker compose logs --tail=100 app

# Filter by time
docker compose logs --since 2h app

# JSON format (for processing)
docker compose logs --json app | jq
```

### Log Rotation

Docker handles log rotation automatically. Configure in `/etc/docker/daemon.json`:

```json
{
  "log-driver": "json-file",
  "log-opts": {
    "max-size": "10m",
    "max-file": "3"
  }
}
```

Restart Docker:

```bash
sudo systemctl restart docker
```

### Centralized Logging (Production)

For production, use centralized logging:

**Options:**
1. **Loki + Promtail** (self-hosted, lightweight)
2. **ELK Stack** (Elasticsearch + Logstash + Kibana)
3. **AWS CloudWatch Logs** (managed service)
4. **Datadog** (SaaS, expensive but comprehensive)

## Health Monitoring Integrations

### Healthchecks.io

Dead-man's switch for cron jobs and periodic tasks:

```bash
# Add to backup script
curl -fsS -m 10 --retry 5 -o /dev/null https://hc-ping.com/YOUR-UUID-HERE

# Example in backup.sh
./backup.sh && curl https://hc-ping.com/YOUR-UUID-HERE || curl https://hc-ping.com/YOUR-UUID-HERE/fail
```

### StatusPage.io

Public status page for users:

1. Create account at https://statuspage.io
2. Add components (Website, API, Database)
3. Configure monitoring integrations
4. Share status page URL with users

## Performance Monitoring

### Application Performance Monitoring (APM)

For advanced performance tracking:

**Options:**
- **Jaeger** (distributed tracing)
- **New Relic** (SaaS APM)
- **Datadog APM** (SaaS)

Example with Jaeger (OpenTelemetry):

```toml
# Cargo.toml
[dependencies]
opentelemetry = "0.20"
opentelemetry-jaeger = "0.19"
tracing-opentelemetry = "0.21"
```

## Grafana Query Examples

### CPU Usage
```promql
100 - (avg by(instance) (rate(node_cpu_seconds_total{mode="idle"}[5m])) * 100)
```

### Memory Usage
```promql
(1 - (node_memory_MemAvailable_bytes / node_memory_MemTotal_bytes)) * 100
```

### Disk I/O
```promql
rate(node_disk_read_bytes_total[5m])
rate(node_disk_written_bytes_total[5m])
```

### Database Connections
```promql
pg_stat_database_numbackends{datname="home_inventory"}
```

### HTTP Request Rate
```promql
rate(http_requests_total[5m])
```

### HTTP Error Rate
```promql
rate(http_requests_total{status=~"5.."}[5m])
```

## Monitoring Best Practices

1. **Monitor metrics, not just uptime** - Track performance trends
2. **Set meaningful alert thresholds** - Avoid alert fatigue
3. **Implement alert routing** - Critical alerts to on-call, warnings to Slack
4. **Document runbooks** - Link alerts to troubleshooting guides
5. **Test alerting** - Regularly verify notifications work
6. **Review dashboards monthly** - Add/remove metrics as needed
7. **Correlate logs with metrics** - Use Grafana's Loki integration
8. **Monitor the monitors** - Ensure monitoring stack is healthy

## Resources

- [Prometheus Documentation](https://prometheus.io/docs/)
- [Grafana Documentation](https://grafana.com/docs/)
- [Loki Documentation](https://grafana.com/docs/loki/latest/)
- [PostgreSQL Exporter](https://github.com/prometheus-community/postgres_exporter)

## Next Steps

- Set up high availability: [High Availability Guide](high-availability.md)
- Harden security: [Security Hardening Guide](security-hardening.md)
- Configure backups: [Database Production Guide](database-production.md)
