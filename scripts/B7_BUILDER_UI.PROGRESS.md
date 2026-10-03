# B7_BUILDER_UI — progress

Branch `wt/b7-builder-ui`, base `4b3eae56a`. Nothing compiled here (house rule).

## DONE
- 2. DnD drop indicators: `dd558057f` (RED: indicator smoke + node tests + before screenshots),
  `aa4f063c8` (fix: drop line at the landing gap/depth, INTO tint+outline, refused-INTO fallback,
  after screenshots), `c8c6bc0df` (live smoke expects the fallback; not run here).
  Screenshots: `scripts/debugger-ui/screenshots/dnd-{before,after}-fix-{light,dark}-*.png`.
- 1. Previews: `16089b8ff` (RED: builder_tests every-builtin thumbnail test, node thumbOf, smoke
  "no visual" card), `4f88c3862` (fix: core `BUILTIN_ELEMENTS` table + `builtin_preview_dom` /
  `builtin_no_visual`, layout `style_detached_dom` + `preview_styled_dom`, card label),
  `3b95d2c2e`, `f572b6603`, `c64c9e937` (lint lockdown), `21f52d59f` (test list: no builtin:img).
- 3 + 4. Export menu and languages: `dec30dadc` (RED: export_tests no_dom_reason, node menu model,
  smoke menu structure / Code (ZIP) / library JSON), `bea930402` (fix: one language list,
  `exportMenu`, Components… library JSON, HTML items removed, guide).

## IN PROGRESS
- (none) - the final report is returned to the parent as text (the harness refuses report files
  from this agent); the parent may commit it as `scripts/B7_BUILDER_UI_2026_09_29.md`.

## NEXT
- Parent: compile + run the Rust suites and `builder-dnd-live.mjs` (commands in the report).

## Open questions
- The debugger page has ONE palette (dark tokens on `:root`, no light mode): the "light" screenshots
  are taken with `prefers-color-scheme: light` emulated and are byte-identical to the dark ones.
- `builtin:map` names two builtins (the image map and the structural map); the name resolves to
  the image map everywhere, so the structural map's card shows the image map's "no visual".
