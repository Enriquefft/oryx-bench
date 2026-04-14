# Contributing to oryx-bench

Thanks for your interest. This document covers the things that aren't
already in `ARCHITECTURE.md` (high-level design) or rustdoc on the
relevant traits.

The architecture has four extension points where you can land cleanly
without touching the core:

- **A new keyboard geometry** → one new file in `src/schema/geometry/`
- **A new lint rule** → one new file in `src/lint/rules/`
- **A new Tier 1 declarative feature** → extend the `features.toml`
  schema in `src/schema/features.rs` and add a generator in
  `src/generate/features.rs`
- **A new overlay cookbook recipe** → add a section to
  `skills/oryx-bench/reference/overlay-cookbook.md` (single source of
  truth — do not duplicate elsewhere)

After any change to lint rules or the CLI surface, run
`cargo xtask gen-skill-docs` to regenerate
`skills/oryx-bench/reference/{lint-rules,command-reference}.md`. CI
verifies these files are up-to-date with the source.

## Dev setup

```bash
git clone https://github.com/enriquefft/oryx-bench
cd oryx-bench

# Build
cargo build

# Run
cargo run -- show

# Test
cargo test

# Lint
cargo clippy -- -D warnings
cargo fmt --check
```

## Architecture in 30 seconds

Read `ARCHITECTURE.md` for the full picture. The short version:

- `src/schema/oryx.rs` — Rust types matching Oryx's GraphQL JSON
  (camelCase, lossless via `#[serde(flatten)] extra`).
- `src/schema/layout.rs` — `layout.toml` schema (local mode visual layout).
- `src/schema/features.rs` — `overlay/features.toml` schema (Tier 1
  declarative QMK features).
- `src/schema/canonical.rs` — the internal `Layout` representation that
  both `oryx.rs` and `layout.rs` deserialize into. The rest of the
  codebase operates on this type.
- `src/schema/keycode.rs` — finite QMK keycode catalog (typed enum +
  `Other(String)` catch-all for forward-compat).
- `src/schema/geometry/` — one file per keyboard. **Extension point.**
- `src/lint/rules/` — one file per rule. **Extension point.**
- `src/generate/` — `CanonicalLayout` + `features.toml` + `overlay/*` →
  `keymap.c` + `config.h` + `rules.mk`. Round-trip-tested against
  fixtures (with explicit caveats for one-way features like macros and
  RGB code).
- `src/render/` — hand-rolled ASCII split-grid renderer (no `tabled`
  dependency) + SVG via subprocess to `keymap-drawer`.
- `src/build/` — **v0.1: docker-only**. Native and Nix backends arrive
  in v0.2.
- `src/flash/` — subprocess handoff to ZSA's
  [`zapp`](https://github.com/zsa/zapp) CLI (>=1.0.0, required on
  PATH). Never invokes `dfu-util` directly.
- `src/skill/` — embedded skill installer (project-local by default).
- `xtask/` — separate workspace crate that calls into `oryx-bench`'s
  registries at runtime to generate the skill reference files. Replaces
  the `build.rs` walking-source-files trap.

## Adding a new keyboard

The architecture is designed for this to be cheap. You should never have to
touch `pull/`, `lint/`, `render/`, `generate/`, `build/`, or `flash/`.

1. **Get the Oryx geometry slug** from the URL of any layout for that
   keyboard (`configure.zsa.io/<slug>/layouts/...`).
2. **Create `src/schema/geometry/<slug>.rs`** implementing the `Geometry`
   trait. The `Voyager` impl is the reference. See the trait's rustdoc for
   each method's contract.
3. **Register it** in `src/schema/geometry/mod.rs::registry()`.
4. **Add a fixture** at `tests/fixtures/<slug>_default.json`. Pull a real
   layout via:
   ```bash
   curl -s -X POST https://oryx.zsa.io/graphql \
     -H "Content-Type: application/json" \
     -d '{"query":"query{layout(hashId:\"HASH\",revisionId:\"latest\",geometry:\"<slug>\"){revision{layers{title position keys}}}}"}'
   ```
