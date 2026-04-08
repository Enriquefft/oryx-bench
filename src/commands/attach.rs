//! `oryx-bench attach --hash <H>` — switch a local-mode project to Oryx mode.
//!
//! Refuses unless `--force` if `layout.toml` has uncommitted git changes
//! (or if we can't determine the working-tree state at all). The check
//! is *fail-closed*: if `git` is missing or the directory isn't a git
//! repo, attach refuses without `--force` so we can't silently destroy
//! uncommitted work.
//!
//! Crash safety: pull happens *before* `layout.toml` is removed and
//! kb.toml is rewritten. If the pull fails, kb.toml and layout.toml are
//! both untouched and the user can retry without data loss.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::Parser;

use crate::config::Project;
use crate::pull::{self, PullOutcome};
use crate::schema::kb_toml::KbToml;
use crate::util::{
    fs as fsx,
    git::{self, WorkingTreeState},
};

#[derive(Parser, Debug)]
pub struct Args {
    /// The Oryx layout hash to attach to.
    #[arg(long)]
    pub hash: String,

    /// Skip the working-tree-clean safety check. Required when
    /// `layout.toml` has uncommitted changes OR when the directory
    /// isn't a git repo (so we can't tell whether your work is
    /// committed).
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: Args, project_override: Option<PathBuf>) -> Result<ExitCode> {
    let project = Project::discover(project_override.as_deref())?;
    if project.is_oryx_mode() {
        bail!(
            "project is already in Oryx mode (hash {})",
            project.cfg.layout.hash_id.as_deref().unwrap_or("?")
        );
    }
    if !project.is_local_mode() {
        bail!("project kb.toml is neither Oryx nor local mode — broken state");
    }

    // Fail-closed safety check. If we can't determine the working tree
    // state we refuse without `--force` — never silently destroy data.
    if !args.force {
        let state = git::working_tree_state(&project.root, "layout.toml")?;
        match state {
            WorkingTreeState::Clean => {}
            WorkingTreeState::Dirty => bail!(
                "layout.toml has uncommitted changes that would be overwritten by attach. \
                 Commit/stash them first or pass --force."
            ),
            WorkingTreeState::NotARepo => bail!(
                "{} is not inside a git repository — cannot verify layout.toml is committed. \
                 Initialize a repo first (`git init && git add . && git commit -m init`) \
                 or pass --force to skip the check.",
                project.root.display()
            ),
        }
    }

    // Construct (but don't write yet) the post-attach kb.toml. We need
    // a temporarily-loaded "would-be" project to feed into pull_now,
    // because pull needs to read [layout] hash_id which the
    // pre-attach kb.toml doesn't have.
    let new_kb = rewrite_kb_toml_for_attach(&project, &args.hash)?;

    // Stage the new kb.toml under .oryx-bench/build/staging-kb.toml so
    // we never write to the real kb.toml until after the pull succeeds.
    let staging_dir = project.cache_dir().join("attach-staging");
    fsx::ensure_dir(&staging_dir)?;
    let staging_kb_path = staging_dir.join("kb.toml");
    fsx::atomic_write(&staging_kb_path, new_kb.as_bytes())?;
    fsx::ensure_dir(&staging_dir.join("pulled"))?;

    // Build a Project rooted at staging_dir so the pull writes into
    // .oryx-bench/build/attach-staging/pulled/revision.json. The
    // post-attach project still uses the real root for everything else.
    let staged =
        Project::load_at(&staging_dir).context("loading staged kb.toml for pull pre-flight")?;
    let outcome = pull::pull_now(&staged, None, true).context(
        "initial pull failed — attach was aborted; layout.toml and kb.toml were not modified",
    )?;

    // Pull succeeded. Now do the destructive part atomically:
    //   1. Move staged pulled/revision.json into the real project's pulled/
    //   2. Replace kb.toml with the new content
    //   3. Remove layout.toml
    fsx::ensure_dir(&project.pulled_dir())?;
    let staged_revision = staging_dir.join("pulled/revision.json");
    let real_revision = project.pulled_revision_path();
    let bytes = std::fs::read(&staged_revision)
        .with_context(|| format!("reading {}", staged_revision.display()))?;
    fsx::atomic_write(&real_revision, &bytes)?;

    fsx::atomic_write(&project.root.join("kb.toml"), new_kb.as_bytes())?;

    let layout_toml = project.root.join("layout.toml");
    if layout_toml.exists() {
        std::fs::remove_file(&layout_toml)
            .with_context(|| format!("removing {}", layout_toml.display()))?;
    }

    // Clean up staging.
    std::fs::remove_dir_all(&staging_dir)
        .with_context(|| format!("removing {}", staging_dir.display()))?;

    println!(
        "{} Attached to Oryx hash '{}'. Pull result: {outcome:?}",
        crate::util::term::OK,
        args.hash
    );
    println!(
        "{} layout.toml has been removed. The visual layout now lives under pulled/.",
        crate::util::term::WARN
    );
    Ok(ExitCode::from(0))
}

fn rewrite_kb_toml_for_attach(project: &Project, hash: &str) -> Result<String> {
    // Re-emit the kb.toml from the typed config + the new hash. Loses
    // any handwritten comments — we document this in the help text.
    let mut cfg: KbToml = project.cfg.clone();
    cfg.layout.hash_id = Some(hash.to_string());
    cfg.layout.local = None;
    toml::to_string_pretty(&cfg).context("re-serializing kb.toml")
}

// Suppress an unused-import warning for `PullOutcome` (it's referenced
// only in the `{outcome:?}` debug format above).
#[allow(dead_code)]
fn _force_pull_outcome_link(_: PullOutcome) {}
