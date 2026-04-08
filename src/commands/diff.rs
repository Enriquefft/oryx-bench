//! `oryx-bench diff [REF]` — semantic diff vs git ref.
//!
//! Pipeline:
//! 1. Read the visual layout source (`pulled/revision.json` or
//!    `layout.toml`) at the given ref via `git show <ref>:<path>`.
//! 2. Parse both old + current into `CanonicalLayout`.
//! 3. Walk position-by-position and report changed bindings.
//! 4. Read `overlay/features.toml` at the same ref and report any
//!    structural differences (achordion timeouts, key_overrides, macros,
//!    config keys).
//!
//! No SVG, no HTML — just a text report. The `--layer` flag scopes the
//! visual diff to one layer.

use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

use anyhow::{bail, Context, Result};
use clap::Parser;

use crate::config::Project;
use crate::schema::canonical::CanonicalLayout;
use crate::schema::features::FeaturesToml;
use crate::schema::geometry;
use crate::schema::layout::LayoutFile;
use crate::schema::oryx;

#[derive(Parser, Debug)]
pub struct Args {
    /// Git reference to diff against (default: HEAD).
    pub git_ref: Option<String>,
    /// Limit visual layout diff to one layer (case-insensitive name).
    #[arg(long)]
    pub layer: Option<String>,
}

pub fn run(args: Args, project_override: Option<PathBuf>) -> Result<ExitCode> {
    let project = Project::discover(project_override.as_deref())?;
    let git_ref = args.git_ref.as_deref().unwrap_or("HEAD");

    if which::which("git").is_err() {
        bail!("`git` not on PATH — diff requires git");
    }

    // Visual layout: read both versions.
    let (old_layout, new_layout) = load_both_layouts(&project, git_ref)?;
    diff_layouts(&old_layout, &new_layout, args.layer.as_deref())?;

    // Overlay features.toml diff. A malformed historical features.toml
    // is a real bug — propagate the error rather than silently treating
    // it as empty (which would make the diff misleading).
    let old_features = match git_show(&project.root, git_ref, "overlay/features.toml")? {
        Some(s) => toml::from_str::<FeaturesToml>(&s)
            .with_context(|| format!("parsing overlay/features.toml at {git_ref}"))?,
        None => FeaturesToml::default(),
    };
    let new_features = FeaturesToml::load_or_default(&project.overlay_features_path())?;
    diff_features(&old_features, &new_features);

    Ok(ExitCode::from(0))
}

/// Load the canonical layout from both the git ref and the working tree.
fn load_both_layouts(
    project: &Project,
    git_ref: &str,
) -> Result<(CanonicalLayout, CanonicalLayout)> {
    if project.is_oryx_mode() {
        let old_raw = git_show(&project.root, git_ref, "pulled/revision.json")?
            .ok_or_else(|| anyhow::anyhow!("pulled/revision.json missing in {git_ref}"))?;
        let old_oryx: oryx::Layout = serde_json::from_str(&old_raw)
            .with_context(|| format!("parsing pulled/revision.json at {git_ref}"))?;
        let old = CanonicalLayout::from_oryx(&old_oryx)?;

        let cur_raw = std::fs::read_to_string(project.pulled_revision_path())?;
        let cur_oryx: oryx::Layout = serde_json::from_str(&cur_raw)?;
        let cur = CanonicalLayout::from_oryx(&cur_oryx)?;
        Ok((old, cur))
    } else if let Some(local_path) = project.local_layout_path() {
        let rel = local_path
            .strip_prefix(&project.root)
            .unwrap_or(&local_path)
            .display()
            .to_string();
        let old_raw = git_show(&project.root, git_ref, &rel)?
            .ok_or_else(|| anyhow::anyhow!("{rel} missing in {git_ref}"))?;
        let old_file: LayoutFile = toml::from_str(&old_raw)?;
        let old = CanonicalLayout::from_local(&old_file)?;

        let cur_raw = std::fs::read_to_string(&local_path)?;
        let cur_file: LayoutFile = toml::from_str(&cur_raw)?;
        let cur = CanonicalLayout::from_local(&cur_file)?;
        Ok((old, cur))
    } else {
        bail!("project kb.toml has neither hash_id nor [layout.local] file")
    }
}

