# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Pre-1.0: minor versions may contain breaking changes.

## [Unreleased]

### Fixed (Phase 2 post-review pass — 22 reviewer findings)

After Phase 2 landed, a strict reviewer agent ran against P2.1–P2.4
and surfaced 22 follow-on items: 2 critical correctness bugs, 12
"important" gaps (4 of which were missing-test-coverage on
critical-path code), and 8 nits. Fixed all 22 in 5 batches with
`cargo test` between batches.

**Critical correctness bugs:**
- **Mid-response IO errors no longer bypass the retry loop.** Previously, a TCP reset / TLS truncation / gzip EOF *after* `send()` succeeded but *during* `read_to_end` would propagate via `?` directly out of the retry loop, breaking the "bounded retry on transient failures" claim. Restructured `read_capped_body` to return a typed `BodyReadError::{Io, TooLarge, NotUtf8}` so `do_post` can reclassify mid-stream IO errors as `PostAttempt::Retriable`, while still treating "too large" and "not UTF-8" as hard failures (where retry can't help).
- **`tt_too_short` description vs code semantic mismatch.** Description said "at or below the disambiguation threshold" but code used strict `<`. A user with `tapping_term_ms = 150` would read the description, expect the rule to fire, and get silence. Description rewritten to "strictly below the 150ms disambiguation minimum"; the comment in `fix_example` now explicitly explains why 180ms is the recommendation (margin of safety above the 150ms minimum) so the apparent inconsistency between threshold and recommendation is documented.

**Important gaps:**
- **`parse_retry_after` no longer silently swallows garbage.** Previously, any unparseable header value (HTTP-date form, "garbage-from-a-load-balancer", random whitespace) returned `Some(Duration::from_secs(5))`, treating junk as a valid "wait 5s" signal. Now returns `None` for anything that isn't a delta-seconds integer, letting the caller fall back to its own backoff schedule. Adding a real HTTP-date parser is left for if Oryx ever actually emits HTTP-date Retry-After values.
- **`Retry-After: 0` no longer creates a hot retry loop.** `do_post` now clamps the parsed value to `BACKOFF_BASE.max(...)` so a misbehaving server sending `Retry-After: 0` waits at least 200ms between attempts. Combined with `MAX_ATTEMPTS = 3`, the worst case is bounded.
- **Backoff jitter is now actually random.** Previously used `SystemTime::now().subsec_nanos() % base_ms` which produced clock-correlated values across NTP-synced hosts (same wallclock → same modulo result, undermining the anti-thundering-herd guarantee). Now uses `rand::thread_rng().gen_range(0..base_ms)`, which on first use seeds itself from the OS RNG. Added `rand = "0.8"` as a direct dep.
- **`do_post`'s 500-not-retried branch documented.** A 5-line comment explains *why* 500 is excluded from the 502/503/504 retry list (Oryx 500s are real server-side bugs that the next retry will hit again; 500 should fail fast and surface as a data error). Pinned by the existing `pull_now_surfaces_http_500` test.
- **`warn_if_stale_s = 0` and `poll_interval_s = 0` rejected at config-load time.** New `KbToml::validate()` method called from `Project::load_at`. Previously `warn_if_stale_s = 0` would make the `not-pulled-recently` lint fire on every single run with no documented "0 = disabled" semantic; `poll_interval_s = 0` would let auto-pull hammer Oryx with no rate limit. Both error with a user-actionable message explaining what to set instead. 3 new unit tests pin the validation.
- **`large_firmware` rule no longer silent on unknown geometry.** Previously a no-op when `geometry::get(...)` returned `None`. Now surfaces a `Severity::Info` issue ("skipped: cannot determine flash budget for unknown geometry '{name}'") so the user can tell why the rule didn't run.
- **`large_firmware` threshold now `WARNING_HEADROOM_BYTES = 4 KB`** instead of `WARNING_THRESHOLD_FRACTION = 0.9375`. Encoded as an absolute byte headroom rather than a fraction so the warning point doesn't silently drift if a future board's flash size isn't a multiple of 64 KB.
- **`flash`/`init` error messages list supported geometries from the registry.** New `geometry::supported_slugs()` helper projects the `REGISTRY` into a sorted comma-separated string. Used in `init`, `flash::run`, and `flash::check_firmware_is_fresh` so the list of valid geometries lives in exactly one place — adding a new board no longer requires editing three error message strings.
- **`flash::project_geometry` helper extracts the registry-lookup boilerplate** out of `run` and `check_firmware_is_fresh`, eliminating the duplication the reviewer flagged.
- **Critical-path test coverage gaps closed:**
  - `pull_now_rejects_oversized_response_body` — wiremock returns a 6 MB body, asserts the `ResponseTooLarge` error path. Pins the 5 MB cap end-to-end.
  - `pull_now_honors_retry_after_then_succeeds` — wiremock returns 429 with `Retry-After: 1`, then 200 on the next attempt; asserts the happy path.
  - `read_cache_treats_corrupt_file_as_default_with_warning` and `read_cache_missing_file_is_silent_default` — pin both branches of the P2.1 corruption-vs-not-found distinction.
  - `local_revision_hash_propagates_parse_error` and `local_revision_hash_missing_file_is_ok_none` — pin the P2.1 Result-returning behavior so a future refactor back to `.ok()?` is caught by CI.
  - `init_oryx_mode_template_round_trips_through_kb_toml_with_typed_defaults` and `init_local_mode_template_round_trips_with_never_auto_pull` — write the init template, parse it back through `KbToml`, and assert each typed default field matches. Future changes to `AutoPull::default()` or `BuildBackend::default()` that don't propagate into the template will fail this test instead of silently shipping the wrong config.
- **`flash::detect_backend` now testable via `Environment` trait.** New `Environment::wally_on_path()` abstracts the `which::which` probe; `RealEnvironment` is the production impl, and tests pass a `StubEnv { wally_present: bool }` to cover the wally-not-installed path that was untestable before. 4 new unit tests cover all 4 BackendChoice variants × wally-present/absent.

**Nits cleaned up:**
- `tt_too_short` `fix_example` adds a `// Recommended is intentionally HIGHER than the 150ms threshold...` comment tying the 180ms minimum recommendation to the 200–220ms sweet spot.
- `serde_json::from_str` for the GraphQL response body now has `.context("parsing Oryx GraphQL response body as JSON")` so the error chain says where the parse failed.
- `util::http::USER_AGENT` is now a `const &'static str` via `concat!("oryx-bench/", env!("CARGO_PKG_VERSION"))` — compile-time evaluated, no allocation. Replaces the `format!()`-in-OnceCell pattern.
- `WARNING_HEADROOM_BYTES` (4 KB) replaces `WARNING_THRESHOLD_FRACTION` in `large_firmware` (see above).
- `AutoPull` schema now uses bare `#[serde(default)]` (relying on the `Default` derive's `#[default] OnRead`) instead of the redundant `#[serde(default = "default_auto_pull")]` + free function. Single source of truth for the default lives on the enum itself.
- Extracted `flash::project_geometry` helper (see above).
- Renamed `flash_backend_unknown_value_bails` → `flash_backend_clap_rejects_unknown_variant` to match the new semantic (parse-time clap rejection rather than runtime "unknown" branch).

**Test count: 236** (up from 220 at the end of Phase 2). +16 new tests across the post-review pass: 2 wiremock (oversized body, Retry-After 429), 4 cache/revision corruption regressions, 2 init template content round-trips, 4 flash environment-stub tests, 3 kb_toml validation tests, plus the 1 backoff_grows update for the new RNG. Clippy clean. Fmt clean. `cargo test --workspace` green from a fresh checkout.

### Fixed (Phase 2 of audit cleanup — 67 "important" items across 8 batches)

Phase 2 followed Phase 1's 27-critical-item pass and cleaned up the
"important" tier of audit findings: silent error swallowing still
present in a few corners, HTTP client production-readiness, stringly
-typed config that should be enums, SSoT drift between hardcoded
constants and the schema defaults, lint-rule false positives and
jargon leakage, codegen edge cases, command UX polish, and test
hygiene. Eight batches, review points between, no regressions.

**P2.1 — Silent error cleanup:**
- `pull::read_cache` now distinguishes `NotFound` (silent) from "cache file is corrupt" (`tracing::warn!`). The previous `.ok().unwrap_or_default()` chain made flaky-network reproductions invisible.
- `pull::local_revision_hash` returns `Result<Option<String>>` — parse errors on `pulled/revision.json` propagate with `.context("parsing ...")` instead of looking like "no local hash" and triggering an immediate overwrite.
- `commands::diff` parses historical `features.toml` via `.with_context(...)` instead of `unwrap_or_default`ing a malformed blob into an empty one.
- `build::input_sha` walkdir errors propagate via `with_context` instead of `filter_map(|e| e.ok())`; silent drops would produce a sha that says "inputs stable" while they actually contained unreadable files, leading to false cache hits and stale firmware.
- `lint::rules::process_record_user_collision` surfaces walkdir + read errors as `Severity::Info` issues instead of silently dropping them via `into_iter().flatten()`.
- `generate::rules_mk` walkdir errors propagate with `.with_context(...)`.

**P2.2 — GraphQL client production-readiness:**
- `util::http::client` now splits `connect_timeout = 5s` from `total_timeout = 15s` so a DNS black-hole fails fast without waiting out the full request budget.
- User-Agent header set to `oryx-bench/<crate-version>` so Oryx operators can correlate server-side errors with client versions.
- `reqwest` gzip feature enabled so the full-layout response compresses on the wire when Oryx supports it.
- `MAX_RESPONSE_BYTES = 5 MiB` cap enforced in `pull::graphql::read_capped_body` via `take(MAX+1).read_to_end()`. Prevents OOM from a buggy/malicious server. New `PullError::ResponseTooLarge` variant.
- Bounded retry with exponential backoff + jitter (max 3 attempts, `BACKOFF_BASE = 200ms`) for 502/503/504 and transient connect/timeout errors. GraphQL queries are read-only and idempotent so retrying is safe. Non-retriable failures (500, 4xx) short-circuit.
- `Retry-After` on 429 honored, capped to 60s so a misbehaving server can't hang the CLI for hours. Supports delta-seconds form; HTTP-date form falls back to a short fixed wait.
- New wiremock tests: `pull_now_retries_transient_503_then_succeeds` (retry path), `pull_now_gives_up_after_persistent_503` (exhaustion path), plus 5 unit tests for `parse_retry_after` and `backoff_delay` growth.

**P2.3 — Stringly-typed config → typed enums:**
- `Achordion.chord_strategy: String` → `ChordStrategy::{OppositeHands, Always, Never}` with `#[serde(rename_all = "snake_case")]` + Display. A typo like `"oppositehands"` now fails at `features.toml` parse time with `unknown variant 'oppositehands', expected one of 'opposite_hands', 'always', 'never'`. The previous `match` on `.as_str()` silently fell through to the default for any typo, so the user thought they'd set `"always"` and got opposite-hands resolution instead.
- `Build.backend: String` → `BuildBackend::{Docker, Auto, Native, Nix}`. Same typo-rejection property (`"dockre"` now fails at kb.toml parse). Native and Nix are reserved variants that the dispatcher rejects with a pointer to `docker`, so users can pin them forwards-compatibly.
- `flash --backend: String` → `BackendChoice` clap `ValueEnum` with `Auto`/`Wally`/`Keymapp`. `--backend dfu-util` now fails at argument parse with `[possible values: auto, wally, keymapp]` instead of reaching a runtime "unknown backend" branch. Concrete `Backend` (Wally/Keymapp) kept distinct because `Auto` is not a strategy — it's resolved by `detect_backend`.
- `AutoPull` gained `Default + Display + as_str()`. Used by the init template (P2.4) so the emitted `kb.toml` references the typed default instead of a hardcoded `"on_read"` literal.

**P2.4 — SSoT extractions:**
- `Geometry::usb_vendor_id() -> &'static str` and `flash_budget_bytes() -> u64` added to the trait. Voyager returns `"0x3297"` / `64 * 1024`. `flash::plan` now pulls target name + vendor ID from the trait (no more hardcoded `"0x3297"` in the flash plan); `large_firmware` lint rule pulls the budget from the trait and uses `WARNING_THRESHOLD_FRACTION = 0.9375` instead of a hardcoded `60 * 1024`. Adding a second board now requires zero changes to the flash/lint paths.
- `not_pulled_recently` lint rule reads `ctx.project.cfg.sync.warn_if_stale_s` instead of a hardcoded 7-day constant. Matches the kb.toml schema default (1 day) and respects per-project overrides.
- `kb_toml::DEFAULT_POLL_INTERVAL_S` and `DEFAULT_WARN_IF_STALE_S` made public. `commands::init` kb.toml template interpolates them (plus `BuildBackend::default()` and `AutoPull::default()`) instead of hardcoding `"60"`/`"86400"`/`"docker"`/`"on_read"` literals. Changing the schema default now flows automatically into fresh-init templates.
- `tt_too_short` lint rule split into `TAPPING_TERM_MINIMUM_MS = 150` (threshold at which the rule fires) and `TAPPING_TERM_RECOMMENDED_MS = 180` (what the message says to increase to). Previously the threshold said 150ms but the message recommended ≥180ms without explaining the 30ms buffer.

**P2.5 — Lint rule polish:**
- `unused-feature-flag` now walks *every* enabled flag in `[features]`, not just a hardcoded `key_overrides`/`combos` pair. A user who set `macros = true` with no `[[macros]]` entries previously got a silent empty compile. New `flag_is_unused` lookup table is the single source of truth for which flags have corresponding declarative sections. Non-declarative flags (`mouse_keys`, `rgb_matrix`, etc.) return `None` so they don't fire false positives.
- `custom-keycode-undefined` now honors Tier 2 overlay files: `overlay/*.zig`, `overlay/*.c`, `overlay/*.h` are scanned for `USER<nn>` literal tokens and those slots are treated as "defined" in addition to the `[[macros]]` entries. The previous version falsely errored when a user legitimately dispatched a custom keycode from Zig — exactly the case the rule's own `fix_example` documents as valid. New `extract_user_tokens` helper with word-boundary check (so `MY_USER00` doesn't match) + digit-normalization (so `USER5` is recognized as `USER05`).
- `process-record-user-collision` now has Zig-syntax test coverage: `export fn process_record_user(...)` and its multi-line variant are both detected, `extern fn` declarations correctly ignored. The scanner works for Zig because trimmed lines with `fn process_record_user(` pass the whitespace-prefix check.
- "Path A" jargon removed from 6 lint rule `fix_example` strings (`kc-no-in-overlay`, `home-row-mods-asymmetric`, `layer-name-collision`, `unknown-layer-ref`, `orphaned-mod-tap`, `unreachable-layer`). Each rule's fix text is now self-contained — a user who hasn't read the skill's SKILL.md can act on it. The Path A/Path B vocabulary remains in the skill docs as defined concepts.
- `CanonicalAction` and `LayerRef` now derive `PartialEq, Eq`. Unblocks future equality checks in diff and lint logic.
- `Keycode::KcRgbHuiHue` renamed to `KcRgbHueUp` for naming consistency with its sibling `KcRgbHueDown`. `"RGB_HUI"` and `"RGB_HUE_INCREASE"` still parse to this variant; emit form is unchanged.
- `lint --rule <id>` now validates against `rules::registry()` and bails with the full list of known rule IDs on typo. `lint.ignore` in kb.toml warns (doesn't error) on unknown IDs — a project-wide suppression is usually deliberate but a typo silently un-suppresses the rule, which the old code made invisible.

**P2.6 — Codegen edge cases + emit fallbacks:**
- `escape_c_string` rewritten to handle `\r`, `\0` (NUL — was truncating C literals at compile time), and other ASCII control characters (`<0x20` / `0x7F` as `\xNN`). UTF-8 passes through unchanged. 5 new unit tests pin the behavior.
- `FeaturesToml::tapping_term_ms()` now returns `Result<Option<u32>>` and rejects values outside `1..=65535` with a clear "out of range" error instead of silent `as u32` wrapping. Non-integer values (`tapping_term_ms = "oops"`) error with `must be an integer, got ...`. 5 new unit tests.
- `config_h::emit_config_h` now validates known-typed keys up front via `features.tapping_term_ms()?` so a bad value fails at generation time (with a line pointer) rather than deep inside QMK's compile.
- `keymap::emit_action` and `emit_key` now return `Result<String>`. `resolve_layer` returns `Err` when the layer ref is unknown — previously fell back to the raw user-facing name which would blow up inside QMK with `'Sym+Num' undeclared`. Errors cascade through `emit_keymap_c`, `emit_keymaps_array`, `generate_all`. New tests assert the error paths: `emit_action_unknown_layer_name_errors`, `emit_action_unknown_layer_index_errors`, `emit_action_custom_falls_back_to_user_literal`.
- `CanonicalAction::Custom(n)` fallback changed from `KC_NO /* missing CK for USERnn */` to the QMK-native `USER<nn>` literal. QMK declares these in its `keycodes.h` so Tier 2 overlay code can dispatch on them without needing our generated enum. The previous fallback silently replaced the key with "does nothing" which meant pressing a Tier-2-handled custom keycode produced no feedback.
- `translate_binding` returns `Result` with `.with_context("translating features.toml binding '...'")` so users get the offending binding in the error. Cascaded through `emit_achordion`, `emit_tapping_term_per_key`.
- `rules_mk.rs` module docs no longer claim Zig support — admitting v0.1 reality (Zig files are scanned by lint but not compiled). Full Zig wiring is tracked as a separate task.
- Removed the `_keep_keycode_in_scope` dead helper and its `#[allow(dead_code)]` attribute in `generate/keymap.rs`. Replaced the top-level `use crate::schema::keycode::Keycode` with test-scoped imports.

**P2.7 — Command UX improvements:**
- `commands::diff::git_show` now uses `git cat-file -e` for the existence probe (exit-code signal, locale-independent) and `git cat-file -p` for the read (raw blob, no smudging). The previous implementation matched English strings `"does not exist"` / `"exists on disk"` in `git show`'s stderr, which silently broke under any non-English git i18n.
- `commands::detach` now has explicit rollback: if the kb.toml write fails after layout.toml was written, delete the stray layout.toml so the user sees the same on-disk state they started with. `pulled/` and cache removal errors downgrade to `eprintln!` warnings rather than leaving a half-detached project.
- `render::ascii::truncate_with_ellipsis` — cells wider than 7 chars now show `…` as the last character to signal truncation. The previous renderer silently cut characters off the right edge, making `LT(Sym+Num, KC_BSPC)` and `LT(Sym+Num,` look identical. 4 new unit tests; 5 render snapshots regenerated.
- `skill::install_global` / `remove_global` now use `directories::BaseDirs::home_dir()` instead of `std::env::var_os("HOME")`. Works on Windows (`%USERPROFILE%`), macOS, and Linux. The previous version errored on Windows with "HOME not set".
- `ProjectError::InvalidConfig` no longer double-prints the source error. `{source}` in the format string + `#[source]` attribute was causing anyhow's chain-aware `:#` formatter to print the underlying toml error twice. Comment explains the reasoning.

**P2.8 — Test hygiene:**
- `flash::keymapp` split: `stage` (public, uses `ProjectDirs` cache) and `stage_into(dest_dir)` (lower-level, takes explicit dest). Test rewritten to use two `TempDir`s so it never touches the real user cache — the previous `stage_writes_firmware_to_cache` test wrote into `~/.cache/oryx-bench/firmware.bin` and could clobber a user's actual staged firmware during `cargo test`.
- `ARCHITECTURE.md` no longer references the non-existent `src/render/svg.rs`. The tree diagram and the "SVG rendering shells out to keymap-drawer" paragraph now explicitly mark that code as v0.2+ future work, matching v0.1 reality where only `render::ascii` exists.

**Phase 2 discoveries filed as separate tasks:**
- **FULL_QUERY combos selection mismatch against live Oryx schema** — discovered during P2.3 end-to-end testing. Our GraphQL `FULL_QUERY` requests `combos` as a scalar, but the live Oryx endpoint now returns it as a `Combo` object requiring a sub-selection. Any real pull against `oryx.zsa.io` currently fails with `Field must have selections`. Must fix before v0.1 ships. Tracked as a separate task.
- **Zig Tier 2 end-to-end wiring** — `rules_mk.rs` module docs (P2.6) admitted that Zig files are scanned by lint but never actually compiled. Proper wiring requires updating the docker backend to invoke `zig cc` on each `overlay/*.zig` and appending the resulting `.o` files to QMK's link step. Tracked as v0.2+ work.

**Test count: 220** (up from 180 at the end of Phase 1, up from 169 at the end of the quality-bar pass). `cargo test` green end-to-end. Phase 2 added 40 new unit and integration tests across the 8 batches. Clippy clean. Fmt clean.

### Fixed (Phase 1 of audit cleanup — 27 critical items)

**Codegen compile bugs (would have failed to compile any real project):**
- `enum custom_keycodes` is now emitted in `_features.h` instead of `keymap.c`, so both translation units see the same custom-keycode IDs. The previous arrangement linked-failed any project with `[[macros]]` defining USERnn slots.
- `with_kc_prefix` rewritten as `normalize_keycode_token` that recurses into paren wrappers. The fixture's `S(GRAVE)` key override now correctly emits `S(KC_GRAVE)` instead of the broken `S(GRAVE)`.
- `mods_to_qmk` validates modifier tokens against the QMK accept set; typos like `LSHF` are rejected at codegen time.
- `emit_combos` now returns `Result` and errors loudly on unknown layer / unknown position / unbound keycode instead of silently writing `// skipped` comments into the C source.
- The codegen layer is now the *single* owner of every file the build pipeline stages. Removed the `FEATURES_HEADER` constant from `build/docker.rs` — `_features.h` is generated by `features::emit_features_h`.

**Data-loss safety gaps:**
- `git::has_uncommitted_changes` is gone. Replaced with `git::working_tree_state` returning `Clean | Dirty | NotARepo` and propagating real errors. `attach` now refuses to touch a non-git directory without `--force` (the previous fail-open behavior could silently delete `layout.toml`).
- `attach` reordered: pull happens *first* into `.oryx-bench/build/attach-staging/`. Only after the pull succeeds is the staged `revision.json` swapped into the real `pulled/`, kb.toml replaced, and `layout.toml` removed. A failed pull now leaves the project fully intact instead of half-attached.
- `render_layout_toml` returns `anyhow::Result<String>` instead of a literal `"# unknown geometry — could not render\n"` placeholder. `detach` now errors cleanly when the geometry is unknown instead of writing the error string as a real layout.toml and then deleting `pulled/`.
- `detach` cleans up the auto-pull cache via `with_context` propagation instead of `let _ = remove_file(...)`.

**Stale milestone scaffolding leaking to user output:**
- Removed `(M2)`, `(M3)` parentheticals from clap help text — they were visible in `oryx-bench --help`.
- Removed "Implemented in task #6" / "task #8" docstring leaks from `cli.rs` and `xtask/src/main.rs`.
- `render/mod.rs` no longer says "M2+" for SVG.
- `build/mod.rs` says "not supported in v0.1" instead of "M5+ work".
- `util/toolchain.rs` says "future native backend" instead of "v0.2+".

**Lying flags removed:**
- `build --release` parsed-and-ignored flag deleted.
- `flash --no-pull` no-op flag deleted (replaced with `flash --force` for the actually-meaningful "skip the freshness check" semantics).
- `setup --full` now actually invokes each detected tool with its appropriate version flag (`--version` / `version`) and surfaces the output. Previously toggled only the description-string rendering.

**Dead config fields:**
- Removed `Build.qmk_pin`, `Build.zig_pin`, `Build.qmk_branch` (legacy alias), `Flash.backend`, `Flash.dry_run`, `Render.default_layer`, `Skill.auto_install` from `kb.toml` schema — every one was parsed but never read by any consumer. The example fixture and the init scaffold templates updated to match. A user setting `[flash] backend = "wally"` in kb.toml will now get a parse error (the field doesn't exist) instead of silent indifference.

**Crash-safety + concurrency:**
- `util::fs::atomic_write` is now actually crash-atomic: adds `tmp.as_file().sync_all()` to flush the kernel page cache before persist, and `dir.sync_all()` on the parent after rename so the directory entry is durable. The "Safe across power loss" docstring is now true.
- `util::lock::ProjectLock` provides advisory file locks via `fs2::FileExt::try_lock_exclusive`. `build::docker::build` takes an exclusive lock on `.oryx-bench/build/build.lock` for the entire build, so two concurrent `oryx-bench build` instances against the same project can't corrupt the staged keymap dir or the cache file.
- `xtask gen-skill-docs` now uses an atomic-write helper (sync + rename) so a killed regen never leaves a partial markdown file in the skills tree.
- Docker invocation passes `--user $UID:$GID` on Unix so produced files are owned by the invoking user, not root.
- The spurious `zsa_voyager_oryx-bench.bin` left in the project root by `qmk compile` is now staged into `.oryx-bench/build/firmware.bin` and removed from the project root, so the user's git tree stays clean.

**Multi-mod combo support:**
- New `CanonicalAction::Modified { mods, base }` variant. Oryx serializes "send X with Ctrl+Shift held" as a regular `tap` action with a non-null `modifiers` field listing which mods to wrap. Previously dropped silently — `keys[45]` in the fixture's `Brd+Sys` layer was a `KC_TAB` with `LCTL+LSFT` that emitted just `KC_TAB`. Now correctly renders as `LCTL(LSFT(KC_TAB))` in the generated C and the ASCII grid. New parser handles both Oryx's object form (`{leftCtrl: true, leftShift: true}`) and the older array form (`["LCTL", "LSFT"]`).

**`flash` build-freshness check:**
- New `flash --force` flag. Without it, `flash` re-derives the input sha (canonical layout + features.toml + overlay/) and refuses to flash if the build cache's recorded sha differs. Previously you could `oryx-bench pull && oryx-bench flash` and silently flash the *old* firmware against the *new* layout.

**Cargo cleanup:**
- Removed `time` and `proptest` from `Cargo.toml` — both unused.
- Added `fs2 = "0.4"` for advisory file locking.
- Bumped flake.nix to actually build the binary via `rustPlatform.buildRustPackage` (was `pkgs.hello`). `nix build` now produces `oryx-bench`. Version derived from `Cargo.toml` so there's a single source of truth.

**Doc drift cleanup:**
- `README.md` top banner no longer says "design phase, v0.0.0" — now correctly states v0.1.0 status with what's shipped vs deferred.
- README's command table updated from "13 commands" to 15 (added `diff`, `upgrade-check`, expanded existing entries with their new flags).
- README's Roadmap section rewritten — no more M1-M4 milestone columns, just "v0.1 (current)" and "future releases".
- README's "Install (planned for v0.1)" section replaced with actual install instructions (`cargo install`, `nix run`, source build).
- ARCHITECTURE.md top banner updated; the M1-M4 milestone column in the command table replaced with a flat list.
- ARCHITECTURE.md `M5+` / `v0.2+` / `M4 work` annotations removed throughout (10+ sites).
- `skills/oryx-bench/SKILL.md` description updated to acknowledge Voyager-only support; commands list now includes `upgrade-check` and `setup --full`.
- `voyager.rs` module docstring updated to use the column-first naming vocabulary.
- `examples/voyager-dvorak/kb.toml` no longer references the dead `qmk_branch` / `flash.backend` / `render.default_layer` fields.

**Other quality improvements:**
- `lint::run_all` now propagates `features.toml` parse errors instead of `unwrap_or_default`-ing a malformed file into an empty one.
- `lint::oryx_newer_than_build` rule rewritten as a real implementation (was a no-op stub) — re-derives the current input sha and compares against `.oryx-bench/build/build.sha`.
- Removed the dead `config-redefine-without-undef` lint rule (its property is now structurally impossible to violate, so the rule was dead code by design). Registry is now 21 rules.
- `flash::sha256_of_file` extracted as the single source of truth for firmware hashing — both `flash::plan` and `build::docker::build` use it so the hash format never drifts.
- Compile-time `const _: ()` assertion in `skill::embedded` ensures none of the bundled markdown files are empty (replaces the runtime `ensure_bundled_in_binary()` dead-call).
- `util::term::{OK, WARN, HINT}` applied consistently — removed the last inline `✓`/`⚠`/`—` literals from `commands::status` and `util::toolchain`.

**Test count: 180 (up from 169). Clippy clean. Fmt clean. End-to-end smoke test against the fixture passes.**

### Fixed (quality bar pass — zero "fix later" TODOs, zero silent error swallowing)

- **Removed `--svg` placeholder flag from `oryx-bench show`.** Was bailing with `"--svg output is M2 work"` while exposed in clap. Faking functionality violates the bar; the flag returns when keymap-drawer is actually wired up.
- **Removed `config-redefine-without-undef` lint rule.** Was a structurally dead no-op stub: the generator's `#undef`-then-`#define` pair makes the property the rule was meant to catch impossible to violate. Dead code violates SSoT. Now down to 21 lint rules.
- **Implemented `oryx-newer-than-build` lint rule properly** (was a no-op stub). Re-derives the current input sha from canonical layout + features.toml + overlay/, compares against `.oryx-bench/build/build.sha`, fires when they differ.
- **`lint::run_all` now propagates parse errors** instead of silently `unwrap_or_default`ing a malformed `features.toml` into an empty one. Lint surface broken overlay configs immediately. Updated all callers (`commands::lint`/`status`/`find`/`explain`/`upgrade_check`) to handle the Result.
- **Auto-pull failures now surface as warnings** in `commands::show`/`build`/`lint` instead of silently being swallowed via `let _ = pull::auto_pull(...)`. Users see `warning: auto-pull failed: ...` and the read still proceeds.
- **Pull cache write failures now `tracing::warn!`** instead of `let _ = write_cache(...)`. Cache writes are best-effort but failures are surfaced.
- **`Docker IMAGE_TAG` now derives from `CARGO_PKG_VERSION`** at compile time (`concat!("ghcr.io/.../oryx-bench-qmk:v", env!("CARGO_PKG_VERSION"))`). Was hardcoded to the stale `v0.0.0`.
- **`_features.h` no longer says "minimal stub"** in a comment. Replaced with a proper SPDX-headered file generated through a `FEATURES_HEADER` constant. Documents that future generator-emitted public symbols land here.
- **Magic numbers replaced with named constants**: `ACHORDION_DEFAULT_TIMEOUT_MS = 800`, `QMK_DEFAULT_TAPPING_TERM_MS = 200`, `TAPPING_TERM_MINIMUM_MS = 150`, `HTTP_TIMEOUT = 15s`. The Voyager flash budget constants were already named.
- **`flash::keymapp::cache_dir` no longer falls back to `cwd/.oryx-bench-cache`** when `ProjectDirs` returns None. Errors cleanly with a useful message pointing the user at `--backend wally` or directly opening Keymapp against the project's `.oryx-bench/build/firmware.bin`.
- **Cross-platform Unicode**: new `util::term` module wraps `console::Emoji` with `OK`, `WARN`, `HINT` constants that fall back to `[OK]`/`[!]`/`[hint]` on legacy terminals. Replaced 16 inline `✓`/`⚠`/`💡` literals across `init`, `pull`, `attach`, `detach`, `build`, `flash`, `lint`, `skill`, `upgrade_check`, `explain`, `flash::keymapp` with the helper.
- **`util::fs::atomic_write`** now propagates the tempfile flush error instead of `.ok()`-ing it. File writes are crash-safe end-to-end.
- **Removed dead `let _ = skill::embedded::ensure_bundled_in_binary()`** call in `init.rs`. Replaced with a `const _: ()` compile-time assertion in `skill::embedded` that fails the build if any embedded skill file is empty — same property, surfaces at compile time, and the dead-call site is gone.
- **`commands::detach` cache cleanup now propagates errors** (was `let _ = std::fs::remove_file(&cache)`).
- **Wording cleanup**: `build/mod.rs` says "not supported in v0.1" instead of "M5+ work"; `util/toolchain.rs` says "future native backend" instead of "v0.2+"; `commands/upgrade_check.rs` removed the "v0.2 work" comment (the snapshot diffing feature is documented in the module docstring as a planned future addition, not as a TODO masking a bug).
- **`tests/codegen_roundtrip.rs` rewritten** to actually do the round-trip. The previous version "explicitly deferred steps 4-7" — that was a fix-later TODO. New version: parses every `LAYOUT_voyager(...)` block out of the generated source, walks each positional arg through the QMK arg-order permutation back to a canonical index, and asserts the canonical key at that index renders to the exact string the generator emitted. Catches the entire class of off-by-N permutation / dropped-hold / wrong-layer-name codegen bugs deterministically, with zero external tools. The optional `qmk c2json` integration test still runs when `qmk` is on PATH.
- **`ARCHITECTURE.md` `layout.toml` example updated** from the obsolete `L_pinky_num`/`L_inner2_top` vocabulary to the column-first `L_outer_num`/`L_inner_top` vocabulary that matches the implementation. The "position names are stable" paragraph now documents the column-first naming explicitly.

**Test count: 169 (up from 164). Clippy clean. Fmt clean. `cargo test --workspace` green from a fresh checkout.**

### Fixed (second post-review correctness pass — TWO release-blocking bugs)

The second review pass caught two release-blocking bugs in the cumulative v0.1 surface; both fixed:

1. **Voyager `POSITION_TABLE` had right-half indices off by 2 and the left thumb cluster pointed at the wrong indices entirely.** Oryx serializes the `keys[]` array as `[left rows 0..24][left thumb 24..26][right rows 26..50][right thumb 50..52]`, but my table treated indices 24-29 as right top row (actually left thumb + first 4 right top keys) and 48-49 as left thumb (actually the rightmost right bottom keys). User-visible: `oryx-bench show` rendered `KC_SPC, KC_CAPS` in the right top row (those are left thumb keys); `oryx-bench find position:R_index_home` returned the wrong key; the `home_row_mods_asymmetric` lint counted the wrong half. Verified the fix end-to-end: Main layer now correctly shows SPC + CAPS in the left thumb cluster and ENTER/BSPC in the right thumb cluster.

2. **Codegen `emit_keymaps_array` iterated keys in Oryx serialization order and emitted them positionally to `LAYOUT_voyager(...)`, which expects a different physical-interleaved order.** QMK's `LAYOUT_voyager` macro takes 52 args in `[L row 0][R row 0][L row 1][R row 1]...[L thumb][R thumb]` order; Oryx serializes left rows then left thumb then right rows then right thumb. Without permutation, every right-side key in the generated `keymap.c` ended up at the wrong physical position — **flashing the firmware would produce a visibly broken layout**. Added `Geometry::qmk_arg_order()` returning a 52-element permutation table, derived from the upstream `keyboard.json` for `keyboards/zsa/voyager` in the firmware24 ZSA QMK fork. Updated `emit_keymaps_array` to walk the permutation. Two new tests pin the permutation: `qmk_arg_order_is_a_complete_permutation` (every canonical index appears exactly once) and `qmk_arg_order_pins_known_positions` (specific anchor points: `QMK[0..6] = canonical 0..6`, `QMK[6..12] = canonical 26..32`, `QMK[48..50] = canonical 24..26`, etc.). End-to-end smoke test: `build --emit-overlay-c` against the fixture now produces a `keymap.c` with each row correctly emitting `[L 6 keys][R 6 keys]` and the thumb row as `[L inner][L outer][R inner][R outer]`.

Plus the secondary reviewer findings:

- Bumped `Cargo.toml` to `version = "0.1.0"`.
- Removed the `_sanitize_c_ident_reexport` leading-underscore re-export.
- Tightened `flash_smoke::flash_without_yes_and_with_no_stdin_bails_safely` to assert `Aborted` is in stdout (was permissive).
- `flash::confirm` now case-insensitive via `eq_ignore_ascii_case` (accepts `Yes`/`yEs`/etc).
- `oryx_combo_to_canonical` emits a `tracing::warn!` instead of silently dropping unknown combo shapes.
- Fixed misleading comment at `tests/lint_rules.rs:220` (was `L_pinky_home`, now correctly `L_outer_home` under the column-first naming).
- Replaced the no-op `fixture_home_row_mods_asymmetric_does_not_falsely_fire` test with a real assertion: the fixture has mods on both halves so the rule should NOT fire — verified.
- Fixed `examples/voyager-dvorak/overlay/features.toml` position-name comment from `L_pinky_num` to `L_outer_num`.
- Added 4 new `GeometryName` round-trip tests (Voyager + Other variants through serde, plus `from_str` parsing).

**Test count: 164 (up from 162). Clippy clean. Fmt clean.**

### Added (M4 implementation)

- **`oryx-bench diff [REF] [--layer NAME]`** — semantic diff vs a git ref:
  - Shells out to `git show <ref>:<path>` for both `pulled/revision.json` (Oryx mode) or `layout.toml` (local mode), parses both into `CanonicalLayout`, walks position-by-position and prints every changed binding (`Layer Position: old → new`).
  - Diffs `overlay/features.toml` separately: changed `[config]` keys, added/removed `[[key_overrides]]`/`[[macros]]`/`[[combos]]`/`[[tapping_term_per_key]]` entries, achordion enabled/strategy/timeout-count changes.
  - `--layer` filters the visual diff to one layer.
  - Bails cleanly if `git` is not on PATH or the file doesn't exist at the given ref.
- **`oryx-bench upgrade-check`** — re-lint after a tool upgrade:
  - Walks the layout looking for `Keycode::Other` instances (uncatalogued keycodes) and surfaces them per layer/position.
  - Re-runs the full lint registry and prints error/warning/info counts.
  - Lists every registered lint rule with its severity and description so users can see what's new since their last release.
  - Snapshot diffing against a `.oryx-bench/upgrade.json` is deferred to v0.2.
- **3 new lint rules** (registry now has 22):
  - `unused-feature-flag` (info) — `[features]` enables `key_overrides`/`combos` but the corresponding section is empty (wastes flash).
  - `large-firmware` (info) — info if `.oryx-bench/build/firmware.bin` is ≥60KB (93.75% of the Voyager's ~64KB budget).
  - `unbound-tapping-term` (warning) — `[[tapping_term_per_key]]` references a binding that doesn't exist anywhere in the layout (dead switch case).
- **5 new lint rule tests** in `tests/lint_rules.rs` (positive + negative coverage for the new rules).

**Test count: 157 (up from 151), all green. Clippy clean. Fmt clean.**

### Added (M3 implementation)

- **`src/flash/`** — flash backends + dispatcher:
  - `wally.rs`: invokes `wally-cli <firmware.bin>` directly when on PATH.
  - `keymapp.rs`: fallback handoff. Stages the firmware at `~/.cache/oryx-bench/firmware.bin` (via `directories::ProjectDirs`) and prints platform-specific Keymapp instructions for Linux / macOS / Windows.
  - `mod.rs`: backend detection (`auto`/`wally`/`keymapp`), `FlashPlan` type with size + sha256 + target metadata, `render_plan` for `--dry-run`, `execute` for the irreversible step.
  - **Never invokes `dfu-util`** — reflected in the keymapp instructions and the docs.
- **`src/commands/flash.rs`** — fully implemented:
  - `--dry-run` prints the flash plan and exits 0 without touching hardware.
  - `--yes` skips the in-process `[y/N]` confirmation prompt (still requires conversational approval when used by an agent).
  - `--backend auto|wally|keymapp` selects the backend explicitly.
  - `--no-pull` is a no-op — flash never auto-pulls per spec (the moment of commitment).
  - Bails cleanly with "run `oryx-bench build` first" if no firmware exists.
  - Handles EOF on stdin gracefully (treats as "no" rather than hanging).
- **`tests/flash_smoke.rs`** — 5 integration tests covering: dry-run output, missing-firmware error path, explicit backend selection, unknown-backend error, and EOF-stdin abort safety. All run without real hardware.

**Test count: 151 (up from 146), all green. Clippy clean. Fmt clean.**

### Fixed (post-review correctness pass)

Reviewer agent caught seven critical correctness gaps in M1+M2; all addressed:

- **Mod-tap canonical conversion**: `oryx_action_to_canonical` now collapses `tap=Keycode + hold=Modifier` into `ModTap`, mirroring the existing `LT` collapse. Without this, every lint rule that reasoned about mod-taps (`mod-tap-on-vowel`, `home-row-mods-asymmetric`, `tt-too-short`) silently never fired on real Oryx layouts.
- **`emit_key` defensive collapse**: keymap.rs now synthesizes `LT`/`ModTap` from any tap+hold pair the canonical pass missed, so `hold` is never silently dropped.
- **USER custom keycode parsing**: both `oryx_action_to_canonical` and `layout::parse_action` now recognize `USERnn` slots and produce `CanonicalAction::Custom(n)`. The two custom-keycode lint rules now reach real projects.
- **Typed `GeometryName` enum**: `CanonicalLayout.geometry` is now a typed enum (`Voyager`/`Other(String)`), per spec at ARCHITECTURE.md:581. `From<&str>` impl preserves ergonomics.
- **Combos in `CanonicalLayout` + `FULL_QUERY`**: added `CanonicalCombo` type and `combos: Vec<CanonicalCombo>` field. `FULL_QUERY` now requests `combos` and `swatch` from Oryx. Best-effort projection from the `oryx::Combo` extras bag.
- **Voyager `POSITION_TABLE` dual-vocabulary fix**: collapsed the inconsistent `L_pinky_*`/`L_pinky_q`/`L_pinky_h` mix into a single column-first naming scheme: `outer/pinky/ring/middle/index/inner` × `num/top/home/bottom`. Each of the 52 indices now has exactly one canonical name.
- **`layout.toml` `inherit = "<layer>"` semantics**: overlay layers in local mode now correctly default unspecified positions to `KC_TRNS` (transparent fall-through), not `KC_NO`. Spec at ARCHITECTURE.md:1199.

Additional important fixes from the same review:

- **`config.h` boolean QMK macros**: maintain an allow-list of define-only toggles (`PERMISSIVE_HOLD`, `HOLD_ON_OTHER_KEY_PRESS`, `RETRO_TAPPING`, `TAPPING_FORCE_HOLD`, `CHORDAL_HOLD`, etc.). `true` emits a bare `#define`, `false` emits nothing — the previous behavior of `#define PERMISSIVE_HOLD false` actually *enabled* the feature.
- **`unknown-keycode` severity**: lowered from Error to Warning to honor the forward-compat invariant. `Keycode::Other(_)` is the catch-all and codegen emits it verbatim; lint shouldn't block real layouts that use catalogued-but-not-yet-known keycodes like `RGB_SLD`.
- **`sanitize_c_ident` relocation**: moved from `lint::rules::layer_name_collision` to a new `schema::naming` module. Codegen no longer imports from lint.
- **`KC_LCTL` dual encoding**: `references_keycode` now matches both `Keycode(KcLctl)` and `Modifier(Lctl)` representations. Also normalizes bare keycode queries (`BSPC` → `KC_BSPC`).
- **`process_record_user_collision` heuristic**: rewrote with a state-machine that distinguishes function definitions from declarations and call-sites. Strips block comments. Tested against 7 positive/negative cases.
- **`home-row-mods-asymmetric`**: now checks both `tap` and `hold` slots (orphaned mod-taps live on `hold`).
- **`status` always queries metadata**: `check_metadata_only` no longer short-circuits via cache age; the spec requires `status` to always do the cheap GraphQL metadata query.
- **`pull` cache backoff on failure**: failed metadata queries now update `last_check_epoch` so a flaky network doesn't hammer Oryx on every read command.
- **`find` lowercase + bare-letter support**: `find a`, `find kc_bspc` now normalize and resolve correctly.
- **Init scaffold cleanup**: removed `pulled/.gitkeep` from Oryx mode; local-mode `layout.toml` scaffold uses dotted-key syntax with commented examples.
- **RGB long-form keycodes**: added `RGB_MODE_FORWARD`/`RGB_HUE_INCREASE`/etc. as aliases.

New tests:

- `tests/lint_rules.rs` — 6 fixture-based tests against the real voyager-dvorak fixture covering `lt-on-high-freq`, `overlay-dangling-keycode`, `overlay-dangling-position`, `unknown-layer-ref`, `orphaned-mod-tap`, `home-row-mods-asymmetric`. These catch any future schema change that breaks lint rule preconditions.
- Mod-tap collapse + USER keycode round-trip tests in `src/schema/canonical.rs`.
- `process_record_user_collision` heuristic: 7 unit tests for definition/declaration/call-site/comment classification.
- `config.h` boolean macro: 3 tests for true/false/non-boolean emission.

**Test count: 146 (up from 127), all green. `cargo clippy -- -D warnings` clean. `cargo fmt --check` clean.**

### Added (M2 implementation)

- Codegen layer (`src/generate/`):
  - `keymap.rs` — emits `keymap.c` with `LAYOUT_voyager(...)` arrays, `enum layers`, `enum custom_keycodes`, and resolves layer references through the sanitized C-ident table.
  - `features.rs` — emits `_features.c` with the `process_record_user` dispatch (chains to `process_record_user_overlay` for Tier 2 hooks), key-override tables (`ko_make_basic`), and per-key tapping term overrides.
  - `config_h.rs` — emits `config.h` with `#undef`/`#define` pairs from `[config]` section.
  - `rules_mk.rs` — emits `rules.mk` with `KEY_OVERRIDE_ENABLE` etc. from `[features]` and walks `overlay/*.c` for `SRC +=` entries.
- Build backend (`src/build/`):
  - `docker.rs` — invokes the bundled `ghcr.io/enriquefft/oryx-bench-qmk:<tag>` image, stages generated files, runs `qmk compile -kb zsa/voyager`, copies the produced `.bin` to `.oryx-bench/build/firmware.bin`. Caches by sha256 of all inputs.
  - Backend dispatcher refuses `native` and `nix` with a clear "M5+ work" message.
- M2 commands fully implemented:
  - `attach --hash <H>` — local mode → Oryx mode, refuses on uncommitted `layout.toml` changes unless `--force`, runs initial pull.
  - `detach` — Oryx mode → local mode (one-way). Renders `pulled/revision.json` back to `layout.toml` via the new `schema/layout::render_layout_toml` writer, removes `pulled/` and the auto-pull cache.
  - `build` — runs the generators + dispatches to the backend, supports `--dry-run`, `--release`, `--emit-overlay-c`, `--no-pull`.
- `util/git.rs` — minimal `has_uncommitted_changes` shell-out (no `git2` dep).
- Tests added:
  - `tests/codegen_roundtrip.rs` — 3 tests: structural sanity on the fixture (4 LAYOUT blocks, LT(SYM_NUM, KC_BSPC) present), features.toml→C output checks, and a `qmk c2json` round-trip that skips gracefully if `qmk` is not on PATH.
  - `tests/attach_detach.rs` — 3 wiremock-backed tests covering attach pulling on first call, detach round-tripping a pulled fixture into `layout.toml`, and detach without `--force` warning only.

### Added (M1 implementation)

- Rust workspace + `xtask` crate scaffolded per ARCHITECTURE.md.
- `flake.nix` + `.envrc` devshell (rustc/cargo/clippy/rustfmt + zig + Python for `qmk`).
- Schema layer (`src/schema/`):
  - `oryx.rs` — GraphQL JSON types with lossless `extra: HashMap` forward-compat.
  - `canonical.rs` — internal `CanonicalLayout` both Oryx and local modes deserialize into, with LT-collapsing normalization.
  - `keycode.rs` — ~190 variant keycode catalog + `Other(String)` catch-all, long/short form round-trip, hand-rolled serde impls.
  - `kb_toml.rs`, `features.rs`, `layout.rs` — project config, overlay features, local-mode layout schemas.
  - `geometry/` — `Geometry` trait + `Voyager` implementation (52 matrix keys, 0 encoders, 4 thumb keys, position name vocabulary).
- Pull layer (`src/pull/`):
  - Blocking reqwest-based GraphQL client with metadata-only + full-layout queries.
  - Auto-pull cache at `.oryx-bench/cache.json` with configurable `poll_interval_s`.
- Hand-rolled ASCII split-grid renderer (`src/render/ascii.rs`).
- Lint layer with 19 registered rules (`src/lint/rules/`):
  - Visual-layout: `lt-on-high-freq`, `unreachable-layer`, `kc-no-in-overlay`, `orphaned-mod-tap`, `unknown-keycode`, `unknown-layer-ref`, `duplicate-action`, `mod-tap-on-vowel`, `home-row-mods-asymmetric`, `layer-name-collision`.
  - Cross-tier: `overlay-dangling-position`, `overlay-dangling-keycode`, `custom-keycode-undefined`, `unreferenced-custom-keycode`, `process-record-user-collision`, `config-redefine-without-undef`.
  - Build/sync state: `tt-too-short`, `not-pulled-recently`, `oryx-newer-than-build` (latter two M2-completed).
- M1 commands (`src/commands/`):
  - `setup`, `init` (both Oryx and local modes), `pull`, `show`, `explain`, `find`, `lint`, `status`, `skill install/remove`.
  - `attach`/`detach`/`build`/`flash`/`diff` stubbed with "M2/M3/M4 work — not implemented in M1" messages.
- Embedded Claude Code skill installer (`src/skill/`) with `include_str!`-bundled files, project-local default install.
- `xtask gen-skill-docs` regenerates `skills/oryx-bench/reference/{lint-rules,command-reference}.md` from the live registries.
- Tests (83 total, all green):
  - Library unit tests (52) covering schemas, keycode parsing, geometry, auto-pull cache, project discovery, atomic writes, skill install, features round-trip.
  - `tests/lint_rules.rs` (21) — positive + negative per rule against the voyager-dvorak fixture.
  - `tests/cli_smoke.rs` (7) — end-to-end init → show → lint → skill install.
  - `tests/skill_drift.rs` (3) — asserts embedded skill files match disk and the generated markdown matches the committed files.
- `cargo clippy -- -D warnings` and `cargo fmt --check` both pass.

### Design

- Initial design committed in `ARCHITECTURE.md`.
- Adopted the **four-tier authoring model**:
  - Tier 0: Oryx UI (visual layout)
  - Tier 1: `overlay/features.toml` (declarative QMK features)
  - Tier 2: `overlay/*.zig` (procedural code, type-safe)
  - Tier 2′: `overlay/*.c` (vendored upstream C libraries)
- Adopted **two visual layout sources** with explicit migration commands:
  - `pulled/revision.json` (Oryx mode, default for Oryx users)
  - `layout.toml` (local mode, no Oryx dependency)
  - `oryx-bench attach` / `oryx-bench detach` for one-time migration
  - Detach is **one-way** — no public Oryx write API
- Adopted **auto-pull on read commands** (default `on_read` with 60s cache)
  so personas using Oryx have zero manual sync ceremony.
- **Verified Zig + QMK link end-to-end**: `@cImport` of QMK headers works,
  `KC_BSPC` resolves correctly, the resulting Zig `.o` links cleanly into
  a real Voyager firmware. See `ARCHITECTURE.md#verification-log`.
- **Cut from v0.1**: native and Nix build backends. Docker is the v0.1
  build path; native and Nix come back in v0.2.
- Cross-tier lint rules added to the spec (catches dangling references
  between `pulled/`, `layout.toml`, and `overlay/`).
- Skill files install **project-local by default** to avoid polluting the
  context budget of unrelated Claude Code sessions.
- Skill reference files for lint rules and command help are **generated by
  an `xtask` binary** (not `build.rs`) so they cannot drift from the
  source they describe.
