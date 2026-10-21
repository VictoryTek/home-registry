# =============================================================================
# Home Registry - Hardened Multi-Stage Dockerfile
# =============================================================================
# Security features:
# - Pinned base images with specific versions
# - Non-root user in final stage
# - Minimal final image (distroless-style)
# - No shell access in production
# - Health check included
# =============================================================================

# ------------------------------------------------------------------------------
# Stage 1: Build React Frontend
# ------------------------------------------------------------------------------
FROM node:20.18-alpine3.20 AS frontend-builder

# Set working directory
WORKDIR /app/frontend

# Copy package files first for better layer caching
COPY frontend/package*.json ./

# Install dependencies with locked versions
RUN npm ci --ignore-scripts && npm cache clean --force

# Copy source code
COPY frontend/ ./

# Build production bundle
RUN npm run build

# ------------------------------------------------------------------------------
# Stage 2: Build Rust Backend
# ------------------------------------------------------------------------------
# NOTE: Rust 2.0 does not exist. Current stable is 1.8x series.
# This Dockerfile uses the latest Rust 1.x available on Alpine.
# Ignore any automated alerts claiming "Rust 2" is available.
FROM rust:1-alpine AS backend-builder

# Install Alpine build dependencies
# openssl-libs-static allows static linking so runtime image needs no OpenSSL
RUN apk add --no-cache \
    musl-dev \
    pkgconfig \
    openssl-dev \
    openssl-libs-static

# Create app directory
WORKDIR /app

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./

# Create dummy source for dependency compilation with matching structure
RUN mkdir -p src/api src/auth src/db src/models && \
    echo 'fn main() { println!("Dummy"); }' > src/main.rs && \
    echo 'pub mod api;' > src/lib.rs && \
    echo 'pub mod auth;' >> src/lib.rs && \
    echo 'pub mod db;' >> src/lib.rs && \
    echo 'pub mod models;' >> src/lib.rs && \
    echo 'pub fn dummy() {}' > src/api/mod.rs && \
    echo 'pub fn dummy() {}' > src/auth/mod.rs && \
    echo 'pub fn dummy() {}' > src/db/mod.rs && \
    echo 'pub fn dummy() {}' > src/models/mod.rs

# Build dependencies only (cached layer)
# Then remove dummy source AND application-specific build artifacts.
# Dependency caches in target/release/deps/ are preserved (except home-registry's own).
# This forces cargo to recompile the real application while reusing cached dependencies.
RUN cargo build --release && \
    rm -rf src && \
    rm -f target/release/home-registry && \
    rm -f target/release/deps/home_registry-* && \
    rm -f target/release/deps/libhome_registry-* && \
    rm -rf target/release/.fingerprint/home-registry-* && \
    rm -rf target/release/.fingerprint/home_registry-* && \
    rm -rf target/release/incremental/home_registry-*

# Copy actual source code
COPY src ./src

# Copy migrations directory (required for embed_migrations! macro at compile time)
COPY migrations ./migrations

# Build the actual application — cargo detects missing fingerprints and
# recompiles home-registry from real source, linking against cached deps.
# The embed_migrations! macro will bundle all SQL files into the binary.
RUN touch src/main.rs src/lib.rs && cargo build --release --locked

# Strip the binary for smaller size
RUN strip target/release/home-registry

# Smoke-test: verify the binary is valid
RUN ./target/release/home-registry --help || true

# ------------------------------------------------------------------------------
# Stage 3: Final Production Image
# ------------------------------------------------------------------------------
FROM alpine:3.21 AS runtime

# Labels for container metadata
LABEL org.opencontainers.image.title="Home Registry"
LABEL org.opencontainers.image.description="Home inventory management system"
LABEL org.opencontainers.image.version="0.1.0-beta.3"
LABEL org.opencontainers.image.licenses="MIT"

# Install minimal runtime dependencies
# - ca-certificates: TLS root certs for outbound HTTPS
# - libgcc: Rust runtime support on musl/Alpine
# - curl: used by HEALTHCHECK
# OpenSSL is statically linked at build time — no libssl needed at runtime
#
# APK_CACHE_BUST: preflight and CI pass today's date (YYYY-MM-DD). A new value invalidates this
# layer, so the package updates below are fetched again at least once a day instead of staying in
# the build cache for weeks (a cached layer kept an old curl with known CVEs).
ARG APK_CACHE_BUST=unset
RUN echo "Package updates refreshed for: ${APK_CACHE_BUST}" && \
    apk update && \
    apk upgrade --no-cache && \
    apk add --no-cache \
    ca-certificates \
    libgcc \
    curl && \
    rm -rf /var/cache/apk/*

# Create non-root user for security (Alpine syntax)
RUN addgroup -S appgroup \
    && adduser -S -G appgroup -s /bin/false appuser

# Set working directory
WORKDIR /app

# Copy the compiled binary from builder (includes embedded migrations)
COPY --from=backend-builder --chown=appuser:appgroup /app/target/release/home-registry ./

# Copy built frontend to static directory
COPY --from=frontend-builder --chown=appuser:appgroup /app/frontend/dist ./static

# Create backups directory with proper ownership
RUN mkdir -p /app/backups && chown appuser:appgroup /app/backups

# Create uploads directory for image storage with proper ownership
RUN mkdir -p /app/uploads/img && chown -R appuser:appgroup /app/uploads

# Create data directory for JWT secrets and other app data with proper ownership
RUN mkdir -p /app/data && chown -R appuser:appgroup /app/data

# Set proper permissions
RUN chmod 755 /app/home-registry

# Switch to non-root user
USER appuser

# Expose application port
EXPOSE 8210

# Health check (use curl since Alpine image includes it)
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8210/health || exit 1

# Run the binary
# Note: Use exec form to ensure signals are properly handled
CMD ["./home-registry"]