/// Walk the layouts and print every position whose binding changed.
fn diff_layouts(
    old: &CanonicalLayout,
    new: &CanonicalLayout,
    only_layer: Option<&str>,
) -> Result<()> {
    let geom = geometry::get(new.geometry.as_str())
        .with_context(|| format!("unknown geometry '{}'", new.geometry))?;

    println!("== Visual layout diff ==");
    let mut any = false;

    // Iterate every layer present in either side.
    let mut layer_names: Vec<&str> = new.layers.iter().map(|l| l.name.as_str()).collect();
    for old_layer in &old.layers {
        if !layer_names.contains(&old_layer.name.as_str()) {
            layer_names.push(old_layer.name.as_str());
        }
    }
    layer_names.sort();

    for name in layer_names {
        if let Some(want) = only_layer {
            if !name.eq_ignore_ascii_case(want) {
                continue;
            }
        }
        let old_layer = old.layers.iter().find(|l| l.name == name);
        let new_layer = new.layers.iter().find(|l| l.name == name);

        match (old_layer, new_layer) {
            (None, Some(_)) => {
                println!("  + layer '{name}' (new)");
                any = true;
            }
            (Some(_), None) => {
                println!("  - layer '{name}' (removed)");
                any = true;
            }
            (Some(o), Some(n)) => {
                let count = o.keys.len().min(n.keys.len()).min(geom.matrix_key_count());
                for idx in 0..count {
                    let od = o.keys[idx].display();
                    let nd = n.keys[idx].display();
                    if od != nd {
                        let pos = geom.index_to_position(idx).unwrap_or("?");
                        println!("  ~ {name} {pos:>16}: {od:>15}  →  {nd}");
                        any = true;
                    }
                }
            }
            (None, None) => {}
        }
    }
    if !any {
        println!("  (no visual layout changes)");
    }
    println!();
    Ok(())
}

/// Diff features.toml entries. Set semantics for vec entries; key/value
/// for the [config] table.
fn diff_features(old: &FeaturesToml, new: &FeaturesToml) {
    println!("== overlay/features.toml diff ==");
    let mut any = false;

    let cfg_keys: std::collections::BTreeSet<&String> =
        old.config.keys().chain(new.config.keys()).collect();
    for k in cfg_keys {
        match (old.config.get(k), new.config.get(k)) {
            (Some(o), Some(n)) if o != n => {
                println!("  ~ [config] {k}: {o} → {n}");
                any = true;
            }
            (None, Some(n)) => {
                println!("  + [config] {k} = {n}");
                any = true;
            }
            (Some(o), None) => {
                println!("  - [config] {k} = {o}");
                any = true;
            }
            _ => {}
        }
    }

    diff_vec_by_display(
        "[[key_overrides]]",
        &old.key_overrides,
        &new.key_overrides,
        |ko| format!("{:?} {} → {}", ko.mods, ko.key, ko.sends),
        &mut any,
    );
    diff_vec_by_display(
        "[[macros]]",
        &old.macros,
        &new.macros,
        |m| format!("{}: {}", m.name, m.sends),
        &mut any,
    );
    diff_vec_by_display(
        "[[combos]]",
        &old.combos,
        &new.combos,
        |c| format!("{:?} → {}", c.keys, c.sends),
        &mut any,
    );
    diff_vec_by_display(
        "[[tapping_term_per_key]]",
        &old.tapping_term_per_key,
        &new.tapping_term_per_key,
        |t| format!("{}: {}ms", t.binding, t.ms),
        &mut any,
    );

    // Achordion: presence + a few headline fields.
    match (&old.achordion, &new.achordion) {
        (Some(o), Some(n)) => {
            if o.enabled != n.enabled {
                println!("  ~ [achordion] enabled: {} → {}", o.enabled, n.enabled);
                any = true;
            }
            if o.chord_strategy != n.chord_strategy {
                println!(
                    "  ~ [achordion] chord_strategy: {} → {}",
                    o.chord_strategy, n.chord_strategy
                );
                any = true;
            }
            if o.timeout.len() != n.timeout.len() {
                println!(
                    "  ~ [achordion] timeout entries: {} → {}",
                    o.timeout.len(),
                    n.timeout.len()
                );
                any = true;
            }
        }
        (None, Some(_)) => {
            println!("  + [achordion] enabled");
            any = true;
        }
        (Some(_), None) => {
            println!("  - [achordion] removed");
            any = true;
        }
        _ => {}
    }

    if !any {
        println!("  (no overlay changes)");
    }
}

