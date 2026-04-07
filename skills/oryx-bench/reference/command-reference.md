# Command reference

> **This file is GENERATED at build time** by the `xtask` binary from the
> clap CLI definitions in `src/cli.rs`. The content below is the
> design-phase manual draft; after M1, `cargo xtask gen-skill-docs` will
> replace it with output identical in shape but generated from the same
> source the binary parses arguments from. CI verifies the file is
> up-to-date.

---

## `oryx-bench setup`

Detect available toolchain (qmk, arm-none-eabi-gcc, zig, docker, wally-cli,
keymapp, kontroll). Print a summary. Idempotent. Does **not** modify state.

```
oryx-bench setup [--verbose]

Options:
  --verbose, -v    Print full version output for each detected tool
```

Use this to verify your environment before running `init` or `build`.

---

## `oryx-bench init`

Create a project skeleton in the current directory. Refuses to overwrite
existing files unless `--force`.

### Oryx mode

```
oryx-bench init --hash <HASH> [--geometry <G>] [--name <NAME>]

Required:
  --hash <HASH>          The Oryx layout hash, e.g. yrbLx
                         Find it in the URL: configure.zsa.io/voyager/layouts/<HASH>/...

Options:
  --geometry <G>         voyager (default in v0.1)
  --name <NAME>          Friendly project name (defaults to current dir basename)
  --no-skill             Don't prompt to install the project-local Claude Code skill
  --force                Overwrite existing files
```

Creates:

```
./kb.toml                            # with [layout] hash_id, geometry
./pulled/                            # empty until first pull
./overlay/README.md
./overlay/features.toml              # empty stub
./.gitignore
```

### Local mode

```
oryx-bench init --blank --geometry <G> [--name <NAME>]

Required:
  --blank                Use local mode (no Oryx hash)
  --geometry <G>         voyager (v0.1 only)

Options:
  --name <NAME>
  --no-skill
  --force
```

Creates:

```
./kb.toml                            # with [layout.local] file = "layout.toml"
./layout.toml                        # empty layout with one base layer scaffold
./overlay/README.md
./overlay/features.toml
./.gitignore
```

---

## `oryx-bench attach`

Switch a local-mode project to Oryx mode.

```
oryx-bench attach --hash <HASH> [--force]

Required:
  --hash <HASH>          The Oryx layout hash to attach to

Options:
  --force                Required if layout.toml has uncommitted changes
```

**Warning**: this **overwrites** `layout.toml` with whatever Oryx
currently has. Local-only edits are lost. The command refuses unless the
working tree is clean or `--force` is passed.

---

## `oryx-bench detach`

Switch an Oryx-mode project to local mode. **One-way.**

```
oryx-bench detach [--force]

Options:
  --force                Skip the confirmation prompt
```

Converts `pulled/revision.json` to `layout.toml`, removes `pulled/`, and
from this point forward `oryx-bench pull` no longer functions in this
project. You can `attach` again later but doing so will *overwrite* your
`layout.toml`.

---

## `oryx-bench pull`

Manually fetch live state from Oryx. Usually unnecessary thanks to
auto-pull on read commands. Use this when you want to force a fetch right
now.

```
oryx-bench pull [--revision <REV>] [--force]

Options:
  --revision <REV>       Specific revision hash, or "latest" (defaults to kb.toml setting)
  --force                Bypass the 60s metadata cache and the `auto_pull = never` setting
```

In **local mode** this command is a no-op (there's nothing to pull).

---

## `oryx-bench show`

Render a layer (or all) as an ASCII split-grid keyboard.

```
oryx-bench show [LAYER] [--names] [--no-pull] [--svg]

Arguments:
  [LAYER]      Layer name (case-insensitive). Default: render all layers.

Options:
  --names      Show position names instead of keycodes
  --no-pull    Skip the auto-pull check (read from local cache only)
  --svg        Output an SVG via keymap-drawer instead of ASCII grid
```

Auto-pull behavior: if it's been more than `poll_interval_s` (default
60s) since the last metadata check, do a cheap GraphQL query to see if
Oryx has updates. If yes, pull silently before rendering.

---

## `oryx-bench explain`

Cross-layer view of a single position.

```
oryx-bench explain <POSITION>

Arguments:
  <POSITION>   Position name (e.g. R_thumb_outer, L_pinky_home)
```

Example:

```
Position: R_thumb_outer (matrix index 51)

  Main:    LT(SymNum, KC_BSPC)         ⚠ lt-on-high-freq
  SymNum:  KC_NO
  System:  KC_TRANSPARENT (falls through to Main → BSPC)
  Gaming:  KC_NO                       ⚠ kc-no-in-overlay
```

---

## `oryx-bench find`

Search across all layers.

```
oryx-bench find <QUERY>

Arguments:
  <QUERY>    One of:
               KC_<NAME>             positions sending this keycode
               layer:<NAME>          all bindings on a layer
               hold:<KEYCODE>        keys with this on hold
               anti:<RULE_ID>        instances of a lint rule
               position:<NAME>       same as `explain`
```

---

## `oryx-bench lint`

Run static analysis. Exit code 0 if clean (errors only), 1 if any errors,
2 if `--strict` and any warnings.

