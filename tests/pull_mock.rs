//! Integration tests for the Oryx GraphQL client + pull_now/auto_pull
//! against a wiremock server. Verifies that the exact query shape we send
//! matches what Oryx would accept, and that response shapes we expect are
//! correctly parsed.
//!
//! All tests in this file are `#[serial]` because they mutate the process
//! `ORYX_GRAPHQL_ENDPOINT` env var.

use std::fs;

use oryx_bench::config::Project;
use oryx_bench::pull::{self, PullOutcome};
use serde_json::json;
use serial_test::serial;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Spins up a local `MockServer`, points `ORYX_GRAPHQL_ENDPOINT` at it,
/// and returns the server handle (drop = shutdown).
async fn start_mock() -> MockServer {
    let server = MockServer::start().await;
    std::env::set_var("ORYX_GRAPHQL_ENDPOINT", format!("{}/graphql", server.uri()));
    server
}

/// Create an Oryx-mode project at `td.path()`.
fn mk_oryx_project(td: &TempDir, auto_pull: &str, poll_interval_s: u64) -> Project {
    fs::write(
        td.path().join("kb.toml"),
        format!(
            r#"
[layout]
hash_id = "yrbLx"
geometry = "voyager"
revision = "latest"

[sync]
auto_pull = "{auto_pull}"
poll_interval_s = {poll_interval_s}
"#
        ),
    )
    .unwrap();
    Project::load_at(td.path()).unwrap()
}

/// Fixture's revision hash (the real one in examples/voyager-dvorak/pulled/revision.json).
const FIXTURE_HASH: &str = "XX44B";

/// Build a GraphQL response for the metadata query.
fn metadata_response(hash: &str) -> serde_json::Value {
    json!({
        "data": {
            "layout": {
                "revision": { "hashId": hash }
            }
        }
    })
}

