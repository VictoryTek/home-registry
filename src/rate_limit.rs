//! Request rate limiting for the API scope.
//!
//! Three fixed-window limiters share one backend per kind across all workers (the backends are
//! built once and cloned, so every worker counts into the same buckets):
//! - burst: at most `burst` requests per client in any 1-second window,
//! - sustained: at most `rps * 60` requests per client in any 60-second window,
//! - auth: at most `auth_per_minute` requests per client to the unauthenticated credential
//!   endpoints in any 60-second window (a flood guard; the per-account lockout in
//!   [`crate::auth::throttle`] is the brute-force defence).
//!
//! Clients are keyed by IP: the TCP peer, or the first `Forwarded`/`X-Forwarded-For` address
//! when `TRUST_PROXY_HEADERS` is on. IPv6 clients are keyed by their /64 prefix.

use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;

use actix_extensible_rate_limit::{
    backend::{memory::InMemoryBackend, SimpleInput},
    RateLimiter,
};
use actix_web::{dev::HttpServiceFactory, dev::ServiceRequest, HttpRequest, Scope};

const DEFAULT_RPS: u64 = 50;
const DEFAULT_BURST: u64 = 100;
const DEFAULT_AUTH_PER_MINUTE: u64 = 20;
const MINUTE: Duration = Duration::from_secs(60);
/// Minimum gap between "looks like a proxy misconfiguration" warnings.
const PROXY_WARN_EVERY_SECS: i64 = 600;

/// Unauthenticated endpoints that run password hashing or verify credentials.
const AUTH_LIMITED_PATHS: [&str; 6] = [
    "/api/auth/login",
    "/api/auth/register",
    "/api/auth/setup",
    "/api/auth/recovery-codes/use",
    "/api/auth/totp/verify",
    "/api/auth/totp/recover",
];

/// Effective rate-limit configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitSettings {
    /// Sustained requests per second per client (enforced over a 60-second window).
    pub rps: u64,
    /// Requests allowed per client within any 1-second window.
    pub burst: u64,
    /// Requests per minute per client to the credential endpoints.
    pub auth_per_minute: u64,
    /// Whether `Forwarded`/`X-Forwarded-For` may be used to identify the client.
    pub trust_proxy_headers: bool,
}

/// Parse a positive integer setting; invalid or zero values fall back to / clamp to sane ones.
fn positive(value: Option<&str>, default: u64) -> u64 {
    value
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(default)
        .max(1)
}

impl RateLimitSettings {
    /// Build settings from raw values (as read from the environment).
    #[must_use]
    pub fn from_values(
        rps: Option<&str>,
        burst: Option<&str>,
        auth_per_minute: Option<&str>,
        trust_proxy_headers: Option<&str>,
    ) -> Self {
        Self {
            rps: positive(rps, DEFAULT_RPS),
            burst: positive(burst, DEFAULT_BURST),
            auth_per_minute: positive(auth_per_minute, DEFAULT_AUTH_PER_MINUTE),
            trust_proxy_headers: trust_proxy_headers
                .is_some_and(|v| matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes")),
        }
    }

    /// Read `RATE_LIMIT_RPS`, `RATE_LIMIT_BURST`, `AUTH_RATE_LIMIT_PER_MINUTE` and
    /// `TRUST_PROXY_HEADERS`.
    #[must_use]
    pub fn from_env() -> Self {
        let var = |name| std::env::var(name).ok();
        Self::from_values(
            var("RATE_LIMIT_RPS").as_deref(),
            var("RATE_LIMIT_BURST").as_deref(),
            var("AUTH_RATE_LIMIT_PER_MINUTE").as_deref(),
            var("TRUST_PROXY_HEADERS").as_deref(),
        )
    }

    /// Log the effective settings and the proxy-header reminder.
    pub fn log_startup(&self) {
        log::info!(
            "Rate limiting: burst {} req/s and sustained {} req/s (60 s window) per client; \
             credential endpoints {} req/min per client; trust_proxy_headers={}",
            self.burst,
            self.rps,
            self.auth_per_minute,
            self.trust_proxy_headers
        );
        if self.trust_proxy_headers {
            log::warn!(
                "TRUST_PROXY_HEADERS is enabled: client IPs are taken from Forwarded / \
                 X-Forwarded-For. The app must only be reachable through your trusted reverse \
                 proxy, otherwise clients can spoof their address and evade rate limits."
            );
        }
    }
}

static TRUST_PROXY_HEADERS: AtomicBool = AtomicBool::new(false);
static LAST_PROXY_WARNING: AtomicI64 = AtomicI64::new(0);

/// Set whether forwarded headers identify the client (done once at startup).
pub fn set_trust_proxy_headers(trust: bool) {
    TRUST_PROXY_HEADERS.store(trust, Ordering::Relaxed);
}

/// Key for one client address: IPv4 as is, IPv6 as its /64 prefix, IPv4-mapped IPv6 as IPv4.
#[must_use]
pub fn ip_key(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                v4.to_string()
            } else {
                let s = v6.segments();
                format!("{:x}:{:x}:{:x}:{:x}::/64", s[0], s[1], s[2], s[3])
            }
        },
    }
}

