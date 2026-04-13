//! attach/detach lifecycle tests. Use the wiremock GraphQL fixture so
//! attach can run a real pull against a fake Oryx.

use std::fs;

use serde_json::json;
use serial_test::serial;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use oryx_bench::commands::{attach, detach, init};
use oryx_bench::config::Project;
use oryx_bench::schema::kb_toml::AutoPull;

const FIXTURE_HASH: &str = "XX44B";

async fn start_mock() -> MockServer {
    let server = MockServer::start().await;
    std::env::set_var("ORYX_GRAPHQL_ENDPOINT", format!("{}/graphql", server.uri()));
    server
}

fn fixture_full_response() -> serde_json::Value {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../examples/voyager-dvorak/pulled/revision.json"
    ))
    .unwrap();
    json!({ "data": { "layout": fixture } })
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn attach_converts_local_to_oryx_mode_and_pulls() {
    let server = start_mock().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "layout": { "revision": { "hashId": FIXTURE_HASH } } }
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture_full_response()))
        .mount(&server)
        .await;

    let td = TempDir::new().unwrap();
    init::init_in(
        td.path(),
        &init::Args {
            hash: None,
            blank: true,
            geometry: "voyager".into(),
            name: Some("test".into()),
            no_skill: true,
            force: false,
        },
    )
    .unwrap();
    assert!(td.path().join("layout.toml").is_file());

    // attach refuses to touch a non-git directory without --force, so
    // the test passes --force here. A separate test pins the safety
    // check itself.
    let project_root = td.path().to_path_buf();
    tokio::task::spawn_blocking(move || {
        attach::run(
            attach::Args {
                hash: "yrbLx".into(),
                force: true,
            },
            Some(project_root),
        )
    })
    .await
    .unwrap()
    .expect("attach succeeds");

    // layout.toml gone, kb.toml has hash_id, pulled/revision.json populated.
    assert!(!td.path().join("layout.toml").exists());
    assert!(td.path().join("pulled/revision.json").is_file());
    let project = Project::load_at(td.path()).unwrap();
    assert!(project.is_oryx_mode());
    assert!(!project.is_local_mode());
    // Attach must restore auto_pull to the Oryx-mode default so the
    // detach→attach round-trip doesn't leave sync silently disabled.
    assert_eq!(
        project.cfg.sync.auto_pull,
        AutoPull::OnRead,
        "attach should restore auto_pull to on_read"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn detach_converts_oryx_to_local_and_renders_layout_toml() {
    // Start an Oryx-mode project, drop in the fixture by hand (no need
    // for wiremock here — detach reads from disk).
    let _server = start_mock().await; // env var still set so accidental pulls would land here

    let td = TempDir::new().unwrap();
    init::init_in(
        td.path(),
        &init::Args {
            hash: Some("yrbLx".into()),
            blank: false,
            geometry: "voyager".into(),
            name: Some("test".into()),
            no_skill: true,
            force: false,
        },
    )
    .unwrap();
    fs::write(
        td.path().join("pulled/revision.json"),
        include_str!("../examples/voyager-dvorak/pulled/revision.json"),
    )
    .unwrap();

    let project_root = td.path().to_path_buf();
    tokio::task::spawn_blocking(move || {
        detach::run(detach::Args { force: true }, Some(project_root))
    })
    .await
    .unwrap()
    .expect("detach succeeds");

    // pulled/ removed, layout.toml created and parses.
    assert!(!td.path().join("pulled").exists());
    assert!(td.path().join("layout.toml").is_file());
    let project = Project::load_at(td.path()).unwrap();
    assert!(project.is_local_mode());
    assert!(!project.is_oryx_mode());
    // Sync settings should be neutralized: auto_pull = "never" since
    // there's no Oryx source to sync with after detach.
    assert_eq!(
        project.cfg.sync.auto_pull,
        AutoPull::Never,
        "detach should set auto_pull to never"
    );

    // Sanity-check that the rendered layout.toml round-trips through the parser.
    let raw = fs::read_to_string(td.path().join("layout.toml")).unwrap();
    let parsed: oryx_bench::schema::layout::LayoutFile = toml::from_str(&raw).unwrap();
    assert_eq!(parsed.layers.len(), 4);
    assert_eq!(parsed.meta.geometry, "voyager");
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn detach_refuses_to_clobber_existing_layout_toml_without_force() {
    // Without --force: refuse to overwrite a pre-existing layout.toml.
    // With --force: overwrite it (user opted in).
    let _server = start_mock().await;

    let td = TempDir::new().unwrap();
    init::init_in(
        td.path(),
        &init::Args {
            hash: Some("yrbLx".into()),
            blank: false,
            geometry: "voyager".into(),
            name: Some("test".into()),
            no_skill: true,
            force: false,
        },
    )
    .unwrap();
    fs::write(
        td.path().join("pulled/revision.json"),
        include_str!("../examples/voyager-dvorak/pulled/revision.json"),
    )
    .unwrap();

    // Plant a user-authored layout.toml.
    let user_authored = "# I authored this; please don't delete me\n[meta]\ntitle = \"draft\"\n";
    fs::write(td.path().join("layout.toml"), user_authored).unwrap();

    // Without --force: must refuse.
    {
        let project_root = td.path().to_path_buf();
        let err = tokio::task::spawn_blocking(move || {
            detach::run(detach::Args { force: false }, Some(project_root))
        })
        .await
        .unwrap()
        .expect_err("detach without --force should refuse to clobber existing layout.toml");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("refusing to overwrite") && msg.contains("layout.toml"),
            "expected clobber-refusal error, got: {msg}"
        );

        // The user-authored file must be untouched.
        let after = fs::read_to_string(td.path().join("layout.toml")).unwrap();
        assert_eq!(after, user_authored);
        // And pulled/ must still be intact (no half-detached state).
        assert!(td.path().join("pulled/revision.json").exists());
    }

    // With --force: must succeed and overwrite layout.toml.
    {
        let project_root = td.path().to_path_buf();
        tokio::task::spawn_blocking(move || {
            detach::run(detach::Args { force: true }, Some(project_root))
        })
        .await
        .unwrap()
        .expect("detach with --force should succeed even with existing layout.toml");

        // layout.toml should now be the generated one, not user_authored.
        let after = fs::read_to_string(td.path().join("layout.toml")).unwrap();
        assert!(
            !after.contains("please don't delete me"),
            "layout.toml should have been overwritten by the generated content"
        );
        // pulled/ must be gone.
        assert!(
            !td.path().join("pulled/revision.json").exists(),
            "pulled/ should have been removed"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn attach_without_force_refuses_in_non_git_dir() {
    // The fail-closed safety check: if the directory isn't a git repo,
    // attach refuses without --force so it can't silently destroy
    // uncommitted layout.toml work.
    let _server = start_mock().await;
    let td = TempDir::new().unwrap();
    init::init_in(
        td.path(),
        &init::Args {
            hash: None,
            blank: true,
            geometry: "voyager".into(),
            name: Some("test".into()),
            no_skill: true,
            force: false,
        },
    )
    .unwrap();

    let project_root = td.path().to_path_buf();
    let err = tokio::task::spawn_blocking(move || {
        attach::run(
            attach::Args {
                hash: "yrbLx".into(),
                force: false,
            },
            Some(project_root),
        )
    })
    .await
    .unwrap()
    .expect_err("attach should refuse without --force in a non-git dir");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("not inside a git repository"),
        "expected non-git refusal, got: {msg}"
    );
    // layout.toml must still exist after the refused attach.
    assert!(
        td.path().join("layout.toml").exists(),
        "attach must not delete layout.toml when it refuses"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn detach_on_local_mode_project_errors_with_nothing_to_detach() {
    // Edge case: running `detach` on a project that's already in local mode
    // (no hash_id, no pulled/) should return an error immediately and leave
    // all files untouched.
    let _server = start_mock().await;

    let td = TempDir::new().unwrap();
    init::init_in(
        td.path(),
        &init::Args {
            hash: None,
            blank: true,
            geometry: "voyager".into(),
            name: Some("test".into()),
            no_skill: true,
            force: false,
        },
    )
    .unwrap();

    // Local-mode init must NOT create pulled/.
    assert!(
        !td.path().join("pulled").exists(),
        "local-mode project should not have a pulled/ directory"
    );
    assert!(
        td.path().join("layout.toml").is_file(),
        "local-mode project must have layout.toml"
    );

    // Snapshot the files before detach so we can verify they're untouched.
    let kb_before = fs::read_to_string(td.path().join("kb.toml")).unwrap();
    let layout_before = fs::read_to_string(td.path().join("layout.toml")).unwrap();

    let project_root = td.path().to_path_buf();
    let err = tokio::task::spawn_blocking(move || {
        detach::run(detach::Args { force: true }, Some(project_root))
    })
    .await
    .unwrap()
    .expect_err("detach on a local-mode project should fail");

    let msg = format!("{err:#}");
    assert!(
        msg.contains("project is not in Oryx mode"),
        "expected 'not in Oryx mode' error, got: {msg}"
    );
    assert!(
        msg.contains("nothing to detach"),
        "expected 'nothing to detach' in error, got: {msg}"
    );

    // Files must be byte-identical — detach must not touch anything.
    let kb_after = fs::read_to_string(td.path().join("kb.toml")).unwrap();
    let layout_after = fs::read_to_string(td.path().join("layout.toml")).unwrap();
    assert_eq!(
        kb_before, kb_after,
        "kb.toml must be untouched after failed detach"
    );
    assert_eq!(
        layout_before, layout_after,
        "layout.toml must be untouched after failed detach"
    );

    // Still no pulled/ directory.
    assert!(
        !td.path().join("pulled").exists(),
        "pulled/ must not exist after failed detach"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn detach_without_force_prints_warning_only() {
    let _server = start_mock().await;
    let td = TempDir::new().unwrap();
    init::init_in(
        td.path(),
        &init::Args {
            hash: Some("yrbLx".into()),
            blank: false,
            geometry: "voyager".into(),
            name: Some("test".into()),
            no_skill: true,
            force: false,
        },
    )
    .unwrap();
    fs::write(
        td.path().join("pulled/revision.json"),
        include_str!("../examples/voyager-dvorak/pulled/revision.json"),
    )
    .unwrap();

    let project_root = td.path().to_path_buf();
    tokio::task::spawn_blocking(move || {
        detach::run(detach::Args { force: false }, Some(project_root))
    })
    .await
    .unwrap()
    .expect("detach succeeds (warn-only path)");

    // Without --force, nothing should have changed.
    assert!(td.path().join("pulled/revision.json").is_file());
    assert!(!td.path().join("layout.toml").exists());
}
