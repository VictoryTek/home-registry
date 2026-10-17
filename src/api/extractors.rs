//! Request-extractor configuration for the API scope.
//!
//! One place sets the body limits and turns every extractor failure (malformed JSON, wrong
//! types, bad query string, unparsable path id) into the standard error envelope
//! `{success: false, error, message}` that all handlers use. actix resolves `app_data` from the
//! innermost scope outward, so this must be registered on the `/api` scope itself: registering
//! it on the `App` is shadowed by any scope-level config (which is how the earlier handler in
//! `main.rs` ended up never running).
//!
//! Client-facing messages never contain what the client sent. serde's "invalid type" messages
//! quote the received value, so those are rewritten to name only the kind of value; everything
//! that is not a recognised, value-free shape becomes a fixed generic sentence. The log line
//! records the method, the route pattern (never the query string or path values) and an error
//! category, never the body or serde's message.

use actix_web::error::{InternalError, JsonPayloadError, PathError, QueryPayloadError};
use actix_web::http::StatusCode;
use actix_web::{web, HttpRequest, HttpResponse, ResponseError};
use log::warn;

use crate::models::ErrorResponse;

/// Largest JSON body accepted (inventory cover images still arrive as base64; see M-19).
pub const JSON_LIMIT: usize = 15 * 1024 * 1024;
/// Largest request payload accepted (multipart image and backup uploads).
pub const PAYLOAD_LIMIT: usize = 20 * 1024 * 1024;
/// Longest field-level message sent to a client.
const MAX_MESSAGE_CHARS: usize = 200;

/// Generic client messages for formats that are not on the allowlist.
const GENERIC_JSON: &str = "The request body does not match the expected format";
const INVALID_JSON: &str = "The request body is not valid JSON";
const GENERIC_QUERY: &str = "Invalid query parameters";
const GENERIC_PATH: &str = "Invalid path parameter";

/// Kinds of received value that serde names in `invalid type:` / `invalid value:` messages
/// (`serde::de::Unexpected`, plus `serde_json`'s `null`).
const RECEIVED_KINDS: [&str; 17] = [
    "boolean",
    "integer",
    "floating point",
    "character",
    "string",
    "byte array",
    "null",
    "unit value",
    "Option value",
    "newtype struct",
    "sequence",
    "map",
    "enum",
    "unit variant",
    "newtype variant",
    "tuple variant",
    "struct variant",
];

/// Messages produced by the standard library's `FromStr` impls when a query value cannot be
/// parsed (`serde_urlencoded` passes them through as custom errors). None quotes the input.
const PARSE_ERRORS: [&str; 10] = [
    "cannot parse integer from empty string",
    "invalid digit found in string",
    "number too large to fit in target type",
    "number too small to fit in target type",
    "number would be zero for non-zero type",
    "cannot parse float from empty string",
    "invalid float literal",
    "provided string was not `true` or `false`",
    "cannot parse char from empty string",
    "too many characters in string",
];

/// Primitive types named in actix's `can not parse "..." to a <type>` path error.
const PATH_TYPES: [&str; 14] = [
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32",
    "f64",
];

/// The standard error envelope as a response.
fn envelope(status: StatusCode, error: &str, message: &str) -> HttpResponse {
    HttpResponse::build(status).json(ErrorResponse {
        success: false,
        error: error.to_string(),
        message: Some(message.to_string()),
    })
}

