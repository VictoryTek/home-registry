<div align="center">
  <img src="frontend/public/logo_full.png" alt="Home Registry Logo" width="400"/>
</div>

# Home Registry
  <p><em>Universal Home Inventory Management System</em></p>
  <p><strong>Version 0.1.0-beta.3</strong></p>

Inspired by HomeBox project, which I used for years. When it was no longer maintained, I thought I would attempt to build a successor to that project.

A modern, universal, web-based home inventory management system built with **Rust + Actix-Web + PostgreSQL**. Keep track of your belongings with an intuitive interface and a simple to use system. Create an inventory for anything you want to track. Create custom Organizers to expand fields that your inventory can track, such as Serial #, Model #, etc. Then add items to your inventory.

## Features

- 🎨 **Modern Web Interface** - Beautiful responsive design with dark/light theme support
- � **Inventory Management** - Organize items by categories, locations, and custom tags
- 🗄️ **Database-Driven** - PostgreSQL backend with comprehensive data relationships
- 🏷️ **Flexible Organization** - Categories, tags, and custom fields for any item type

## Quick Start with Docker Compose

The easiest way to try Home Registry is with Docker Compose, which sets up both the PostgreSQL database and the application:

### 1. Create Configuration File

**First-time setup:**

```bash
# Copy the example environment file
cp .env.example .env

# Edit .env and set your database password
# POSTGRES_PASSWORD=your_secure_password

# The docker-compose.yml has a random default, but you should set your own:
# - Open .env in a text editor
# - Set POSTGRES_PASSWORD to your secure password (16+ characters)
# - Save and close
```

**Recommended password requirements:**
- Minimum 16 characters
- Mix of uppercase, lowercase, numbers, and symbols
- Avoid these characters in passwords: `@` `:` `/` (they conflict with connection strings)
- Use a password manager to generate strong passwords
- Example strong password: `7mK$9pQx2#nLwR5tY8vB3zF`

### 2. Start the Application

```bash
docker compose up -d
```

The application will be available at `http://YOUR_IP_ADDRESS:8210`

**Default Configuration:**
- **Application Port:** `8210` (customizable via `.env`)
- **Database:** `home_inventory`
- **Username:** `postgres`
- **Password:** Set in your `.env` file (defaults to a random password if not configured)
  - **⚠️ Security Note:** Always set your own password in `.env` - never rely on the default

**Data Persistence:**
- Database data is persisted in the `pgdata` Docker volume
- Application data (including JWT secrets) is stored in the `appdata` volume
- Database backups are stored in the `backups` volume
- Your data will survive container restarts and updates

### 3. First-Time Setup

After starting containers:

1. Open your browser to `http://YOUR_IP_ADDRESS:8210`
2. Create your admin account
3. Start adding your inventory items!

**⚠️  Security Checklist for Production:**
- ✅ Set strong `POSTGRES_PASSWORD` in `.env` (16+ characters)
- ✅ Set explicit `JWT_SECRET` in `.env` for token consistency
- ✅ Adjust `RATE_LIMIT_RPS` and `RATE_LIMIT_BURST` based on your traffic
- ✅ Never commit your `.env` file to Git
- ✅ Regularly backup your database (see Backup section)

**Useful Commands:**
```bash
# Start the application
docker-compose up -d

# build the application
docker-compose build

# Stop the application
docker-compose down

# Stop and remove all data (⚠️ destructive)
docker-compose down -v

# View database logs
docker-compose logs -f db
```

### Docker Compose Configuration

Here's the complete `docker-compose.yml` file:

```yaml
services:
  app:
    image: ghcr.io/victorytek/home-registry:beta
    container_name: home-registry-app
    depends_on:
      db:
        condition: service_healthy
    environment:
      DATABASE_URL: postgres://postgres:${POSTGRES_PASSWORD:-uK8m3NvQ7wPxRj2Y5tLz}@db:5432/home_inventory
      # ☝️ Database password matches db service above. Override in .env file!
      PORT: ${PORT:-8210}
      RUST_LOG: ${RUST_LOG:-info}
      JWT_SECRET: ${JWT_SECRET}
      JWT_TOKEN_LIFETIME_HOURS: ${JWT_TOKEN_LIFETIME_HOURS:-24}
      RATE_LIMIT_RPS: ${RATE_LIMIT_RPS:-100}
      RATE_LIMIT_BURST: ${RATE_LIMIT_BURST:-200}
    ports:
      - "8210:8210"
    volumes:
      - appdata:/app/data
      - backups:/app/backups
    restart: unless-stopped
    healthcheck:
      test: ["CMD-SHELL", "curl -f http://localhost:8210/health || exit 1"]
      interval: 30s
      timeout: 10s
      start_period: 10s
      retries: 3

  db:
    image: postgres:17
    container_name: home-registry-db
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD:-uK8m3NvQ7wPxRj2Y5tLz}
      # ☝️ Default above is randomly generated. Override in .env file!
      POSTGRES_DB: home_inventory
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 5s
      timeout: 5s
      retries: 5
    restart: unless-stopped

volumes:
  pgdata:
    name: home-registry-pgdata
  appdata:
    name: home-registry-appdata
  backups:
    name: home-registry-backups
```

