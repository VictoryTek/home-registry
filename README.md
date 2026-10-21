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
- 📦 **Inventory Management** - Organize items with your own organizers (dropdown, text and photo fields per inventory), locations and notes
- 🗄️ **Database-Driven** - PostgreSQL backend with comprehensive data relationships
- 🏷️ **Flexible Organization** - Define organizers per inventory (Room, Brand, Photo, ...) instead of a fixed set of categories

## Quick Start with Docker Compose

The easiest way to try Home Registry is with Docker Compose, which sets up both the PostgreSQL database and the application:

### 1. Create Configuration File

**First-time setup:**

```bash
# Copy the example environment file
cp .env.example .env

# .env is ignored by Git, so your values stay on your machine
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
# Build the image. --pull and the date build argument refresh the base image and the package
# updates: without the argument, Docker keeps the cached apk layer indefinitely and the image can
# keep outdated packages.
docker build --pull --build-arg APK_CACHE_BUST=$(date -u +%F) -t home-registry .

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
| `RUST_LOG` | Logging level (`error`, `warn`, `info`, `debug`, `trace`); do not use `debug` or `trace` for `actix_web` in production (see **What is logged**) | `info` | No |
| `JWT_SECRET` | Secret key for JWT token signing (auto-generated if not set) | Auto-generated | No* |
| `JWT_TOKEN_LIFETIME_HOURS` | JWT token expiration time in hours | `24` | No |
| `RATE_LIMIT_RPS` | Sustained API requests per second per client (enforced over 60 s) | `50` | No |
| `RATE_LIMIT_BURST` | Maximum API requests per client in any 1-second window | `100` | No |
| `AUTH_RATE_LIMIT_PER_MINUTE` | Requests per minute per client to login, register, invitation check, password reset, setup and recovery endpoints | `20` | No |
| `TRUST_PROXY_HEADERS` | Identify clients by `Forwarded` / `X-Forwarded-For` (only behind a trusted reverse proxy) | `false` | No |

**\*JWT_SECRET Note:** If not explicitly set, a random secret is generated and persisted to `/app/data/jwt_secret`. This ensures tokens remain valid across container restarts. For production, it's recommended to set this explicitly.

**Rate Limiting Explained:**
- **RATE_LIMIT_RPS**: Sustained request rate per client. `100` allows 6,000 requests in any 60-second window.
- **RATE_LIMIT_BURST**: Short spikes. `200` allows at most 200 requests per client within any 1-second window.
- **AUTH_RATE_LIMIT_PER_MINUTE**: Flood guard for the unauthenticated credential endpoints. Repeated wrong passwords or recovery codes are additionally locked out per username and client address (5 failures lock that pair for 15 minutes; the response is `429` with a `Retry-After` header).
- **TRUST_PROXY_HEADERS**: Clients are identified by their connection address. Behind a reverse proxy that is the proxy's address, so every user would share one rate-limit bucket. Set this to `true` **only** if the app is reachable exclusively through your proxy (do not publish port 8210 to the network); otherwise clients can spoof their address and evade the limits. Your proxy must also *overwrite* (not append to) `X-Forwarded-For`/`Forwarded`, e.g. nginx `proxy_set_header X-Forwarded-For $remote_addr;`, because the first address in the header is the one used. The server logs a warning if it sees forwarding headers while this is off. IPv6 clients are limited per `/64` prefix.
- **Use Case**: Protects your server from being overwhelmed by aggressive API clients, accidental infinite loops, or potential DoS attacks.
- **Production Recommendation**: Start with `RPS: 100` and `BURST: 200`, then adjust based on your usage patterns and server capacity.

**Sessions and sign-out:**
- The **Sign out** button only clears the token stored in your browser. The token itself stays valid until it expires (`JWT_TOKEN_LIFETIME_HOURS`, default 24 hours); logging out does not revoke it.
- To really end sign-ins on other devices, use **Settings → Active Sessions → Sign out all other devices** (`POST /api/auth/logout-all`). Changing your password or turning two-factor authentication on or off does the same automatically, and the device you do it on stays signed in. An administrator resetting your password, deactivating you or changing your role signs you out on every device.

**What is logged:**
- The access log records the client address, the method, the matched route pattern (for example `/api/inventories/{id}`, not `/api/inventories/42`), the status, the response size and the duration. It never records query strings, the `Referer` and `User-Agent` headers or values in path parameters, and a method outside GET, HEAD, POST, PUT, DELETE, PATCH, OPTIONS, CONNECT and TRACE is logged as `OTHER`. The access log, the rejected-request lines and the unknown-endpoint line described below therefore contain nothing the client chose.
- Search terms travel in query strings: the item search API is `GET /api/items/search?q=...`, and the inventory page and search page keep what you searched for in their address (`/inventory/3?q=drill`, `/search?q=drill`). The application never logs them, but **your reverse proxy's access log does**: anyone who can read that log can read what users searched for. The deprecated `GET /api/items/search/{query}` puts the term in the path, which the proxy logs as well. Keep the proxy's logs as private as the data itself.
- This applies to those HTTP-layer lines only. Application and audit log lines elsewhere (for example "created item", "listed backups for user", or a file name in an error) can still contain item, inventory and organizer names, file names and usernames; review them before sharing logs.
- A request that matches no route is logged as `<unmatched>` and the 404 response does not repeat the URL. The application deliberately does not log unknown URLs: use your reverse proxy's access log to see probes, scans and user agents.
- A request whose body, query string or path is rejected is logged as `Rejected request: <method> "<route>" (<category>)`. Neither the content nor the validation message is logged, and the error response never repeats what was sent.
- Do not run production with `RUST_LOG=actix_web=debug` (or `debug` / `trace` globally): actix-web's own debug lines for failed request parsing include the concrete request path, which this application otherwise keeps out of its logs.

### Registration, invitations and password resets

Who can create an account is a setting, under **Settings → User Management → Registration** (administrators only):

| Mode | Who can register |
|------|------------------|
| **Invite only** (default) | Only someone holding an invitation link from an administrator. |
| **Open** | Anyone who can reach the sign-in page. |
| **Closed** | Nobody. Administrators can still add users, and invitations that were already made keep their expiry and work again if you switch back to invite only. |

- **Invitation links:** **Invite Someone** creates a link such as `https://your-host/register#invite=...` that works once and expires after 7 days. The account it creates is always an ordinary, active user; an invitation cannot make an administrator. Unused invitations are listed and can be revoked.
- **Password reset links:** the key button next to a user creates `https://your-host/reset#token=...`, valid for 24 hours. Whoever opens it chooses a new password (the same rules as everywhere else) and is signed out on every device; they are not signed in automatically, and two-factor authentication still applies when they sign in. A newer link for the same user replaces the older one, and deleting the user ends it. There is no typed-password reset in the interface any more.
- **Copy the link when it is shown.** Only a SHA-256 fingerprint of the token is stored, so the link cannot be shown again; create a new one if it is lost.
- **Why the token is after a `#`:** a browser never sends the part after `#` to the server, so tokens stay out of your reverse proxy's access log and out of `Referer` headers. The page removes it from the address bar as soon as it has read it, so reloading the page needs the link again.
- **Backups:** the (hashed) tokens are part of a backup, but **restoring revokes every unused invitation and reset link**, so a backup can never bring back a link that has since been used or revoked.
- The invitation and reset endpoints share the per-client limit for credential endpoints (`AUTH_RATE_LIMIT_PER_MINUTE`).

