# Architecture

This document is the canonical design reference. Read it before contributing
or proposing changes. It is also the document Claude Code reads when working
on the codebase.

> **Status**: v0.1.0 implemented. Voyager-only, Docker build backend.
> See [`CHANGELOG.md`](CHANGELOG.md) for what's actually shipped.
> Sections in this doc that describe v0.2+ features (Moonlander/Ergodox
> geometries, native/nix backends, SVG rendering, `oryx-bench live`,
> `oryx-bench tui`) are forward-looking design notes, not current code.

## What this tool is

`oryx-bench` is a Rust CLI for managing ZSA keyboard layouts. It is built
around two ideas:

1. **Source-of-truth factoring by concern.** A keyboard layout has multiple
   independent concerns (where keys live, what they do behaviorally,
   advanced QMK feature configuration). Each concern lives in exactly one
   place. The build pipeline merges them deterministically.

2. **Heterogeneous authoring surfaces.** Different users prefer different
   tools for different concerns. Oryx (the ZSA web app) is great for visual
   layout editing. Local files are better for QMK behavior code. A
   declarative TOML format is better for everything in between. We support
   all three, plus the ability to skip Oryx entirely.

## Goals

1. **Single source of truth per concern.** No fact about the keyboard lives
   in two places.
2. **Visual editing preserved.** Oryx remains the editor for layout
   positions for users who want it.
3. **Code authoring power.** Advanced QMK features (achordion, key
   overrides, custom keycodes, RGB code) live in the project, owned by the
   user.
4. **Modern languages.** Tier 1 (declarative) is TOML; Tier 2 (procedural)
   is Zig. C is supported only as a compatibility tier for vendored
   upstream code.
5. **Deterministic, reproducible builds.** Same inputs → same firmware byte
   for byte. Cacheable.
6. **Cross-platform install.** Linux, macOS. NixOS as a peer, not a
   prerequisite.
7. **AI-agent-friendly.** Claude Code is a first-class user via a
   project-local skill bundled with the binary.
8. **Easy to extend.** New keyboards, new lint rules, new overlay features
   should each be a single file added in one place.
9. **Optional Oryx.** A user who doesn't want any cloud dependency can run
   the entire workflow locally.

## Non-goals

- Replacing Oryx's visual editor.
- Pushing local layout changes back to Oryx (no public write API; not worth
  the fragility).
- Live keyboard editing à la Vial. (Out of scope; integrate with
  `kontroll` instead of replacing it.)
- Supporting non-ZSA keyboards. The QMK ecosystem is huge; we are scoped to
  ZSA boards because Oryx is ZSA-specific and our value-add is the Oryx
  integration.
