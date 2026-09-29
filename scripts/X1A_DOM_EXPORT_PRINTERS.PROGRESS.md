# X1a: DOM export in 16 more printers (progress checkpoint)

Branch `wt/x1a-dom-export-printers` (from `0a326afe5`). Languages: java, kotlin, csharp, go,
swift, node, ruby, php, lua, zig, nim, d, ocaml, haskell, julia, pascal. Nothing is compiled
here (house rule).

## DONE

- eef8f44c8 test: RED Julia union variant by field name (coordinator addition, F1's `_pad0`)
- d40e51839 fix: Julia `az_union(U, V, tag, payload...)` builds the variant by field name
- 07ae46413 test: RED structure test (flag list of 20, DOM outputs balanced) + hand-written
  `dom_card` for java, go, swift
- 4f0b15494 feat: shared `dom.rs` (`is_dom_item`, `WrapperDomSyntax` / `WrapperDom`,
  `wrapper_dom_limitation`, `registration_note`, `one_line`), `mod.rs` (`call_param_names`
  for the multi-arg Dom ctors; `item_dom_blocker` uses `is_dom_item`); Java
- 240e0ebc8 feat: Go (wrapper layer, dot-at-line-end chains), Swift (native API)
- 2a7c082f2 test(builder): export tests follow exports_dom() (they assumed Java had none)
- 6d5b10881 feat: kotlin, csharp, node, ruby, lua, php (php: app limited) + lang::unicode_utf16

## IN PROGRESS

- nim, d, zig, julia / ocaml, haskell, pascal:
  implemented by helper agents in this worktree (no commits by them); reviewed and committed
  here.

## NEXT

1. Review + commit each group; delete the stale `dom_app` goldens of each language.
2. Report `scripts/X1A_DOM_EXPORT_PRINTERS_2026_09_29.md`.

## Design (for a resumed session)

- A printer whose binding builds a DOM through another layer than CSS (wrapper classes with
  native strings) implements `dom::WrapperDomSyntax` on a small `XxxDom` struct and prints DOM
  items with `WrapperDom(XxxDom)` (`dom::is_dom_item` picks it). A printer whose CSS syntax is
  already the native API (Swift, D) implements `method` / `param` / `concat` on its own syntax.
- Registration: every one of the 16 prints `dom::registration_note(lib, REASON)` with a precise,
  binding-specific reason (none can build a ComponentDef whose raw `render_fn` returns
  `ResultStyledDomRenderDomError` by value and hand it the built Dom).

## Open questions

- `layout/src/e2e/export.rs` (not this task's file) names library files
  `components/<lib>.<ext>` and writes build instructions only for rust / c / cpp / python.
