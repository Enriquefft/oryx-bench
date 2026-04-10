//! GraphQL client for the Oryx public endpoint.
//!
//! Only two queries are needed:
//!
//! 1. [`metadata_query`] — returns just `{ revision { hashId } }`. ~1KB response.
//! 2. [`full_layout_query`] — returns the full layout JSON. Larger but still small.
//!
//! No auth. The transport is hardened with:
//!
//! - Crate-versioned `User-Agent` header (set in [`crate::util::http`]) so
//!   Oryx operators can correlate server-side errors with client versions.
//! - Bounded retry with exponential backoff + jitter on transient
//!   failures (502/503/504, connect failures, request timeouts). GraphQL
//!   queries are read-only and idempotent so retrying is safe.
//! - `Retry-After` honored on 429 responses (capped so a malicious or
//!   misconfigured server can't pin the CLI for hours).
//! - Response body capped at [`crate::util::http::MAX_RESPONSE_BYTES`]
//!   to prevent OOM from a buggy or malicious server.
//!
//! All of the above lives here (not in [`crate::util::http`]) because
//! the policy is GraphQL-specific: a different caller might tolerate
//! different status codes or larger payloads.

use std::io::Read;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::error::PullError;
use crate::util::http;

/// The Oryx public GraphQL endpoint. Can be overridden in tests via
/// the `ORYX_GRAPHQL_ENDPOINT` environment variable.
pub fn endpoint() -> String {
    std::env::var("ORYX_GRAPHQL_ENDPOINT")
        .unwrap_or_else(|_| "https://oryx.zsa.io/graphql".to_string())
}

/// Maximum number of attempts (including the first) for a single POST.
/// `3` strikes a balance between resilience to a transient blip and
/// "don't make the user wait forever on a hard outage".
const MAX_ATTEMPTS: u32 = 3;

/// Base interval for the exponential-backoff retry. The actual wait
/// before retry `n` is `BACKOFF_BASE * 2^n + jitter(0..BACKOFF_BASE)`.
const BACKOFF_BASE: Duration = Duration::from_millis(200);

/// Upper bound on a server-supplied `Retry-After` value. Without this
/// cap a misbehaving server could pin the CLI for hours.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(60);

#[derive(Serialize)]
struct GqlReq<'a> {
    query: &'a str,
    variables: serde_json::Value,
}

#[derive(Deserialize)]
struct GqlResp<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct MetadataPayload {
    layout: Option<MetadataLayout>,
}

#[derive(Deserialize)]
struct MetadataLayout {
    revision: MetadataRevision,
}

#[derive(Deserialize)]
struct MetadataRevision {
    #[serde(rename = "hashId")]
    hash_id: String,
}

const METADATA_QUERY: &str = r#"
query Meta($hashId: String!, $geometry: String!, $revisionId: String!) {
  layout(hashId: $hashId, geometry: $geometry, revisionId: $revisionId) {
    revision { hashId }
  }
}
"#;

// NOTE: as of the 2026-Q2 Oryx schema, `combos` is an object type
// (`[Combo!]`) and the bare `combos` selection that worked under the
// older schema now produces a hard error: `Field must have selections
// (field 'combos' returns Combo but has no selections)`. Each subfield
// listed below is justified by an actual downstream consumer; do not
// add fields here without a use site, since unused fields are wasted
// bandwidth and a forwards-compat hazard.
const FULL_QUERY: &str = r#"
query Full($hashId: String!, $geometry: String!, $revisionId: String!) {
  layout(hashId: $hashId, geometry: $geometry, revisionId: $revisionId) {
    hashId title geometry privacy
    revision {
      hashId qmkVersion title createdAt model md5
      layers { title position keys }
      combos {
        # Matrix indices the user has to chord. Used by
        # `oryx_combo_to_canonical` (src/schema/canonical.rs) to populate
        # `CanonicalCombo.keys` after translation through
        # `Geometry::index_to_position`.
        keyIndices
        # Layer this combo is bound to (numeric layer index). Used by
        # `oryx_combo_to_canonical` to populate `CanonicalCombo.layer`
        # after resolving the index against the revision's layer list.
        layerIdx
        # The action emitted when the combo fires (a JSON object that
        # mirrors the `oryx::Action` shape used by ordinary keys). Used
        # by `oryx_combo_to_canonical` to populate `CanonicalCombo.sends`
        # via `oryx_action_to_canonical`.
        trigger
      }
      config
      swatch
    }
  }
}
"#;