### Reports and CSV export

Open an inventory and choose **Report** to see its items, statistics and a breakdown, and to download them as CSV.

- **Group by:** the breakdown groups the items by any *select* organizer of the inventory (the first one in display order by default). Items without a value, or whose option was deleted, are in a group called **No value**. An item has at most one value per organizer, so every item is counted once.
- **Filters:** dates, prices and one dropdown per select organizer (*Any*, an option, or *No value*). The item list, the statistics, the breakdown and the CSV always describe the same filtered items. A missing quantity counts as 1, a missing price adds nothing to a value and is left out of the average.
- **CSV:** one column per select or text organizer after *Description*. If some exported items still hold a value in the old `category` / `location` fields (see below), *Legacy category* / *Legacy location* columns are added. Text that starts with `=`, `+`, `-`, `@`, a tab, a carriage return or a line feed gets a leading `'` so spreadsheets do not run it as a formula.
- **API:** `GET /api/reports/inventory` (also `/statistics` and `/categories`, with the same parameters) takes `inventory_id`, `from_date`, `to_date`, `min_price`, `max_price`, `group_by=<select organizer id>` and `organizers=<organizer id>:<option id or none>[,...]` (several options of one organizer match any of them; different organizers must all match; at most 50 pairs). `group_by` and `organizers` need `inventory_id`. **Deprecated:** without `group_by` the breakdown is by the legacy `category` text, which is removed together with the legacy columns.
- **Legacy values:** before organizers existed, items had plain `category` and `location` text. Migration V011 turned them into "Category" and "Location" organizers, except in inventories that already had an organizer with that name, and API clients can still write the old fields. To see whether any item holds a legacy value that no organizer carries:

  ```sql
  SELECT
    count(*) FILTER (WHERE NULLIF(category, '') IS NOT NULL) AS items_with_legacy_category,
    count(*) FILTER (WHERE NULLIF(location, '') IS NOT NULL) AS items_with_legacy_location
  FROM items;
  ```

### Backups and restore

Administrators create, download, upload and restore backups under **Settings → Backup & Restore**. Backups are JSON files in `/app/backups` (the `backups` volume).