/// Parse an address as found in forwarding headers: `ip`, `ip:port` or `[ip]:port`.
fn parse_forwarded_address(raw: &str) -> Option<IpAddr> {
    let raw = raw.trim();
    raw.parse::<IpAddr>()
        .ok()
        .or_else(|| raw.parse::<SocketAddr>().ok().map(|a| a.ip()))
}

/// Decide the client key from the TCP peer and the (optional) forwarded address.
///
/// The forwarded address is only used when `trust_proxy_headers` is set and it parses; anything
/// else falls back to the peer so a malformed header cannot dodge the limit.
#[must_use]
pub fn client_key(
    peer: Option<IpAddr>,
    forwarded: Option<&str>,
    trust_proxy_headers: bool,
) -> String {
    let chosen = if trust_proxy_headers {
        forwarded.and_then(parse_forwarded_address).or(peer)
    } else {
        peer
    };
    chosen.map_or_else(|| "unknown".to_string(), ip_key)
}

/// Warn (rarely) when forwarding headers arrive while they are being ignored: behind a reverse
/// proxy this makes every client share the proxy's address and one rate-limit bucket.
fn warn_if_proxy_misconfigured(has_forwarded_header: bool) {
    if !has_forwarded_header {
        return;
    }
    let now = chrono::Utc::now().timestamp();
    let last = LAST_PROXY_WARNING.load(Ordering::Relaxed);
    if now - last >= PROXY_WARN_EVERY_SECS
        && LAST_PROXY_WARNING
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    {
        log::warn!(
            "Requests carry Forwarded/X-Forwarded-For headers but TRUST_PROXY_HEADERS is off. \
             If the app runs behind a reverse proxy this is a misconfiguration: all clients \
             share the proxy's address and therefore one rate-limit bucket. Set \
             TRUST_PROXY_HEADERS=true only if the app is reachable solely through that proxy."
        );
    }
}

fn key_from_parts(
    peer: Option<IpAddr>,
    forwarded: Option<&str>,
    has_forwarded_header: bool,
) -> String {
    let trust = TRUST_PROXY_HEADERS.load(Ordering::Relaxed);
    if !trust {
        warn_if_proxy_misconfigured(has_forwarded_header);
    }
    client_key(peer, forwarded, trust)
}

/// Client key for a request inside a handler.
#[must_use]
pub fn client_ip_key(req: &HttpRequest) -> String {
    let has_forwarded =
        req.headers().contains_key("forwarded") || req.headers().contains_key("x-forwarded-for");
    let info = req.connection_info();
    key_from_parts(
        req.peer_addr().map(|a| a.ip()),
        info.realip_remote_addr(),
        has_forwarded,
    )
}