## Running with Docker (Standalone)

If you prefer to run just the application container (with an external PostgreSQL database), you can use the standalone Docker command:

```bash
# Build the image
docker build -t home-registry .

# Run the container
docker run -d \
  --name home-registry \
  -p 8210:8210 \
  -e DATABASE_URL="postgres://postgres:password@your-db-host:5432/home_inventory" \
  -e PORT=8210 \
  -e RUST_LOG=info \
  -e JWT_TOKEN_LIFETIME_HOURS=24 \
  -e RATE_LIMIT_RPS=100 \
  -e RATE_LIMIT_BURST=200 \
  -v home-registry-data:/app/data \
  -v home-registry-backups:/app/backups \
  home-registry
```

**Important Notes:**
- Replace `your-db-host` with your PostgreSQL server address
- Ensure your PostgreSQL database is accessible from the container
- The `/app/data` volume persists JWT secrets and other application data
- The `/app/backups` volume stores database backup files
- Rate limiting protects your API from being overwhelmed (adjust RPS/BURST as needed)

## Environment Variables

The application supports the following configuration through environment variables:

| Variable | Description | Default | Required |
|----------|-------------|---------|----------|
| `DATABASE_URL` | PostgreSQL connection string | - | ✅ Yes |
| `PORT` | HTTP server port | `8210` | No |
| `RUST_LOG` | Logging level (`error`, `warn`, `info`, `debug`, `trace`) | `info` | No |
| `JWT_SECRET` | Secret key for JWT token signing (auto-generated if not set) | Auto-generated | No* |
| `JWT_TOKEN_LIFETIME_HOURS` | JWT token expiration time in hours | `24` | No |
| `RATE_LIMIT_RPS` | Maximum API requests per second | `50` | No |
| `RATE_LIMIT_BURST` | Burst capacity for temporary traffic spikes | `100` | No |

**\*JWT_SECRET Note:** If not explicitly set, a random secret is generated and persisted to `/app/data/jwt_secret`. This ensures tokens remain valid across container restarts. For production, it's recommended to set this explicitly.

**Rate Limiting Explained:**
- **RATE_LIMIT_RPS**: Controls sustained API request throughput. If set to `100`, the server accepts up to 100 requests per second continuously.
- **RATE_LIMIT_BURST**: Allows temporary spikes above the RPS limit. With `BURST: 200`, the server can handle short bursts of 200 requests before enforcing the RPS limit.
- **Use Case**: Protects your server from being overwhelmed by aggressive API clients, accidental infinite loops, or potential DoS attacks.
- **Production Recommendation**: Start with `RPS: 100` and `BURST: 200`, then adjust based on your usage patterns and server capacity.

## Nix / NixOS Deployment

