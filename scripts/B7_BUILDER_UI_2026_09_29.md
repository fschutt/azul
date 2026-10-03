# B7: AzBuilder UI fixes from the user's own testing (report, 2026-09-30)

Branch `wt/b7-builder-ui`, 14 commits after `4b3eae56a` (tip `a47352d74`). The agent could not
create this file itself, so the parent committed it from its final message. The Rust was
written without compiling; the parent compiled and ran it on integration.

The user reported four things:

1. "lots of components don't have a preview in the browser - maybe no example configured?"
2. "the dnd doesn't highlight the 'insert as child' properly, the highlighting is very broken"
3. "the 'export > code' functionality is also missing lots of languages"
4. "the 'compile css...' should be grouped under 'Export > Compile > (css, dom)' and
   'Export > Subtree as Component' and 'Export > Components'"

## 1. Previews

**Cause.** 98 of the 116 builtins returned `empty: true`:

- about 45 text elements had `""` as their default text;
- about 30 containers had nothing to draw;
- a raw `<input>` / `<select>` / `<textarea>` stays an empty box until form controls are
  resolved, which the window does and the thumbnail did not;
- the rest have nothing to show on their own.

**Fix.** One table, `BUILTIN_ELEMENTS` in `core/src/xml.rs`, replaces the list of
`builtin_component_def` calls. Each element has its tag, name, default text and a preview:

- `Itself`: the element with its default text, or drawing by itself (`<hr>`);
- `Example`: extra attributes or children the preview shows but a drop does not insert;
- `NoVisual(reason)`: nothing to show; the card says so.

Thumbnails and `get_component_preview` go through ONE function, `builder::preview_styled_dom`,
which resolves form controls and icons the way the window does.

A card with nothing to show gets a small "no visual" label, with the reason as its tooltip. Those
stay in the palette because they are still useful to drop (`<br>` into a `<p>`, `<option>` into a
`<select>`). The document-structure and `<head>` tags stay hidden as before; that JS list is now
`NOT_IN_PALETTE`.

| Components | Before | After |
|---|---|---|
| h1-h6, p, summary, li, th, a, legend, label | text | unchanged |
| hr | its rule | unchanged |
| button | raw UA button | Button widget |
| span, pre, code, blockquote, figcaption, address, icon, dt, dd, menuitem, caption, td, inline text elements (strong ... bdi, rt, rp, data, output) | empty (default text `""`) | non-empty default text (a drop inserts it too) |
| div, header, footer, section, article, aside, nav, main, dialog | empty | dashed box with the element's name |
| figure / fieldset / details | empty | box + figcaption / box + legend + label / summary |
| ul, ol, menu, dir / dl | empty | 2 list items / term + description |
| table / thead / tbody, tfoot / tr | empty | header and body rows / a header row / a body row / 2 cells |
| ruby / rtc | empty | "Ruby" + rt / rt |
| form / input / textarea / select | empty (raw controls) | label + TextInput / TextInput with placeholder / TextArea with placeholder / DropDown with 2 options |
| svg | empty | a circle and a square |
| br, wbr, pagebreak, col, colgroup, option, optgroup, datalist, progress, meter, canvas, object, embed, audio, video, param, source, track, map (image map), area | empty | "no visual", each with its reason |
| html, head, title, body, meta, link, script, style, base | empty, not in the palette | "no visual", still not in the palette |

Result: 84 elements now preview and 29 say "no visual", up from 15 elements before.
`<progress>` and `<meter>` are "no visual" only because azul does not draw them yet. There is no
`builtin:img` component.

## 2. The drag-and-drop highlight

**What the screenshots showed.**

- INTO: only a faint row tint, with no sign of where the child goes.
- AFTER on an expanded container: the line was drawn between the container and its first child.
  That looks like "insert as first child", but the node lands after the whole subtree.
- A `<div>` over the middle of a `<p>`: no highlight at all, and the drop was refused.

**Fix.**

- One drop line marks the gap where the node will land, starting at the indent it lands at.
- INTO: the row is tinted and outlined, and the line sits one level deeper, below the row's last
  child.
- A refused INTO falls back to before / after by halves. Row drops use the same rule the
  window-canvas drops already used (`rowDrop`).
- Below the last row, a line means "append to `<body>`".
- dragleave, drop and dragend all clear the highlight.
- The page's colour tokens are kept.

Screenshots: `scripts/debugger-ui/screenshots/dnd-{before,after}-fix-{light,dark}-*.png`. The
debugger page has no light mode, so light and dark are byte-identical. The new smoke
`builder-dnd-indicator-smoke.mjs` checks all of this. Check 3 in `builder-dnd-smoke.mjs` and
`builder-dnd-live.mjs` asserted the old refusal and is updated.

## 3. Missing languages in Export > Code

**Cause.** `debugger.html` hard-coded four items (Rust, C, C++, Python), and `debugger-export.js`
kept the same four as its fallback (`DOM_FALLBACK`).

**Fix.**

- Both lists are deleted. Code (ZIP) is built from `get_codegen_languages`, the ONE list every
  other dialog already uses.
- Non-DOM languages are listed but disabled. The server sends a `no_dom_reason` per language,
  which shows as the tooltip.
- With no answer from the server, the menu shows one disabled line saying so, never a guessed
  list.

## 4. The Export menu: where each item went

Nothing was removed.

| Old item | New place |
|---|---|
| Compile CSS to... | Compile > CSS... |
| HTML -> DOM (code)... | Compile > DOM... |
| Subtree -> code... | Subtree as Component... |
| Component -> code... | Components... |
| Component Library (JSON) | Components... -> "Library as JSON" (disabled with a reason for builtin libraries) |
| Code > Rust / C / C++ / Python | Code (ZIP) > every language |
| Project as JSON, E2E Tests (CLI format), Builder document (JSON) | unchanged |
| Row menu: Export as code... / Component -> code... / Compile its CSS to... | Subtree as Component... / Export this component... / Compile its CSS... |

The guide is `doc/guide/en/architecture/gui-builder.md`.

## api.json

No changes. Two JSON fields were added to existing ops: `no_visual` on `get_component_thumbnail`
and `no_dom_reason` on `get_codegen_languages`.

## Duplicates found

- `builtin:map` names both the image map and the structural map. Lookups by name get the image
  map. This predates the branch.
- `debugger.js` still has the old `exportCode` binary-zip handler, which `debugger-export.js`
  replaces at load. It is dead code.
- `_downloadJSON` (debugger.js) and `download` (debugger-export.js) do the same job.

## Tests

| Suite | Result on the branch |
|---|---|
| node debugger-dnd | 18/18 |
| node debugger-dnd-extras | 17/17 |
| node debugger-export | 16/16 |
| node debugger-project | 13/13 |
| smoke builder-dnd | 26/26 |
| smoke builder-dnd-indicator (new) | 19/19 |
| smoke builder-export | 50/50 |
| smoke builder-extras | 40/40 |
| smoke builder-project | 42/42 |

Rust tests, run by the parent:

- `cargo test -p azul-core --lib a_builtins_preview_adds_its_example`
- `cargo test -p azul-core --lib a_builtin_without_a_box_of_its_own`
- `cargo test -p azul-layout --features e2e-server --lib every_visual_builtin_has_a_palette_preview`
- `cargo test -p azul-layout --features e2e-server --lib builder_tests`
- `cargo test -p azul-layout --features e2e-server --lib export_tests`

## Left

- `<progress>` / `<meter>` could become widgets.
- The debugger page has no light mode.
- Thumbnails always render light on white.
