# B7_BUILDER_UI — progress

Branch `wt/b7-builder-ui`, base `4b3eae56a`. Nothing compiled here (house rule).

## DONE
- 2. DnD drop indicators: `dd558057f` (RED: indicator smoke + node tests + before screenshots),
  `aa4f063c8` (fix: drop line at the landing gap/depth, INTO tint+outline, refused-INTO fallback,
  after screenshots). Screenshots: `scripts/debugger-ui/screenshots/dnd-{before,after}-fix-{light,dark}-*.png`.

- 1. Previews: `16089b8ff` (RED: builder_tests every-builtin thumbnail test, node thumbOf, smoke
  "no visual" card), `4f88c3862` (fix: core `BUILTIN_ELEMENTS` table + `builtin_preview_dom` /
  `builtin_no_visual`, layout `style_detached_dom` + `preview_styled_dom`, card label).

## IN PROGRESS
- 3. Export > Code (ZIP) languages

## NEXT
3. Export > Code (ZIP): the one language list (`get_codegen_languages`), non-DOM disabled with reason.
4. Export menu: Compile > (CSS…, DOM…), Subtree as Component…, Components…; tests, smokes, guide.

## Open questions
- The debugger page has ONE palette (dark tokens on `:root`, no light mode): the "light" screenshots
  are taken with `prefers-color-scheme: light` emulated and are byte-identical to the dark ones.
