# Home Registry - Production Deployment Documentation

Complete guides for deploying Home Registry in production environments with HTTPS, monitoring, backups, and high availability.

## 🚀 Quick Start (15 Minutes)

**New to deployment?** Start here:

**➡️ [Quick Start Guide](quickstart.md)** - Deploy with automatic HTTPS in <15 minutes using Caddy

This guide gets you from zero to a production-ready deployment with:
- ✅ Automatic HTTPS with Let's Encrypt
- ✅ PostgreSQL database
- ✅ Health monitoring
- ✅ Automated backups
- ✅ Security hardening checklist

---

## 📖 Documentation Overview

### Core Deployment Guides

| Guide | Purpose | Time Required |
|-------|---------|---------------|
| **[Quick Start](quickstart.md)** | Fastest path to production deployment | 15 minutes |
| **[Security Hardening](security-hardening.md)** | Comprehensive security checklist | 30-60 minutes |
| **[Database Production Setup](database-production.md)** | PostgreSQL tuning, backups, replication | 1-2 hours |
| **[Monitoring & Logging](monitoring-logging.md)** | Prometheus, Grafana, Loki stack | 1-2 hours |
| **[High Availability](high-availability.md)** | Multi-instance deployment patterns | 2-4 hours |
| **[Troubleshooting](troubleshooting.md)** | Common issues and solutions | Reference |

### Reverse Proxy Options

Choose a reverse proxy based on your needs:

| Reverse Proxy | Best For | Complexity | HTTPS Setup |
|---------------|----------|------------|-------------|
| **[Caddy](reverse-proxy-caddy.md)** | Quick deployment, automatic SSL | ⭐ Easy | Automatic |
| **[Nginx](reverse-proxy-nginx.md)** | Enterprise environments, manual control | ⭐⭐ Moderate | Manual (Certbot) |
| **[Traefik](reverse-proxy-traefik.md)** | Container orchestration, Docker Swarm | ⭐⭐⭐ Advanced | Automatic |

**Recommendation:**
- **Self-hosting or quick deployment**: Use Caddy (automatic HTTPS, zero-config)
- **Enterprise or existing infrastructure**: Use Nginx (more control, advanced features)
- **Container orchestration**: Use Traefik (Docker-native, service discovery)

---

## 📁 Configuration Examples

Production-ready configuration files you can copy and customize:

### Docker Compose

**[docker-compose-production.yml](../examples/docker-compose-production.yml)**
- Complete production setup with Caddy, PostgreSQL, and monitoring
- Resource limits and health checks
- Network segmentation (frontend/backend)
- Includes Uptime Kuma for uptime monitoring

### Reverse Proxy Configurations

**[Caddyfile](../examples/Caddyfile)**
- Automatic HTTPS with Let's Encrypt
- Security headers (HSTS, CSP, XSS protection)
- Caching strategies for assets/uploads
- Load balancing support

**[nginx.conf](../examples/nginx.conf)**
- Production nginx configuration with TLSv1.3
- Rate limiting zones for API and authentication
- Security headers and OCSP stapling
- Load balancing for multiple instances

### Backup & Restore Scripts

**[backup.sh](../examples/backup.sh)**
- Automated PostgreSQL backup script
- Off-site backup support (S3, Backblaze B2, Restic)
- Backup verification and retention management
- Cron-ready with logging

**[restore.sh](../examples/restore.sh)**
- Safe database restore with safety backups
- Interactive confirmation prompts
- Integrity verification
- Post-restore health checks

---

## 🎯 Deployment Scenarios

Choose your deployment scenario based on your requirements:

### Scenario 1: Self-Hosted (Single User/Family)

**Requirements:**
- 1-10 concurrent users
- 99% uptime acceptable
- Budget-conscious
- Minimal maintenance

**Recommended Stack:**
- **Server:** VPS (2 CPU, 4GB RAM, 50GB SSD) - $5-15/month
- **Reverse Proxy:** Caddy (automatic HTTPS)
- **Monitoring:** Uptime Kuma (lightweight)
- **Backup:** Daily cron job + Backblaze B2

**Setup Time:** ~20 minutes  
**Guides:** [Quick Start](quickstart.md) + [Security Hardening](security-hardening.md)

---

### Scenario 2: Small Business (10-100 Users)

**Requirements:**
- 10-100 concurrent users
- 99.5% uptime target
- Business critical
- Professional monitoring

