//! `not-pulled-recently` — `pulled/revision.json` mtime is older than
//! the user's `[sync] warn_if_stale_s` threshold. Oryx mode only.

use std::time::{Duration, SystemTime};

use crate::lint::{Issue, LintContext, LintRule, Severity};

pub struct Rule;

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "not-pulled-recently"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn description(&self) -> &'static str {
        "`pulled/revision.json` mtime is older than `[sync] warn_if_stale_s` (Oryx mode only — no-op in local mode)."
    }
    fn why_bad(&self) -> &'static str {
        "You may have edited in Oryx since the last pull. Local state could be stale."
    }
    fn fix_example(&self) -> &'static str {
        "`oryx-bench pull`."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        if !ctx.project.is_oryx_mode() {
            return Vec::new();
        }
        // Threshold is configured per-project via `[sync] warn_if_stale_s`
        // in kb.toml. Reading it here (instead of hardcoding 7 days as
        // the rule used to) means the user's `init`-time default of 1
        // day is what actually fires the rule, and a project that
        // explicitly raises the threshold doesn't get spurious info-level
        // noise.
        let threshold = Duration::from_secs(ctx.project.cfg.sync.warn_if_stale_s);
        let path = ctx.project.pulled_revision_path();
        let Ok(meta) = std::fs::metadata(&path) else {
            return Vec::new();
        };
        let Ok(modified) = meta.modified() else {
            return Vec::new();
        };
        let Ok(age) = SystemTime::now().duration_since(modified) else {
            return Vec::new();
        };
        if age > threshold {
            vec![Issue {
                rule_id: self.id().to_string(),
                severity: self.severity(),
                message: format!(
                    "pulled/revision.json is {} old — run `oryx-bench pull`",
                    humantime::format_duration(age)
                ),
                layer: None,
                position_index: None,
            }]
        } else {
            Vec::new()
        }
    }
}
