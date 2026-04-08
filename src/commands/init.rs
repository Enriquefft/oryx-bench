//! `oryx-bench init` — create a project skeleton.
//!
//! Two modes:
//!   --hash <H>                Oryx mode (creates pulled/ empty for first pull)
//!   --blank --geometry <G>    local mode (creates layout.toml scaffold)

use std::path::Path;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::Parser;

use crate::schema::geometry;
use crate::schema::kb_toml::{
    AutoPull, BuildBackend, DEFAULT_POLL_INTERVAL_S, DEFAULT_WARN_IF_STALE_S,
};
use crate::util::fs as fsx;

#[derive(Parser, Debug)]
pub struct Args {
    /// The Oryx layout hash. Mutually exclusive with `--blank`.
    #[arg(long, conflicts_with = "blank")]
    pub hash: Option<String>,

    /// Use local mode (no Oryx hash).
    #[arg(long)]
    pub blank: bool,

    /// Keyboard geometry (voyager in v0.1).
    #[arg(long, default_value = "voyager")]
    pub geometry: String,

    /// Friendly project name (defaults to current dir basename).
    #[arg(long)]
    pub name: Option<String>,

    /// Don't prompt to install the project-local Claude Code skill.
    #[arg(long)]
    pub no_skill: bool,

    /// Overwrite existing files.
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: Args) -> Result<ExitCode> {
    let cwd = std::env::current_dir().context("failed to read current working directory")?;
    init_in(&cwd, &args)?;
    Ok(ExitCode::from(0))
}

pub fn init_in(target: &Path, args: &Args) -> Result<()> {
    if !args.blank && args.hash.is_none() {
        bail!("oryx-bench init requires either --hash <HASH> or --blank");
    }
    if !geometry::is_known(&args.geometry) {
        bail!(
            "unknown geometry '{}' — supported: {}",
            args.geometry,
            geometry::supported_slugs()
        );
    }

    let name = args
        .name
        .clone()
        .or_else(|| {
            target
                .file_name()
                .and_then(|s| s.to_str().map(String::from))
        })
        .unwrap_or_else(|| "my-keyboard".to_string());

    if args.blank {
        init_local_mode(target, &name, &args.geometry, args.force)?;
    } else if let Some(hash) = &args.hash {
        init_oryx_mode(target, &name, hash, &args.geometry, args.force)?;
    }

    // Print the "install the skill" nudge unless explicitly disabled.
    if !args.no_skill {
        println!(
            "\n{} Using Claude Code? Run `oryx-bench skill install` to add the\n   project-local skill that teaches Claude about this tool.",
            crate::util::term::HINT
        );
    }

    Ok(())
}

fn init_oryx_mode(
    target: &Path,
    name: &str,
    hash: &str,
    geometry: &str,
    force: bool,
) -> Result<()> {
    let backend = BuildBackend::default();
    let auto_pull = AutoPull::default();
    let kb_toml = format!(
        r#"# kb.toml — project configuration for {name}

[layout]
# Find your hash in the Oryx URL: configure.zsa.io/voyager/layouts/<HASH>/...
hash_id  = "{hash}"
geometry = "{geometry}"
revision = "latest"

[build]
backend = "{backend}"           # v0.1 supports `docker` (or `auto`)

[sync]
auto_pull       = "{auto_pull}"  # on_read | on_demand | never
poll_interval_s = {DEFAULT_POLL_INTERVAL_S}         # cap how often we ping Oryx
warn_if_stale_s = {DEFAULT_WARN_IF_STALE_S}      # 1 day; surfaces in `oryx-bench status`

[lint]
ignore = []
strict = false
"#
    );

    write_with_force(&target.join("kb.toml"), &kb_toml, force)?;
    // Per spec, pulled/ starts empty (no .gitkeep). Git will track it
    // once `oryx-bench pull` lands a file.
    fsx::ensure_dir(&target.join("pulled"))?;
    write_overlay_scaffold(target, force)?;
    write_gitignore(target, force)?;

    println!(
        "{} Created Oryx-mode project '{name}' at {}\n  Run: oryx-bench pull && oryx-bench show",
        crate::util::term::OK,
        target.display()
    );
    Ok(())
}

