# Lint rules

> **This file is GENERATED at build time** by the `xtask` binary from the
> registered rules in `src/lint/rules/`. The content below is the
> design-phase manual draft; after M1, `cargo xtask gen-skill-docs` will
> replace it with output identical in shape but generated from the same
> source the binary uses to evaluate rules. CI verifies the file is
> up-to-date.

Each rule below has: ID, severity, what it catches, why it's bad, and the
recommended fix.

---

## Visual-layout rules (catch issues in `pulled/revision.json` or `layout.toml`)

### `lt-on-high-freq`

**Severity**: Error
**Catches**: Layer-tap (`LT(layer, key)`) where `key` is one of `KC_BSPC`,
`KC_SPC`, `KC_ENT`, `KC_DEL`, `KC_TAB`, or `KC_ESC`.

**Why bad**: Tap-hold resolves on a tapping term. Below it = tap; above =
hold. For high-frequency keys you press hundreds of times an hour, the
boundary is hit *constantly*: a fast Backspace burst crosses the term and
triggers the layer; a brief intentional layer hold falls below the term
and injects a stray Backspace.

**Recommended fix**: Almost never "move the key" — add achordion to
`overlay/features.toml`. Achordion forces tap-hold to only resolve as
hold when the next key is on the opposite hand, eliminating the misfire
class. See `reference/overlay-cookbook.md#achordion`.

If the user explicitly asks to move the key (e.g., for ergonomic
reasons), that's Path A and fine — guide them through the Oryx click or
the `layout.toml` edit.

---

### `unreachable-layer`

**Severity**: Error
**Catches**: A layer with no `MO`, `LT`, `TG`, `TO`, `TT`, or `DF`
reference from any reachable layer.

**Why bad**: A layer that can't be activated is dead code — it consumes
firmware space and mental overhead.

**Recommended fix**: Path A. Either remove the layer or add an activation
key from another layer.

---

### `kc-no-in-overlay`

**Severity**: Warning
**Catches**: A non-base layer position bound to `KC_NO` (dead key) when
the base layer at the same position has a real binding. Almost always
the user meant `KC_TRANSPARENT` (fall-through).

**Why bad**: `KC_NO` does nothing; `KC_TRANSPARENT` falls through to the
next active layer. They look identical in Oryx's grid view but produce
wildly different behavior.

**Recommended fix**: Path A. Open the affected layer, set the position
to "Transparent" instead of "Empty".

---

### `orphaned-mod-tap`

**Severity**: Warning
**Catches**: A key with `tap: null` and `hold: <plain modifier>`. This is
the encoding Oryx produces when you start with a mod-tap and clear the
tap action.

**Why bad**: Functionally works as a plain modifier, but the encoding
signals "this used to be a mod-tap" and creates code-review confusion.

**Recommended fix**: Path A. Remove and re-add as a plain modifier.

---

### `unknown-keycode`

**Severity**: Error
**Catches**: A `code` field in pulled JSON that doesn't match any
catalogued QMK keycode.

**Why bad**: Either Oryx introduced a new code we haven't catalogued, or
the JSON is corrupt. The generator emits the literal string into the
generated `keymap.c`, which will fail to compile if it's truly unknown.

**Recommended fix**: File an issue with the unknown code name. We add it
to `src/schema/keycode.rs` and ship a new release. As a workaround, the
catch-all `Keycode::Other(String)` preserves the literal so manual
intervention is possible.

---

### `unknown-layer-ref`

**Severity**: Error
**Catches**: A layer-affecting action (`MO`, `LT`, `TG`, etc.) whose
`layer` field points to a nonexistent layer index.