**Recommended Stack:**
- **Server:** Dedicated server or cloud instance (4 CPU, 8GB RAM, 200GB SSD) - $50-200/month
- **Reverse Proxy:** Nginx (more control)
- **Monitoring:** Prometheus + Grafana + Loki
- **Database:** PostgreSQL with daily backups + off-site replication
- **Backup:** Automated backups with Restic

**Setup Time:** 2-3 hours  
**Guides:** [Nginx](reverse-proxy-nginx.md) + [Monitoring](monitoring-logging.md) + [Database](database-production.md)

---

### Scenario 3: Enterprise (100-1000+ Users)

**Requirements:**
- 100-1000+ concurrent users
- 99.9%+ uptime SLA
- High availability
- Compliance requirements

**Recommended Stack:**
- **Infrastructure:** Kubernetes cluster (3+ nodes) or Docker Swarm
- **Load Balancer:** Cloud load balancer (AWS ALB, GCP Load Balancer)
- **Reverse Proxy:** Traefik (Docker-native) or Nginx (traditional)
- **Database:** Managed PostgreSQL (AWS RDS, GCP Cloud SQL) with read replicas
- **Monitoring:** Full observability stack (Prometheus, Grafana, Loki, Tempo, Alertmanager)
- **Backup:** Automated cross-region backups with point-in-time recovery