In addition to the Docker path above, home-registry ships a [Nix flake](https://nixos.wiki/wiki/Flakes)
exposing a package and a NixOS module for running it as a native systemd service,
with no container runtime involved.

**Note on `DATABASE_URL`:** the app's `DATABASE_URL` parser splits on `@`, `:` and `/`
without percent-decoding, so a password containing any of those characters breaks the
connection string. The app also accepts discrete `POSTGRES_HOST` / `POSTGRES_PORT` /
`POSTGRES_USER` / `POSTGRES_PASSWORD` / `POSTGRES_DB` environment variables instead —
`POSTGRES_PASSWORD` is read as a single opaque string with no splitting, so any
password works. The NixOS module below always uses this discrete-variable path and
never sets `DATABASE_URL`.

**Migrations:** the binary embeds all SQL migrations at compile time and runs them
itself against the configured database on every startup, before it binds the HTTP
port (see `src/main.rs`). The NixOS module does not run a separate migration step.

### Building the package

```bash
nix build .#default
./result/bin/home-registry
```

### Using the NixOS module

Add this flake as an input and import `nixosModules.default`:

```nix
{
  inputs.home-registry.url = "github:your-org/home-registry";

  outputs = { self, nixpkgs, home-registry, ... }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        home-registry.nixosModules.default
        {
          services.home-registry = {
            enable = true;
            port = 8210;
            openFirewall = true;
            environmentFile = "/run/secrets/home-registry.env"; # POSTGRES_PASSWORD=...
            database = {
              createLocally = true; # provisions services.postgresql locally
              user = "home_registry";
              name = "home_registry";
            };
          };
        }
      ];
    };
  };
}
```

Set `database.createLocally = false` and `database.host`/`database.port` to point at
an externally-managed PostgreSQL instance instead; `environmentFile` must still
provide `POSTGRES_PASSWORD` for that role.

#### Module options (`services.home-registry`)

| Option | Type | Default | Description |
|---|---|---|---|
| `enable` | bool | `false` | Enable the service. |
| `package` | package | `pkgs.home-registry` (via this flake's overlay) | Package to run. |
| `port` | port | `8210` | HTTP listen port. |
| `dataDir` | path | `/var/lib/home-registry` | Holds the persisted JWT secret, uploaded images, backups, and a symlink to the packaged static frontend assets. At the default path the service runs with `DynamicUser`; a custom path switches to a dedicated `home-registry` system user. |
| `environmentFile` | path or null | `null` | `EnvironmentFile` for secrets (`POSTGRES_PASSWORD`, optional `JWT_SECRET`). Required when `database.createLocally = true`. Never stored in the Nix store. |
| `openFirewall` | bool | `true` | Open `port`/tcp in `networking.firewall`. |
| `rateLimitRps` | int or null | `null` | Optional `RATE_LIMIT_RPS` override. |
| `rateLimitBurst` | int or null | `null` | Optional `RATE_LIMIT_BURST` override. |
| `database.createLocally` | bool | `false` | Provision a local PostgreSQL database/role via `services.postgresql.ensureDatabases`/`ensureUsers`, and sync its password from `environmentFile` on every service start. |
| `database.host` | str | `"127.0.0.1"` | `POSTGRES_HOST`. |
| `database.port` | port | `5432` | `POSTGRES_PORT`. |
| `database.user` | str | `"home_registry"` | `POSTGRES_USER`. |
| `database.name` | str | `"home_registry"` | `POSTGRES_DB`. |

## Production Deployment

For production deployments with HTTPS, reverse proxy, monitoring, and high availability, see our comprehensive deployment guides:

📚 **[Complete Deployment Documentation](docs/deployment/)**

### Quick Links

- **[⚡ Quick Start (15 minutes)](docs/deployment/quickstart.md)** - Deploy with HTTPS in <15 minutes using Caddy
- **[🔒 Security Hardening](docs/deployment/security-hardening.md)** - Production security checklist
- **[🗄️ Database Production Guide](docs/deployment/database-production.md)** - PostgreSQL tuning, backups, replication
- **[📊 Monitoring & Logging](docs/deployment/monitoring-logging.md)** - Prometheus, Grafana, Loki setup
- **[🔄 High Availability](docs/deployment/high-availability.md)** - Multi-instance deployment patterns

### Reverse Proxy Options

- **[Nginx](docs/deployment/reverse-proxy-nginx.md)** - Enterprise reverse proxy with manual SSL
- **[Caddy](docs/deployment/reverse-proxy-caddy.md)** - Automatic HTTPS (recommended for quick setup)
- **[Traefik](docs/deployment/reverse-proxy-traefik.md)** - Docker-native with service discovery

### Configuration Examples

Production-ready configuration files are available in [docs/examples/](docs/examples/):
- [docker-compose-production.yml](docs/examples/docker-compose-production.yml) - Complete production setup
- [nginx.conf](docs/examples/nginx.conf) - Nginx reverse proxy configuration
- [Caddyfile](docs/examples/Caddyfile) - Caddy automatic HTTPS configuration
- [backup.sh](docs/examples/backup.sh) - Automated backup script
- [restore.sh](docs/examples/restore.sh) - Database restore script

### Need Help?

- See [Troubleshooting Guide](docs/deployment/troubleshooting.md) for common issues
- Review [Deployment Overview](docs/deployment/) for architecture guidance