/// A field, variant or type name as it appears in a contract: letters, digits, `_` and `-`.
fn is_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// The sanitized form of a serde or `serde_urlencoded` error message, or `None` if its format is
/// not one of those listed below. The caller then sends a generic sentence, so a format that is
/// new or custom can never carry client input through. Mapped formats (`serde` 1.x derive,
/// `serde_json` 1.x, `serde_urlencoded` 0.7):
///
/// - ``missing field `x` `` and ``duplicate field `x` `` are kept: the name is the struct's own.
/// - ``unknown field `x`, expected ...`` and ``unknown variant `x`, expected ...`` lose the
///   name the client sent and keep the list of valid names.
/// - ``invalid type: <kind> <value>, expected <what>`` and ``invalid value: ...`` lose the
///   received value.
/// - ``invalid length N, expected <what>`` is kept.
/// - ``data did not match any variant of untagged enum X`` loses the type name.
/// - The standard library's integer, float, bool and char parse errors are kept.
#[must_use]
pub fn sanitize_serde_message(raw: &str) -> Option<String> {
    let message = strip_position(raw);

    let sanitized = if let Some(name) = quoted_name(message, "missing field `") {
        format!("missing field `{name}`")
    } else if let Some(name) = quoted_name(message, "duplicate field `") {
        format!("duplicate field `{name}`")
    } else if let Some(rest) = message.strip_prefix("unknown field `") {
        format!("unknown field{}", valid_names_tail(rest)?)
    } else if let Some(rest) = message.strip_prefix("unknown variant `") {
        format!("unknown variant{}", valid_names_tail(rest)?)
    } else if let Some(rest) = message.strip_prefix("invalid type: ") {
        format!("invalid type: {}", without_received_value(rest)?)
    } else if let Some(rest) = message.strip_prefix("invalid value: ") {
        format!("invalid value: {}", without_received_value(rest)?)
    } else if let Some(rest) = message.strip_prefix("invalid length ") {
        let (count, expected) = rest.split_once(", expected ")?;
        if count.is_empty() || !count.bytes().all(|b| b.is_ascii_digit()) || count.len() > 9 {
            return None;
        }
        format!(
            "invalid length {count}, expected {}",
            expected_text(expected)?
        )
    } else if message.starts_with("data did not match any variant") {
        "data did not match any variant".to_string()
    } else if PARSE_ERRORS.contains(&message) {
        message.to_string()
    } else {
        return None;
    };

    if sanitized.chars().count() > MAX_MESSAGE_CHARS {
        let cut: String = sanitized.chars().take(MAX_MESSAGE_CHARS).collect();
        return Some(format!("{cut}..."));
    }
    Some(sanitized)
}

/// The identifier in a message that is exactly the prefix, the identifier and a closing backtick.
fn quoted_name<'a>(message: &'a str, prefix: &str) -> Option<&'a str> {
    message
        .strip_prefix(prefix)?
        .strip_suffix('`')
        .filter(|name| is_identifier(name))
}

/// For the text after "unknown field `" / "unknown variant `": the trailing list of valid
/// names (", expected one of `a`, `b`", ", expected `a`" or ", there are no fields"), without
/// the client's name in front of it.
///
/// The client's name is arbitrary text and may itself contain ", expected ...", so the tail is
/// the earliest suffix that is made only of that grammar and identifiers. Text the client put
/// before the real list cannot satisfy it: the real list is always preceded by a closing
/// backtick, which is not a valid separator between items.
fn valid_names_tail(after_prefix: &str) -> Option<String> {
    after_prefix
        .match_indices("`, ")
        .map(|(i, _)| &after_prefix[i + 1..])
        .find(|tail| is_valid_names_list(tail))
        .map(str::to_string)
}

fn is_valid_names_list(tail: &str) -> bool {
    if tail == ", there are no fields" || tail == ", there are no variants" {
        return true;
    }
    let Some(list) = tail.strip_prefix(", expected ") else {
        return false;
    };
    let list = list.strip_prefix("one of ").unwrap_or(list);
    list.replace(" or ", ", ").split(", ").all(|item| {
        item.strip_prefix('`')
            .and_then(|i| i.strip_suffix('`'))
            .is_some_and(is_identifier)
    })
}

/// Text after `invalid type: ` / `invalid value: `, with the received value removed:
/// `string "abc", expected i32` becomes `string, expected i32`. `None` if the received kind is
/// not a known one, the literal is not followed by ", expected ", or the expectation is not a
/// plain phrase.
fn without_received_value(rest: &str) -> Option<String> {
    // "unit value" must win over "unit variant", so take the longest matching kind
    let kind = RECEIVED_KINDS
        .iter()
        .filter(|k| rest.starts_with(**k))
        .max_by_key(|k| k.len())?;
    let after_kind = &rest[kind.len()..];

    let after_literal = match after_kind.chars().next()? {
        ',' => after_kind,
        ' ' => {
            let literal = &after_kind[1..];
            if let Some(after_tick) = literal.strip_prefix('`') {
                &after_tick[after_tick.find('`')? + 1..]
            } else {
                skip_debug_string(literal)?
            }
        },
        _ => return None,
    };
    let expected = after_literal.strip_prefix(", expected ")?;
    Some(format!("{kind}, expected {}", expected_text(expected)?))
}

