# oryx-bench

> A workbench for ZSA keyboard layouts. Visual editing in Oryx (or no Oryx
> at all), modern declarative config + Zig for advanced features, one-command
> deterministic builds, designed to be driven by humans **and** by Claude Code.

> **Status: v0.1.0.** Voyager support, Docker build backend, wally-cli +
> Keymapp flash, full lint suite. Moonlander/Ergodox geometries and
> native+nix build backends are tracked for a future release. The full
> design spec is in [`ARCHITECTURE.md`](ARCHITECTURE.md).

## What it does

`oryx-bench` lets you manage a [ZSA](https://www.zsa.io/) keyboard layout
through whichever combination of editing surfaces you prefer:

- **Visual layout** — keep editing in [Oryx](https://configure.zsa.io/) like
  you do today, *or* author it locally in a TOML file with no cloud
  dependency
- **Advanced QMK features** (achordion, key overrides, macros, combos,
  tap-hold tuning) — declarative TOML, no C required
- **Custom code** (state machines, RGB animations, custom keycodes with
  state) — modern Zig, type-safe, no C required
- **Vendored upstream C libraries** — drop them in `overlay/` unmodified

The pieces merge into a single deterministic build. Same inputs → same
firmware bytes. Reproducible from one local directory.

## The case for it (a real example)

You're a Voyager user with a Dvorak layout. You put Backspace on the right
thumb as a layer-tap (`LT(SymNum, KC_BSPC)`) so holding it gets you to your
symbols layer. It works for a while, then you notice that fast Backspace
bursts occasionally type a stray symbol — the layer-tap timing is firing
mid-erase. You hit the canonical **LT-on-high-frequency-key footgun**.

The "right" fix is **achordion**: a tap-hold disambiguation library that
forces the layer to only activate when the next key is on the *opposite
hand*. Vanilla Oryx can't express achordion. The community workaround
([`poulainpi/oryx-with-custom-qmk`](https://github.com/poulainpi/oryx-with-custom-qmk))
ships layout source through GitHub Actions every time you change anything.

With `oryx-bench`:

1. `oryx-bench lint` flags `lt-on-high-freq` on your right thumb
2. You add three lines to `overlay/features.toml`:
   ```toml
   [achordion]
   enabled = true
   chord_strategy = "opposite_hands"
   [[achordion.timeout]]
   binding = "LT(SymNum, BSPC)"
   ms = 600
   ```
3. `oryx-bench build && oryx-bench flash`
4. Backspace stops misfiring. Your visual layout in Oryx is unchanged.

That's the whole pitch. You keep editing visually wherever you like (Oryx,
Keymapp, or local TOML). You add behavior with declarative config or Zig.
A workbench, not a replacement.

## Three editing surfaces, your choice

| Surface | Format | Use it for |
|---|---|---|
| **Oryx UI** (web) | visual click-and-drag | Where each key sends what, layer organization, basic combos |
| **`overlay/features.toml`** | declarative TOML | Achordion, key overrides, macros, tap-hold tuning, `config.h` settings — ~90% of "advanced QMK" needs |
| **`overlay/*.zig`** | type-safe Zig code | State machines, RGB animations, custom keycodes with state — the ~9% of cases that need real code |
| **`overlay/*.c`** | vendored C | Drop in any third-party QMK library you don't want to translate |

You can use any combination. Edit visually in Oryx and build locally. Skip
Oryx entirely and author `layout.toml` by hand. Whatever fits your workflow.

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the full tier model and how
the pieces compose.

## Five user personas, all supported

| Persona | What they do | Sync friction |
|---|---|---|
| **Oryx-only purist** | Edits in Oryx, flashes via Keymapp GUI, never touches us | Zero (they don't run us) |
| **Oryx + read-only oryx-bench** | Edits in Oryx, uses us to lint/visualize, flashes via Keymapp | Zero (auto-pull) |
| **Oryx + full oryx-bench** | Visual in Oryx, behavior in `overlay/`, flashes via us | Zero (auto-pull) |
| **Local-only** | `layout.toml` + `overlay/`, no Oryx at all | Zero (no Oryx involved) |
| **Switcher** | Started in Oryx, then `oryx-bench detach` to local mode | One-time `detach` |

The "auto-pull" mechanism means a user editing in Oryx sees their changes
reflected in `oryx-bench show` immediately, without ever typing
`oryx-bench pull`. The CLI does a cheap GraphQL metadata check on every
read command (cached for 60s) and pulls silently if Oryx has a newer
revision.

The honest limit: persona 5 cannot push changes back to Oryx after
detaching — Oryx has no public write API. We document this loudly.

## Install

`oryx-bench` is one Rust binary. v0.1 supports the Docker build backend;
the QMK build toolchain is pinned in a Docker image at
`ghcr.io/enriquefft/oryx-bench-qmk:v<VERSION>` containing `qmk`,
`arm-none-eabi-gcc`, `zig`, and the pinned ZSA fork.

```bash
# Linux / macOS (x86_64 and arm64) — recommended
curl -fsSL https://raw.githubusercontent.com/enriquefft/oryx-bench/main/scripts/install.sh | sh

# Cargo (any platform with Rust installed)
cargo install --locked oryx-bench

# Nix flake (Linux/macOS)
nix run github:enriquefft/oryx-bench -- --help

# From source
git clone https://github.com/enriquefft/oryx-bench
cd oryx-bench && cargo build --release
```

Native and Nix build backends are tracked for a future release;
everything else works on every platform.

## Quickstart

**Oryx mode** (you have an existing Oryx layout):

```bash
mkdir my-voyager && cd my-voyager
oryx-bench init --hash YOUR_LAYOUT_HASH    # find this in the Oryx URL
oryx-bench skill install                    # optional, project-local Claude Code skill
oryx-bench show                             # auto-pulls from Oryx, renders the active layer
oryx-bench lint                             # check for known footguns
oryx-bench build && oryx-bench flash
```

**Local mode** (you want zero cloud dependency):

```bash
mkdir my-voyager && cd my-voyager
oryx-bench init --blank --geometry voyager
$EDITOR layout.toml                         # author your visual layout by hand
oryx-bench show
oryx-bench build && oryx-bench flash
```

A complete worked example using a real Dvorak Voyager layout (with the
LT-on-Backspace bug, achordion fix, and key overrides) lives in
[`examples/voyager-dvorak/`](examples/voyager-dvorak/).

## What's in your project

After `oryx-bench init` (Oryx mode):

```
my-voyager/
├── kb.toml                       # project config (hash, geometry, build/flash/sync settings)
├── pulled/                       # COMMITTED — Oryx state, fetched by `oryx-bench pull`
│   └── revision.json
├── overlay/
│   ├── README.md                 # what each file is for
│   ├── features.toml             # Tier 1 declarative QMK features
│   ├── *.zig                     # Tier 2 procedural code (when you need it)
│   └── *.c                       # Tier 2′ vendored upstream libraries
├── .claude/                      # OPTIONAL — only after `oryx-bench skill install`
│   └── skills/oryx-bench/
└── .gitignore
```

In local mode, replace `pulled/` with `layout.toml`.

## The 15 commands

| Command | What it does |
|---|---|
| `oryx-bench setup [--full]` | Detect toolchain (qmk, gcc-arm, zig, docker, wally-cli, keymapp, kontroll). Idempotent. `--full` runs each tool's `--version` for debugging. |
| `oryx-bench init` | Create project skeleton. `--hash` for Oryx mode, `--blank` for local mode. |
| `oryx-bench attach --hash <H>` | Switch local-mode project to Oryx mode. Refuses without `--force` if `layout.toml` has uncommitted changes (or if the dir isn't a git repo). |
| `oryx-bench detach [--force]` | Switch Oryx-mode project to local mode. **One-way.** |
| `oryx-bench pull` | Manually fetch Oryx state. (Usually unnecessary thanks to auto-pull.) |
| `oryx-bench show [LAYER]` | Render layer(s) as ASCII split-grid. Auto-pulls if stale. |
| `oryx-bench explain POSITION` | Cross-layer view of one position. |
| `oryx-bench find QUERY` | Search across layers (`KC_BSPC`, `layer:SymNum`, `hold:LSHIFT`, `anti:lt-on-high-freq`, `position:R_thumb_outer`). |
| `oryx-bench lint [--strict] [--rule ID] [--format text\|json]` | Static analysis with 21 lint rules. |
| `oryx-bench status` | One-screen overview of project, sync, build cache, lint. |
| `oryx-bench build [--dry-run] [--emit-overlay-c]` | Compile firmware via the bundled Docker image. Cached. |
| `oryx-bench flash [--dry-run] [--yes] [--force]` | Flash via wally-cli or Keymapp instructions. Refuses to flash a stale build unless `--force`. Requires explicit confirmation. |
| `oryx-bench diff [REF] [--layer NAME]` | Semantic diff of the visual layout + overlay vs a git ref (default `HEAD`). |
| `oryx-bench upgrade-check` | Re-run lint with the current keycode catalog after a tool upgrade. Surfaces uncatalogued keycodes. |
| `oryx-bench skill install [--global]` | Install the project-local Claude Code skill. |

## Designed for Claude Code

The tool ships an optional **project-local** Claude Code skill at
`./.claude/skills/oryx-bench/` after `oryx-bench skill install`. The skill
is bundled into the binary (no external registry) and is project-scoped by
default — it only loads when Claude Code is invoked from inside your
keyboard project, so it doesn't pollute the context budget of unrelated
sessions.

A `--global` flag exists for users with multiple keyboard projects who
prefer machine-wide install, but it's discouraged for the context-pollution
reason.

The skill teaches Claude about the tier model, the workflows, the lint
rules, and the overlay recipes. With it loaded, you can ask things like:

- "audit my layout for ergonomic issues"
- "fix the LT-on-Backspace misfire"
- "make Shift+Backspace send Delete"
- "swap the positions of Q and ;" (Claude gives you the Oryx clicks)
- "tune the right thumb tap-hold so it stops misfiring"

Claude reads your layout, runs the relevant commands, edits `overlay/`
files where appropriate, instructs you to make visual changes in Oryx
where appropriate, and asks for your approval before flashing.

## Recovery

If a build produces a bad firmware, your layout is never lost:

- **In Oryx mode**: your visual layout is on Oryx's servers. Re-pull and
  re-flash a known-good version. The Voyager has a physical reset button
  + Keymapp GUI as a recovery path.
- **In local mode**: your `layout.toml` and overlay files are in git.
  `git checkout` a known-good commit, rebuild, reflash.

We never invoke `dfu-util` directly. The Voyager's flashing protocol is
custom and bricking risk is real; we always go through `wally-cli` or
Keymapp.

## Roadmap

**v0.1 (current release)** — Voyager-only, Docker build backend, full
authoring + lint + flash surface:

- `setup`, `init` (both modes), `pull`, `show`, `explain`, `find`, `lint`,
  `status`, `skill install/remove` (read-side surface)
- `attach`, `detach`, `build` (docker), `flash` (wally + Keymapp fallback)
- `diff` (semantic vs git ref), `upgrade-check` (re-lint after tool upgrade)
- 21 lint rules including the LT-on-high-freq footgun, achordion + key-override
  + combo + macro codegen, structural codegen round-trip test

**Future releases**

- Native and Nix build backends
- Moonlander and Ergodox geometries
- `oryx-bench live` (layer state via kontroll/Keymapp gRPC)
- `oryx-bench tui` (in-terminal layout editor for local mode)
- User-defined lint rules
- SVG rendering via keymap-drawer subprocess

## License

MIT. See [LICENSE](LICENSE).

## Prior art and credits

- [ZSA Technology Labs](https://www.zsa.io/) for Oryx, Keymapp, `kontroll`,
  and the public GraphQL endpoint at `oryx.zsa.io/graphql`.
- [`poulainpi/oryx-with-custom-qmk`](https://github.com/poulainpi/oryx-with-custom-qmk)
  for proving the overlay-merge pattern works (we adapted the model for
  local CLI use instead of GitHub Actions).
- [`caksoylar/keymap-drawer`](https://github.com/caksoylar/keymap-drawer)
  — planned SVG renderer integration (not yet wired up in v0.1).
- [Achordion](https://getreuer.info/posts/keyboards/achordion/) by Pascal
  Getreuer — the bundled tap-hold disambiguation library.
- The [QMK](https://qmk.fm/) and [Zig](https://ziglang.org/) projects.