/// Generic vec-by-display diff. Items are compared by their rendered
/// string form (`render(item)`); not order-sensitive.
fn diff_vec_by_display<T, F>(section: &str, old: &[T], new: &[T], render: F, any: &mut bool)
where
    F: Fn(&T) -> String,
{
    use std::collections::BTreeSet;
    let old_set: BTreeSet<String> = old.iter().map(&render).collect();
    let new_set: BTreeSet<String> = new.iter().map(&render).collect();
    for added in new_set.difference(&old_set) {
        println!("  + {section} {added}");
        *any = true;
    }
    for removed in old_set.difference(&new_set) {
        println!("  - {section} {removed}");
        *any = true;
    }
}

/// `git show <ref>:<path>` from `repo_root`. Returns `Ok(None)` if the
/// path doesn't exist at the given ref (e.g. file added since `ref`).
/// Read `<git_ref>:<rel_path>` from the repository at `repo_root`.
///
/// Returns `Ok(None)` if the *file* doesn't exist at that ref (new
/// file, file not yet present in history, etc.) and `Ok(Some(...))`
/// on success. Distinguishes three failure modes that
/// `git cat-file -e` alone would collapse together:
///
///   1. **Bad ref** (e.g. `HAED` typo): hard error with "unknown
///      revision" so the user gets a typo hint instead of "no
///      changes". Verified up front via `git rev-parse --verify
///      <ref>^{commit}` which fails with locale-independent exit
///      code 1 on any unrecognized rev.
///   2. **Ref is fine, file isn't a blob at that ref** (typical
///      "this file is new on this branch" case): `Ok(None)`.
///   3. **Ref is fine, path resolves to a non-blob** (e.g. a tree
///      because the user accidentally pointed at a directory): hard
///      error via `git cat-file -t` so we don't try to parse a tree
///      listing as TOML.
///
/// All git invocations use the existence-by-exit-code form, never
/// stderr parsing, so the helper is locale-independent. (The
/// previous version of this code matched English strings like
/// `"does not exist"` in `git show`'s stderr, which silently broke
/// under non-English git i18n.)
fn git_show(repo_root: &std::path::Path, git_ref: &str, rel_path: &str) -> Result<Option<String>> {
    // Step 1: validate the ref itself. `rev-parse --verify <ref>^{commit}`
    // resolves the ref to a commit object; non-zero exit means the
    // ref is not recognized at all (typo, wrong remote, etc.).
    let rev_parse = Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{git_ref}^{{commit}}"),
        ])
        .current_dir(repo_root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("invoking git rev-parse --verify {git_ref}^{{commit}}"))?;
    if !rev_parse.success() {
        bail!(
            "unknown git revision '{git_ref}' — check the spelling, or pass an explicit commit / tag / branch name"
        );
    }

    let spec = format!("{git_ref}:{rel_path}");

    // Step 2: existence + type probe. `cat-file -t` prints the
    // object type ("blob", "tree", "commit", "tag") on stdout if
    // the object exists, exits non-zero if not. We need both
    // because `cat-file -p` on a tree would print a tree listing
    // and we'd happily parse it as TOML/JSON further down.
    let type_out = Command::new("git")
        .args(["cat-file", "-t", &spec])
        .current_dir(repo_root)
        .stderr(Stdio::null())
        .output()
        .with_context(|| format!("invoking git cat-file -t {spec}"))?;
    if !type_out.status.success() {
        // The ref is valid (we just verified) but the path doesn't
        // exist at that ref. That's the "new file" case.
        return Ok(None);
    }
    let object_type = String::from_utf8_lossy(&type_out.stdout).trim().to_string();
    if object_type != "blob" {
        bail!(
            "{spec} resolves to a {object_type}, not a file — does the path point at a directory?"
        );
    }

    // Step 3: fetch the blob. Using `cat-file -p` (not `git show`)
    // so the output is the raw bytes without any smudging or
    // decoration applied by git's textconv filters.
    let out = Command::new("git")
        .args(["cat-file", "-p", &spec])
        .current_dir(repo_root)
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("invoking git cat-file -p {spec}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("git cat-file -p {spec} failed: {stderr}");
    }
    Ok(Some(String::from_utf8(out.stdout).with_context(|| {
        format!("blob at {spec} is not valid UTF-8")
    })?))
}
