//! Home Registry - A home inventory management system
//!
//! This is the main entry point for the Home Registry server.

#![deny(unsafe_code)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

use actix_cors::Cors;
use actix_files as fs;
use actix_web::{
    middleware::{DefaultHeaders, Logger},
    web, App, HttpResponse, HttpServer, Responder,
};
use dotenvy::dotenv;
use refinery::embed_migrations;
use std::env;

// Use the library crate
use home_registry::rate_limit::{RateLimitSettings, RateLimits};
use home_registry::{api, auth, db};

// Embed migrations from the migrations directory at compile time
// This allows the application to run migrations programmatically on startup
embed_migrations!("migrations");

async fn health() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "service": "home-registry",
        "version": env!("CARGO_PKG_VERSION"),
        "timestamp": chrono::Utc::now()
    }))
}

// Serve index.html for client-side routing (SPA fallback)
async fn spa_fallback() -> actix_web::Result<fs::NamedFile> {
    Ok(fs::NamedFile::open("static/index.html")?)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    env_logger::init();

    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT").unwrap_or_else(|_| "8210".to_string());

    log::info!("Starting Home Inventory server at http://{}:{}", host, port);
    log::info!(
        "Environment: {}",
        env::var("RUST_ENV").unwrap_or_else(|_| "development".to_string())
    );

    // Ensure uploads directory exists for image storage
    match std::fs::create_dir_all("uploads/img") {
        Ok(()) => log::info!("Uploads directory ready: uploads/img/"),
        Err(e) => log::warn!(
            "Could not create uploads/img directory: {}. Image uploads will be unavailable.",
            e
        ),
    }

    // Initialize JWT secret at startup (will auto-generate if not found)
    let _ = auth::get_or_init_jwt_secret();
    log::info!(
        "JWT token lifetime: {} hours",
        auth::jwt_token_lifetime_hours()
    );

    // Initialize database pool with proper error handling (no panics)
    let pool = match db::get_pool() {
        Ok(p) => {
            log::info!("Database pool initialized successfully");
            p
        },
        Err(e) => {
            log::error!("Failed to initialize database pool: {}", e);
            std::process::exit(1);
        },
    };

    // Migration V026 deletes tables no release ever wrote to. If they hold data anyway, write a
    // backup first, and do not migrate (or start) if that backup cannot be written.
    if let Err(e) = home_registry::startup::protect_legacy_data_before_migration(
        &pool,
        std::path::Path::new(home_registry::api::backup::BACKUPS_DIR),
    )
    .await
    {
        log::error!("Refusing to start: {}", e);
        std::process::exit(1);
    }

    // Run database migrations automatically at startup
    // Migrations are embedded in the binary and applied idempotently
    log::info!("Running database migrations...");
    let mut client = match pool.get().await {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to get database connection for migrations: {}", e);
            std::process::exit(1);
        },
    };

    match migrations::runner().run_async(&mut **client).await {
        Ok(report) => {
            let applied_count = report.applied_migrations().len();
            if applied_count > 0 {
                log::info!(
                    "Database migrations completed successfully. Applied {} new migration(s)",
                    applied_count
                );
            } else {
                log::info!("Database schema is up to date. No new migrations to apply");
            }
        },
        Err(e) => {
            log::error!("Database migrations failed: {}", e);
            log::error!(
                "Cannot start application with outdated database schema. \
                 Please check migration files and database connectivity."
            );
            std::process::exit(1);
        },
    }

    // Drop the migration client back to the pool
    drop(client);
    log::info!("Migration client returned to pool");

    // Rate limiting configuration (RATE_LIMIT_RPS, RATE_LIMIT_BURST, AUTH_RATE_LIMIT_PER_MINUTE,
    // TRUST_PROXY_HEADERS). The limiter state is created once here and shared by all workers.
    let rate_limit_settings = RateLimitSettings::from_env();
    rate_limit_settings.log_startup();
    let rate_limits = RateLimits::new(rate_limit_settings);

    HttpServer::new(move || {
        // Configure CORS
        let cors = Cors::default()
            .allowed_origin_fn(|origin, _req_head| {
                // Allow requests with no origin (same-origin requests)
                // Allow localhost in development
                let origin_str = origin.to_str().unwrap_or("");
                origin_str.starts_with("http://localhost")
                    || origin_str.starts_with("https://localhost")
                    || origin_str.starts_with("http://127.0.0.1")
                    || origin_str.starts_with("https://127.0.0.1")
            })
            .allowed_methods(vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"])
            .allowed_headers(vec![
                actix_web::http::header::AUTHORIZATION,
                actix_web::http::header::CONTENT_TYPE,
                actix_web::http::header::ACCEPT,
            ])
            .supports_credentials()
            .max_age(3600);

        App::new()
            .app_data(web::Data::new(pool.clone()))
            // Allow up to 15 MiB JSON bodies (default is 32KB, too small for image uploads)
            .app_data(
                web::JsonConfig::default()
                    .limit(15_728_640)
                    .error_handler(|err, req| {
                        let detail = format!("JSON payload error on {}: {}", req.path(), err);
                        log::error!("{}", detail);
                        let response = HttpResponse::BadRequest().json(serde_json::json!({
                            "success": false,
                            "error": "Request body error",
                            "message": detail
                        }));
                        actix_web::error::InternalError::from_response(err, response).into()
                    })
            )
            // Set payload limit to 20 MiB (default is 256KB, too small for image uploads)
            .app_data(web::PayloadConfig::new(20 * 1024 * 1024))
            // Security headers
            .wrap(DefaultHeaders::new()
                .add(("X-Frame-Options", "DENY"))
                .add(("X-Content-Type-Options", "nosniff"))
                .add(("X-XSS-Protection", "1; mode=block"))
                .add(("Referrer-Policy", "strict-origin-when-cross-origin"))
                .add(("Permissions-Policy", "geolocation=(), microphone=(), camera=()"))
                // CSP: Allow external resources for fonts (Google Fonts, Font Awesome) and blob URLs for image processing
                // Updated to fix CSP violations for Font Awesome CDN and blob URL image uploads
                .add(("Content-Security-Policy", 
                      "default-src 'self'; \
                       script-src 'self' 'unsafe-inline' 'unsafe-eval' https://use.fontawesome.com https://cdnjs.cloudflare.com; \
                       style-src 'self' 'unsafe-inline' https://fonts.googleapis.com https://use.fontawesome.com https://cdnjs.cloudflare.com; \
                       img-src 'self' data: blob: https:; \
                       font-src 'self' https://fonts.gstatic.com https://use.fontawesome.com https://cdnjs.cloudflare.com data:; \
                       connect-src 'self' https://fonts.googleapis.com https://fonts.gstatic.com https://cdnjs.cloudflare.com; \
                       frame-ancestors 'none'")))
            .wrap(cors)
            .wrap(Logger::default())
            // API routes - apply rate limiting ONLY to API endpoints, not static assets
            // This prevents rate limiting from affecting frontend assets, logos, health checks, etc.
            .service(
                rate_limits.protect(api::init_routes()) // Rate limit scoped to /api/* routes only
            )
            .route("/health", web::get().to(health))
            // Serve static assets (js, css, images, etc.)
            // Versioned assets with content hashes can be cached indefinitely
            .service(
                fs::Files::new("/assets", "static/assets")
                    .use_last_modified(true)
                    .use_etag(true)
            )
            // Serve uploaded images with caching
            .service(
                fs::Files::new("/uploads/img", "uploads/img")
                    .use_last_modified(true)
                    .use_etag(true)
            )
            // Serve PWA icon files from icons directory with caching
            .service(
                fs::Files::new("/icons", "static/icons")
                    .use_last_modified(true)
                    .use_etag(true)
            )
            // Root route - serve index.html with no-cache to ensure updates are detected
            .route("/", web::get().to(|| async {
                fs::NamedFile::open_async("static/index.html")
                    .await
                    .map(|file| {
                        file.customize()
                            .insert_header(("Cache-Control", "no-cache, must-revalidate"))
                    })
            }))
            // Logo files at root level - cache for 24 hours
            .route("/logo_icon.png", web::get().to(|| async {
                fs::NamedFile::open_async("static/logo_icon.png")
                    .await
                    .map(|file| {
                        file.customize()
                            .insert_header(("Cache-Control", "public, max-age=86400"))
                    })
            }))
            .route("/logo_full.png", web::get().to(|| async {
                fs::NamedFile::open_async("static/logo_full.png")
                    .await
                    .map(|file| {
                        file.customize()
                            .insert_header(("Cache-Control", "public, max-age=86400"))
                    })
            }))
            .route("/logo_full2.png", web::get().to(|| async {
                fs::NamedFile::open_async("static/logo_full2.png")
                    .await
                    .map(|file| {
                        file.customize()
                            .insert_header(("Cache-Control", "public, max-age=86400"))
                    })
            }))
            .route("/logo_full3.png", web::get().to(|| async {
                fs::NamedFile::open_async("static/logo_full3.png")
                    .await
                    .map(|file| {
                        file.customize()
                            .insert_header(("Cache-Control", "public, max-age=86400"))
                    })
            }))
            .route("/logo_icon3.png", web::get().to(|| async {
                fs::NamedFile::open_async("static/logo_icon3.png")
                    .await
                    .map(|file| {
                        file.customize()
                            .insert_header(("Cache-Control", "public, max-age=86400"))
                    })
            }))
            .route("/favicon.ico", web::get().to(|| async {
                fs::NamedFile::open_async("static/icons/icon-32.png")
                    .await
                    .map(|file| {
                        file.customize()
                            .insert_header(("Cache-Control", "public, max-age=86400"))
                    })
            }))
            .route("/favicon.png", web::get().to(|| async {
                fs::NamedFile::open_async("static/icons/icon-32.png")
                    .await
                    .map(|file| {
                        file.customize()
                            .insert_header(("Cache-Control", "public, max-age=86400"))
                    })
            }))
            // PWA Manifest (backwards compatibility route for manifest.json)
            // Both routes serve the same file with consistent 10-minute cache
            .route("/manifest.json", web::get().to(|| async {
                fs::NamedFile::open_async("static/manifest.webmanifest")
                    .await
                    .map(|file| {
                        file.customize()
                            .insert_header(("Cache-Control", "public, max-age=600, must-revalidate"))
                    })
            }))
            // Service Worker files for PWA - MUST have no-cache for SW update mechanism
            .route("/sw.js", web::get().to(|| async {
                fs::NamedFile::open_async("static/sw.js")
                    .await
                    .map(|file| {
                        file.customize()
                            .insert_header(("Cache-Control", "no-cache, max-age=0, must-revalidate"))
                    })
            }))
            // Workbox runtime - hash-based filename, safe to cache forever
            .route("/workbox-{filename:.*}.js", web::get().to(|path: web::Path<String>| async move {
                let filename = path.into_inner();
                fs::NamedFile::open_async(format!("static/workbox-{filename}.js"))
                    .await
                    .map(|file| {
                        file.customize()
                            .insert_header(("Cache-Control", "public, max-age=31536000, immutable"))
                    })
            }))
            // PWA Manifest - update every 10 minutes (app name/icons)
            .route("/manifest.webmanifest", web::get().to(|| async {
                fs::NamedFile::open_async("static/manifest.webmanifest")
                    .await
                    .map(|file| {
                        file.customize()
                            .insert_header(("Cache-Control", "public, max-age=600, must-revalidate"))
                    })
            }))
            // Catch-all for SPA client-side routing - serve index.html for everything else
            // This comes last so API and static routes are handled first
            .route("/{path:.*}", web::get().to(spa_fallback))
    })
    .bind(format!("{host}:{port}"))?
    .run()
    .await
}