/// Issue the cheap metadata-only query. Returns the remote revision hash.
pub fn metadata_query(hash_id: &str, geometry: &str, revision_id: &str) -> Result<String> {
    let req = GqlReq {
        query: METADATA_QUERY,
        variables: serde_json::json!({
            "hashId": hash_id,
            "geometry": geometry,
            "revisionId": revision_id,
        }),
    };
    let resp: GqlResp<MetadataPayload> = post(&req)?;
    if let Some(errs) = resp.errors {
        return Err(PullError::GraphQl(errs.to_string()).into());
    }
    let data = resp
        .data
        .ok_or_else(|| PullError::GraphQl("no data field in response".into()))?;
    let layout = data.layout.ok_or_else(|| PullError::LayoutNotFound {
        hash_id: hash_id.to_owned(),
    })?;
    Ok(layout.revision.hash_id)
}

/// Issue the full layout query. Returns the JSON `layout` sub-object the
/// caller can write to `pulled/revision.json` after pretty-printing.
pub fn full_layout_query(
    hash_id: &str,
    geometry: &str,
    revision_id: &str,
) -> Result<serde_json::Value> {
    let req = GqlReq {
        query: FULL_QUERY,
        variables: serde_json::json!({
            "hashId": hash_id,
            "geometry": geometry,
            "revisionId": revision_id,
        }),
    };
    let resp: GqlResp<serde_json::Value> = post(&req)?;
    if let Some(errs) = resp.errors {
        return Err(PullError::GraphQl(errs.to_string()).into());
    }
    let data = resp
        .data
        .ok_or_else(|| PullError::GraphQl("no data field in response".into()))?;
    let layout = match data.get("layout") {
        Some(v) if !v.is_null() => v.clone(),
        _ => return Err(PullError::LayoutNotFound { hash_id: hash_id.to_owned() }.into()),
    };
    Ok(layout)
}

fn post<T: for<'de> Deserialize<'de>>(req: &GqlReq) -> Result<GqlResp<T>> {
    let body = post_with_retry(req)?;
    let parsed: GqlResp<T> = serde_json::from_str(&body)
        .map_err(PullError::from)
        .context("parsing Oryx GraphQL response body as JSON")?;
    Ok(parsed)
}

/// Outcome of a single POST attempt. Hard errors (4xx other than 429,
/// 5xx other than 502/503/504, parse failures, body-too-large) are
/// returned as `Err` from [`do_post`] directly so they short-circuit
/// the retry loop.
enum PostAttempt {
    /// 2xx response, body successfully read and within the size cap.
    Success(String),
    /// 429 — caller should sleep at least this long before the next try.
    RetryAfter(Duration),
    /// 502/503/504 or transient connect/timeout error — retry with backoff.
    Retriable(anyhow::Error),
}