fn init_local_mode(target: &Path, name: &str, geometry: &str, force: bool) -> Result<()> {
    let backend = BuildBackend::default();
    // Local mode has nothing to pull, so the auto-pull machinery is
    // explicitly disabled here regardless of the global default.
    let auto_pull = AutoPull::Never;
    let kb_toml = format!(
        r#"# kb.toml — project configuration for {name}

[layout]
geometry = "{geometry}"

[layout.local]
file = "layout.toml"

[build]
backend = "{backend}"      # v0.1 supports `docker` (or `auto`)

[sync]
auto_pull = "{auto_pull}"     # local mode has nothing to pull

[lint]
ignore = []
strict = false
"#
    );

    let layout_toml = format!(
        r#"# layout.toml — local-mode visual layout
#
# Hand-author the layout here. Positions default to KC_NO; use
# `inherit = "<layer>"` on overlay layers to default to KC_TRNS instead.

[meta]
title    = "{name}"
geometry = "{geometry}"

[[layers]]
name     = "Main"
position = 0

# Bind one position per line. Comments next to each binding are encouraged
# — that's the main reason to author locally instead of via Oryx.
[layers.keys]
# L_pinky_home = "A"
# R_thumb_outer = {{ tap = "BSPC", hold = "MO(SymNum)" }}
"#
    );

    write_with_force(&target.join("kb.toml"), &kb_toml, force)?;
    write_with_force(&target.join("layout.toml"), &layout_toml, force)?;
    write_overlay_scaffold(target, force)?;
    write_gitignore(target, force)?;

    println!(
        "{} Created local-mode project '{name}' at {}\n  Edit layout.toml, then: oryx-bench show",
        crate::util::term::OK,
        target.display()
    );
    Ok(())
}

fn write_overlay_scaffold(target: &Path, force: bool) -> Result<()> {
    fsx::ensure_dir(&target.join("overlay"))?;
    let readme = r#"# overlay/

Everything in this directory is **your** code. oryx-bench merges it with
the visual layout (from Oryx or layout.toml) to produce the firmware.

- `features.toml` — Tier 1, declarative QMK features
- `*.zig`         — Tier 2, procedural code (state machines, animations)
- `*.c`           — Tier 2′, vendored upstream C libraries (paste-only)

See ARCHITECTURE.md for the four-tier model.
"#;
    let features = r#"# overlay/features.toml — declarative QMK features.

[config]
tapping_term_ms = 200
# permissive_hold = false
# hold_on_other_key_press = false

# [achordion]
# enabled = true
# chord_strategy = "opposite_hands"

# [[key_overrides]]
# mods  = ["LSHIFT"]
# key   = "BSPC"
# sends = "DELETE"

[features]
key_overrides = false
combos        = false
caps_word     = false
mouse_keys    = false
"#;
    write_with_force(&target.join("overlay/README.md"), readme, force)?;
    write_with_force(&target.join("overlay/features.toml"), features, force)?;
    Ok(())
}

fn write_gitignore(target: &Path, force: bool) -> Result<()> {
    let content = r#"# Build outputs
*.bin
*.hex
*.elf

# oryx-bench transient files
.oryx-bench/
result
result-*

# Editor
.idea/
.vscode/
*.swp
.DS_Store
"#;
    write_with_force(&target.join(".gitignore"), content, force)?;
    Ok(())
}

