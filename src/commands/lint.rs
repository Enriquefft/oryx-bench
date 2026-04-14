//! `oryx-bench lint` — run static analysis.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Result};
use clap::Parser;
use clap::ValueEnum;

use crate::config::Project;
use crate::lint::{self, Severity};

#[derive(Parser, Debug)]
pub struct Args {
    /// Fail on warnings as well as errors.
    #[arg(long)]
    pub strict: bool,

    /// Run only this rule.
    #[arg(long)]
    pub rule: Option<String>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    pub format: Format,

    /// Skip auto-pull.
    #[arg(long)]
    pub no_pull: bool,
}

#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum Format {
    Text,
    Json,
}

pub fn run(args: Args, project_override: Option<PathBuf>) -> Result<ExitCode> {
    let project = Project::discover(project_override.as_deref())?;

    // Single source of truth: the registry. Used for both --rule
    // validation and kb.toml `lint.ignore` validation so typos fail
    // loudly instead of silently reporting "0 issues".
    let known_rule_ids: HashSet<&'static str> =
        lint::rules::registry().iter().map(|r| r.id()).collect();

    if let Some(rule) = &args.rule {
        if !known_rule_ids.contains(rule.as_str()) {
            bail!(
                "unknown --rule '{rule}'. Known rules:\n  {}",
                sorted_rule_list(&known_rule_ids)
            );
        }
    }

    // Warn (don't error) on unknown rule IDs in `lint.ignore` — a
    // project-wide suppression is usually a deliberate act, but a
    // typo there silently un-suppresses the rule, which the old code
    // made invisible. Surfacing it keeps the user informed without
    // breaking a working CI pipeline over a config typo.
    for r in &project.cfg.lint.ignore {
        if !known_rule_ids.contains(r.as_str()) {
            eprintln!(
                "warning: kb.toml [lint] ignore contains unknown rule id '{r}' — it will not suppress anything"
            );
        }
    }

    if !args.no_pull && project.is_oryx_mode() {
        if let Err(e) = crate::pull::auto_pull(&project) {
            eprintln!("warning: auto-pull failed: {e:#}");
        }
    }

    let layout = project.canonical_layout()?;
    let mut issues = lint::run_all(&layout, &project)?;

    if let Some(rule) = &args.rule {
        issues.retain(|i| &i.rule_id == rule);
    }

    // Filter ignored rules from kb.toml.
    let ignore = &project.cfg.lint.ignore;
    issues.retain(|i| !ignore.iter().any(|r| r == &i.rule_id));

    let errors = issues
        .iter()
        .filter(|i| matches!(i.severity, Severity::Error))
        .count();
    let warnings = issues
        .iter()
        .filter(|i| matches!(i.severity, Severity::Warning))
        .count();
    let info = issues
        .iter()
        .filter(|i| matches!(i.severity, Severity::Info))
        .count();

    match args.format {
        Format::Text => {
            if issues.is_empty() {
                println!("{} No lint issues.", crate::util::term::OK);
            } else {
                for issue in &issues {
                    println!(
                        "  {}  [{}]  {}{}  — {}",
                        issue.severity.tag(),
                        issue.rule_id,
                        issue.layer.as_deref().unwrap_or("-"),
                        issue
                            .position_index
                            .map(|i| format!(":{i}"))
                            .unwrap_or_default(),
                        issue.message,
                    );
                }
            }
            println!(
                "\n{} error(s), {} warning(s), {} info",
                errors, warnings, info
            );
        }
        Format::Json => {
            let j = serde_json::to_string_pretty(&issues)?;
            println!("{j}");
        }
    }

    let strict_fail = project.cfg.lint.strict || args.strict;
    if errors > 0 {
        Ok(ExitCode::from(1))
    } else if strict_fail && warnings > 0 {
        Ok(ExitCode::from(2))
    } else {
        Ok(ExitCode::from(0))
    }
}

/// Return the known rule IDs sorted, joined by `"\n  "` so they line
/// up in a terminal error message. Kept separate from `run` so tests
/// can cover the formatting without spinning up a project.
fn sorted_rule_list(known: &HashSet<&'static str>) -> String {
    let mut ids: Vec<&&'static str> = known.iter().collect();
    ids.sort();
    ids.iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
        .join("\n  ")
}
