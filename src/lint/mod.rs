//! Lint trait, issue type, rule registry, and markdown generator.
//!
//! Each rule implements [`LintRule`] and is registered in
//! [`rules::registry`]. The `xtask gen-skill-docs` binary walks the
//! registry and emits `skills/oryx-bench/reference/lint-rules.md`.
//!
//! A lint rule is a pure function
//! `(CanonicalLayout, Project, FeaturesToml) -> Vec<Issue>`. No side
//! effects, no I/O beyond what the caller already loaded.

pub mod rules;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::config::Project;
use crate::schema::canonical::CanonicalLayout;
use crate::schema::features::FeaturesToml;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn tag(&self) -> &'static str {
        match self {
            Severity::Error => "error  ",
            Severity::Warning => "warning",
            Severity::Info => "info   ",
        }
    }
}

/// A single lint diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    #[serde(default)]
    pub layer: Option<String>,
    #[serde(default)]
    pub position_index: Option<usize>,
}

/// Context passed to each lint rule.
pub struct LintContext<'a> {
    pub layout: &'a CanonicalLayout,
    pub project: &'a Project,
    pub features: &'a FeaturesToml,
}

/// A single lint rule.
pub trait LintRule: Send + Sync {
    /// Stable identifier (e.g. "lt-on-high-freq"). Used in CLI output and kb.toml ignore lists.
    fn id(&self) -> &'static str;

    /// Default severity when the rule fires.
    fn severity(&self) -> Severity;

    /// One-sentence summary for the rule.
    fn description(&self) -> &'static str;

    /// Paragraph explaining the motivation.
    fn why_bad(&self) -> &'static str;

    /// Concrete remediation.
    fn fix_example(&self) -> &'static str;

    /// Run the rule.
    fn check(&self, ctx: &LintContext) -> Vec<Issue>;
}

/// Run every registered rule against the given project and layout, and
/// return all issues in registry order.
///
/// Returns an error if `overlay/features.toml` exists but is malformed.
/// A missing features.toml is *not* an error — the rules treat it as
/// "no overlay declared" and run their visual-layout checks normally.
pub fn run_all(layout: &CanonicalLayout, project: &Project) -> anyhow::Result<Vec<Issue>> {
    let features =
        FeaturesToml::load_or_default(&project.overlay_features_path()).with_context(|| {
            format!(
                "loading {} for lint",
                project.overlay_features_path().display()
            )
        })?;
    let ctx = LintContext {
        layout,
        project,
        features: &features,
    };
    let mut all = Vec::new();
    for rule in rules::registry() {
        all.extend(rule.check(&ctx));
    }
    Ok(all)
}

/// Generate the markdown body of `skills/oryx-bench/reference/lint-rules.md`
/// from the live registry. Called by `xtask gen-skill-docs`.
pub fn gen_lint_rules_markdown() -> String {
    let mut out = String::new();
    out.push_str(
        "# Lint rules\n\n\
         > **This file is GENERATED at build time** by the `xtask` binary from the\n\
         > registered rules in `src/lint/rules/`. Do not edit by hand — run\n\
         > `cargo xtask gen-skill-docs` to regenerate. CI verifies the file is\n\
         > up-to-date.\n\n\
         Each rule below has: ID, severity, what it catches, why it's bad, and the\n\
         recommended fix.\n\n---\n\n",
    );
    for rule in rules::registry() {
        out.push_str(&format!("### `{}`\n\n", rule.id()));
        out.push_str(&format!("**Severity**: {:?}\n\n", rule.severity()));
        out.push_str(&format!("**Catches**: {}\n\n", rule.description()));
        out.push_str(&format!("**Why bad**: {}\n\n", rule.why_bad()));
        out.push_str(&format!("**Recommended fix**: {}\n\n", rule.fix_example()));
        out.push_str("---\n\n");
    }
    out.push_str(
        "## How to add a rule\n\n\
         See `CONTRIBUTING.md`. Briefly:\n\n\
         1. Create `src/lint/rules/<rule_id>.rs` implementing `LintRule`\n\
         2. Register in `src/lint/rules/mod.rs::registry()`\n\
         3. Add positive + negative tests in `tests/lint_rules.rs`\n\
         4. Run `cargo xtask gen-skill-docs` — this file regenerates from the\n   registry\n\
         5. CI verifies the committed file matches the generator output\n",
    );
    out
}