fn post_with_retry(req: &GqlReq) -> Result<String> {
    let client = http::client()?;
    let url = endpoint();
    let mut attempt: u32 = 0;

    loop {
        match do_post(client, &url, req)? {
            PostAttempt::Success(body) => return Ok(body),
            PostAttempt::RetryAfter(wait) => {
                attempt += 1;
                if attempt >= MAX_ATTEMPTS {
                    bail!(
                        "Oryx GraphQL returned HTTP 429 (rate limited) after {MAX_ATTEMPTS} attempts — giving up"
                    );
                }
                tracing::debug!(
                    attempt,
                    wait_ms = wait.as_millis() as u64,
                    "Oryx returned 429; honoring Retry-After before retrying"
                );
                std::thread::sleep(wait);
            }
            PostAttempt::Retriable(e) => {
                attempt += 1;
                if attempt >= MAX_ATTEMPTS {
                    return Err(e.context(format!(
                        "Oryx GraphQL POST failed after {MAX_ATTEMPTS} attempts"
                    )));
                }
                let backoff = backoff_delay(attempt - 1);
                tracing::debug!(
                    attempt,
                    backoff_ms = backoff.as_millis() as u64,
                    error = %e,
                    "retrying Oryx GraphQL after transient failure"
                );
                std::thread::sleep(backoff);
            }
        }
    }
}

fn do_post(client: &reqwest::blocking::Client, url: &str, req: &GqlReq) -> Result<PostAttempt> {
    let resp = match client.post(url).json(req).send() {
        Ok(r) => r,
        Err(e) => {
            let context = "POST to oryx.zsa.io/graphql failed";
            if is_retriable_network_error(&e) {
                return Ok(PostAttempt::Retriable(
                    anyhow::Error::new(e).context(context),
                ));
            }
            return Err(anyhow::Error::new(e).context(context));
        }
    };

    let status = resp.status();

    // 429 must be handled BEFORE we read the body so we can grab the
    // Retry-After header off the still-borrowable response.
    if status.as_u16() == 429 {
        let wait = parse_retry_after(resp.headers()).unwrap_or(BACKOFF_BASE);
        // Clamp the wait so a misbehaving server sending `Retry-After: 0`
        // can't turn the retry loop into a hot loop. BACKOFF_BASE is
        // already the floor for our own backoff path, so reuse it.
        let wait = wait.max(BACKOFF_BASE);
        return Ok(PostAttempt::RetryAfter(wait));
    }

    // Read the body. Distinguish three failure modes:
    //
    //   1. `BodyReadError::Io` — connection reset / TLS truncation /
    //      gzip EOF mid-stream. These are transient transport errors
    //      and the right response is to retry, just like a `send()`
    //      timeout. The previous version let these errors propagate
    //      via `?` straight out of the retry loop, breaking the
    //      "bounded retry on transient failures" claim.
    //   2. `BodyReadError::TooLarge` — server sent more than our
    //      MAX_RESPONSE_BYTES cap. Hard error: the next retry will
    //      almost certainly produce the same oversized response.
    //   3. `BodyReadError::NotUtf8` — server sent non-UTF-8 bytes
    //      claiming to be JSON. Hard error: the server is broken in
    //      a way retry won't fix.
    let body = match read_capped_body(resp) {
        Ok(b) => b,
        Err(BodyReadError::Io(e)) => {
            return Ok(PostAttempt::Retriable(
                anyhow::Error::new(e).context("reading response body from oryx.zsa.io/graphql"),
            ));
        }
        Err(BodyReadError::TooLarge) => {
            return Err(PullError::ResponseTooLarge {
                limit: http::MAX_RESPONSE_BYTES,
            }
            .into());
        }
        Err(BodyReadError::NotUtf8(e)) => {
            return Err(anyhow::anyhow!("response body is not valid UTF-8: {e}"));
        }
    };

    if status.is_success() {
        return Ok(PostAttempt::Success(body));
    }

    // 502/503/504 are transient gateway problems and worth retrying.
    if matches!(status.as_u16(), 502..=504) {
        return Ok(PostAttempt::Retriable(
            PullError::HttpStatus {
                status: status.as_u16(),
                body,
            }
            .into(),
        ));
    }

    // 500 is deliberately NOT retried. Empirically, an Oryx 500
    // means a real server-side bug (the GraphQL resolver panicked,
    // a database constraint failed, etc.) and retrying just
    // produces the same panic against the same bug. The architecture
    // spec also classifies 500 as a "data error" — the user wants
    // to see the failure immediately rather than wait through the
    // backoff loop. Our test `pull_now_surfaces_http_500` pins this
    // behavior.
    //
    // 5xx codes other than 500/502/503/504 (e.g. 507 Insufficient
    // Storage, 520+ Cloudflare codes) are also hard errors here.
    // If a real-world Oryx deployment starts emitting any of those
    // routinely, add them to the 502..=504 range above with a
    // citation in this comment.
    Err(PullError::HttpStatus {
        status: status.as_u16(),
        body,
    }
    .into())
}