fn write_with_force(path: &Path, content: &str, force: bool) -> Result<()> {
    if path.exists() && !force {
        bail!(
            "refusing to overwrite existing file: {} (use --force to overwrite)",
            path.display()
        );
    }
    if let Some(parent) = path.parent() {
        fsx::ensure_dir(parent)?;
    }
    fsx::atomic_write(path, content.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn init_oryx_mode_creates_expected_files() {
        let td = TempDir::new().unwrap();
        init_in(
            td.path(),
            &Args {
                hash: Some("yrbLx".into()),
                blank: false,
                geometry: "voyager".into(),
                name: Some("test".into()),
                no_skill: true,
                force: false,
            },
        )
        .unwrap();
        assert!(td.path().join("kb.toml").is_file());
        assert!(td.path().join("overlay/features.toml").is_file());
        assert!(td.path().join("overlay/README.md").is_file());
        assert!(td.path().join(".gitignore").is_file());
        assert!(td.path().join("pulled").is_dir());
    }

    #[test]
    fn init_oryx_mode_template_round_trips_with_literal_expected_values() {
        // Regression: this test deliberately asserts against
        // **literal** variant names (not `default()`) so a future PR
        // that changes a default WITHOUT also updating the template
        // is caught. The previous form `parsed.build.backend ==
        // BuildBackend::default()` was tautological — both sides
        // would change together and the test would still pass even
        // if the template stopped emitting the line entirely.
        //
        // We also assert on the rendered raw TOML to pin the
        // string form. If a future change reorders fields, drops
        // a field, or changes the default value, this test fires.
        use crate::schema::kb_toml::{
            AutoPull, BuildBackend, KbToml, DEFAULT_POLL_INTERVAL_S, DEFAULT_WARN_IF_STALE_S,
        };
        let td = TempDir::new().unwrap();
        init_in(
            td.path(),
            &Args {
                hash: Some("yrbLx".into()),
                blank: false,
                geometry: "voyager".into(),
                name: Some("template-roundtrip".into()),
                no_skill: true,
                force: false,
            },
        )
        .unwrap();
        let raw = std::fs::read_to_string(td.path().join("kb.toml")).unwrap();

        // Pin the raw string. If the template changes shape (field
        // order, formatting, default values), these checks fire.
        assert!(
            raw.contains(r#"hash_id  = "yrbLx""#),
            "raw kb.toml missing hash_id line:\n{raw}"
        );
        assert!(raw.contains(r#"geometry = "voyager""#));
        assert!(raw.contains(r#"backend = "docker""#));
        assert!(raw.contains(r#"auto_pull       = "on_read""#));
        assert!(raw.contains(&format!("poll_interval_s = {DEFAULT_POLL_INTERVAL_S}")));
        assert!(raw.contains(&format!("warn_if_stale_s = {DEFAULT_WARN_IF_STALE_S}")));

        // Parse and pin the typed-shape view too.
        let parsed: KbToml = toml::from_str(&raw).expect("init template parses");
        assert_eq!(parsed.layout.hash_id.as_deref(), Some("yrbLx"));
        assert_eq!(parsed.layout.geometry, "voyager");
        assert_eq!(parsed.build.backend, BuildBackend::Docker);
        assert_eq!(parsed.sync.auto_pull, AutoPull::OnRead);
        assert_eq!(parsed.sync.poll_interval_s, DEFAULT_POLL_INTERVAL_S);
        assert_eq!(parsed.sync.warn_if_stale_s, DEFAULT_WARN_IF_STALE_S);
        // The template must pass the same cross-field validation
        // that `Project::load_at` enforces.
        parsed.validate().expect("template passes validation");
    }

    #[test]
    fn init_local_mode_template_round_trips_with_never_auto_pull() {
        // Local mode hardcodes `auto_pull = "never"` regardless of
        // the global default — local-mode projects have nothing to
        // pull, so the override is intentional. Same literal-value
        // discipline as the Oryx-mode test above: assert variants
        // by name, not against `default()`.
        use crate::schema::kb_toml::{AutoPull, BuildBackend, KbToml};
        let td = TempDir::new().unwrap();
        init_in(
            td.path(),
            &Args {
                hash: None,
                blank: true,
                geometry: "voyager".into(),
                name: Some("local-roundtrip".into()),
                no_skill: true,
                force: false,
            },
        )
        .unwrap();
        let raw = std::fs::read_to_string(td.path().join("kb.toml")).unwrap();
        assert!(raw.contains(r#"auto_pull = "never""#));
        assert!(raw.contains(r#"backend = "docker""#));
        assert!(raw.contains(r#"file = "layout.toml""#));

        let parsed: KbToml = toml::from_str(&raw).expect("local template parses");
        assert_eq!(parsed.sync.auto_pull, AutoPull::Never);
        assert_eq!(parsed.build.backend, BuildBackend::Docker);
        assert!(parsed.layout.hash_id.is_none());
        assert!(parsed.layout.local.is_some());
        parsed.validate().expect("template passes validation");
    }

    #[test]
    fn init_local_mode_creates_layout_toml() {
        let td = TempDir::new().unwrap();
        init_in(
            td.path(),
            &Args {
                hash: None,
                blank: true,
                geometry: "voyager".into(),
                name: Some("local-test".into()),
                no_skill: true,
                force: false,
            },
        )
        .unwrap();
        assert!(td.path().join("layout.toml").is_file());
        let layout = std::fs::read_to_string(td.path().join("layout.toml")).unwrap();
        assert!(layout.contains("position = 0"));
    }

    #[test]
    fn init_refuses_to_overwrite_without_force() {
        let td = TempDir::new().unwrap();
        std::fs::write(td.path().join("kb.toml"), "existing\n").unwrap();
        let err = init_in(
            td.path(),
            &Args {
                hash: None,
                blank: true,
                geometry: "voyager".into(),
                name: Some("x".into()),
                no_skill: true,
                force: false,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("refusing to overwrite"));
    }
}