/// The remainder after a `Debug`-quoted string at the start of `s`, skipping backslash escapes.
fn skip_debug_string(s: &str) -> Option<&str> {
    s.strip_prefix('"')?;
    let mut escaped = false;
    for (i, c) in s.char_indices().skip(1) {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            return Some(&s[i + 1..]);
        }
    }
    None
}

/// What serde says it expected: a short static phrase such as `i32`, `a string` or `a tuple of
/// size 2`. A struct's own name is replaced, because it is an implementation detail.
fn expected_text(expected: &str) -> Option<&str> {
    if expected.starts_with("struct ") || expected.starts_with("tuple struct ") {
        return Some("an object");
    }
    let plain = !expected.is_empty()
        && expected.len() <= 60
        && expected
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b' ' || b == b'_');
    plain.then_some(expected)
}

/// Drop `serde_json`'s trailing " at line N column M".
fn strip_position(message: &str) -> &str {
    if let Some(index) = message.rfind(" at line ") {
        let tail = &message[index + " at line ".len()..];
        let mut parts = tail.split(" column ");
        let (line, column) = (parts.next(), parts.next());
        let digits = |s: Option<&str>| {
            s.is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        };
        if digits(line) && digits(column) && parts.next().is_none() {
            return &message[..index];
        }
    }
    message
}

/// Sanitize actix's path error, `can not parse "<segment>" to a <type>`: the segment is
/// removed. `None` for any other format.
#[must_use]
pub fn sanitize_path_message(raw: &str) -> Option<String> {
    let after_literal = skip_debug_string(raw.strip_prefix("can not parse ")?)?;
    let ty = after_literal.strip_prefix(" to a ")?;
    PATH_TYPES
        .contains(&ty)
        .then(|| format!("The path parameter must be a valid {ty}"))
}

/// What to tell the client about a JSON extraction failure: status, `error`, `message`, and a
/// category for the log.
#[must_use]
pub fn describe_json_error(
    err: &JsonPayloadError,
) -> (StatusCode, &'static str, String, &'static str) {
    const BAD_BODY: &str = "Invalid request body";
    match err {
        JsonPayloadError::OverflowKnownLength { .. } | JsonPayloadError::Overflow { .. } => (
            StatusCode::PAYLOAD_TOO_LARGE,
            "Request body too large",
            format!(
                "The request body may not be larger than {} MiB",
                JSON_LIMIT / (1024 * 1024)
            ),
            "json_too_large",
        ),
        JsonPayloadError::ContentType => (
            StatusCode::BAD_REQUEST,
            BAD_BODY,
            "The Content-Type must be application/json".to_string(),
            "json_content_type",
        ),
        JsonPayloadError::Deserialize(e) if e.classify() == serde_json::error::Category::Data => {
            match sanitize_serde_message(&e.to_string()) {
                Some(message) => (StatusCode::BAD_REQUEST, BAD_BODY, message, "json_data"),
                None => (
                    StatusCode::BAD_REQUEST,
                    BAD_BODY,
                    GENERIC_JSON.to_string(),
                    "json_data_unrecognized",
                ),
            }
        },
        JsonPayloadError::Deserialize(_) => (
            StatusCode::BAD_REQUEST,
            BAD_BODY,
            INVALID_JSON.to_string(),
            "json_syntax",
        ),
        JsonPayloadError::Payload(e) => (
            e.status_code(),
            BAD_BODY,
            "The request body could not be read".to_string(),
            "json_payload",
        ),
        // Only produced when serialising a response, never by the extractor
        _ => (
            StatusCode::BAD_REQUEST,
            BAD_BODY,
            "The request body is not valid".to_string(),
            "json_other",
        ),
    }
}

