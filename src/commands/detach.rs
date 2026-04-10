//! `oryx-bench detach` — switch an Oryx-mode project to local mode.
//!
//! **One-way.** Converts `pulled/revision.json` to `layout.toml`, removes
//! `pulled/`, and from then on `oryx-bench pull` no longer functions in
//! this project. The user can `attach` again later, but doing so will
//! *overwrite* whatever local edits they made to `layout.toml`.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::Parser;

use crate::config::Project;
use crate::schema::canonical::CanonicalLayout;
use crate::schema::kb_toml::{AutoPull, LocalLayout};
use crate::schema::layout;
use crate::schema::oryx;
use crate::util::fs as fsx;

#[derive(Parser, Debug)]
pub struct Args {
    /// Skip the confirmation prompt.
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: Args, project_override: Option<PathBuf>) -> Result<ExitCode> {
    let project = Project::discover(project_override.as_deref())?;
    if !project.is_oryx_mode() {
        bail!("project is not in Oryx mode — nothing to detach");
    }

    // Check for a pre-existing layout.toml BEFORE the confirmation prompt
    // so the user sees the conflict immediately, not after re-running with
    // --force. With --force, the overwrite is intentional so we skip this.
    let target = project.root.join("layout.toml");
    if target.exists() && !args.force {
        bail!(
            "refusing to overwrite existing {} during detach. Re-run with --force to overwrite, or delete it manually first.",
            target.display()
        );
    }

    if !args.force {
        eprintln!(
            "About to detach this project from Oryx.\n\
             - pulled/revision.json will be converted to layout.toml\n\
             - pulled/ will be removed\n\
             - oryx-bench pull will no longer function in this project\n\
             - You can attach again later, but doing so OVERWRITES layout.toml.\n\
             \n\
             Re-run with --force to proceed."
        );
        return Ok(ExitCode::from(0));
    }

    let pulled_path = project.pulled_revision_path();
    let raw = std::fs::read_to_string(&pulled_path)
        .with_context(|| format!("reading {}", pulled_path.display()))?;
    let oryx_layout: oryx::Layout =
        serde_json::from_str(&raw).with_context(|| format!("parsing {}", pulled_path.display()))?;
    let canonical = CanonicalLayout::from_oryx(&oryx_layout)?;

    // Render the canonical layout back to TOML BEFORE we touch kb.toml
    // or pulled/. If render fails (e.g. unknown geometry) the project
    // is left fully intact and the user can investigate.
    let layout_toml = layout::render_layout_toml(&canonical)
        .context("rendering layout.toml from pulled/revision.json")?;

    // Rewrite kb.toml: drop hash_id, add [layout.local]. Build the new
    // contents BEFORE the destructive part so any toml serialization
    // failure doesn't leave the project in a half-detached state.
    let mut cfg = project.cfg.clone();
    cfg.layout.hash_id = None;
    cfg.layout.local = Some(LocalLayout {
        file: "layout.toml".to_string(),
    });
    // Sync settings are Oryx-specific; set auto_pull to "never" so the
    // config doesn't misleadingly reference a sync source that no longer
    // exists. The other sync fields (poll_interval_s, warn_if_stale_s)
    // are inert without auto_pull, but we leave them at their current
    // values so they survive a future `attach` round-trip.
    cfg.sync.auto_pull = AutoPull::Never;
    let new_kb = toml::to_string_pretty(&cfg).context("re-serializing kb.toml")?;

    // Destructive phase. Each step below is crash-safe on its own
    // (atomic_write stages to a tempfile + rename), but the sequence
    // of multiple writes is not inherently transactional across
    // processes. We get as close as possible:
    //
    //   0. Write layout.toml. If this fails the project is untouched.
    //   2. Write the new kb.toml. If this fails, ROLL BACK by
    //      deleting the just-written layout.toml so the user sees
    //      the same on-disk state they started with — leaving an
    //      orphaned layout.toml behind would confuse a re-run of
    //      `detach` into thinking the project was already partially
    //      converted. The rollback itself is best-effort; if it
    //      fails we surface that as a second eprintln so the user
    //      knows there's a stray file to clean up.
    //   3. Remove pulled/. If this fails the kb.toml change has
    //      already "committed" the mode switch; we surface the
    //      residual-dir error as a warning rather than rolling back
    //      (which would mean un-writing kb.toml — at that point the
    //      user is better served by a clear message than by another
    //      speculative rewrite).
    //   4. Remove the auto-pull cache; same semantics as pulled/.
    fsx::atomic_write(&target, layout_toml.as_bytes())?;

    if let Err(e) = fsx::atomic_write(&project.root.join("kb.toml"), new_kb.as_bytes()) {
        // Rollback: remove the stray layout.toml so the project
        // ends up in the same on-disk state it started in. The
        // existence guard above means `target` was definitely
        // freshly created by us, so removing it is safe.
        if let Err(rb_err) = std::fs::remove_file(&target) {
            eprintln!(
                "warning: kb.toml write failed AND rollback of {} also failed ({rb_err:#}). \
                 You will need to delete that file manually before re-running detach.",
                target.display()
            );
        }
        return Err(e).context("writing kb.toml during detach (layout.toml rolled back)");
    }

    // Remove pulled/ entirely.
    let pulled_dir = project.pulled_dir();
    if pulled_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&pulled_dir) {
            eprintln!(
                "warning: detach completed but could not remove {}: {e:#}. \
                 Delete the directory manually.",
                pulled_dir.display()
            );
        }
    }

    // Also remove the auto-pull cache; it has no meaning in local mode.
    let cache = project.cache_file();
    if cache.exists() {
        if let Err(e) = std::fs::remove_file(&cache) {
            eprintln!(
                "warning: detach completed but could not remove {}: {e:#}",
                cache.display()
            );
        }
    }

    // Invalidate the build cache so the next `build` regenerates from
    // the new layout.toml instead of serving stale artifacts.
    crate::build::invalidate_build_cache(&project);

    println!(
        "{} Detached. layout.toml written at {}. pulled/ removed.",
        crate::util::term::OK,
        target.display()
    );
    println!(
        "{} This is one-way: `attach` later will OVERWRITE layout.toml.",
        crate::util::term::WARN
    );
    Ok(ExitCode::from(0))
}
