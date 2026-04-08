//! Thin wrapper around `reqwest::blocking::Client`.
//!
//! All oryx-bench HTTP traffic flows through [`client`] so policy
//! (timeouts, User-Agent, transport encoding, body cap) lives in one
//! place. The retry/backoff loop and the per-call response-size cap
//! live in [`crate::pull::graphql`] which is the only HTTP caller in
//! the project.

use std::time::Duration;

use anyhow::Result;
use once_cell::sync::OnceCell;

static CLIENT: OnceCell<reqwest::blocking::Client> = OnceCell::new();

/// Time allowed to establish a TCP + TLS connection. Kept short so a
/// DNS black-hole or firewall drop fails fast instead of waiting out
/// the full request timeout.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Total time allowed for a single HTTP request (connect + send + receive).
/// Oryx's GraphQL endpoint typically returns the metadata query in under
/// 200ms and the full layout query in under 1s; 15s is generous enough
/// that intermittent network slowness doesn't fail the build, but tight
/// enough that the CLI doesn't hang for minutes on a dead endpoint.
const TOTAL_TIMEOUT: Duration = Duration::from_secs(15);

/// Cap on the size of any response body we'll buffer into memory. Set
/// well above the expected full-layout response size (~100KB) but low
/// enough that a buggy or malicious server can't OOM the CLI by
/// streaming gigabytes at us.
pub const MAX_RESPONSE_BYTES: usize = 5 * 1024 * 1024;

/// Crate-versioned User-Agent string. Including the version makes it
/// possible for Oryx operators to correlate server-side error rates
/// with specific client versions when investigating regressions.
///
/// `concat!` + `env!` evaluates at compile time, so this is a real
/// `&'static str` baked into the binary — no per-call allocation,
/// not even the one-shot allocation a `format!()` call would do
/// inside the OnceCell init closure.
const USER_AGENT: &str = concat!("oryx-bench/", env!("CARGO_PKG_VERSION"));

/// A shared blocking HTTP client.
pub fn client() -> Result<&'static reqwest::blocking::Client> {
    CLIENT
        .get_or_try_init(|| {
            reqwest::blocking::Client::builder()
                .connect_timeout(CONNECT_TIMEOUT)
                .timeout(TOTAL_TIMEOUT)
                .user_agent(USER_AGENT)
                .gzip(true)
                .build()
        })
        .map_err(Into::into)
}