**Setup Time:** 1-2 days  
**Guides:** [High Availability](high-availability.md) + [Traefik](reverse-proxy-traefik.md) + [Database Replication](database-production.md#postgresql-replication)

---

## 🔒 Security Considerations

**Pre-Deployment Security Checklist:**
- [ ] Set strong `POSTGRES_PASSWORD` (16+ characters)
- [ ] Configure firewall (allow only 80/443, block 5432/8210)
- [ ] Enable HTTPS with valid SSL certificate
- [ ] Review rate limiting settings
- [ ] Set explicit `JWT_SECRET` for multi-instance deployments
- [ ] Configure backup encryption

**Post-Deployment Security:**
- [ ] Test HTTPS configuration with SSL Labs (target: A+ rating)
- [ ] Verify security headers with securityheaders.com
- [ ] Enable fail2ban for repeated authentication failures
- [ ] Configure log monitoring and alerting
- [ ] Schedule quarterly security audits

**See:** [Complete Security Hardening Guide](security-hardening.md)

---

## 📊 Monitoring & Observability

### Metrics to Monitor

**Application Metrics:**
- Request rate (requests per second)
- Response time (P50, P95, P99)
- Error rate (4xx, 5xx responses)
- Active users (concurrent sessions)

**Database Metrics:**
- Connection count
- Query performance (slow queries)
- Database size and growth rate
- Replication lag (if using replication)

**System Metrics:**
- CPU usage
- Memory usage
- Disk I/O and space
- Network bandwidth

**Recommended Observability Stack:**
- **Metrics:** Prometheus + Grafana
- **Logs:** Grafana Loki + Promtail
- **Tracing:** Grafana Tempo (optional)
- **Alerting:** Alertmanager + PagerDuty/Slack
- **Uptime:** Uptime Kuma or UptimeRobot

**See:** [Monitoring & Logging Guide](monitoring-logging.md)

---

## 💾 Backup & Disaster Recovery

### Backup Strategy

**Recommended 3-2-1 Backup Rule:**
- **3** copies of data (original + 2 backups)
- **2** different storage types (local disk + cloud)
- **1** off-site backup (different physical location)

**Backup Frequency:**
- **Production:** Daily automated backups (minimum)
- **Critical Systems:** Hourly backups + transaction log archiving
- **Development:** Weekly backups

**Retention Policy:**
- Daily backups: Keep 7 days
- Weekly backups: Keep 4 weeks
- Monthly backups: Keep 6 months
- Yearly backups: Keep 7 years (compliance)

**Testing:**
- Test restore procedure quarterly
- Document Recovery Time Objective (RTO): <1 hour
- Document Recovery Point Objective (RPO): <24 hours

**Automated Backup Script:** [backup.sh](../examples/backup.sh)  
**Restore Procedure:** [restore.sh](../examples/restore.sh)

**See:** [Database Production Guide - Backup Strategies](database-production.md#backup-strategies)

---

## 🔄 High Availability Patterns

### Single-Server HA (~99% uptime)
- Automatic container restart policies
- Health checks with automatic recovery
- Local database backups

### Multi-Instance HA (~99.9% uptime)
- Load balancer (Nginx/HAProxy) distributing traffic across 2+ app instances
- PostgreSQL primary with streaming replication to standby
- Automated failover with Patroni or manual failover procedures

### Multi-Region HA (~99.99% uptime)
- Geographic distribution across multiple data centers
- Cross-region database replication
- Global load balancer (AWS Route 53, Cloudflare)
- Disaster recovery automation

**See:** [High Availability Deployment Guide](high-availability.md)

---

## 🛠️ Troubleshooting

### Common Issues

| Issue | Likely Cause | Quick Fix |
|-------|--------------|-----------|
| App won't start | Database connection failure | Check DATABASE_URL, verify database is running |
| Can't access over HTTPS | SSL certificate issue | Check certificate paths, verify DNS, check cert renewal |
| Slow performance | Database not tuned | Apply PostgreSQL tuning, add indexes, check connection pool |
| High memory usage | Connection pool too large | Reduce max_connections, tune shared_buffers |
| 429 rate limit errors | Rate limiting too strict | Increase RATE_LIMIT_RPS/BURST or review client behavior |

**See:** [Complete Troubleshooting Guide](troubleshooting.md)

---

## 📚 Additional Resources

### Official Documentation
- [Main README](../../README.md) - Project overview and quick start
- [CHANGELOG](../../CHANGELOG.md) - Version history and release notes
- [Development Standards](../../scripts/DEV_STANDARDS.md) - Development guidelines

### External Resources
- [PostgreSQL Documentation](https://www.postgresql.org/docs/current/)
- [Docker Documentation](https://docs.docker.com/)
- [Nginx Documentation](https://nginx.org/en/docs/)
- [Caddy Documentation](https://caddyserver.com/docs/)
- [Traefik Documentation](https://doc.traefik.io/traefik/)
- [Prometheus Documentation](https://prometheus.io/docs/)
- [Grafana Documentation](https://grafana.com/docs/)

### Security Resources
- [Mozilla SSL Configuration Generator](https://ssl-config.mozilla.org/)
- [SSL Labs Server Test](https://www.ssllabs.com/ssltest/)
- [Security Headers Scanner](https://securityheaders.com/)
- [OWASP Cheat Sheet Series](https://cheatsheetseries.owasp.org/)

---

## 🤝 Getting Help

**Issues or questions about deployment?**

1. Check the [Troubleshooting Guide](troubleshooting.md)
2. Review relevant deployment guides above
3. Open an issue on [GitHub](https://github.com/victorytek/home-registry/issues)
4. Include:
   - Deployment scenario (self-hosted/business/enterprise)
   - Error messages and logs
   - Configuration files (remove sensitive data)
   - Steps to reproduce the issue

---

## 📋 Deployment Checklist

Use this checklist to ensure a successful production deployment:

### Planning Phase
- [ ] Review deployment scenarios and choose appropriate stack
- [ ] Estimate resource requirements (CPU, RAM, disk, bandwidth)
- [ ] Select hosting provider and provision server(s)
- [ ] Register domain name and configure DNS
- [ ] Plan backup strategy and retention policy
- [ ] Define RTO and RPO targets

### Pre-Deployment
- [ ] Review and follow [Security Hardening Guide](security-hardening.md)
- [ ] Configure reverse proxy (Nginx/Caddy/Traefik)
- [ ] Obtain SSL certificate (automatic with Caddy, manual with Certbot)
- [ ] Set strong passwords in `.env` file
- [ ] Configure firewall rules (ufw/iptables)
- [ ] Set up monitoring and alerting

### Deployment
- [ ] Copy production configuration files
- [ ] Update domain names in configurations
- [ ] Start services with Docker Compose
- [ ] Verify health checks pass
- [ ] Test HTTPS connection (SSL Labs test)
- [ ] Create first admin account
- [ ] Configure automated backups

### Post-Deployment
- [ ] Verify monitoring dashboards show data
- [ ] Test backup and restore procedures
- [ ] Configure alert notifications (email/Slack/PagerDuty)
- [ ] Review application logs for errors
- [ ] Document deployment for team/future reference
- [ ] Set up regular maintenance schedule (updates, security patches)

### Ongoing Maintenance
- [ ] Monitor application and system metrics
- [ ] Review logs weekly for errors or anomalies
- [ ] Test backup restores quarterly
- [ ] Apply security updates monthly
- [ ] Review and adjust resource allocation as needed
- [ ] Conduct security audits quarterly

---

**Ready to deploy?** Start with the [Quick Start Guide](quickstart.md) and deploy in <15 minutes!