/// Failure modes for [`read_capped_body`]. Distinguished from a flat
/// `anyhow::Error` because the caller's response is type-dependent —
/// transient IO errors should retry, oversized payloads should fail
/// hard, and non-UTF-8 means the server is broken in a way retry
/// won't fix.
enum BodyReadError {
    /// Transient transport error (connection reset, TLS truncation,
    /// gzip EOF mid-stream). Caller should retry.
    Io(std::io::Error),
    /// Server sent more than [`http::MAX_RESPONSE_BYTES`].
    TooLarge,
    /// Body bytes were not valid UTF-8.
    NotUtf8(std::string::FromUtf8Error),
}

/// Buffer the response body into memory, refusing to read more than
/// [`http::MAX_RESPONSE_BYTES`]. Returning a typed error variant
/// (rather than `anyhow::Error`) lets the caller decide whether the
/// failure is retriable.
fn read_capped_body(
    resp: reqwest::blocking::Response,
) -> std::result::Result<String, BodyReadError> {
    let mut buf = Vec::with_capacity(4096);
    // `take(N)` reads at most N bytes; we ask for `MAX + 1` so we can
    // detect overflow without confusing "exactly MAX" with "more than
    // MAX".
    let mut limited = resp.take((http::MAX_RESPONSE_BYTES as u64).saturating_add(1));
    limited.read_to_end(&mut buf).map_err(BodyReadError::Io)?;
    if buf.len() > http::MAX_RESPONSE_BYTES {
        return Err(BodyReadError::TooLarge);
    }
    String::from_utf8(buf).map_err(BodyReadError::NotUtf8)
}

/// True for reqwest errors that represent a transient network condition
/// worth retrying. We deliberately exclude TLS / decode / status errors
/// since those won't be resolved by waiting and trying again. Mid-
/// response IO errors (connection reset etc.) take a separate path
/// via [`BodyReadError::Io`] in [`do_post`].
fn is_retriable_network_error(e: &reqwest::Error) -> bool {
    e.is_timeout() || e.is_connect()
}

/// Parse a `Retry-After` header value. Supports the delta-seconds form
/// (e.g. `Retry-After: 30`); the HTTP-date form (RFC 7231 §7.1.3) and
/// any unparseable garbage both return `None` so the caller falls back
/// to its own backoff schedule rather than treating junk as a valid
/// "wait 5s" signal. Adding a real HTTP-date parser would mean
/// pulling in `httpdate` and is left for a future change if Oryx
/// actually starts emitting HTTP-date Retry-After values in practice.
///
/// Delta-seconds values are clamped to [`MAX_RETRY_AFTER`] so a
/// misbehaving server can't hang the CLI for hours.
fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let raw = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    let trimmed = raw.trim();
    let secs: u64 = trimmed.parse().ok()?;
    Some(Duration::from_secs(secs).min(MAX_RETRY_AFTER))
}

