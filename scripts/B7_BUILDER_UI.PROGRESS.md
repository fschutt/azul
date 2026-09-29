# B7_BUILDER_UI — progress

Branch `wt/b7-builder-ui`, base `4b3eae56a`. Nothing compiled here (house rule).

## DONE
- (none yet)

## IN PROGRESS
- 2. DnD drop indicators: before screenshots + RED smoke

## NEXT
1. Previews: RED Rust test (every visual builtin previews, the rest say why) → core table
   `BUILTIN_ELEMENTS` (text default + preview example / no-visual reason) → layout thumbnail
   resolves form controls + icons → card "no visual" label.
2. DnD indicators: fix CSS + zone drawing, after screenshots in `scripts/debugger-ui/screenshots/`.
3. Export > Code (ZIP): the one language list (`get_codegen_languages`), non-DOM disabled with reason.
4. Export menu: Compile > (CSS…, DOM…), Subtree as Component…, Components…; tests, smokes, guide.

## Open questions
- The debugger page has ONE palette (dark tokens on `:root`, no light mode): "light" screenshots are
  taken with `prefers-color-scheme: light` emulated and look the same.
