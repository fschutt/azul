# X1a: DOM export in 16 more printers (progress checkpoint)

Branch `wt/x1a-dom-export-printers` (from `0a326afe5`). Languages: java, kotlin, csharp, go,
swift, node, ruby, php, lua, zig, nim, d, ocaml, haskell, julia, pascal. Nothing is compiled
here (house rule).

## DONE

- eef8f44c8 test: RED Julia union variant by field name (coordinator addition, F1's `_pad0`)
- d40e51839 fix: Julia `az_union(U, V, tag, payload...)` builds the variant by field name
- 07ae46413 test: RED structure test (flag list of 20, DOM outputs balanced) + hand-written
  `dom_card` for java, go, swift
- 4f0b15494 feat: shared `dom.rs` / `mod.rs` helpers; Java
- 240e0ebc8 feat: Go (wrapper layer, dot-at-line-end chains), Swift (native API)
- 2a7c082f2 test(builder): export tests follow exports_dom() (they assumed Java had none)
- 6d5b10881 feat: kotlin, csharp, node, ruby, lua, php (php: app limited) + lang::unicode_utf16
- b6e3af7a0 feat: d, julia, nim, zig (zig registers) + dom::chained_dot_at_line_end
- 7f33c8a14 feat: ocaml, haskell, pascal (pascal registers) + dom::chained_infix
- report: `scripts/X1A_DOM_EXPORT_PRINTERS_2026_09_29.md`

## IN PROGRESS

(none)

## NEXT

- Parent: compile, bless, run the commands in the report (section 6); merge with X1b.

## Open questions

- `layout/src/e2e/export.rs` (not this task's file) names library files
  `components/<lib>.<ext>` and writes build instructions only for rust / c / cpp / python
  (see the report, section 7).