/// Build a GraphQL response for the full layout query, echoing the fixture.
fn full_response() -> serde_json::Value {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../examples/voyager-dvorak/pulled/revision.json"
    ))
    .unwrap();
    json!({
        "data": {
            "layout": fixture
        }
    })
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn pull_now_writes_revision_json_on_first_pull() {
    let server = start_mock().await;
    // First request is metadata; second is full layout.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(metadata_response(FIXTURE_HASH)))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(full_response()))
        .mount(&server)
        .await;

    let td = TempDir::new().unwrap();
    let project = mk_oryx_project(&td, "on_read", 60);

    let outcome = tokio::task::spawn_blocking(move || pull::pull_now(&project, None, true))
        .await
        .unwrap()
        .expect("pull succeeds");

    match outcome {
        PullOutcome::Pulled { from, to } => {
            assert_eq!(from, None);
            assert_eq!(to, FIXTURE_HASH);
        }
        other => panic!("expected Pulled, got {other:?}"),
    }

    // revision.json should now exist and be parseable.
    let revision_json = td.path().join("pulled/revision.json");
    assert!(revision_json.is_file());
    let raw = fs::read_to_string(&revision_json).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    // We write the `layout` subobject.
    assert_eq!(parsed.get("hashId").and_then(|h| h.as_str()), Some("yrbLx"));
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn pull_now_up_to_date_when_hashes_match() {
    let server = start_mock().await;

    // Pre-seed a local revision.json with the same hash the server returns.
    let td = TempDir::new().unwrap();
    fs::create_dir_all(td.path().join("pulled")).unwrap();
    let fixture = include_str!("../examples/voyager-dvorak/pulled/revision.json");
    fs::write(td.path().join("pulled/revision.json"), fixture).unwrap();

    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(metadata_response(FIXTURE_HASH)))
        .mount(&server)
        .await;

    // auto_pull = "on_read" so the poll_interval cache gate matters; set it
    // to 0 so the cache age check always fires.
    let project = mk_oryx_project(&td, "on_read", 1);
    let outcome = tokio::task::spawn_blocking(move || pull::pull_now(&project, None, false))
        .await
        .unwrap()
        .expect("pull succeeds");
    assert!(
        matches!(outcome, PullOutcome::UpToDate),
        "expected UpToDate, got {outcome:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn pull_now_surfaces_http_500() {
    let server = start_mock().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .mount(&server)
        .await;

    let td = TempDir::new().unwrap();
    let project = mk_oryx_project(&td, "on_read", 60);
    let err = tokio::task::spawn_blocking(move || pull::pull_now(&project, None, true))
        .await
        .unwrap()
        .expect_err("should fail");
    let msg = format!("{err:#}");
    assert!(msg.contains("500"), "unexpected error: {msg}");
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn pull_now_surfaces_graphql_errors_field() {
    let server = start_mock().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "errors": [{ "message": "layout not found" }]
        })))
        .mount(&server)
        .await;

    let td = TempDir::new().unwrap();
    let project = mk_oryx_project(&td, "on_read", 60);
    let err = tokio::task::spawn_blocking(move || pull::pull_now(&project, None, true))
        .await
        .unwrap()
        .expect_err("should fail");
    let msg = format!("{err:#}");
    assert!(msg.contains("layout not found"), "unexpected error: {msg}");
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn pull_now_surfaces_layout_not_found_on_null() {
    // When Oryx returns {"data": {"layout": null}} for a non-existent hash,
    // the CLI should surface "layout '…' not found on Oryx" instead of a
    // raw serde deserialization error.
    let server = start_mock().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "layout": null }
        })))
        .mount(&server)
        .await;

    let td = TempDir::new().unwrap();
    let project = mk_oryx_project(&td, "on_read", 60);
    let err = tokio::task::spawn_blocking(move || pull::pull_now(&project, None, true))
        .await
        .unwrap()
        .expect_err("should fail");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("not found on Oryx"),
        "expected 'not found on Oryx', got: {msg}"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn pull_now_retries_transient_503_then_succeeds() {
    // Verifies that a 503 from Oryx (gateway flake) is silently retried
    // by the GraphQL client and the pull eventually succeeds. wiremock
    // matches mounts in LIFO order, so mounting the 503 last with
    // `up_to_n_times(1)` makes it serve once and then fall through to
    // the success mocks below.
    let server = start_mock().await;

    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(full_response()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(metadata_response(FIXTURE_HASH)))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(503).set_body_string("temporarily unavailable"))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let td = TempDir::new().unwrap();
    let project = mk_oryx_project(&td, "on_read", 60);

    let outcome = tokio::task::spawn_blocking(move || pull::pull_now(&project, None, true))
        .await
        .unwrap()
        .expect("retry path eventually succeeds");

    assert!(
        matches!(outcome, PullOutcome::Pulled { .. }),
        "expected Pulled after retry, got {outcome:?}"
    );
    assert!(td.path().join("pulled/revision.json").is_file());
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn pull_now_honors_retry_after_then_succeeds() {
    // Verifies the 429 happy path: server returns 429 with a small
    // Retry-After, the client sleeps for that long, the next attempt
    // succeeds. Wiremock matches mounts LIFO, so the 429 mock is
    // mounted last with up_to_n_times(1).
    let server = start_mock().await;

    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(full_response()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(metadata_response(FIXTURE_HASH)))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "1")
                .set_body_string("rate limited"),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let td = TempDir::new().unwrap();
    let project = mk_oryx_project(&td, "on_read", 60);
    let outcome = tokio::task::spawn_blocking(move || pull::pull_now(&project, None, true))
        .await
        .unwrap()
        .expect("retry-after path eventually succeeds");
    assert!(
        matches!(outcome, PullOutcome::Pulled { .. }),
        "expected Pulled after Retry-After, got {outcome:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn pull_now_rejects_oversized_response_body() {
    // Pin the 5MB body cap end-to-end. The cap exists to prevent OOM
    // from a buggy / malicious server; without a test the limit could
    // silently regress.
    let server = start_mock().await;
    let huge = "a".repeat(6 * 1024 * 1024); // 6 MB > 5 MB cap
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_string(huge))
        .mount(&server)
        .await;

    let td = TempDir::new().unwrap();
    let project = mk_oryx_project(&td, "on_read", 60);
    let err = tokio::task::spawn_blocking(move || pull::pull_now(&project, None, true))
        .await
        .unwrap()
        .expect_err("oversized body should fail");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("exceeds") && msg.contains("byte"),
        "expected size-limit error, got: {msg}"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn pull_now_gives_up_after_persistent_503() {
    // Hard outage: every attempt sees a 503. We should bail out with
    // an error mentioning the attempt count instead of looping forever.
    let server = start_mock().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(503).set_body_string("still down"))
        .mount(&server)
        .await;

    let td = TempDir::new().unwrap();
    let project = mk_oryx_project(&td, "on_read", 60);
    let err = tokio::task::spawn_blocking(move || pull::pull_now(&project, None, true))
        .await
        .unwrap()
        .expect_err("persistent 503 should fail");
    let msg = format!("{err:#}");
    assert!(msg.contains("503"), "unexpected error: {msg}");
    assert!(
        msg.contains("after 3 attempts") || msg.contains("attempts"),
        "expected attempt count in error: {msg}"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn full_layout_query_response_deserializes_into_oryx_schema() {
    // This test exists so a breaking change in our oryx.rs schema is
    // caught as soon as we pull the fixture through the real code path.
    let server = start_mock().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(metadata_response(FIXTURE_HASH)))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(full_response()))
        .mount(&server)
        .await;

    let td = TempDir::new().unwrap();
    let project = mk_oryx_project(&td, "on_read", 60);
    tokio::task::spawn_blocking(move || pull::pull_now(&project, None, true))
        .await
        .unwrap()
        .expect("pull succeeds");

    let raw = fs::read_to_string(td.path().join("pulled/revision.json")).unwrap();
    let parsed: oryx_bench::schema::oryx::Layout = serde_json::from_str(&raw).unwrap();
    assert_eq!(parsed.hash_id, "yrbLx");
    assert_eq!(parsed.geometry, "voyager");
    assert_eq!(parsed.revision.layers.len(), 4);
    let canonical = oryx_bench::schema::canonical::CanonicalLayout::from_oryx(&parsed).unwrap();
    assert!(canonical.layers.iter().any(|l| l.name == "Main"));
}