**Why bad**: Build will fail (or, worse, silently activate the wrong
layer if the index is technically valid in QMK's numbering).

**Recommended fix**: Path A. Fix the dangling reference in Oryx or
`layout.toml`.

---

### `duplicate-action`

**Severity**: Info
**Catches**: Two positions on the same layer producing the same effect.

**Why often not bad**: Often intentional — e.g., Backspace bound on a
thumb *and* duplicated in the symbol layer's row 2 so you can erase
while holding the layer key.

**Recommended fix**: Review and accept, or remove the duplicate if
unintended.

---

### `mod-tap-on-vowel`

**Severity**: Info
**Catches**: Home-row mod (`MT(MOD_*, KC_<vowel>)`) on a vowel position.

**Why flagged**: Vowels appear in fast bigrams in many languages, which
causes more mod-tap misfires than mods on consonants.

**Recommended fix**: Either accept (and add achordion to mitigate), or
move the mod to a consonant position.

---

### `home-row-mods-asymmetric`

**Severity**: Info
**Catches**: Home-row mods on the left half but not the right (or vice
versa).

**Why flagged**: Asymmetric mods make muscle memory harder.

**Recommended fix**: Either accept, or mirror the stack via Path A.

---

### `layer-name-collision`

**Severity**: Error
**Catches**: Two layers whose titles sanitize to the same C identifier.
For example, `"Sym + Num"` and `"Sym Num"` both sanitize to `SYM_NUM`.

**Why bad**: The generator can't produce a valid `enum layers` with
duplicate names; build fails.

**Recommended fix**: Path A. Rename one of the colliding layers in Oryx
or `layout.toml`.

---

## Cross-tier rules (catch issues across the visual layout AND overlay)

### `overlay-dangling-position`

**Severity**: Error
**Catches**: `overlay/features.toml` references a position name (e.g.,
`L_pinky_home`) that doesn't exist in the current geometry, OR
references a binding (`LT(SymNum, BSPC)`) where no key in the visual
layout has that binding.

**Why bad**: The generator can't resolve the reference. Build will fail
or (worse) silently apply the feature to nothing.

**Recommended fix**: Either fix the position/binding name in the TOML,
or update Oryx to add the missing binding.

---

### `overlay-dangling-keycode`

**Severity**: Error
**Catches**: `overlay/features.toml` references a keycode (e.g., for a
key override) that isn't bound anywhere in the visual layout.

**Why bad**: A key override on a key that doesn't exist on the keyboard
is silently dead — the override never fires.

**Recommended fix**: Either bind the keycode in Oryx, or remove the
override.

---

### `custom-keycode-undefined`

**Severity**: Error
**Catches**: The visual layout binds a `USERnn` keycode but no `[[macros]]`
or Tier 2 file defines what `USERnn` does.

**Why bad**: Pressing the key does nothing.

**Recommended fix**: Either add a `[[macros]]` entry in `features.toml`
with `slot = "USERnn"`, or add a Tier 2 dispatch in a `.zig` file, or
remove the binding from Oryx.

---

### `unreferenced-custom-keycode`

**Severity**: Info
**Catches**: An overlay defines a custom keycode (`[[macros]]` with a
`slot` or a `.zig` dispatch) but no layer in the visual layout binds
that USERnn slot.

**Why flagged**: Dead code.

**Recommended fix**: Either bind it in Oryx, or remove from `features.toml`.

---

### `process-record-user-collision`

**Severity**: Error
**Catches**: A Tier 2 file (`*.zig` or vendored `*.c`) defines
`process_record_user` directly, colliding with the generator's auto-
emitted `process_record_user`.

**Why bad**: The link step fails with a duplicate symbol error.

**Recommended fix**: Rename the Tier 2 function to
`process_record_user_overlay`. The generated `process_record_user`
dispatches to `_overlay` after handling its own concerns. Same applies to
`matrix_scan_user`, `keyboard_post_init_user`, etc. — all hooks that the
generator might emit.

---

### `config-redefine-without-undef`

**Severity**: Warning
**Catches**: `overlay/features.toml` `[config]` section defines a macro
Oryx already set in its generated `config.h`, without the generator
emitting an `#undef` first.

**Why bad**: Compiler warning `-Wmacro-redefined`. Generally harmless but
noisy.

**Recommended fix**: The generator should be doing this automatically.
If you see this rule fire, it's a bug — file an issue.

---

## Build/sync state rules

### `tt-too-short`

**Severity**: Warning
**Catches**: Effective `TAPPING_TERM` < 150ms (after considering
`features.toml` `[config]` overrides) when any mod-tap or layer-tap is in
the layout.

**Why bad**: Below 150ms, the tap/hold boundary is too tight even for
fast typists. Constant misfires.

**Recommended fix**: Set `tapping_term_ms` in `[config]` of
`features.toml` to ≥180ms. 200–220ms is the sweet spot for most users.

---

### `not-pulled-recently`

**Severity**: Info
**Catches**: `pulled/pulled-at.iso` is more than 7 days old (Oryx mode
only — no-op in local mode).

**Why flagged**: You may have edited in Oryx since the last pull. Local
state could be stale.

**Recommended fix**: `oryx-bench pull`.

---

### `oryx-newer-than-build`

**Severity**: Warning
**Catches**: `pulled/revision.sha256` differs from the sha recorded in the
last successful build.

**Why flagged**: You pulled fresh state from Oryx, but the firmware on
your keyboard is still based on the previous pull.

**Recommended fix**: `oryx-bench build` (and then `flash` after review).

---

## How to add a rule

See `CONTRIBUTING.md`. Briefly:

1. Create `src/lint/rules/<rule_id>.rs` implementing `LintRule`
2. Register in `src/lint/rules/mod.rs::registry()`
3. Add positive + negative tests in `tests/lint_rules.rs`
4. Run `cargo xtask gen-skill-docs` — this file regenerates from the
   registry
5. CI verifies the committed file matches the generator output
