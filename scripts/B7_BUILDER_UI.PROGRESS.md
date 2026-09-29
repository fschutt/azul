# B7_BUILDER_UI — progress

Branch `wt/b7-builder-ui`, base `4b3eae56a`. Nothing compiled here (house rule).

## DONE
- 2. DnD drop indicators: `dd558057f` (RED: indicator smoke + node tests + before screenshots),
  `aa4f063c8` (fix: drop line at the landing gap/depth, INTO tint+outline, refused-INTO fallback,
  after screenshots). Screenshots: `scripts/debugger-ui/screenshots/dnd-{before,after}-fix-{light,dark}-*.png`.

## IN PROGRESS
- 1. Previews: RED Rust test

## NEXT
1. Previews: RED Rust test (every visual builtin previews, the rest say why) → core table
   `BUILTIN_ELEMENTS` (text default + preview example / no-visual reason) → layout thumbnail
   resolves form controls + icons → card "no visual" label.
3. Export > Code (ZIP): the one language list (`get_codegen_languages`), non-DOM disabled with reason.
4. Export menu: Compile > (CSS…, DOM…), Subtree as Component…, Components…; tests, smokes, guide.

## Open questions
- The debugger page has ONE palette (dark tokens on `:root`, no light mode): the "light" screenshots
  are taken with `prefers-color-scheme: light` emulated and are byte-identical to the dark ones.