- **What is in a backup:** every table in the database, read from one consistent snapshot, plus the database schema version and the app version. New tables are included automatically. **Uploaded photos (the `uploads` volume) are not part of a backup**; back that volume up separately.
- **Treat backup files as secrets.** They contain password hashes, recovery-code hashes and the encrypted two-factor secrets. Someone holding a backup can try to guess a weak `JWT_SECRET` offline (the two-factor secrets are encrypted with a key derived from it unless you set `TOTP_ENCRYPTION_KEY`). Use a long random `JWT_SECRET`, and set `TOTP_ENCRYPTION_KEY` separately so changing `JWT_SECRET` later does not make two-factor authentication unreadable.
- **Restoring replaces everything** with the backup's contents, in one transaction (any error leaves the database untouched). A backup of the current data is written first, and the restore is aborted if that fails. **All sessions end**: everyone, including the administrator who restored, signs in again. **Unused invitation and password-reset links are revoked** as well (see *Registration, invitations and password resets*).
- **Compatibility:** backups from older versions restore (tables or columns that no longer exist are skipped, and the result lists them). A backup made by a **newer** version than the server is refused; update the server first.
- **Two-factor key:** if the backup contains two-factor enrolment encrypted with a different key than this server's, the restore is refused. Start the server with the original `TOTP_ENCRYPTION_KEY` (or `JWT_SECRET`) and restore again. Alternatively choose *Restore Without 2FA* in the second dialog: everything else is restored, two-factor authentication is cleared for the affected users, and until they set it up again they can sign in with their password alone.
- **Registration mode:** a restore keeps the server's current registration mode (open, invite only or closed) whatever the backup held, so restoring an old backup never reopens registration. A server that has never had a mode chosen stays invite only.
- **File names:** only names generated by the application (`home_registry_<timestamp>[_<suffix>].json`, `home_registry_auto_pre_restore_...`) are listed and can be downloaded, restored or deleted. A `.json` file with another name placed in the folder by hand is ignored; rename it to the pattern to use it.

### Upgrade notes

**Reports and CSV (no migration).** The CSV export's *Category* and *Location* columns are replaced by one column per organizer, so scripts that read the file by column name may need updating; *Legacy category* / *Legacy location* columns appear only when some item still holds such a value. Text cells that start with `=`, `+`, `-`, `@`, a tab or a line break get a leading `'`. Report statistics now follow the filters, and a report filtered by date works (it used to fail).

**Migration V028: registration is now invite-only.** Until now anyone who could reach the sign-in page could create an account, with no way to turn that off. Every install, new or upgraded, now starts as **invite only**. If you relied on open sign-up, an administrator can switch it back under **Settings → User Management → Registration** (and sees a notice there after an upgrade until they dismiss it). The server logs a one-time warning on the first start after the upgrade. Custom clients of `POST /api/auth/register` now need an `invite_token` unless the mode is open. This migration adds the `app_settings` and `auth_tokens` tables.

**Migration V026 removes unused tables.** It drops `categories`, `tags`, `item_tags`, `custom_fields`, `item_custom_values`, `password_reset_tokens` and the column `items.category_id`. The feature they were meant for was replaced by organizers, and password reset by recovery codes and two-factor authentication. No tagged release (v0.1.0-beta.1 to v0.1.0-beta.3) ever wrote to any of them, so they should be empty. (A short-lived password-reset feature wrote `password_reset_tokens` on the development branch for a few days in February 2026, before beta.1.) The item's own `category` text field is not affected.

To check your database before upgrading:

```sql
SELECT 'categories' AS source, count(*) FROM categories
UNION ALL SELECT 'tags', count(*) FROM tags
UNION ALL SELECT 'item_tags', count(*) FROM item_tags
UNION ALL SELECT 'custom_fields', count(*) FROM custom_fields
UNION ALL SELECT 'item_custom_values', count(*) FROM item_custom_values
UNION ALL SELECT 'password_reset_tokens', count(*) FROM password_reset_tokens
UNION ALL SELECT 'items.category_id', count(*) FROM items WHERE category_id IS NOT NULL;
```

- **If any count is above zero,** the server writes a backup before migrating and logs the counts and the file name at WARN. It is `backups/home_registry_pre_migration_v026_<timestamp>_<id>.json` and also appears under **Settings → Backup & Restore**. If that backup cannot be written (for example the `backups` volume is read-only), the server **refuses to start** and the database is left unchanged; fix the volume and start again.
- **The app cannot restore those rows after the migration:** a restore skips tables that no longer exist and reports how many rows it skipped. They stay readable in the backup file.
- **Migration V027: items must name their inventory.** `POST /api/items` now requires `inventory_id` (a positive integer) and returns `400` without it; before, an omitted value silently filed the item into inventory #1. The web interface already sent it. Custom API clients must be updated, and the database column no longer has a default. Existing items are unchanged.
- **Downgrading** to a version older than V026 against a migrated database fails, because the older version expects those tables. To go back, restore a backup taken before the upgrade using the older version.

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
| `authRateLimitPerMinute` | int or null | `null` | Optional `AUTH_RATE_LIMIT_PER_MINUTE` override. |
| `trustProxyHeaders` | bool | `false` | Sets `TRUST_PROXY_HEADERS`; enable only behind a trusted reverse proxy. |
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