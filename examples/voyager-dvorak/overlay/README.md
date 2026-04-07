# overlay/

This directory holds everything that gets layered on top of the
Oryx-managed visual layout (in `pulled/revision.json`). The build pipeline
deterministically merges these files with the generated `keymap.c` to
produce the final firmware.

## What lives here

| File pattern | Tier | Purpose | Edited by |
|---|---|---|---|
| `features.toml` | 1 | Declarative QMK features (achordion, key overrides, macros, combos, config) | You or Claude, declaratively |
| `*.zig` | 2 | Procedural code for stateful behavior, RGB animations, custom keycodes with state | You or Claude, in modern type-safe Zig |
| `*.c`, `*.h` | 2′ | Vendored upstream C libraries (paste from GitHub, do not modify) | Nobody — paste-only |

This example project uses **Tier 1 only** (`features.toml`). It expresses
the achordion fix for the LT-on-Backspace bug + key overrides + tapping
term tuning, all declaratively. No C, no Zig — just configuration.

If you wanted to add stateful behavior on top (e.g., a custom keycode that
toggles between work and personal email on alternating presses), you'd
add a `state.zig` file in this directory.

## What's in `features.toml` here

- **`[config]`** — global tunables (`tapping_term_ms`, etc.)
- **`[achordion]`** — the canonical fix for the LT-on-Backspace bug. The
  visual layout in Oryx has `LT(SymNum, KC_BSPC)` on the right thumb;
  achordion makes it actually work without misfires by requiring the next
  key to be on the opposite hand before triggering the layer hold.
- **`[[key_overrides]]`** — `Shift+Backspace → Delete`, `Shift+Esc → ~`
- **`[[macros]]`** — placeholder for sending strings (none defined here)
- **`[features]`** — feature flag toggles for `rules.mk`

## Single source of truth — what to edit where

| Want to change... | Edit |
|---|---|
| Where a key sits on the keyboard | Oryx web UI, then `oryx-bench pull` |
| Achordion timeout for a specific key | `features.toml` `[[achordion.timeout]]` |
| Add a new key override | `features.toml` `[[key_overrides]]` |
| Add a custom keycode that types a string | `features.toml` `[[macros]]`, then bind to USERnn in Oryx |
| Add tap dance / state machine logic | new `*.zig` file in this directory (Tier 2) |
| Use a community QMK module | paste its `.c`/`.h` into this directory (Tier 2′) |

**Never edit `pulled/revision.json` directly.** It's overwritten on every
`oryx-bench pull`. Visual layout changes go through Oryx.

## Lint discipline

`oryx-bench lint` runs against the *merged* layout (visual + behavior).
After editing anything in this directory:

```bash
oryx-bench lint
oryx-bench build
oryx-bench diff       # see what's about to ship
```

Only `oryx-bench flash` after explicit human approval.