/// Client key for a request inside the rate-limit middleware.
fn service_request_key(req: &ServiceRequest) -> String {
    let has_forwarded =
        req.headers().contains_key("forwarded") || req.headers().contains_key("x-forwarded-for");
    let info = req.connection_info();
    key_from_parts(
        req.peer_addr().map(|a| a.ip()),
        info.realip_remote_addr(),
        has_forwarded,
    )
}

/// Decode `%XX` escapes the way routing does, so `/api/auth/%6Cogin` is recognised as login.
/// Invalid escapes are kept literally; invalid UTF-8 is replaced (it cannot match a route).
fn percent_decode_path(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok());
            if let Some(byte) = hex {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Whether `path` is one of the credential endpoints covered by the auth limiter.
/// The raw request path is percent-decoded first because routing decodes it before matching.
#[must_use]
pub fn is_auth_limited_path(path: &str) -> bool {
    let decoded = percent_decode_path(path);
    let path = decoded.strip_suffix('/').unwrap_or(&decoded);
    AUTH_LIMITED_PATHS.contains(&path)
}

fn input(interval: Duration, max_requests: u64, key: String) -> SimpleInput {
    SimpleInput {
        interval,
        max_requests,
        key,
    }
}

/// The shared limiter state; clone it freely, clones count into the same buckets.
#[derive(Clone)]
pub struct RateLimits {
    settings: RateLimitSettings,
    burst: InMemoryBackend,
    sustained: InMemoryBackend,
    auth: InMemoryBackend,
}

impl RateLimits {
    /// Create the backends. Must run inside the actix runtime (the backends spawn a
    /// garbage-collection task).
    #[must_use]
    pub fn new(settings: RateLimitSettings) -> Self {
        set_trust_proxy_headers(settings.trust_proxy_headers);
        Self {
            settings,
            burst: InMemoryBackend::builder().build(),
            sustained: InMemoryBackend::builder().build(),
            auth: InMemoryBackend::builder().build(),
        }
    }

    /// Wrap `scope` with the burst, sustained and credential-endpoint limiters.
    #[must_use]
    pub fn protect(&self, scope: Scope) -> impl HttpServiceFactory + 'static {
        let RateLimitSettings {
            rps,
            burst,
            auth_per_minute,
            ..
        } = self.settings;

        let burst_limiter =
            RateLimiter::builder(self.burst.clone(), move |req: &ServiceRequest| {
                let key = format!("burst:{}", service_request_key(req));
                std::future::ready(Ok(input(Duration::from_secs(1), burst, key)))
            })
            .add_headers()
            .build();

        let sustained_limiter =
            RateLimiter::builder(self.sustained.clone(), move |req: &ServiceRequest| {
                let key = format!("sustained:{}", service_request_key(req));
                std::future::ready(Ok(input(MINUTE, rps.saturating_mul(60), key)))
            })
            .build();

        let auth_limiter = RateLimiter::builder(self.auth.clone(), move |req: &ServiceRequest| {
            let input = if is_auth_limited_path(req.path()) {
                input(
                    MINUTE,
                    auth_per_minute,
                    format!("auth:{}", service_request_key(req)),
                )
            } else {
                // Not a credential endpoint: effectively unlimited under a shared dummy key
                input(MINUTE, u64::MAX, "auth:skip".to_string())
            };
            std::future::ready(Ok(input))
        })
        .build();

        scope
            .wrap(auth_limiter)
            .wrap(sustained_limiter)
            .wrap(burst_limiter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_and_invalid_values_are_clamped_not_fatal() {
        let s = RateLimitSettings::from_values(Some("0"), Some("0"), Some("0"), None);
        assert_eq!((s.rps, s.burst, s.auth_per_minute), (1, 1, 1));
        let s = RateLimitSettings::from_values(Some("abc"), Some(""), Some("-5"), None);
        assert_eq!(
            (s.rps, s.burst, s.auth_per_minute),
            (DEFAULT_RPS, DEFAULT_BURST, DEFAULT_AUTH_PER_MINUTE)
        );
        let s = RateLimitSettings::from_values(Some("75"), Some("150"), Some("30"), None);
        assert_eq!((s.rps, s.burst, s.auth_per_minute), (75, 150, 30));
    }

    #[test]
    fn defaults_apply_when_unset() {
        let s = RateLimitSettings::from_values(None, None, None, None);
        assert_eq!(
            s,
            RateLimitSettings {
                rps: 50,
                burst: 100,
                auth_per_minute: 20,
                trust_proxy_headers: false
            }
        );
    }

    #[test]
    fn trust_proxy_headers_is_opt_in() {
        for on in ["true", "TRUE", "1", "yes", " true "] {
            assert!(RateLimitSettings::from_values(None, None, None, Some(on)).trust_proxy_headers);
        }
        for off in ["false", "0", "no", "", "maybe"] {
            assert!(
                !RateLimitSettings::from_values(None, None, None, Some(off)).trust_proxy_headers
            );
        }
    }

    #[test]
    fn ipv6_is_keyed_by_its_64_prefix() {
        let a: IpAddr = "2001:db8:1:2:aaaa:bbbb:cccc:dddd".parse().unwrap();
        let b: IpAddr = "2001:db8:1:2:1111:2222:3333:4444".parse().unwrap();
        let other: IpAddr = "2001:db8:1:3::1".parse().unwrap();
        assert_eq!(ip_key(a), ip_key(b));
        assert_ne!(ip_key(a), ip_key(other));
        assert_eq!(ip_key(a), "2001:db8:1:2::/64");
    }

    #[test]
    fn ipv4_and_ipv4_mapped_ipv6_share_a_key() {
        let v4: IpAddr = "203.0.113.7".parse().unwrap();
        let mapped: IpAddr = "::ffff:203.0.113.7".parse().unwrap();
        assert_eq!(ip_key(v4), "203.0.113.7");
        assert_eq!(ip_key(mapped), "203.0.113.7");
    }

    #[test]
    fn forwarded_address_is_ignored_unless_trusted() {
        let peer: IpAddr = "10.0.0.1".parse().unwrap();
        assert_eq!(
            client_key(Some(peer), Some("198.51.100.9"), false),
            "10.0.0.1"
        );
        assert_eq!(
            client_key(Some(peer), Some("198.51.100.9"), true),
            "198.51.100.9"
        );
    }

    #[test]
    fn forwarded_address_forms_are_parsed_and_garbage_falls_back_to_peer() {
        let peer: IpAddr = "10.0.0.1".parse().unwrap();
        assert_eq!(
            client_key(Some(peer), Some("198.51.100.9:4711"), true),
            "198.51.100.9"
        );
        assert_eq!(
            client_key(Some(peer), Some("[2001:db8::1]:4711"), true),
            "2001:db8:0:0::/64"
        );
        assert_eq!(client_key(Some(peer), Some("not-an-ip"), true), "10.0.0.1");
        assert_eq!(client_key(None, None, true), "unknown");
    }

    #[test]
    fn only_credential_endpoints_are_auth_limited() {
        for p in [
            "/api/auth/login",
            "/api/auth/register",
            "/api/auth/setup",
            "/api/auth/recovery-codes/use",
            "/api/auth/totp/verify",
            "/api/auth/totp/recover",
            "/api/auth/login/",
            "/api/auth/%6Cogin",
            "/api/auth/log%69n",
            "/api/%61uth/register",
            "/api/auth/recovery-codes/%75se",
        ] {
            assert!(is_auth_limited_path(p), "{p}");
        }
        for p in [
            "/api/auth/setup/status",
            "/api/auth/profile",
            "/api/inventories",
            "/api/auth/recovery-codes/generate",
            "/api/auth/log%zzin",
            "/health",
        ] {
            assert!(!is_auth_limited_path(p), "{p}");
        }
    }
}
