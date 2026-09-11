# Widget theme migration — working plan

Working document for an in-progress refactor. **Delete it when
`scripts/check_widget_theme_migration.py` exits 0** and the work is merged.

Run the check for the current state; it is the definition of "done", not this
prose:

```sh
python3 scripts/check_widget_theme_migration.py          # all phases
python3 scripts/check_widget_theme_migration.py --phase 1
python3 scripts/check_widget_theme_migration.py --json
```

Baseline when this plan was written: 7 of 8 checks failing — 115 bare style
fields, 171 interactive-state rules sitting in widget files, 0 gradients in the
flora theme.

---

## Why

Two themes exist, `flat` (default) and `flora` (from `doc/templates/flora.css`),
and each mode has a light and a dark variant. Three things stand in the way of
that actually working.

**Widgets cannot express "no opinion".** A style field is a plain
`CssPropertyWithConditionsVec`, and constructors fill it with the default. A
theme then cannot tell "the caller wants exactly these properties" from "nobody
said anything", so it can never substitute its own — every widget arrives
pre-styled by the widget file.

**Interactive states are declared where the theme is not in scope.** Hover,
active and focus live in the widget files; light/dark lives in the theme files.
The theme modules contain zero state rules, so nothing ever prompted for a dark
variant, and today **every** hover highlight, pressed state and focus ring in
this toolkit paints its light-mode colour on a dark surface. This is structural:
fixing the colours in place would leave the next widget with the same hole.

**flora does not use gradients.** `flora.css` has 52 `linear-gradient` and 20
`radial-gradient` declarations; `flora.rs` has none, and its `RT`/`RB`,
`HT`/`HB`, `PT`/`PB` tokens — which exist to *be* gradient stops for the raised,
hovered and pressed control faces — are each referenced exactly once, by their
own definition. The "skeuomorphic rich theme" currently renders flat.

---

## Phase 1 — every style field becomes `OptionCssPropertyWithConditionsVec`

`None` means no opinion: the theme resolves it. `Some(vec)` means the caller
chose, and `Some(empty)` is a real answer — "no properties" — distinct from
`None`.

Per field:

1. `pub x_style: CssPropertyWithConditionsVec`
   → `pub x_style: OptionCssPropertyWithConditionsVec`
2. the constructor stops pre-filling it: `OptionCssPropertyWithConditionsVec::None`
3. one resolver on the widget holds what the constructor used to put there:

   ```rust
   #[must_use]
   pub fn resolved_x_style(&self) -> CssPropertyWithConditionsVec {
       self.x_style.clone().into_option().unwrap_or_else(|| /* the old default */)
   }
   ```

4. `set_x_style` / `with_x_style` wrap in `Some(..)`, so the public API is
   unchanged for anyone who was setting one
5. both theme modules and the tests call the resolver, never the field

The resolver is the point: the default has to live somewhere, and putting it in
one function keeps `flat` and `flora` from each growing their own copy of the
same `match` (which had already happened for `CheckBox`).

**Tests** assert against the resolver, not the raw field. A test that reads the
field after this change reads `None` and passes whatever the widget renders.

Done: `alert`, `check_box`. `2fd0034d1..` has the shape.

## Phase 2 — interactive states move into the theme modules, with dark twins

Move every `on_hover` / `on_active` / `on_focus` rule out of the widget files
and into `flat.rs` and `flora.rs`, and give each one a dark counterpart using
the constructors added for this:

```rust
CssPropertyWithConditions::dark_on_hover(prop)
CssPropertyWithConditions::dark_on_active(prop)
CssPropertyWithConditions::dark_on_focus(prop)
```

Conditions conjoin, so these mean "dark **and** hovered". Colours come from the
theme's tokens (`HT`/`HB` for hover, `PT`/`PB` for pressed, `GLOW`/`ACC` for
focus) rather than literals — both themes now expose the same token names, which
is what lets a rule be written once per theme instead of once per widget.

Widgets with state rules today, largest first: `tabs` (39), `list_view` (60),
`text_input` (24), `button` (11), `ribbon` (11), `text_area` (8),
`quick_access` (6), `combobox` (5), `statusbar` (4), `backstage`, `titlebar`,
`tree_view` (1 each).

## Phase 3 — flora uses flora.css's gradients

Transcribe the gradients from `doc/templates/flora.css` into `flora.rs` via
`StyleBackgroundContent::LinearGradient`, keeping the stops and angles. The
`RT`/`RB`, `HT`/`HB` and `PT`/`PB` pairs are the raised / hover / pressed faces
and should become two-stop vertical gradients rather than staying unused.

`flat` deliberately keeps flat fills: its stop pairs are equal values, so the
same widget code reads correctly under both themes.

---

## Ground rules

- `api.json` is machine-owned: edit only through a Python
  `json.load` → change → `json.dumps(d, indent=4, ensure_ascii=False)`
  round-trip written **without** a trailing newline, then confirm
  `./target/release/azul-doc normalize` reports it already normalized.
- Release profile only; never build debug in this repo.
- Before pushing Rust: `cargo clippy -p azul-core -p azul-css -p azul-layout
  --all-targets -- -D warnings`, which CI enforces for those three crates.
- `cargo test -p azul-layout --lib` was already failing before this work
  started (the earlier partial conversion updated the library but not the
  `#[cfg(test)]` modules). Phase 1 repairs it; it is not a regression to chase.
- When rewriting field reads to `resolved_x()` with a regex, EXCLUDE the
  resolver's own body — `self.x_style` inside `resolved_x_style()` must stay the
  field, or the function calls itself. It compiles as far as a confusing
  "method not found on CssPropertyWithConditionsVec" on `.into_option()`.
- Commit per widget or per small batch, never one sweeping commit — a mistake in
  a mechanical pass this size needs to be bisectable.