/// Compute the backoff delay before retry attempt `retry_attempt`
/// (zero-indexed: `retry_attempt = 0` is the first retry). Uses
/// exponential backoff with **uniformly random jitter** to avoid the
/// thundering-herd problem when many clients all hit the same outage
/// and recover together.
///
/// Jitter is sourced from `rand::thread_rng()`, which on first use
/// seeds itself from the OS RNG. The previous implementation used
/// `SystemTime::now().subsec_nanos() % base_ms` which is correlated
/// across NTP-synced hosts (same wallclock → same modulo result) and
/// undermines the anti-herd guarantee.
/// Compile-time floor on `1u64 << SHIFT_CAP` so a future bump to
/// `MAX_ATTEMPTS` past 7 trips immediately instead of silently
/// flatlining backoff at `2^6 * BACKOFF_BASE`.
const BACKOFF_SHIFT_CAP: u32 = 6;
const _: () = assert!(
    MAX_ATTEMPTS - 1 <= BACKOFF_SHIFT_CAP,
    "MAX_ATTEMPTS - 1 must fit within BACKOFF_SHIFT_CAP; bump the cap if you raised MAX_ATTEMPTS"
);

fn backoff_delay(retry_attempt: u32) -> Duration {
    use rand::Rng;
    let base_ms = BACKOFF_BASE.as_millis() as u64;
    // Cap the shift defensively; in practice `MAX_ATTEMPTS` keeps the
    // input well below this. Removing the cap entirely would make
    // `1u64 << retry_attempt` UB at retry_attempt >= 64. The
    // `const _` assertion above ties the cap to MAX_ATTEMPTS so a
    // future bump can't silently outgrow it.
    let shift = retry_attempt.min(BACKOFF_SHIFT_CAP);
    let expo = base_ms.saturating_mul(1u64 << shift);
    let jitter = rand::thread_rng().gen_range(0..base_ms.max(1));
    Duration::from_millis(expo.saturating_add(jitter))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_retry_after_seconds() {
        let mut h = reqwest::header::HeaderMap::new();
        h.insert(reqwest::header::RETRY_AFTER, "12".parse().unwrap());
        assert_eq!(parse_retry_after(&h), Some(Duration::from_secs(12)));
    }

    #[test]
    fn parse_retry_after_clamps_huge_values() {
        let mut h = reqwest::header::HeaderMap::new();
        h.insert(reqwest::header::RETRY_AFTER, "999999".parse().unwrap());
        assert_eq!(parse_retry_after(&h), Some(MAX_RETRY_AFTER));
    }

    #[test]
    fn parse_retry_after_missing_header() {
        let h = reqwest::header::HeaderMap::new();
        assert_eq!(parse_retry_after(&h), None);
    }

    #[test]
    fn parse_retry_after_http_date_returns_none() {
        // We don't parse HTTP-date and don't pretend to. Returning None
        // lets the caller fall back to its own backoff schedule rather
        // than silently treating junk as a valid "wait 5s" signal.
        let mut h = reqwest::header::HeaderMap::new();
        h.insert(
            reqwest::header::RETRY_AFTER,
            "Wed, 21 Oct 2026 07:28:00 GMT".parse().unwrap(),
        );
        assert_eq!(parse_retry_after(&h), None);
    }

    #[test]
    fn parse_retry_after_garbage_returns_none() {
        // Any unparseable string returns None — no silent fallback.
        let mut h = reqwest::header::HeaderMap::new();
        h.insert(
            reqwest::header::RETRY_AFTER,
            "garbage-from-a-load-balancer".parse().unwrap(),
        );
        assert_eq!(parse_retry_after(&h), None);
    }

    #[test]
    fn backoff_grows_exponentially() {
        let base = BACKOFF_BASE.as_millis() as u64;
        let ms = |d: Duration| d.as_millis() as u64;
        // attempt 0: base * 1 + jitter < base * 2
        assert!(ms(backoff_delay(0)) >= base);
        assert!(ms(backoff_delay(0)) < base * 2);
        // attempt 1: base * 2 + jitter < base * 3
        assert!(ms(backoff_delay(1)) >= base * 2);
        assert!(ms(backoff_delay(1)) < base * 3);
        // attempt 2: base * 4 + jitter < base * 5
        assert!(ms(backoff_delay(2)) >= base * 4);
        assert!(ms(backoff_delay(2)) < base * 5);
    }
}
