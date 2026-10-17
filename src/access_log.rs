//! Access log format.
//!
//! actix's default format prints the whole request line (`%r`), including the query string,
//! and the `Referer`, which can carry one too. Query strings can hold tokens or search terms,
//! so this format logs the path only and leaves the referer out. The `User-Agent` is left out for
//! the same reason: nothing the client chose is written by the HTTP-layer logs. An extension
//! method (HTTP lets a client invent them) is logged as `OTHER`. The path is the matched route
//! pattern (`/api/inventories/{id}`), not the concrete path, so that values in path parameters
//! (search terms, tokens, a mistyped id) are not written to the log either. A request that
//! matched no route has no pattern; it is logged as [`UNMATCHED`], never with its path, because
//! nothing says which part of an arbitrary URL is a secret. The reverse proxy's access log is the
//! place to look for probes of unknown URLs and for user agents.

use actix_web::http::Method;
use actix_web::middleware::Logger;

/// The methods that are logged by name; HTTP lets a client invent others.
const STANDARD_METHODS: [&str; 9] = [
    "GET", "HEAD", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "CONNECT", "TRACE",
];

/// The method as it is written to a log: its name if it is a standard one, otherwise `OTHER`,
/// since an extension method is text the client chose.
#[must_use]
pub fn loggable_method(method: &Method) -> &'static str {
    STANDARD_METHODS
        .iter()
        .copied()
        .find(|name| *name == method.as_str())
        .unwrap_or("OTHER")
}

/// What is logged in place of the route for a request that matched none.
pub const UNMATCHED: &str = "<unmatched>";

/// `peer "METHOD /route" status bytes seconds`
pub const ACCESS_LOG_FORMAT: &str = r#"%a "%{METHOD}xi %{ROUTE}xo" %s %b %T"#;

/// The access logger used by the server (and by the tests that check what it records).
#[must_use]
pub fn access_logger() -> Logger {
    Logger::new(ACCESS_LOG_FORMAT)
        .custom_request_replace("METHOD", |req| loggable_method(req.method()).to_string())
        .custom_response_replace("ROUTE", |res| {
            let req = res.request();
            req.match_pattern().unwrap_or_else(|| UNMATCHED.to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_methods_are_logged_by_name_and_others_as_other() {
        for name in STANDARD_METHODS {
            let method = Method::from_bytes(name.as_bytes()).unwrap();
            assert_eq!(loggable_method(&method), name);
        }
        for name in ["SENTINEL-1", "get", "PROPFIND", "X"] {
            let method = Method::from_bytes(name.as_bytes()).unwrap();
            assert_eq!(loggable_method(&method), "OTHER", "{name}");
        }
    }
}