5. **Add tests** in `tests/codegen_roundtrip.rs` and
   `tests/render_snapshot.rs` for the new fixture.
6. **Done.** No other modules touched. If you find yourself editing one of
   the directories listed above, that's a leaky abstraction — please file an
   issue instead.

## Adding a new lint rule

1. **Create `src/lint/rules/<rule_id>.rs`** implementing the `LintRule`
   trait. Required methods: `id`, `severity`, `description`, `why_bad`,
   `fix_example`, `check`.
2. **Register it** in `src/lint/rules/mod.rs::registry()`.
3. **Add tests** in `tests/lint_rules.rs` — one positive, one negative.
4. **Build**: `cargo build` regenerates `lint-rules.md` from the registry.
   The skill's reference now describes your rule.

### What makes a good lint rule

- Catches a real bug class, not stylistic preference
- Near-zero false positives on real-world layouts
- Has an actionable fix the user can apply in under 5 minutes
- Explains the *why* in the `why_bad` field (mandatory)
- Pure function: `(Layout, Geometry) -> Vec<Issue>`. No side effects.

### Severity guidance

| Severity | When | Examples |
|---|---|---|
| **Error** | real footgun, no upside | `lt-on-high-freq`, `unreachable-layer`, `unknown-keycode` |
| **Warning** | probably wrong but might be intentional | `kc-no-in-overlay`, `orphaned-mod-tap`, `tt-too-short` |
| **Info** | stylistic or worth noticing | `duplicate-action`, `home-row-mods-asymmetric` |

`oryx-bench lint --strict` exits non-zero on warnings as well as errors.

## Adding an overlay cookbook recipe

Cookbook recipes live in `skills/oryx-bench/reference/overlay-cookbook.md`.
Each recipe is a complete, copy-pasteable example with:

- A short description of what it does and when to use it
- The full `overlay/*.c` (and `.h` if needed) file content
- Any required `overlay/rules.append.mk` line
- A brief explanation of how it works
- Known caveats / when NOT to use it

The cookbook is the single source of truth — it's loaded into Claude Code's
context when relevant via the skill's reference mechanism. Do not duplicate
recipes anywhere else.

## Running the round-trip codegen tests

The most important tests live in `tests/codegen_roundtrip.rs`. They:

1. Load a fixture `revision.json`
2. Run our generator → `keymap.c`
3. Pipe through `qmk c2json` → JSON
4. Assert structurally equal to the input

This is the contract between us and ZSA's qmk_firmware fork. If it breaks,
either the fork's keycode mapping changed (update `MOD_TAP_MAP` and friends in
`src/schema/keycode.rs`) or the generator has a regression.

```bash
# Requires qmk on PATH
cargo test --test codegen_roundtrip
```

## Releasing

1. Update `CHANGELOG.md` (rename `[Unreleased]` to `[0.x.y] - YYYY-MM-DD`, add footer link).
2. Bump version in `Cargo.toml`.
3. Tag: `git tag v0.x.y && git push --tags`.
4. CI (`release.yml`) automatically:
   - Builds static Linux binaries (x86_64 + aarch64 musl) and attaches them to the GitHub release.
   - Deploys an updated `PKGBUILD` to the AUR (`oryx-bench`).
   - Publishes to crates.io (requires `CRATES_IO_TOKEN` secret in the repo).

## Code style

- `cargo fmt` on save
- `clippy -- -D warnings` is enforced in CI
- Prefer `anyhow::Result` for application code, `thiserror` for library-style
  modules with structured errors
- Tests use `assert_cmd` for end-to-end CLI tests, `insta` for snapshot tests
- Don't add unsafe code without a comment justifying it

## License

By contributing, you agree your contributions are licensed under the MIT
License (see `LICENSE`).