/// Text for a log line about a `serde_json` failure on client-supplied JSON: the sanitized
/// message for a data error, a fixed phrase otherwise. Never a received value or a name the
/// client chose.
#[must_use]
pub fn loggable_json_error(e: &serde_json::Error) -> String {
    if e.classify() == serde_json::error::Category::Data {
        sanitize_serde_message(&e.to_string()).unwrap_or_else(|| GENERIC_JSON.to_string())
    } else {
        INVALID_JSON.to_string()
    }
}

/// Log a rejected request without the body, the query string, serde's message or the values in
/// path parameters: the matched route pattern stands in for the path, or `<unmatched>` when
/// there is none (for example an extractor on a `default_service`).
fn log_rejection(req: &HttpRequest, category: &str) {
    // Never the concrete path: without a matched route nothing says which part of it is a secret
    let route = req
        .match_pattern()
        .unwrap_or_else(|| crate::access_log::UNMATCHED.to_string());
    let method = crate::access_log::loggable_method(req.method());
    warn!("Rejected request: {method} {route:?} ({category})");
}

/// JSON extractor: limit and error envelope.
#[must_use]
pub fn json_config() -> web::JsonConfig {
    web::JsonConfig::default()
        .limit(JSON_LIMIT)
        .error_handler(|err, req| {
            let (status, error, message, category) = describe_json_error(&err);
            log_rejection(req, category);
            let response = envelope(status, error, &message);
            InternalError::from_response(err, response).into()
        })
}

/// Query-string extractor: error envelope.
#[must_use]
pub fn query_config() -> web::QueryConfig {
    web::QueryConfig::default().error_handler(|err: QueryPayloadError, req| {
        let QueryPayloadError::Deserialize(inner) = &err else {
            log_rejection(req, "query_unrecognized");
            let response = envelope(
                StatusCode::BAD_REQUEST,
                "Invalid query parameters",
                GENERIC_QUERY,
            );
            return InternalError::from_response(err, response).into();
        };
        let (message, category) = match sanitize_serde_message(&inner.to_string()) {
            Some(message) => (message, "query"),
            None => (GENERIC_QUERY.to_string(), "query_unrecognized"),
        };
        log_rejection(req, category);
        let response = envelope(
            StatusCode::BAD_REQUEST,
            "Invalid query parameters",
            &message,
        );
        InternalError::from_response(err, response).into()
    })
}