```
oryx-bench lint [--strict] [--rule <ID>] [--format <FORMAT>] [--no-pull]

Options:
  --strict             Fail on warnings as well as errors
  --rule <ID>          Run only this rule
  --format <FORMAT>    text (default) or json
  --no-pull            Skip auto-pull
```

---

## `oryx-bench status`

One-screen overview. Always cheap (no full pull, just metadata query).
**This should be the first command you run in any session.**

```
oryx-bench status [--no-pull]
```

Sample output:

```
Project:  voyager-dvorak
Mode:     Oryx (hash yrbLx, layout name "Dvorak")

Sources:
  pulled/revision.json   2 hours ago, sha 7b3a...
  overlay/features.toml  12 minutes ago
  overlay/*.zig          0 files
  overlay/*.c            0 files

Sync:
  ✓ Up to date with Oryx (last metadata check 8s ago)

Build:
  ✓ Build cache fresh (last build 12 minutes ago)
  ⚠ Built since last flash — `oryx-bench flash` to ship

Lint:
  0 errors, 1 warning (mod-tap-on-vowel)

Git:
  ✓ No uncommitted changes outside pulled/

Toolchain:
  build:  docker (image ghcr.io/enriquefft/oryx-bench-qmk:v0.1.0)
  flash:  wally-cli 2.0.1
```

---

## `oryx-bench diff`

Semantic diff vs git ref, rendered as before/after grids of just the
changed keys. Diffs both `pulled/revision.json` (or `layout.toml`) and
the `overlay/` files.

```
oryx-bench diff [REF] [--layer <NAME>]

Arguments:
  [REF]              Git reference (default: HEAD)

Options:
  --layer <NAME>     Limit to one layer
```

---

## `oryx-bench build`

Compile firmware. Cached: same input → same output, no rebuild.

```
oryx-bench build [--dry-run] [--release] [--emit-overlay-c] [--no-pull]

Options:
  --dry-run            Show what would be built; don't actually build
  --release            Build with optimizations (LTO, strip)
  --emit-overlay-c     Save the generated overlay C source to ./.oryx-bench/build/ for inspection
  --no-pull            Skip auto-pull
```

Output: a path to the built `.bin`, plus stages a copy at
`./.oryx-bench/build/firmware.bin`.

In v0.1, the only available backend is `docker`. Native and Nix backends
arrive in v0.2.

---

## `oryx-bench flash`

Flash the firmware to a connected keyboard. **Requires explicit user
confirmation** unless `--yes` is passed.

```
oryx-bench flash [--dry-run] [--yes] [--backend <BACKEND>] [--no-pull]

Options:
  --dry-run              Print what would be flashed; don't flash
  --yes                  Skip the CLI's confirmation prompt
                         (still requires conversational approval — use this in agent loops)
  --backend <BACKEND>    auto (default), wally, keymapp
  --no-pull              Skip auto-pull (already the default for flash)
```

Behavior:

1. If `wally-cli` is on PATH (and backend is auto/wally), invoke it
   directly with the latest built firmware
2. Otherwise: copy the firmware to `~/.cache/oryx-bench/firmware.bin` and
   print platform-specific Keymapp GUI instructions
3. **Never** invoke `dfu-util` directly — the Voyager flashing protocol is
   custom and bricking risk is real

`--dry-run` output:

```
Would flash:
  firmware:  /tmp/.oryx-bench/build/firmware.bin
  size:      54918 bytes
  sha256:    7b3a1e...
  target:    ZSA Voyager (vendor 0x3297)
  via:       wally-cli
```

Auto-pull behavior: **never** auto-pulls. Flashing is the moment of
commitment; you flash exactly what you just looked at, no surprises.

---

## `oryx-bench skill install`

Install the project-local Claude Code skill at
`./.claude/skills/oryx-bench/`.

```
oryx-bench skill install [--global] [--force]

Options:
  --global    Install to ~/.claude/skills/ instead of project-local
              DISCOURAGED — eats context budget in unrelated Claude Code sessions
  --force     Overwrite existing files
```

---

## `oryx-bench skill remove`

Uninstall the skill.

```
oryx-bench skill remove [--global]
```

---

## Global flags

Available on all commands:

```
  --project <PATH>     Path to the project root (default: discover from cwd)
  --color <WHEN>       auto (default) | always | never
  --verbose, -v        Increase logging verbosity (repeatable)
  -h, --help           Show help
  -V, --version        Show version
```

---

## Environment variables

```
ORYX_BENCH_LOG=trace          Set log level (trace/debug/info/warn/error)
ORYX_BENCH_NO_COLOR=1         Disable ANSI colors (same as --color=never)
ORYX_BENCH_CACHE_DIR=<path>   Override cache directory location
NO_COLOR=1                    Standard env var, also respected
```

---

## Exit codes

```
0   Success
1   Lint errors / build failure / explicit failure
2   Lint warnings (only with --strict)
64  Usage error (bad arguments)
65  Data error (corrupt revision.json or layout.toml, unknown keycode)
69  Service unavailable (Oryx GraphQL down)
74  IO error (couldn't write file, network failure)
```
