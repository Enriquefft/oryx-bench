//! Auto-pull cache behavior tests. Covers branches not exercised by the
//! lib unit tests: cache-stale-but-up-to-date, cache-stale-and-pulled,
//! on_demand, and force-bypass.

use std::fs;

use oryx_bench::config::Project;
use oryx_bench::pull::{self, PullOutcome};
use serde_json::json;
use serial_test::serial;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const FIXTURE_HASH: &str = "XX44B";

async fn start_mock() -> MockServer {
    let server = MockServer::start().await;
    std::env::set_var("ORYX_GRAPHQL_ENDPOINT", format!("{}/graphql", server.uri()));
    server
}

fn mk_project(td: &TempDir, auto_pull: &str, poll_interval_s: u64) -> Project {
    fs::write(
        td.path().join("kb.toml"),
        format!(
            r#"
[layout]
hash_id = "yrbLx"
geometry = "voyager"

[sync]
auto_pull = "{auto_pull}"
poll_interval_s = {poll_interval_s}
"#
        ),
    )
    .unwrap();
    Project::load_at(td.path()).unwrap()
}

fn seed_local_revision(td: &TempDir) {
    fs::create_dir_all(td.path().join("pulled")).unwrap();
    fs::write(
        td.path().join("pulled/revision.json"),
        include_str!("../examples/voyager-dvorak/pulled/revision.json"),
    )
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn auto_pull_stale_cache_matching_hash_is_up_to_date() {
    let server = start_mock().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "layout": { "revision": { "hashId": FIXTURE_HASH } } }
        })))
        .mount(&server)
        .await;

    let td = TempDir::new().unwrap();
    seed_local_revision(&td);
    let project = mk_project(&td, "on_read", 1); // cache always stale (poll_interval can't be 0 per validation)

    let outcome = tokio::task::spawn_blocking(move || pull::auto_pull(&project))
        .await
        .unwrap()
        .expect("auto_pull succeeds");
    assert!(
        matches!(outcome, PullOutcome::UpToDate),
        "expected UpToDate, got {outcome:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn auto_pull_stale_cache_differing_hash_pulls_full() {
    let server = start_mock().await;
    // Metadata returns a different hash than the seeded revision.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "layout": { "revision": { "hashId": "NEWHASH" } } }
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    // Full layout returned on the second request.
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../examples/voyager-dvorak/pulled/revision.json"
    ))
    .unwrap();
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "layout": fixture }
        })))
        .mount(&server)
        .await;

    let td = TempDir::new().unwrap();
    seed_local_revision(&td);
    let project = mk_project(&td, "on_read", 1);

    let outcome = tokio::task::spawn_blocking(move || pull::auto_pull(&project))
        .await
        .unwrap()
        .expect("auto_pull succeeds");
    match outcome {
        PullOutcome::Pulled { from, to } => {
            assert_eq!(from.as_deref(), Some(FIXTURE_HASH));
            assert_eq!(to, "NEWHASH");
        }
        other => panic!("expected Pulled, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn auto_pull_on_demand_mode_skips() {
    let _server = start_mock().await; // still set env var so accidental requests would land here
    let td = TempDir::new().unwrap();
    let project = mk_project(&td, "on_demand", 1);
    let outcome = tokio::task::spawn_blocking(move || pull::auto_pull(&project))
        .await
        .unwrap()
        .expect("auto_pull succeeds");
    assert!(matches!(outcome, PullOutcome::Skipped));
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn pull_now_force_bypasses_never_mode() {
    let server = start_mock().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "layout": { "revision": { "hashId": FIXTURE_HASH } } }
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../examples/voyager-dvorak/pulled/revision.json"
    ))
    .unwrap();
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "layout": fixture }
        })))
        .mount(&server)
        .await;

    let td = TempDir::new().unwrap();
    let project = mk_project(&td, "never", 60);
    let outcome = tokio::task::spawn_blocking(move || pull::pull_now(&project, None, true))
        .await
        .unwrap()
        .expect("pull succeeds");
    assert!(matches!(outcome, PullOutcome::Pulled { .. }));
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn pull_now_never_mode_without_force_skips() {
    let _server = start_mock().await;
    let td = TempDir::new().unwrap();
    let project = mk_project(&td, "never", 60);
    let outcome = tokio::task::spawn_blocking(move || pull::pull_now(&project, None, false))
        .await
        .unwrap()
        .expect("pull succeeds");
    assert!(matches!(outcome, PullOutcome::Skipped));
}