/// Path extractor: error envelope. The framework's own status (404) is kept: an id that does
/// not parse is a resource that does not exist.
#[must_use]
pub fn path_config() -> web::PathConfig {
    web::PathConfig::default().error_handler(|err: PathError, req| {
        let (message, category) = match &err {
            PathError::Deserialize(inner) => match sanitize_path_message(&inner.to_string()) {
                Some(message) => (message, "path"),
                None => (GENERIC_PATH.to_string(), "path_unrecognized"),
            },
            _ => (GENERIC_PATH.to_string(), "path_unrecognized"),
        };
        log_rejection(req, category);
        let response = envelope(StatusCode::NOT_FOUND, "Invalid path parameter", &message);
        InternalError::from_response(err, response).into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SENTINEL: &str = "SENTINEL-2b8e";

    fn clean(raw: &str) -> Option<String> {
        sanitize_serde_message(raw)
    }

    #[test]
    fn contract_names_are_kept_and_the_position_is_dropped() {
        assert_eq!(
            clean("missing field `inventory_id` at line 1 column 17").unwrap(),
            "missing field `inventory_id`"
        );
        assert_eq!(
            clean("duplicate field `name` at line 1 column 30").unwrap(),
            "duplicate field `name`"
        );
        assert_eq!(
            clean("invalid length 3, expected a tuple of size 2").unwrap(),
            "invalid length 3, expected a tuple of size 2"
        );
        assert_eq!(
            clean("data did not match any variant of untagged enum Secret").unwrap(),
            "data did not match any variant"
        );
    }

    #[test]
    fn the_name_the_client_sent_is_dropped_from_unknown_field_and_variant() {
        for (raw, want) in [
            (
                format!("unknown field `{SENTINEL}`, expected one of `a`, `b` at line 3 column 9"),
                "unknown field, expected one of `a`, `b`",
            ),
            (
                format!("unknown field `{SENTINEL}`, expected `a`"),
                "unknown field, expected `a`",
            ),
            (
                format!("unknown field `{SENTINEL}`, expected `a` or `b`"),
                "unknown field, expected `a` or `b`",
            ),
            (
                format!("unknown field `{SENTINEL}`, there are no fields"),
                "unknown field, there are no fields",
            ),
            (
                format!(
                    "unknown variant `{SENTINEL}`, expected one of `view`, `edit_items`, `edit_inventory` at line 1 column 40"
                ),
                "unknown variant, expected one of `view`, `edit_items`, `edit_inventory`",
            ),
            (
                format!("unknown variant `{SENTINEL}`, there are no variants"),
                "unknown variant, there are no variants",
            ),
        ] {
            let out = clean(&raw).unwrap_or_else(|| panic!("not recognised: {raw}"));
            assert_eq!(out, want, "{raw}");
            assert!(!out.contains(SENTINEL), "{out}");
        }
    }

    #[test]
    fn a_name_crafted_to_look_like_the_valid_list_cannot_get_text_through() {
        for name in [
            format!("x`, expected `{SENTINEL}"),
            format!("x`, expected one of `{SENTINEL}`, `"),
            format!("x`, expected `{SENTINEL}` or `"),
            format!("{SENTINEL}`, there are no fields"),
            format!("`, expected `{SENTINEL}`, expected one of `{SENTINEL}"),
        ] {
            for tail in [
                ", expected one of `a`, `b`",
                ", expected `a`",
                ", expected `a` or `b`",
                ", there are no fields",
            ] {
                let raw = format!("unknown field `{name}`{tail}");
                // Either the real list alone, or nothing: never the client's text
                if let Some(out) = clean(&raw) {
                    assert!(!out.contains(SENTINEL), "{raw} -> {out}");
                    assert!(out.ends_with(tail), "{raw} -> {out}");
                }
            }
        }
        // Only the final list is taken, never anything before it
        assert_eq!(
            clean(&format!(
                "unknown field `{SENTINEL}`, expected `x`, expected `y`"
            ))
            .as_deref(),
            Some("unknown field, expected `y`")
        );
    }

    #[test]
    fn received_values_are_removed_from_type_and_value_errors() {
        for (raw, want) in [
            (
                format!(r#"invalid type: string "{SENTINEL}", expected i32 at line 1 column 9"#),
                "invalid type: string, expected i32",
            ),
            (
                "invalid type: integer `5`, expected a string at line 1 column 3".to_string(),
                "invalid type: integer, expected a string",
            ),
            (
                "invalid type: floating point `1.5`, expected i32 at line 1 column 5".to_string(),
                "invalid type: floating point, expected i32",
            ),
            (
                "invalid type: boolean `true`, expected i32".to_string(),
                "invalid type: boolean, expected i32",
            ),
            (
                "invalid type: null, expected i32 at line 1 column 4".to_string(),
                "invalid type: null, expected i32",
            ),
            (
                "invalid type: map, expected i32".to_string(),
                "invalid type: map, expected i32",
            ),
            (
                "invalid type: unit variant, expected newtype variant".to_string(),
                "invalid type: unit variant, expected newtype variant",
            ),
            (
                "invalid value: integer `99999999999`, expected i32".to_string(),
                "invalid value: integer, expected i32",
            ),
            (
                "invalid type: character `a`, expected a string".to_string(),
                "invalid type: character, expected a string",
            ),
            (
                format!(r#"invalid type: string "{SENTINEL}", expected struct CreateItemRequest"#),
                "invalid type: string, expected an object",
            ),
        ] {
            let out = clean(&raw).unwrap_or_else(|| panic!("not recognised: {raw}"));
            assert_eq!(out, want, "{raw}");
            assert!(!out.contains(SENTINEL), "{out}");
        }
    }

    #[test]
    fn tricky_string_literals_do_not_leak_or_confuse_the_parser() {
        // The literal contains the separator, an escaped quote and a backtick
        let raw = r#"invalid type: string "a, expected b \" `x` secret", expected i32 at line 1 column 40"#;
        let out = clean(raw).unwrap();
        assert_eq!(out, "invalid type: string, expected i32");
        assert!(!out.contains("secret"));
    }

    #[test]
    fn unrecognised_or_malformed_formats_fall_back_to_none() {
        for raw in [
            // Custom errors from a Deserialize impl may contain anything
            "password hunter2 is too weak",
            "expected value at line 1 column 1",
            "",
            // A literal that is not followed by ", expected " cannot be redacted safely
            r#"invalid type: string "oops" trailing junk"#,
            "invalid type: string `unterminated, expected i32",
            // Unknown received kind
            "invalid type: widget `w`, expected i32",
            // An expectation that is not a plain phrase
            "invalid type: map, expected one of `a`, \"b\"",
            // Names that are not identifiers
            "missing field `a b`",
            "missing field `x` and then some",
            "unknown variant `x`",
            "invalid length abc, expected i32",
            // uuid's parse error quotes the input
            "invalid character: expected an optional prefix of `urn:uuid:` followed by [0-9a-fA-F-], found `z` at 1",
        ] {
            assert_eq!(clean(raw), None, "{raw}");
        }
    }

    #[test]
    fn the_listed_parse_errors_are_the_standard_librarys_own_wording() {
        let actual = [
            "".parse::<i32>().unwrap_err().to_string(),
            "x".parse::<i32>().unwrap_err().to_string(),
            "99999999999".parse::<i32>().unwrap_err().to_string(),
            "-99999999999".parse::<i32>().unwrap_err().to_string(),
            "0".parse::<std::num::NonZeroU32>().unwrap_err().to_string(),
            "".parse::<f64>().unwrap_err().to_string(),
            "x".parse::<f64>().unwrap_err().to_string(),
            "x".parse::<bool>().unwrap_err().to_string(),
            "".parse::<char>().unwrap_err().to_string(),
            "xy".parse::<char>().unwrap_err().to_string(),
        ];
        for message in &actual {
            assert!(PARSE_ERRORS.contains(&message.as_str()), "{message}");
            assert_eq!(clean(message).as_deref(), Some(message.as_str()));
        }
        assert_eq!(actual.len(), PARSE_ERRORS.len());
    }

    #[test]
    fn path_errors_lose_the_segment() {
        assert_eq!(
            sanitize_path_message(&format!(r#"can not parse "{SENTINEL}" to a i32"#)).as_deref(),
            Some("The path parameter must be a valid i32")
        );
        // A segment that looks like the rest of the message
        assert_eq!(
            sanitize_path_message(r#"can not parse "x\" to a i32 \" to a u8" to a u64"#).as_deref(),
            Some("The path parameter must be a valid u64")
        );
        for raw in [
            "wrong number of parameters: 1 expected 2",
            r#"can not parse "x" to a Widget"#,
            r#"can not parse "x" to a i32 and more"#,
            r#"can not parse "unterminated to a i32"#,
            "invalid character: expected an optional prefix of `urn:uuid:`",
            "",
        ] {
            assert_eq!(sanitize_path_message(raw), None, "{raw}");
        }
    }

    #[test]
    fn json_errors_in_unrecognised_formats_get_the_generic_sentence() {
        let custom: serde_json::Error = serde::de::Error::custom(format!("{SENTINEL} is bad"));
        let (status, _error, message, category) =
            describe_json_error(&JsonPayloadError::Deserialize(custom));
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            message,
            "The request body does not match the expected format"
        );
        assert_eq!(category, "json_data_unrecognized");
    }

    #[test]
    fn messages_are_one_short_line() {
        let names = (0..100).map(|i| format!("`field_{i}`")).collect::<Vec<_>>();
        let long = format!("unknown field `x`, expected one of {}", names.join(", "));
        let out = clean(&long).unwrap();
        assert!(
            out.chars().count() <= MAX_MESSAGE_CHARS + 3,
            "{}",
            out.len()
        );
        assert!(out.ends_with("..."));
    }

    #[test]
    fn position_stripping_only_touches_a_real_suffix() {
        assert_eq!(strip_position("x at line 1 column 2"), "x");
        assert_eq!(
            strip_position("x at line one column 2"),
            "x at line one column 2"
        );
        assert_eq!(strip_position("no suffix"), "no suffix");
    }
}