/// Pin the 2026-Q2 Oryx `combos` schema end-to-end. The Oryx server
/// changed `combos` from a scalar to an object type with required
/// `keyIndices`/`layerIdx`/`trigger` subfields, and the bare `combos`
/// selection that worked under the old schema now produces a hard
/// error. This test mounts a wiremock that returns the new shape and
/// asserts the response makes it through:
///
///   1. `pull::pull_now` (writes `pulled/revision.json`)
///   2. `oryx::Layout` deserialization (`Combo.key_indices`, `layer_idx`,
///      `trigger`)
///   3. `CanonicalLayout::from_oryx` (translates `keyIndices` to
///      position names via the geometry, resolves `layerIdx` to a layer
///      name, runs `trigger` through the standard action translator)
///
/// If a future Oryx schema renames any of those subfields, this test
/// fires before the change ships to production and the user notices a
/// silent combo regression.
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn pull_now_round_trips_object_typed_combos() {
    // A minimal-but-realistic full-layout response: one layer (so the
    // combo's `layerIdx: 0` resolves), one combo with two key indices
    // and a trigger. We construct this inline (rather than using
    // `full_response()`) to keep this test independent of the example
    // fixture's evolution.
    let layout_json = json!({
        "hashId": "yrbLx",
        "title": "ComboTest",
        "geometry": "voyager",
        "privacy": false,
        "revision": {
            "hashId": FIXTURE_HASH,
            "qmkVersion": "24.0",
            "title": "combo regression",
            "createdAt": "2026-04-07 00:00:00 UTC",
            "model": "v1",
            "md5": "deadbeefdeadbeefdeadbeefdeadbeef",
            "layers": [
                {
                    "title": "Main",
                    "position": 0,
                    // 52 keys for voyager — fill with KC_NO; only the
                    // combo's chord positions and the trigger keycode
                    // matter for what this test pins.
                    "keys": (0..52).map(|_| json!({
                        "tap": { "code": "KC_NO" },
                        "hold": null,
                        "doubleTap": null,
                        "tapHold": null
                    })).collect::<Vec<_>>()
                }
            ],
            "combos": [
                {
                    "keyIndices": [16, 17],
                    "layerIdx": 0,
                    "name": "esc-combo",
                    "trigger": {
                        "code": "KC_ESCAPE",
                        "modifier": null,
                        "modifiers": null
                    }
                }
            ],
            "config": {},
            "swatch": null
        }
    });
    let full_body = json!({ "data": { "layout": layout_json } });

    let server = start_mock().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(metadata_response(FIXTURE_HASH)))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(full_body))
        .mount(&server)
        .await;

    let td = TempDir::new().unwrap();
    let project = mk_oryx_project(&td, "on_read", 60);
    tokio::task::spawn_blocking(move || pull::pull_now(&project, None, true))
        .await
        .unwrap()
        .expect("pull with object-typed combos succeeds");

    // Stage 1: re-read the file pull_now wrote and parse it back as
    // `oryx::Layout` (so we know our typed Combo struct accepts the
    // wire shape).
    let raw = fs::read_to_string(td.path().join("pulled/revision.json")).unwrap();
    let parsed: oryx_bench::schema::oryx::Layout = serde_json::from_str(&raw).unwrap();
    let combos = parsed
        .revision
        .combos
        .as_ref()
        .expect("combos field present and non-null");
    assert_eq!(combos.len(), 1, "fixture defines exactly one combo");
    assert_eq!(combos[0].key_indices, vec![16, 17]);
    assert_eq!(combos[0].layer_idx, 0);

    // Stage 2: lift the parsed Oryx layout into the canonical
    // representation and assert the combo lands intact (position
    // names, layer name, emitted keycode).
    let canonical = oryx_bench::schema::canonical::CanonicalLayout::from_oryx(&parsed).unwrap();
    assert_eq!(
        canonical.combos.len(),
        1,
        "exactly one combo should round-trip into canonical"
    );
    let combo = &canonical.combos[0];
    assert_eq!(
        combo.keys,
        vec!["L_index_home".to_string(), "L_inner_home".to_string()],
        "key indices 16/17 should resolve to L_index_home/L_inner_home on voyager"
    );
    assert_eq!(combo.layer.as_deref(), Some("Main"));
    assert_eq!(combo.sends, "KC_ESC");
}