- Telemetry of any kind.
- Auto-update.
- Arbitrary firmware flashing protocols (we never invoke `dfu-util`
  directly; ZSA's flashing protocol is custom and bricking risk is real).

---

## The four-tier authoring model

Different kinds of changes belong in different places. Each tier has its
own format, owns a non-overlapping concern, and is edited by a different
combination of (human, Claude Code, Oryx UI).

### Tier 0 — Oryx UI (web app)

- **Format**: ZSA's hosted web editor at `configure.zsa.io`
- **Scope**: visual layout. *Which physical key position sends which
  keycode.* Layer organization. Tags. RGB color assignments per key. Basic
  combos.
- **Edited by**: the user, in a browser
- **Where the data ends up**: `pulled/revision.json` (after `oryx-bench
  pull`)
- **Note**: Oryx is **upstream** of the build pipeline, like a Figma file
  feeding a build. There is no "push to Oryx" because Oryx is not a peer
  database.

### Tier 1 — `overlay/features.toml` (declarative QMK features)

- **Format**: TOML in the project
- **Scope**: anything that *would need C code* if you were hand-rolling it,
  but is really *configuration*. Achordion, key overrides, macros, combos
  beyond Oryx's UI capability, tap-hold tuning, `config.h` `#define`s,
  `rules.mk` feature flags.
- **Edited by**: the user or Claude Code, declaratively
- **What happens at build time**: `oryx-bench build` reads the TOML and
  generates the equivalent C code into the build directory. The user never
  sees the generated C.
- **Why this tier exists**: ~90% of QMK customization is configuration, not
  code. We refuse to make users learn C just to enable a feature whose
  every-user-writes-the-same-five-lines pattern is a configuration shape.

### Tier 2 — `overlay/*.zig` (procedural code, type-safe)

- **Format**: Zig source files
- **Scope**: things that need *actual code logic*. State machines,
  conditional behavior, RGB matrix animations, custom keycodes with state,
  tap dance, host communication via Raw HID, hooks for boot/suspend/wake,
  anything Tier 1's declarative shape can't capture.
- **Edited by**: the user or Claude Code, in Zig
- **What happens at build time**: pre-compiled to ARM Cortex-M4 object
  files via `zig build-obj`, then linked into the QMK firmware via
  `LDFLAGS`. **Verified end-to-end** (see `Verification log` below).
- **Why Zig**: same metal-target binary as C, but with type safety,
  `@cImport` of QMK headers (no bindgen), better error messages, and
  `comptime`. Zig is the modern C-replacement for embedded; this is the
  bet.

### Tier 2′ — `overlay/*.c` (vendored upstream C)

- **Format**: C source files in the same directory as Tier 2
- **Scope**: third-party QMK code the user wants to drop in *unmodified*.
  Achordion's library body, getreuer's modules, drashna's helpers — any
  community C the user finds on GitHub.
- **Edited by**: nobody. **You don't *write* code in this tier — you only
  paste it.** It is a compatibility tier so that the entire existing QMK
  ecosystem of small C libraries is reachable.
- **What happens at build time**: same as Tier 2 — picked up by the build
  via `SRC += foo.c` in the auto-generated Make additions. ABI-compatible
  with Tier 2 Zig objects.

Tiers 2 and 2′ share infrastructure (same directory, same build mechanism,
ABI-compatible). The split is about **authoring intent**: write new things
in Zig; only keep C for vendored libraries.

### Tier visualization

```
                       ┌──────────────────┐
                       │  Tier 0: Oryx    │  visual layout
                       │  (web editor)    │  (cloud)
                       └────────┬─────────┘
                                │
                                │ oryx-bench pull (GraphQL, no auth)
                                │
                                ▼
                  ┌─────────────────────────┐
   OR             │  pulled/revision.json   │   (Oryx mode)
                  └─────────────────────────┘
                                │
                                │
                  ┌─────────────────────────┐
                  │     layout.toml         │   (local-only mode)
                  └─────────────────────────┘
                                │
                                ▼
                  ┌──────────────────────────────────┐
                  │  internal Layout representation  │
                  └────────────────┬─────────────────┘
                                   │
                                   ▼  + Tier 1, 2, 2′
                  ┌──────────────────────────────────┐
                  │   firmware build pipeline        │
                  │                                  │
                  │   overlay/features.toml ─┐       │
                  │   overlay/*.zig ─────────┤       │
                  │   overlay/*.c ───────────┤       │
                  │                          │       │
                  │   ZSA qmk_firmware fork  │       │
                  │   (pinned)               │       │
                  │                          ▼       │
                  │                    keymap.c +    │
                  │                    overlay.o +   │
                  │                    config.h +    │
                  │                    rules.mk      │
                  │                          │       │
                  │                          ▼       │
                  │                    firmware.bin  │
                  └──────────────────────────────────┘
                                   │
                                   │ oryx-bench flash
                                   ▼
                              🎹 keyboard
```

---

## Two sources, one canonical layout

The visual layout has exactly one canonical source per project, but that
source can be one of two formats:

| Mode | Source file | Editor | When to use |
|---|---|---|---|
| **Oryx mode** | `pulled/revision.json` | Oryx web UI | You like the visual editor and Oryx's community features |
| **Local mode** | `layout.toml` | Text editor (or Claude Code) | You want zero cloud dependency, full git history of layout changes, layout in a private repo, or comments next to bindings |

**At any one moment, exactly one is active.** Both files cannot exist in
the same project. `oryx-bench init` picks the mode at project creation;
migration between modes is an explicit one-time command (`attach` /
`detach`).

The build, lint, render, flash, skill, and overlay machinery are
**identical** in both modes. They consume an internal `Layout`
representation that both `pulled/revision.json` and `layout.toml`
deserialize into. Adding the second source path is implementation-cheap
because it's a parser, not a new architecture.

### Switching modes

Two commands manage the boundary:

```
oryx-bench init --hash YRBLX           # → Oryx mode
oryx-bench init --blank --geometry voyager   # → local mode

oryx-bench detach                      # Oryx mode → local mode
oryx-bench attach --hash YRBLX         # local mode → Oryx mode
```

`detach` is **one-way and explicit**: it converts the current
`pulled/revision.json` into a `layout.toml`, deletes `pulled/`, and from
that point forward `oryx-bench pull` no longer functions in this project.
You can `attach` again later but doing so will *overwrite* your local
`layout.toml` with whatever Oryx currently has, **losing any
divergent local edits.** This is the only way Oryx can re-enter the loop;
there is no merge.

We document this loudly. We do not ship a `push` command that pretends to
write back to Oryx — there is no public write API and we will not build on
reverse-engineered fragility.

---

## The five user personas

The model holds up for the following workflow combinations. Each persona
has zero ongoing manual sync steps after the initial `init` (and one extra
step for persona 5's detach).

| # | Persona | Visual editing | Behavior editing | Flashing | Sync friction |
|---|---|---|---|---|---|
| 1 | **Oryx-only purist** | Oryx web | (none) | Keymapp GUI | They never run `oryx-bench`. Zero. |
| 2 | **Oryx + read-only oryx-bench** | Oryx web | (none) | Keymapp GUI | Zero. Auto-pull keeps `oryx-bench show`/`lint`/`status` in sync transparently. |
| 3 | **Oryx + full oryx-bench** | Oryx web (visual) + `overlay/` (behavior) | overlay files | `oryx-bench flash` | Zero. Auto-pull handles Oryx side; overlay files are local. |
| 4 | **Local-only** | `layout.toml` | overlay files | `oryx-bench flash` | Zero. No Oryx involvement at all. |
| 5 | **Switcher** | started in Oryx, then `oryx-bench detach` | overlay files | `oryx-bench flash` | One-time `detach`. After that, zero. |

The honest limit: **persona 5 cannot push their local changes back to
Oryx.** Going back means re-typing the layout in Oryx by hand, then
`attach`-ing. We document this prominently.

### Why personas 2 and 3 have zero friction

The thing that would otherwise create friction for personas 2/3 is "I edit
in Oryx, switch to my terminal, and have to remember to pull before any
oryx-bench command." We eliminate that with **auto-pull on read commands**
(see next section).

---

## Auto-pull mechanism

Goal: a user who edits in Oryx and then runs `oryx-bench show` should see
the new state immediately, with no `oryx-bench pull` ceremony.

Mechanism: every read-side command does a *cheap GraphQL metadata query*
(returns just the latest revision hash, ~1KB response) and compares
against the local cache's hash. If different, it pulls the full state
silently before running the command. A 60-second cache prevents three
back-to-back commands from making three GraphQL calls.

```toml
# kb.toml — defaults shown
[sync]
auto_pull        = "on_read"   # on_read | on_demand | never
poll_interval_s  = 60          # cap how often we ping Oryx
warn_if_stale_s  = 86400       # surface a hint in `status` if no pull in 1 day
```

### Per-command behavior

| Command | Auto-pull behavior |
|---|---|
| `show`, `explain`, `find` | Auto-pull if last metadata check > `poll_interval_s` ago and Oryx has a newer revision. Silent if no change; one-line notice if pulled. |
| `lint` | Same as above. After auto-pull, lint runs against the new state. |
| `build` | Same. If a pull happens, prints "Oryx had updates, pulled new revision X" before compiling. |
| `flash` | **Never auto-pulls.** Flashing is the moment of commitment; you flash exactly what you just looked at, no surprises. |
| `status` | Always does the metadata query (cheap, no full pull). Shows local revision, Oryx revision, last full pull, and any divergence. Never silent. |

### Disabling auto-pull

Users who want full manual control set `auto_pull = "on_demand"` in
`kb.toml`. The CLI also accepts `--no-pull` on read commands as a one-shot
override.

In **local mode** (`layout.toml`), auto-pull is a no-op — there's no Oryx
to pull from. The setting is ignored.

---

## Single source of truth, factored

Each concern owns exactly one file. No two files describe the same fact.
The build deterministically merges them.

| Concern | Source of truth | Edited by | Tier |
|---|---|---|---|
| Visual layout (positions, layer organization, basic combos) | `pulled/revision.json` *or* `layout.toml` | Oryx UI / text editor | 0 |
| Declarative QMK features (achordion, key overrides, macros, config.h, rules.mk) | `overlay/features.toml` | text editor / Claude | 1 |
| Procedural code (state machines, RGB animations, custom hooks) | `overlay/*.zig` | text editor / Claude | 2 |
| Vendored upstream C libraries | `overlay/*.c` | (paste only) | 2′ |
| Project meta-config (hash, geometry, build/flash backends, sync settings, lint ignores) | `kb.toml` | text editor / Claude | n/a |
| Built firmware | `result/firmware.bin` (gitignored) | derived from all of the above | n/a |

These never collide because they target *different concerns* and *different
files*. Oryx structurally cannot express achordion. The overlay should not
manage letter positions. The kb.toml does not contain layout data.

---

## Repo layout

```
oryx-bench/
├── README.md                          # quickstart, install, screenshots
├── ARCHITECTURE.md                    # this file
├── CONTRIBUTING.md                    # how to add keyboards / lint rules / overlay features
├── CHANGELOG.md
├── LICENSE                            # MIT
├── Cargo.toml
├── Cargo.lock
├── src/                               # Rust source
│   ├── main.rs
│   ├── cli.rs                         # clap definitions
│   ├── config.rs                      # kb.toml parsing + project root discovery
│   ├── error.rs
│   ├── commands/                      # one file per subcommand
│   │   ├── mod.rs
│   │   ├── setup.rs
│   │   ├── init.rs                    # --hash | --blank
│   │   ├── attach.rs                  # local mode → Oryx mode
│   │   ├── detach.rs                  # Oryx mode → local mode
│   │   ├── pull.rs
│   │   ├── show.rs
│   │   ├── explain.rs
│   │   ├── find.rs
│   │   ├── lint.rs
│   │   ├── diff.rs                    # semantic diff vs git ref
│   │   ├── build.rs
│   │   ├── flash.rs                   # supports --dry-run, --yes
│   │   ├── status.rs
│   │   └── skill.rs                   # install / remove
│   ├── schema/                        # serde types and lookup tables
│   │   ├── mod.rs
│   │   ├── oryx.rs                    # Oryx GraphQL JSON shape (camelCase, lossless)
│   │   ├── layout.rs                  # layout.toml schema (local mode)
│   │   ├── features.rs                # overlay/features.toml schema (Tier 1)
│   │   ├── kb_toml.rs                 # kb.toml schema
│   │   ├── keycode.rs                 # QMK keycode catalog (typed enum + Other catch-all)
│   │   ├── canonical.rs               # the internal Layout representation both sources produce
│   │   └── geometry/                  # extension point for new keyboards
│   │       ├── mod.rs                 # Geometry trait + registry
│   │       ├── voyager.rs             # Voyager-specific positions, encoder=0, thumbs=4
│   │       └── README.md              # how to add a new keyboard
│   ├── pull/
│   │   ├── mod.rs                     # auto-pull logic + 60s cache
│   │   └── graphql.rs                 # GraphQL client + queries (metadata + full)
│   ├── generate/
│   │   ├── mod.rs                     # canonical Layout + features.toml + overlay/ → keymap.c + config.h + rules.mk
│   │   ├── keymap.rs                  # the LAYOUT() emitter
│   │   ├── features.rs                # features.toml → C source
│   │   ├── config_h.rs                # config.h emitter
│   │   └── rules_mk.rs                # rules.mk emitter (SRC += for overlay/*.c; Zig wiring is v0.2+)
│   ├── render/
│   │   ├── mod.rs
│   │   └── ascii.rs                   # hand-rolled split-grid renderer (NOT tabled)
│   │   # svg.rs (keymap-drawer subprocess wrapper) is v0.2+ — not in v0.1.
│   ├── lint/
│   │   ├── mod.rs                     # rule runner, Issue type
│   │   └── rules/                     # one file per rule (extension point)
│   │       ├── mod.rs                 # registry
│   │       ├── lt_on_high_freq.rs
│   │       ├── unreachable_layer.rs
│   │       ├── kc_no_in_overlay.rs
│   │       ├── orphaned_mod_tap.rs
│   │       ├── unknown_keycode.rs
│   │       ├── unknown_layer_ref.rs
│   │       ├── duplicate_action.rs
│   │       ├── mod_tap_on_vowel.rs
│   │       ├── tt_too_short.rs
│   │       ├── home_row_mods_asymmetric.rs
│   │       ├── not_pulled_recently.rs
│   │       ├── oryx_newer_than_build.rs
│   │       ├── overlay_dangling_position.rs    # cross-tier: features.toml or zig references a position that doesn't exist
│   │       ├── overlay_dangling_keycode.rs     # cross-tier: features.toml references a keycode not in any visual binding
│   │       ├── custom_keycode_undefined.rs     # cross-tier: visual layout binds USERnn but no overlay defines it
│   │       └── unreferenced_custom_keycode.rs  # info: overlay defines a custom keycode no layer uses
│   ├── build/
│   │   ├── mod.rs                     # backend dispatch (v0.1: docker-only)
│   │   └── docker.rs                  # bundled image at ghcr.io/enriquefft/oryx-bench-qmk
│   ├── flash/
│   │   ├── mod.rs                     # detect + dispatch + --dry-run + --yes
│   │   ├── wally.rs                   # wally-cli wrapper (if available)
│   │   └── keymapp.rs                 # GUI handoff (writes .bin to known cache, prints instructions)
│   ├── skill/
│   │   ├── mod.rs                     # install/remove logic, project-local default
│   │   └── embedded.rs                # include_str! of the SKILL.md tree
│   └── util/
│       ├── mod.rs
│       ├── git.rs                     # shells out to `git` (no git2 dep)
│       ├── toolchain.rs               # which() detection of qmk, gcc-arm, zig, docker, wally-cli, keymapp
│       ├── fs.rs                      # atomic write, project root discovery
│       └── http.rs
├── xtask/                             # cargo xtask for codegen of skill reference files
│   ├── Cargo.toml
│   └── src/
│       └── main.rs                    # `cargo xtask gen-skill-docs` → updates skills/oryx-bench/reference/{lint-rules,command-reference}.md
├── skills/                            # canonical skill source (the binary embeds these)
│   └── oryx-bench/
│       ├── SKILL.md                   # entry point — what Claude reads first
│       └── reference/
│           ├── workflows.md           # daily-use playbooks
│           ├── lint-rules.md          # GENERATED by xtask from src/lint/rules/
│           ├── overlay-cookbook.md    # achordion / key overrides / macros recipes (TOML form + Zig form)
│           └── command-reference.md   # GENERATED by xtask from clap defs
├── tests/
│   ├── fixtures/
│   │   ├── voyager_dvorak.json        # snapshot of a real Oryx layout
│   │   ├── voyager_clean.json
│   │   ├── voyager_with_combos.json
│   │   └── layout_local.toml          # a local-mode layout.toml fixture
│   ├── codegen_roundtrip.rs           # generate keymap.c → qmk c2json → assert canonical equality
│   ├── lint_rules.rs                  # one test per rule, positive + negative
│   ├── render_snapshot.rs             # insta snapshots of show/explain/find
│   ├── pull_mock.rs                   # wiremock-based GraphQL pull tests
│   ├── auto_pull.rs                   # cache hit/miss/staleness behavior
│   ├── attach_detach.rs               # mode switching round trip
│   ├── skill_install.rs               # tests `skill install` project-local + global + idempotency
│   ├── skill_drift.rs                 # asserts the bundled skill files match skills/oryx-bench/ on disk
│   └── cli_smoke.rs                   # end-to-end: init → pull (mock) → show → lint
├── examples/
│   └── voyager-dvorak/                # template that `oryx-bench init` writes (sans .claude)
│       ├── kb.toml
│       ├── pulled/
│       │   └── revision.json          # the user's actual current Oryx state (as of design phase)
│       ├── overlay/
│       │   ├── README.md
│       │   ├── features.toml          # Tier 1 declarative — achordion + key overrides + tapping term
│       │   └── _vendored/             # any Tier 2′ C files would land here, prefixed for clarity
│       └── .gitignore
├── packaging/
│   ├── nix/
│   │   ├── flake.nix                  # nix run / nix build / dev shell
│   │   └── module.nix                 # optional NixOS module
│   ├── homebrew/
│   │   └── oryx-bench.rb              # tap formula
│   ├── aur/
│   │   └── PKGBUILD
│   └── docker/
│       ├── Dockerfile                 # bundles qmk + arm-none-eabi-gcc + zig + ZSA qmk_firmware
│       ├── pin.txt                    # specific zsa/qmk_firmware commit hash
│       └── README.md                  # spec for what's in the image
├── scripts/
│   ├── install.sh                     # curl-pipe-bash installer
│   └── release.sh
├── .github/workflows/
│   ├── ci.yml                         # cargo test, clippy, fmt, xtask drift check, on every push
│   ├── release.yml                    # cargo dist on tag
│   └── docker.yml                     # publish image to ghcr.io
├── flake.nix                          # root convenience for nix users
└── .gitignore
```

---

## Module breakdown

### `src/schema/oryx.rs` — the Oryx GraphQL data model

Rust types matching the GraphQL response shape with `#[serde(rename_all =
"camelCase")]` because Oryx returns camelCase. All optional fields use
`#[serde(default)]` so missing fields don't fail deserialization. Forward
compatibility via `#[serde(flatten)] extra: HashMap<String, Value>` on
every struct.

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub hash_id: String,
    pub title: String,
    pub geometry: String,                       // "voyager", "moonlander", ...
    pub privacy: bool,
    pub revision: Revision,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    pub hash_id: String,
    pub qmk_version: String,
    pub title: String,
    pub created_at: String,                     // ISO 8601 from Oryx
    pub model: String,                          // "v1", etc.
    pub md5: String,
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub combos: Vec<Combo>,
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub swatch: Option<Vec<String>>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Layer {
    pub title: String,
    pub position: u8,
    pub keys: Vec<Key>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Key {
    #[serde(default)]
    pub tap: Option<Action>,
    #[serde(default)]
    pub hold: Option<Action>,
    #[serde(default)]
    pub double_tap: Option<Action>,
    #[serde(default)]
    pub tap_hold: Option<Action>,
    #[serde(default)]
    pub tapping_term: Option<u32>,
    #[serde(default)]
    pub custom_label: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub emoji: Option<String>,
    #[serde(default)]
    pub glow_color: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Action {
    pub code: String,                           // "KC_BSPC", "MO", "LT", etc. — see schema/keycode.rs
    #[serde(default)]
    pub layer: Option<u8>,
    #[serde(default)]
    pub modifier: Option<String>,
    #[serde(default)]
    pub modifiers: Option<Vec<String>>,
    #[serde(default, rename = "macro")]
    pub macro_: Option<MacroDef>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
```

The `extra: HashMap<String, Value>` on every struct is **mandatory**, not
optional. It guarantees we don't drop fields Oryx may add in the future,
which would silently corrupt round-trip behavior. The
`tests/codegen_roundtrip.rs` suite verifies this by deserializing,
re-serializing, and asserting structural equality on each fixture.

### `src/schema/canonical.rs` — the internal `Layout` representation

The shape both `oryx.rs` (Oryx mode) and `layout.rs` (local mode)
deserialize into. This is the *single* type the rest of the codebase
operates on. Adding a new layout source format is a matter of adding a new
deserializer that produces `canonical::Layout`.

```rust
pub struct CanonicalLayout {
    pub geometry: GeometryName,         // enum
    pub title: String,
    pub layers: Vec<CanonicalLayer>,
    pub combos: Vec<CanonicalCombo>,
    pub config: BTreeMap<String, ConfigValue>,
}

pub struct CanonicalLayer {
    pub name: String,                   // human title from Oryx, sanitized for C-ident in generation
    pub position: u8,                   // matches Oryx's Layer.position (0..n)
    pub keys: Vec<CanonicalKey>,
}

pub struct CanonicalKey {
    pub tap: Option<CanonicalAction>,
    pub hold: Option<CanonicalAction>,
    pub double_tap: Option<CanonicalAction>,
    pub tap_hold: Option<CanonicalAction>,
    pub tapping_term: Option<u32>,
    pub custom_label: Option<String>,
}

pub enum CanonicalAction {
    Keycode(Keycode),                                       // typed enum from schema/keycode.rs
    Mo  { layer: LayerRef },                                // momentary
    Tg  { layer: LayerRef },                                // toggle
    To  { layer: LayerRef },
    Tt  { layer: LayerRef },
    Df  { layer: LayerRef },
    Lt  { layer: LayerRef, tap: Box<CanonicalAction> },     // layer-tap
    ModTap { mod_: Modifier, tap: Box<CanonicalAction> },
    Modifier(Modifier),
    Macro(MacroId),
    Custom(CustomKeycodeId),                                // USER01..USER15
    Transparent,                                            // KC_TRNS
    None,                                                   // KC_NO
}

pub enum LayerRef {
    Name(String),                       // resolved by lookup table
    Index(u8),                          // raw integer (only when name is unknown)
}
```

### `src/schema/keycode.rs` — the QMK keycode catalog

A finite enum (~250 variants) with a `Other(String)` catch-all for
forward-compat. Each variant has metadata via methods on the enum:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Keycode {
    KcNo,
    KcTransparent,
    KcA, KcB, KcC, /* ... */
    KcBspc, KcDel, KcEnt, KcSpc, KcTab, KcEsc,
    KcLctl, KcLsft, KcLalt, KcLgui,
    /* ... */
    Other(String),                      // for forward-compat with new QMK keycodes
}

impl Keycode {
    pub fn is_high_frequency(&self) -> bool {
        matches!(self, Keycode::KcBspc | Keycode::KcDel | Keycode::KcEnt
                     | Keycode::KcSpc  | Keycode::KcTab | Keycode::KcEsc)
    }

    pub fn is_modifier(&self) -> bool { /* ... */ }
    pub fn is_alpha(&self) -> bool { /* ... */ }
    pub fn is_vowel(&self) -> bool { /* ... */ }
    pub fn category(&self) -> KeycodeCategory { /* ... */ }
    pub fn from_str(s: &str) -> Self { /* parses, returns Other(s) on miss */ }
    pub fn to_str(&self) -> Cow<'static, str> { /* canonical name */ }
}
```

Tests assert that every `Keycode` variant round-trips through `to_str /
from_str`. The catch-all `Other(String)` ensures the codegen still works
for keycodes we haven't catalogued — it emits the literal string into the
generated `keymap.c`. New QMK releases get folded in by adding variants;
the lint rules then start being able to reason about them.

### `src/schema/geometry/` — the keyboard extension point

Each geometry implements a trait. Adding a new keyboard is "create one
file in this directory and register it." See `CONTRIBUTING.md` for the
step-by-step.

```rust
pub trait Geometry: Send + Sync {
    /// Stable identifier matching Oryx's `geometry` field.
    fn id(&self) -> &'static str;

    /// Human display name.
    fn display_name(&self) -> &'static str;

    /// Number of matrix keys (excludes encoders).
    fn matrix_key_count(&self) -> usize;

    /// Number of encoders. Voyager: 0. Moonlander: 2. Ergodox EZ: 1.
    fn encoder_count(&self) -> usize;

    /// Position name → index in the flat matrix array.
    fn position_to_index(&self, name: &str) -> Option<usize>;

    /// Reverse map.
    fn index_to_position(&self, index: usize) -> Option<&'static str>;

    /// Layout for the ASCII split-grid renderer.
    fn ascii_layout(&self) -> &'static GridLayout;

    /// QMK keyboard target name (e.g., "zsa/voyager", "zsa/moonlander").
    fn qmk_keyboard(&self) -> &'static str;

    /// Default LAYOUT() macro name for the QMK keymap.c.
    fn layout_macro(&self) -> &'static str;
}

pub struct GridLayout {
    pub halves: u8,                     // 2 for split, 1 for unibody
    pub rows: &'static [GridRow],
    pub thumb_clusters: &'static [ThumbCluster],
}
```

The trait deliberately separates `matrix_key_count` (the LAYOUT macro
positions) from `encoder_count` (which lives in QMK's separate
`encoder_map_t` data structure). The Voyager has 52 matrix keys and 0
encoders. The Moonlander has 72 matrix keys and 2 encoders. Adding
encoder support to the trait was an explicit design fix from the
architecture review — it would have leaked otherwise.

Concrete sketch for the Moonlander has been written as a
review-unblocking exercise (verifies no Voyager-specific assumptions
slipped into the trait); see `src/schema/geometry/README.md`.

### `src/schema/features.rs` — Tier 1 declarative QMK features

The schema for `overlay/features.toml`. Strongly typed, validated at parse
time, with cross-tier validation deferred to lint rules.

See the **`overlay/features.toml` schema** section below for the full
structure.

### `src/schema/layout.rs` — local-mode visual layout

The schema for `layout.toml`. Maps to the same `CanonicalLayout` as the
Oryx parser. See the **`layout.toml` schema** section below.

### `src/schema/kb_toml.rs` — project meta-config

The schema for `kb.toml`. See the **`kb.toml` reference** section below.

### `src/pull/` — Oryx integration

```rust
pub fn pull_if_stale(project: &Project, cli_no_pull: bool) -> Result<PullOutcome> {
    if cli_no_pull || project.cfg.sync.auto_pull == AutoPull::Never { return Ok(PullOutcome::Skipped); }
    if project.cfg.sync.auto_pull == AutoPull::OnDemand { return Ok(PullOutcome::Skipped); }

    if cache_age(&project) < project.cfg.sync.poll_interval_s { return Ok(PullOutcome::CacheHit); }

    let remote_hash = graphql::metadata_only(&project.cfg.layout.hash_id, &project.cfg.layout.geometry)?;
    if remote_hash == project.local_revision_hash() {
        touch_cache(&project)?;
        return Ok(PullOutcome::UpToDate);
    }

    let full = graphql::full_layout(&project.cfg.layout.hash_id, &project.cfg.layout.geometry)?;
    write_atomic(&project.pulled_path(), &full)?;
    Ok(PullOutcome::Pulled { from: ..., to: ... })
}
```

The cheap metadata query is one paragraph of GraphQL that returns just
`{ revision { hashId } }`. Saves bandwidth and rate-limit budget.

### `src/generate/` — the codegen layer

Translates `CanonicalLayout` + `features.toml` + `overlay/*` into the C
source files QMK consumes. Three sub-modules:

- `keymap.rs` — emits `keymap.c` with the `LAYOUT(...)` arrays
- `features.rs` — emits the C bodies for everything `features.toml`
  declares (achordion config, key overrides table, custom keycodes enum +
  process_record_user dispatch, combos, etc.)
- `config_h.rs` — emits `config.h` from features.toml's `[config]` section
- `rules_mk.rs` — emits `rules.mk` additions from features.toml + walks
  `overlay/` for `*.zig` and `*.c` to add to `SRC +=`

### Tap dance support

QMK's tap dance feature lets a single physical key behave differently based on
how many times it is tapped. The most common use case is "tap once for X, tap
twice quickly for Y." Oryx exposes this as the `double_tap` field on each key.

In oryx-bench, tap dances are not a Tier 1 feature the user declares in
`features.toml`. They are inferred from the visual layout: any key with a
non-null `double_tap` field becomes a tap dance entry at codegen time. The
user never writes tap-dance C by hand.

#### Pipeline

A `double_tap` key flows through five stages:

1. **Canonical schema** (`src/schema/canonical.rs`). `CanonicalKey.double_tap`
   is an `Option<CanonicalAction>` — the same action type used for `tap` and
   `hold`. Both parsers (Oryx and local mode) populate it from the
   corresponding source field. A key can have `double_tap` alone, or `tap` +
   `double_tap`, or other combinations. Not all combinations are valid (see
   "Supported combinations" below).

2. **Table builder** (`src/generate/mod.rs`). `build_tap_dance_table()` scans
   every layer for keys with `double_tap` set and assigns each a unique
   0-based `td_index`. Each `TapDanceEntry` records the layer position, key
   index, single-tap action (if any), and double-tap action. The table is a
   `Vec<TapDanceEntry>`, not a map — index assignment is sequential and
   deterministic.

3. **Keymap emission** (`src/generate/keymap.rs`). When emitting the
   `LAYOUT(...)` array, the codegen checks whether the current key has an
   entry in the tap-dance table. If it does, it emits `TD(n)` instead of the
   normal keycode token. A defensive guard in `emit_key()` rejects any key
   that still has `double_tap` set at emission time — such keys must be
   intercepted by the tap-dance table lookup, and reaching `emit_key()` means
   the table lookup was skipped (an internal bug).

4. **Features emission** (`src/generate/features.rs`). Two outputs:
   - `_features.h`: declares `enum tap_dance_ids { TD_0, TD_1, ... }` and
     `extern tap_dance_action_t tap_dance_actions[]` so `keymap.c` can
     reference both. The enum is only emitted when the table is non-empty.
   - `_features.c`: defines `tap_dance_actions[]` with one
     `ACTION_TAP_DANCE_DOUBLE(single, double)` per entry. The single-tap
     argument comes from the key's `tap` field (or `KC_NO` if absent); the
     double-tap argument comes from `double_tap`.

5. **rules.mk** (`src/generate/rules_mk.rs`). `TAP_DANCE_ENABLE = yes` is
   auto-enabled when the tap-dance table is non-empty, regardless of whether
   the user declared it in `[features]`. If the user explicitly set
   `tap_dance = true` in features, the normal feature-flag path handles it
   and no duplicate line is emitted.

#### Supported combinations

QMK's `ACTION_TAP_DANCE_DOUBLE(kc1, kc2)` only handles two outcomes (single
tap, double tap). This limits which `CanonicalKey` field combinations can be
translated:

| Fields present | Generated C | Notes |
|---|---|---|
| `double_tap` only | `ACTION_TAP_DANCE_DOUBLE(KC_NO, action)` | Single-tap does nothing; double-tap fires `action`. |
| `tap` + `double_tap` | `ACTION_TAP_DANCE_DOUBLE(tap_action, double_action)` | Single tap fires `tap_action`; double tap fires `double_action`. |
| `hold` + `double_tap` (no `tap`) | **Build error** | Requires `ACTION_TAP_DANCE_FN_ADVANCED` (three outcomes: nothing / hold / double-tap). Not yet supported. |
| `tap` + `hold` + `double_tap` | **Build error** | Three-way conflict. QMK has no built-in action that combines hold-tap semantics with tap-dance counting. |
| `tap_hold` + `double_tap` | **Build error** | `tap_hold` (Oryx's "also send on hold" feature) has no QMK equivalent at all when combined with tap dance. |

The errors are loud (`anyhow::bail!`) with messages that identify the layer
name, key position, and which combination is unsupported. Silent drops are
never acceptable — a key that the user sees in Oryx doing one thing but the
firmware silently ignores is a correctness bug.

#### Guard: explicit opt-out

If the user has `tap_dance = false` in `[features]` but the layout contains
keys with `double_tap`, the generated firmware would be broken: `TD(n)`
macros would be emitted into `keymap.c` but `TAP_DANCE_ENABLE` would be `no`,
causing a QMK compile error. The codegen catches this before writing any
files and fails with an explicit message telling the user to either remove
the `tap_dance = false` setting or remove the `double_tap` keys from the
layout.

### Codegen contract — what round-trips and what doesn't

The `tests/codegen_roundtrip.rs` test asserts: `revision.json → keymap.c
(via our generator) → keymap.json (via qmk c2json) → CanonicalLayout (via
qmk's parser) → assert canonical-equal to the input`.

This contract is **not** lossless for every Oryx feature. The exclusions:

| Oryx feature | Round-trip status | Why |
|---|---|---|
| Plain bindings (KC_*) | ✅ Lossless | One-to-one C macro mapping |
| MO / LT / TG / TO / TT / DF | ✅ Lossless | Direct C macro |
| Mod-tap (LCTL_T, etc.) | ✅ Lossless | Direct C macro |
| Tap-hold with `MO` as hold (= LT) | ✅ Lossless after canonical normalization | We split LT(N, K) back into `tap=K, hold=MO{layer=N}` for comparison |
| Combos (Oryx UI combos) | ⚠️ Generator emits as Tier 1; round-trip via `c2json` does not see them (they live in a separate QMK structure) | Tested separately via direct combo array comparison |
| Macros (Oryx-defined) | ⚠️ Emitted as `SEND_STRING` in `process_record_user`; one-way (c2json doesn't reverse) | Tested via separate macro fixture |
| Custom keycodes (USER01..15) | ⚠️ Emitted as enum members + dispatch; one-way | Tested via separate fixture |
| RGB per-key (`glowColor`) | ⚠️ Emitted as `rgb_matrix_set_color_by_index` calls in a user hook; one-way | Tested via separate fixture |

The round-trip test runs **after a canonical normalization** that drops
the one-way features and applies the LT splitting. The normalization
function is documented in `tests/codegen_roundtrip.rs` and is the explicit
spec of "what we promise round-trips."

For the one-way features, separate tests verify the *generated* C is
correct against hand-written reference C in fixtures. They don't try to
parse it back.

### Symbolic layer references

A long-running risk is hard-coded layer integers in overlay code:
`features.toml` says `LT(1, KC_BSPC)` and silently breaks if Oryx
reorders layers. **We don't allow this.** All overlay references to
layers use **layer names**, not integers:

```toml
[[achordion.timeout]]
binding = "LT(SymNum, BSPC)"   # ← layer NAME, not integer
ms = 600
```

The generator resolves names to integers at build time using the
canonical layout's layer table. A new lint rule
(`overlay_dangling_position`) catches dangling references — if
`features.toml` mentions `SymNum` but the canonical layout has no layer
by that name (perhaps it was renamed in Oryx), lint hard-errors before
build.

### Layer identity / sanitization

Oryx layer titles can contain spaces and special characters
(`"Sym + Num!"`). The generator must produce a valid C identifier for
the QMK enum. We sanitize via:

```
title = "Sym + Num!"
↓ sanitize_c_ident
ident = "SYM_NUM"
```

Rules:
1. Uppercase
2. Non-alphanumeric → underscore
3. Collapse repeated underscores
4. Strip leading/trailing underscores
5. Prefix with `L_` if the result starts with a digit

If two layers sanitize to the same identifier, the codegen auto-disambiguates
by appending the layer position (e.g. `LAYER_1`, `LAYER_2`). The
`layer_name_collision` lint fires as a **Warning** recommending unique names
for readability, but the build succeeds regardless.

### `src/render/` — visualization

ASCII grid (`render::ascii`) is **hand-rolled**, not built on `tabled`.
The Voyager's split-grid-with-thumb-cluster shape doesn't fit `tabled`'s
rectangular model and would force a fight against the library. ~80 lines
of straightforward formatting code with `console` for ANSI styling.

SVG rendering is planned for v0.2+ and will shell out to
`keymap-drawer`. The path will be: convert our `CanonicalLayout` into
keymap-drawer's YAML format, write to a temp file, invoke
`keymap-drawer draw`, capture the SVG. Single subprocess per render.
Not wired in v0.1 — `render::ascii` is the only backend.

### `src/lint/rules/` — the lint extension point

Each rule implements `LintRule` and is registered in `rules/mod.rs`. The
`xtask gen-skill-docs` walks the registry at *runtime* (from a small
binary, not from `build.rs`) and emits
`skills/oryx-bench/reference/lint-rules.md`. The committed file is
checked in CI via `cargo xtask gen-skill-docs && git diff --exit-code
skills/`.

This avoids the build.rs trap (build scripts cannot easily compile and
execute downstream code).

Cross-tier rules are first-class:

| Rule | Catches |
|---|---|
| `overlay_dangling_position` | `features.toml` references a position name not in the canonical layout |
| `overlay_dangling_keycode` | `features.toml` references a keycode (e.g., for a key override) not bound anywhere in the visual layout |
| `custom_keycode_undefined` | Oryx visual layout binds USER03 but no overlay file defines it |
| `unreferenced_custom_keycode` | Overlay defines `CK_EMAIL` but no visual layout binds USER01 |
| `process_record_user_collision` | Two overlay files (or features.toml-generated + a hand-written `.zig`) both define `process_record_user` without a clear ownership marker |

### `src/build/` — the build pipeline (v0.1: docker-only)

**Cut from the original design:** native and nix backends. v0.1 ships
with **docker only**. Reasoning: every install path (Mac, Linux, NixOS)
already has docker as a viable install. The bundled image
(`ghcr.io/enriquefft/oryx-bench-qmk:<sha>`) is one thing to maintain,
one set of bug reports, one reproducibility story. Native and nix
backends are tracked for a future release.

The Docker image contents are pinned and documented in
`packaging/docker/README.md`:

```
ghcr.io/enriquefft/oryx-bench-qmk:<release-tag>
├── debian:bookworm-slim base
├── arm-none-eabi-gcc 13.2.1
├── arm-none-eabi-binutils 2.42
├── newlib-arm-none-eabi 4.4
├── qmk 1.2.0 + python deps (appdirs, hjson, jsonschema, milc, pygments, dotty-dict, pillow)
├── zig 0.13.0  # pinned per release
└── zsa/qmk_firmware @ <commit-sha>  # pinned per release
```

Image size target: ≤ 1GB compressed. Pulled once on first build, cached
forever in the local Docker store.

### `src/flash/` — flashing

Two backends, with detection and fallback:

1. **`wally-cli`** if on PATH — invoked directly
2. **Keymapp GUI handoff** as fallback — copies the `.bin` to
   `~/.cache/oryx-bench/firmware.bin` and prints platform-specific
   instructions for how to open Keymapp's flasher and select that file

We **never** invoke `dfu-util`. The Voyager's flashing protocol is
custom and bricking risk is real.

`flash --dry-run`: prints the path, size, sha256, and target device of
the firmware that would be flashed, then exits. Used by Claude Code to
confirm "this is what we'd ship" before asking for human approval.

`flash --yes`: skips the interactive confirmation prompt for use in
agent loops. The agent must still have explicit user approval in the
conversation; this flag just bypasses the CLI's own re-confirmation.

### `src/skill/` — embedded skill installer

The skill files (SKILL.md + reference/*) are bundled into the binary via
`include_str!`. Two of the four reference files are *generated* — see
the xtask section below.

Install location:

- **Project-local default**: `<project>/.claude/skills/oryx-bench/`
- **Global (discouraged)**: `~/.claude/skills/oryx-bench/` via `--global`

Project-local is the default and is the only mode mentioned in user-
facing docs. The `--global` flag exists but the help text and `setup`
both warn that it pollutes the context budget of every unrelated Claude
Code session.

### `xtask/` — the codegen-of-docs binary

Replaces the `build.rs` walking-source-files trap. `xtask` is a separate
crate in the workspace that depends on the main `oryx-bench` crate, so
it can call into `lint::rules::registry()` and `cli::Cli::command()` at
runtime to generate markdown.

```bash
cargo xtask gen-skill-docs
```

This regenerates `skills/oryx-bench/reference/lint-rules.md` and
`skills/oryx-bench/reference/command-reference.md`. CI runs:

```bash
cargo xtask gen-skill-docs
git diff --exit-code skills/oryx-bench/reference/
```

…to assert the committed files are up-to-date with the source. Drift is
caught at PR time.

---

## The `kb.toml` reference

Per-project file. Lives at the project root.

```toml
# kb.toml — project configuration

[layout]
hash_id  = "yrbLx"          # required in Oryx mode; absent in local mode
geometry = "voyager"        # voyager | moonlander | ergodox (v0.1: voyager only)
revision = "latest"         # or a specific revision hash to pin

[layout.local]              # only present in local mode
file = "layout.toml"        # path relative to project root

[build]
backend    = "docker"       # v0.1: docker (or "auto", which resolves to docker)
qmk_pin    = "auto"         # "auto" uses the docker image's bundled fork; or a commit SHA
zig_pin    = "auto"         # same — pinned per docker image release

[sync]
auto_pull       = "on_read" # on_read | on_demand | never
poll_interval_s = 60        # cap how often we check Oryx for updates
warn_if_stale_s = 86400     # 1 day — surface a hint in `status` if no full pull

[lint]
ignore = []                 # list of rule IDs to silence (e.g. ["mod-tap-on-vowel"])
strict = false              # treat warnings as errors
```

### Revision pinning semantics

- `revision = "latest"` (default): every `oryx-bench pull` fetches
  whatever Oryx considers the latest revision of this layout.
- `revision = "<hash>"` (pinned): pull only fetches that specific
  revision. If Oryx has a newer revision available,
  `oryx-bench status` warns about it but `pull` does NOT auto-bump.
  Bumping is a manual edit of `kb.toml`.

The pin lets users freeze a known-good version of their layout while
they iterate on overlay code without worrying about visual layout drift.

### QMK fork upgrade story

`qmk_pin = "auto"` defers to whatever version is bundled in the docker
image. When ZSA rolls `firmware24` → `firmware25`, the user updates
oryx-bench (`cargo install --force` or equivalent), which pulls a new
docker image tag with the new fork. They get a notice on the next build:

```
Warning: this project was last built against oryx-bench 0.4.x (qmk firmware24).
This is oryx-bench 0.5.0 (qmk firmware25). The QMK ecosystem may have
changes that affect your overlay. Run `oryx-bench upgrade-check` to scan.
```

`oryx-bench upgrade-check` re-runs lint with the new keycode
catalog, surfaces any new rules, and checks for keycodes that have been
renamed or removed.

---

## The `overlay/features.toml` schema (Tier 1)

```toml
# overlay/features.toml — declarative QMK features
#
# Anything in here gets compiled into the firmware via generated C code.
# You never see the generated C unless you explicitly ask for it via
# `oryx-bench build --emit-overlay-c`.

# ── Global tunables ─────────────────────────────────────────────────────
[config]
tapping_term_ms      = 220
permissive_hold      = false
hold_on_other_key_press = false
caps_word_idle_timeout_ms = 5000

# ── Achordion (the LT-on-high-freq fix) ─────────────────────────────────
[achordion]
enabled        = true
chord_strategy = "opposite_hands"   # opposite_hands | always | never

  [[achordion.timeout]]
  binding = "LT(SymNum, BSPC)"     # symbolic — uses Oryx layer name
  ms      = 600

  [[achordion.timeout]]
  binding = "LT(System, DEL)"
  ms      = 600

  [[achordion.no_streak]]
  binding = "LT(SymNum, BSPC)"

  # Same-hand chords you want to allow despite the opposite-hands rule:
  # [[achordion.same_hand_allow]]
  # tap_hold = "LSFT_T(KC_A)"
  # other    = "KC_R"

# ── Key overrides ───────────────────────────────────────────────────────
[[key_overrides]]
mods  = ["LSHIFT"]
key   = "BSPC"
sends = "DELETE"

[[key_overrides]]
mods  = ["LSHIFT"]
key   = "ESC"
sends = "S(GRAVE)"

[[key_overrides]]
mods  = ["LCTRL"]
key   = "SCLN"
sends = "COLN"

# ── Macros (string-sending custom keycodes) ─────────────────────────────
[[macros]]
name  = "CK_EMAIL"          # binds to a USER slot in Oryx; oryx-bench picks the slot
sends = "you@example.com"

[[macros]]
name  = "CK_GIT_STATUS"
sends = "git status\n"

# ── Combos ──────────────────────────────────────────────────────────────
# Prefer Oryx UI for combos when possible (it's editable visually). Use
# overlay combos when you need per-layer scope or custom timeouts.
[[combos]]
keys      = ["L_index_top", "L_middle_top"]   # symbolic position names
sends     = "ESC"
layer     = "Main"                             # only fires on this layer
timeout_ms = 30

# ── Per-key tapping term overrides ──────────────────────────────────────
[[tapping_term_per_key]]
binding = "LCTL_T(KC_A)"
ms      = 180

# ── Vendored upstream that needs a feature flag ─────────────────────────
[features]
key_overrides = true        # enables KEY_OVERRIDE_ENABLE in rules.mk
combos        = true        # enables COMBO_ENABLE
caps_word     = true        # enables CAPS_WORD_ENABLE
mouse_keys    = false
```

The generator translates this into:

- A patch to `config.h` (the `[config]` section)
- A patch to `rules.mk` (`KEY_OVERRIDE_ENABLE = yes`, etc., from `[features]`)
- A generated `_features.c` containing:
  - `enum custom_keycodes { ... }` (from `[[macros]]`)
  - `key_override_t` definitions (from `[[key_overrides]]`)
  - `combos[]` array and `process_combo_event` (from `[[combos]]`)
  - `process_record_user` dispatch that handles macros and chains to
    user-authored Tier 2 code if present
  - `get_tapping_term` overrides (from `[[tapping_term_per_key]]`)
  - Achordion glue + per-key `achordion_*` callbacks (from `[achordion]`)
- A vendored `_achordion.c` (the upstream library body) is bundled in the
  binary and emitted only if `[achordion] enabled = true`

The user never sees `_features.c` or `_achordion.c` unless they pass
`--emit-overlay-c` to `build`. They live in the docker container's build
directory.

### `process_record_user` ownership

This is a known collision risk: the generated `_features.c` defines a
`process_record_user`, and a Tier 2 `overlay/foo.zig` might want to as
well. The generator handles this by:

1. The generated `process_record_user` always runs first
2. After dispatching its own (macros, custom keycodes from features.toml),
   it calls `process_record_user_overlay(keycode, record)` if any Tier 2
   file declares one (detected by symbol scan)
3. Tier 2 code that wants the hook implements
   `process_record_user_overlay` instead of `process_record_user`
4. The lint rule `process_record_user_collision` catches any Tier 2 file
   that defines `process_record_user` directly and tells the user to
   rename it

This is documented in `skills/oryx-bench/reference/overlay-cookbook.md`
prominently — it's the most likely source of weird build errors for
Tier 2 authors.

---

## The `layout.toml` schema (local mode)

```toml
# layout.toml — local-mode visual layout
#
# This file replaces pulled/revision.json for users who don't want to use
# Oryx. Authoring is by hand or by Claude Code. Comments are encouraged.

[meta]
title    = "Dvorak Custom"
geometry = "voyager"

# ── Layer 0: Main (Dvorak) ─────────────────────────────────────────────
[[layers]]
name = "Main"
position = 0

# Keys are addressed by symbolic position name. Any position you don't
# specify defaults to KC_NO. Use `inherit = "<layer>"` to default to
# transparent fall-through instead.
#
# Position naming is column-first: <HAND>_<COL>_<ROW> where COL is one
# of outer/pinky/ring/middle/index/inner and ROW is one of
# num/top/home/bottom. The "outer" column is the leftmost extension on
# the left half (and the rightmost on the right half).
[layers.keys]
L_outer_num   = { tap = "DEL",  hold = "MO(System)" }
L_pinky_num   = "1"
L_ring_num    = "2"
L_outer_top   = "LGUI"
L_pinky_top   = "QUOTE"
L_ring_top    = "COMMA"
L_middle_top  = "DOT"
L_index_top   = "P"
L_inner_top   = "Y"
L_outer_home  = { hold = "LSHIFT" }
L_pinky_home  = "A"
# ... etc, one entry per position you want bound
R_thumb_inner = { tap = "ENTER", hold = "LALT" }
R_thumb_outer = { tap = "BSPC",  hold = "MO(SymNum)" }

# ── Layer 1: SymNum (overlay) ──────────────────────────────────────────
[[layers]]
name = "SymNum"
position = 1
inherit = "Main"            # KC_TRNS by default; only override what differs
[layers.keys]
L_outer_num   = "TAB"
R_pinky_home  = "MINUS"
R_index_home  = "5"
# ...

# ── Layer 2: System (overlay) ──────────────────────────────────────────
[[layers]]
name = "System"
position = 2
inherit = "Main"
[layers.keys]
# ...

# ── Layer 3: Gaming (overlay, currently unreachable) ───────────────────
[[layers]]
name = "Gaming"
position = 3
inherit = "Main"            # was the bug — original Oryx had KC_NO here
[layers.keys]
L_index_top  = "Q"
L_middle_top = "W"
# ...
# Lint will flag this layer as unreachable until you bind a key to enter it.
```

### Hand-authoring ergonomics

Authoring 52 keys × 4 layers in TOML by hand is real work. We mitigate:

- `inherit = "Main"` defaults all unspecified positions to `KC_TRNS` so
  overlay layers only need to mention the *differences*
- Position names are stable across geometries (column-first:
  `<HAND>_<COL>_<ROW>` where COL ∈ outer/pinky/ring/middle/index/inner
  and ROW ∈ num/top/home/bottom) so you only learn one vocabulary
- The compact form `L_pinky_top = "1"` for plain keys keeps line count
  manageable
- `oryx-bench show` renders the result to ASCII so you can verify each
  edit visually
- A future `oryx-bench tui` could provide a small interactive grid
  editor for users who really want one without a browser dependency

For users who don't want to author by hand: use Oryx mode. Hand-edit
local mode is for power users who specifically want it.

---

## CLI command surface

All v0.1 commands are implemented. The previous milestone column
(M1–M4) has been removed; everything in the table below ships in
v0.1.0.

| Command | Purpose |
|---|---|
| `oryx-bench setup [--full]` | Detect toolchain. No state changes. Idempotent. |
| `oryx-bench init --hash <H>` | Create Oryx-mode project. |
| `oryx-bench init --blank` | Create local-mode project (writes `layout.toml`). |
| `oryx-bench pull` | Manually fetch Oryx state. (Usually unnecessary thanks to auto-pull.) |
| `oryx-bench show [LAYER]` | Render a layer (or all) as ASCII split-grid. |
| `oryx-bench explain POS` | Cross-layer view of a position. |
| `oryx-bench find QUERY` | Search across layers. |
| `oryx-bench lint [--strict]` | Static analysis (21 rules). |
| `oryx-bench status` | One-screen overview of project state, sync, build cache, lint. |
| `oryx-bench skill install [--global]` | Install project-local Claude Code skill. |
| `oryx-bench skill remove` | Uninstall the skill. |
| `oryx-bench attach --hash <H>` | local mode → Oryx mode (refuses on dirty git working tree without `--force`). |
| `oryx-bench detach` | Oryx mode → local mode (one-way). |
| `oryx-bench build [--dry-run]` | Compile firmware via the Docker backend. |
| `oryx-bench flash [--dry-run] [--yes] [--force]` | Flash firmware (requires explicit user approval). Refuses to flash a stale build unless `--force`. |
| `oryx-bench diff [REF]` | Semantic diff vs git ref. |
| `oryx-bench upgrade-check` | Re-run lint with the current keycode catalog after a tool update. |

### Init command spec

`oryx-bench init` creates the following files in the current directory
(refuses to overwrite existing files):

**Oryx mode** (`init --hash <H>`):
```
./kb.toml                            # with [layout] hash_id, geometry
./pulled/                            # empty (first `oryx-bench pull` populates)
./overlay/README.md                  # placeholder explaining the directory
./overlay/features.toml              # empty stub with commented examples
./.gitignore                         # ignores result/, .oryx-bench/, etc.
```

After creation, prints:

```
✓ Created Oryx-mode project at ./
  Run: oryx-bench pull && oryx-bench show

💡 Using Claude Code? Run `oryx-bench skill install` to add the
   project-local skill that teaches Claude about this tool.
```

**Local mode** (`init --blank --geometry <G>`):
```
./kb.toml                            # with [layout.local] file = "layout.toml"
./layout.toml                        # empty layout with one base layer scaffold
./overlay/README.md
./overlay/features.toml
./.gitignore
```

The user has 7 days to make changes before lint starts complaining about
`not-pulled-recently` (Oryx mode only). In local mode that rule is a
no-op.

### Flash command — dry-run and approval semantics

`oryx-bench flash` asks for confirmation by default:

```
$ oryx-bench flash
About to flash:
  firmware:  /tmp/oryx-bench-build/firmware.bin
  size:      54918 bytes
  sha256:    7b3a1e...
  target:    ZSA Voyager (vendor 0x3297)
  via:       wally-cli

Continue? [y/N]
```

Flags:
- `--dry-run`: print the same info, exit without flashing
- `--yes`: skip the prompt (still requires the build to be successful)
- `--no-pull`: don't auto-pull before flashing (already the default for flash)

For Claude Code: the agent should always run `flash --dry-run` first to
show the user exactly what will ship, then run `flash --yes` once the
user has approved in the conversation.

---

## Network, security, telemetry

The only network call `oryx-bench` makes is `POST
https://oryx.zsa.io/graphql` during pull (manual or auto). Specifically:

- A *metadata-only* query returning `{ revision { hashId } }` (~1KB) on
  every read command, gated by `poll_interval_s` cache
- A *full layout* query (~50KB) only when the metadata indicates the
  cache is stale
- No auth headers
- No User-Agent string beyond the default reqwest one
- No retry logic that hits Oryx hard
- No telemetry, no auto-update checks, no analytics

`oryx-bench build --backend docker` pulls the build image from
`ghcr.io/enriquefft/oryx-bench-qmk:<sha>` on first use. Same image, no
talkback.

In **local mode**, there are zero network calls. Period.

---

## Testing strategy

| Layer | Test type | Tool |
|---|---|---|
| Schema parsing (Oryx) | Unit + property | serde + proptest with real fixtures |
| Schema parsing (layout.toml) | Unit + round-trip | serde + manually crafted fixtures |
| Schema parsing (features.toml) | Unit + cross-tier validation | serde + paired with layout fixtures |
| Generator (one-way features) | Direct comparison against hand-written reference C | std test |
| Generator (round-trippable features) | Round-trip via `qmk c2json` after canonical normalization | std test, requires qmk on PATH |
| Lint rules (per rule) | Positive + negative | std test |
| Cross-tier lint rules | Multi-fixture | std test |
| Render (ASCII) | Snapshot | `insta` |
| Render (SVG) | File-exists + size sanity | std test (full SVG comparison is too brittle) |
| CLI end-to-end | Real init → mock pull → show → lint → build (docker) | `assert_cmd` + `wiremock` for GraphQL |
| Auto-pull cache | Cache hit/miss/staleness | std test with mock clock |
| Skill drift | Embedded skill files match `skills/` on disk | xtask invocation in CI |
| Build | Docker image cached, full build only on dep changes | GH Actions matrix |

The codegen round-trip is the most important test. The lint rule tests
are the second most important. Both are pure functions; both run in
under 5 seconds for the entire suite.

---

## Verification log

Things we've already proven work, before writing any production code.
Each is a hard data point that the design rests on.

### V1: ZSA qmk_firmware fork builds on NixOS

**Verified:** `nix-shell -p qmk python3Packages.{appdirs,hjson,jsonschema,milc,pygments,dotty-dict,pillow}` followed by `git submodule update --init --recursive --depth=1` then `qmk compile -kb zsa/voyager -km default` produces `zsa_voyager_default.bin` (50846 bytes). First build time ~5min on a warm Nix store.

### V2: Overlay merge works

**Verified:** A `.c` file dropped in a keymap dir with one `SRC += foo.c` line in `rules.mk` is picked up by the build, linked into the firmware, and the symbol is reachable from `keymap.c` via `extern`. Tested with a no-op marker function; verified in the linked ELF symbol table.

### V3: kontroll runs

**Verified:** `nix run nixpkgs#kontroll list` runs cleanly, reports the API socket path it expects (`~/.config/.keymapp/keymapp.sock`), and exits 0 when the socket is missing. **Confirmed kontroll has no `flash` subcommand** — it's runtime-only (set-layer, set-rgb, etc.). For flashing we use `wally-cli` or Keymapp GUI.

### V4: Oryx GraphQL is unauthenticated and live

**Verified:** `POST https://oryx.zsa.io/graphql` with the introspection query returns the schema. The `layout(hashId, revisionId, geometry)` query returns the live current state, with `revisionId: "latest"` always reflecting the most recent edit (no compile step required). The `/source/{hash}` endpoint by contrast returns *only the most recently compiled* source for the layout's *original* geometry — not usable for our purposes.

### V5: The user's layout is fetchable

**Verified:** `curl -s -X POST https://oryx.zsa.io/graphql -d '{"query": "...layout(hashId: yrbLx, geometry: voyager, revisionId: latest)..."}'` returns 4 layers (Main, Sym+Num, Brd+Sys, Gaming), 52 keys per layer, with the `LT(SymNum, KC_BSPC)` on the right thumb confirming the bug. The full response is committed at `examples/voyager-dvorak/pulled/revision.json` (3788 lines) and is the test fixture for the Voyager geometry.

### V6: Zig + QMK link

**Verified:** A Zig file with `@cImport({ @cInclude("keycodes.h"); })` plus three exported C-ABI functions, compiled with:

```
zig build-obj zig_overlay.zig \
  -target thumb-freestanding-eabihf -mcpu cortex_m4 -O ReleaseSmall \
  -I/path/to/zsa-qmk/quantum
```

…produces a 908-byte ELF32 ARM object. `@cImport` correctly resolved
`KC_BSPC` to `0x2A` and `KC_A` to `0x04` (verified by disassembling the
output: `movs r0, #42; bx lr`). One `LDFLAGS += $(CURDIR)/.../zig_overlay.o`
line in the keymap's `rules.mk` was sufficient to link the Zig object
into the QMK build. Resulting firmware (54918 bytes) contains all three
Zig symbols at flash addresses `0x080022f8`, `0x080022fc`, `0x08002300`,
plus the C-side `keepalive` BSS variables that reference them. **No ABI
mismatches, no link warnings related to Zig**, only a benign newlib
`.note.GNU-stack` warning unrelated to Zig.

This is the load-bearing verification for the Tier 2 design.

### V7: The Oryx public source endpoint is unreliable

**Verified:** `GET https://oryx.zsa.io/source/yrbLx` returns the Moonlander source from 2021-03-05 — the *first compiled* geometry, regardless of the layout's current state. Adding `?geometry=voyager` is silently ignored. Conclusion: we cannot use `/source/` for pull. We must generate `keymap.c` ourselves from the GraphQL JSON. (This is the design.)

### V8: The user's local Keymapp cache is also stale

**Verified:** The sqlite cache at `~/.config/.keymapp/keymapp.sqlite3` contains revision `zKOp4` from 2024-10-04, but the live Oryx state (via GraphQL) shows revision `XX44B` from 2024-12-14. Real-world demonstration that the GraphQL endpoint is the only correct pull source — both `/source/` and the Keymapp cache lag behind live Oryx.

---

## Why Rust + Zig

**Rust for the CLI:**
- Single static binary for distribution to non-Python users
- `cargo dist` automates per-platform GitHub releases
- `serde` makes the Oryx JSON parsing self-documenting and type-checked
- `kontroll` is Rust — ZSA ecosystem alignment for any future integrations
- Sub-10ms startup matters for a CLI Claude Code invokes tens of times
  per session
- `include_str!` lets the binary ship the skill files atomically

**Zig for Tier 2 overlay code:**
- Same metal-target binary as C
- `@cImport` reads QMK headers directly (no bindgen, no codegen layer)
- Type safety, comptime, real error messages
- Pre-1.0 status mitigated by pinning the Zig version per oryx-bench
  release
- Cortex-M4 first-class target
- Verified end-to-end (V6 above)

C is not a goal of the project. C exists only as a compatibility tier
(Tier 2′) for vendored upstream code we don't author.

---

## Why Oryx is optional

The previous design treated Oryx as a hard dependency. The realization
during design review was that this excludes a meaningful audience:

- People who don't want a ZSA account
- People who don't want their layout on a third-party server
- People who want git history of their visual layout (not just behavior)
- People who want to build firmware in CI without Oryx auth
- People who want comments and refactoring tools applied to their
  layout itself

Adding `layout.toml` as a peer to `pulled/revision.json` costs ~300 LOC
of TOML schema + parser + the migration commands, and unlocks personas 4
and 5 entirely. The Oryx-mode users are unaffected (it's still the
default).

The framing: **`oryx-bench` is the workbench for ZSA keyboards. Oryx-friendly,
not Oryx-required.**

---

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| ZSA changes the GraphQL schema | Pin a query version; integration test runs on every release; failure mode is "pull fails loudly", not silent corruption |
| ZSA's `qmk_firmware` fork moves | Pin commit SHA in the docker image; bump explicitly per release |
| Generator misses a rare keycode | `Keycode::Other(String)` catch-all preserves the literal; lint surfaces for review; codegen still works |
| `wally-cli` is deprecated and may eventually break | Fallback path is "copy `.bin` and tell user to use Keymapp GUI" — always works |
| Big initial docker pull (~1GB) | Cached forever after first pull; `setup` warns about size before first build |
| Zig 0.x breaking changes | Pin Zig version per release; bump deliberately; no SemVer guarantees while Zig is pre-1.0 |
| Oryx is down | Pull fails; local-mode users unaffected; Oryx-mode users can `oryx-bench detach` to switch to local mode if outage is long |
| User accidentally runs `attach` and overwrites local edits | `attach` requires `--force` if a `layout.toml` exists with uncommitted changes; otherwise refuses |
| User flashes a bricking firmware (theoretically) | Voyager has a physical reset button + ZSA's recovery procedure via Keymapp; firmware lint catches obvious issues; we never invoke `dfu-util` directly |

---

## What's tracked for a future release

Explicitly cut from v0.1 to ship faster:

- **Native and Nix build backends.** Docker is the v0.1 path. Native and
  Nix come back in v0.2 once we have docker stable.
- **`oryx-bench live`** (kontroll integration for runtime layer state).
- **Moonlander and Ergodox geometries.** Voyager only in v0.1. Adding
  geometries is per-keyboard work (a single file in `src/schema/geometry/`
  + a fixture + tests) and is documented in `CONTRIBUTING.md`.
- **`oryx-bench tui`** for hand-editing local mode visually.
- **`oryx-bench upgrade-check`** for QMK fork bumps.
- **User-defined lint rules** via a DSL.
- **A `cargo xtask gen-fixture <hash>`** convenience for downloading test
  fixtures.

What ships in v0.1.0:

- Read-side: setup, init (both modes), pull, show, explain, find,
  lint, status, skill install/remove, auto-pull cache
- Write-side: attach, detach, build (docker only), all generators
  (keymap, features.c, features.h, config.h, rules.mk)
- Hardware: flash (wally + Keymapp fallback), --dry-run, --yes,
  --force; build-freshness check refuses stale flashes
- Polish: diff (semantic vs git ref), upgrade-check, 21 lint rules

---

## Open questions

These remain undecided as of v0.1.0:

1. **Should the `xtask gen-skill-docs` output also include a versioned
   change log so users can see what's new in `lint-rules.md` between
   releases?** Lean yes; tracked for a future release.
2. **Should `init` accept a `--from <other-project>` flag** to bootstrap
   from a sibling project's overlay/ directory? Useful for users with
   multiple keyboards. Defer to v0.2.
3. **Should we ship a JSON-output flag (`--json`) on every read command**
   for programmatic use (especially by Claude Code)? Probably yes, defer
   to a future release.
4. **Should we publish a public Oryx layout that users can fork** as the
   "starter template" for an oryx-bench-friendly layout? Low cost; defer
   until we have v0.1 users to ask.
5. **Should `setup` test that the docker image is pullable** by actually
   pulling it, or just check that the docker daemon is reachable?
   Lean: just check the daemon; pulling 1GB during `setup` is rude.
