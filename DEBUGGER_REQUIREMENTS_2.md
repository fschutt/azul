# Debugger Requirements — Phase 2

## 1. App State Snapshots (Save / Restore / Alias)

### 1.1 Snapshot Management

The debugger needs a snapshot system that lets users **save** the current app
state under a user-chosen ID (e.g. `"initial-state"`, `"after-login"`) and
**restore** any saved snapshot later.

- **Save**: A "📷 Save Snapshot" button in the App State panel header opens an
  inline input field. The user types an alias name and confirms.  The current
  `app.state.appStateJson` is deep-cloned and stored under that key in
  `app.state.snapshots` (a `{ [alias]: jsonValue }` map).
- **List**: Below the App State tree, show a collapsible "Saved Snapshots"
  section listing every alias as a clickable row with:
  - The alias name (editable — pencil icon → inline rename, same as test names)
  - A "Restore" button (replaces the live app state via `set_app_state`)
  - A "Delete" button (removes the snapshot)
- **Restore**: Clicking "Restore" sends the stored JSON via
  `{ op: 'set_app_state', state: <snapshot> }`, then calls
  `handlers.loadAppState()` and `handlers.refreshDOM()` to reflect the change.
- **Persistence**: Snapshots are included in `handlers.save()` (localStorage)
  and in the Export / Import JSON (see §2).

### 1.2 Using Snapshots in E2E Tests

E2E test steps should be able to reference a snapshot alias instead of
providing a full inline JSON blob:

- New E2E step: `{ "op": "restore_snapshot", "alias": "initial-state" }`.
  The client-side runner resolves the alias from `app.state.snapshots` and
  sends `set_app_state` with the stored JSON.  The server-side runner
  should accept a `snapshots` map alongside the test definition so it can
  resolve aliases without a round-trip.
- The `E2eSetup.app_state` field (Rust) should also accept a string alias
  (try to look it up from a provided `snapshots` map, fall back to treating
  it as inline JSON).
- The "Add Step" form should offer `restore_snapshot` with a dropdown of all
  saved aliases.

### 1.3 Rust-Side Changes

- Extend the `RunE2eTests` request payload to accept an optional
  `snapshots: HashMap<String, serde_json::Value>`.
- When evaluating `restore_snapshot` or when `E2eSetup.app_state` is a
  string, look it up in the provided map.
- No persistent storage on the Rust side — the JS client owns the snapshot
  data and sends it along.

---

## 2. Export / Import JSON Overhaul

### 2.1 Clean Export

`exportProject()` currently serialises `app.state.tests` verbatim, which
includes internal runtime fields (`_result`, `lastResponse`, `status`,
`error`, `screenshot`, `duration_ms`) that are useless in a saved project
file.

**Fix:** Strip internal fields before serialisation:

```js
tests: app.state.tests.map(function(t) {
    return {
        name: t.name,
        steps: t.steps.map(function(s) {
            return { op: s.op, params: s.params || {} };
        }),
    };
}),
```

### 2.2 Include Important Metadata

The project export should additionally contain:

| Key              | Content                                                      |
|------------------|--------------------------------------------------------------|
| `snapshots`      | `app.state.snapshots` — all saved app-state aliases          |
| `htmlTree`       | Result of `get_node_hierarchy` at export time (optional)     |
| `resolvedSymbols`| A map of `{ address → resolvedInfo }` collected during the   |
|                  | session (see §6)                                             |
| `componentRegistry` | Last loaded component list (if available)                |

### 2.3 Import

`importProject()` should restore `snapshots` and (optionally) warn if the
HTML tree in the project differs from the current live tree.

---

## 3. Response Panel — Tree View

### Current State

The "Response" section in the Step Details panel (`showStepDetails()`)
dumps `step.lastResponse` as a `JSON.stringify(…, null, 2)` string inside
a `<pre>` block.

### Required Change

Reuse the existing `app.json` tree widget (the same one used by the App
State panel) to render the response:

```js
// In showStepDetails(), replace the <pre> block with:
var responseContainer = document.createElement('div');
responseContainer.id = 'step-response-tree';
app.json.render('step-response-tree', step.lastResponse);
```

The tree should be read-only (no inline editing) — either add a
`readOnly` option to `app.json.render()` or skip attaching edit handlers
when the container is not `app-state-tree`.

---

## 4. Component Registry — Fix Loading

### Bug

`handlers.loadComponents()` is never called.  The "components" sidebar view
starts with the placeholder text "Loading component registry…" and never
progresses.

### Fix

Call `handlers.loadComponents()` when the user switches to the components
view.  In `switchView('components')` (or the equivalent sidebar tab click
handler), add:

```js
if (view === 'components') {
    handlers.loadComponents();
}
```

Also consider calling it once during `init()` so the data is ready when the
user first opens the panel.

---

## 5. Runner Button Labels

### Current State

The toolbar has icon-only buttons with these tooltips:

| Icon           | Tooltip              | Action                      |
|----------------|----------------------|-----------------------------|
| ▶ (green)     | "Run (client)"       | `app.runner.run()`          |
| ☁↑            | "Run on server"      | `app.runner.runServerSide()`|
| ☁✓            | "Run all on server"  | `app.runner.runAllServerSide()` |

### Required Changes

1. **Rename tooltips:**
   - "Run (client)" → **"Run"**
   - "Run on server" → **"Run headless"**
   - "Run all on server" → **"Run all headless"**

2. **Headless execution in a separate window:**
   The "Run headless" mode should ideally not interfere with the main UI
   window.  Since the server-side runner already executes inside the
   application process, it operates without user-visible rendering.
   The tooltip / UI should clarify that "headless" means the test runs
   server-side without visual feedback (no window interaction), while
   "Run" drives the visible window step-by-step.

3. **Optional UX:** Show a small "headless" badge or indicator next to the
   cloud icon so the distinction is clear without hovering.

---

## 6. Function Pointer Resolving — Enhanced

### 6.1 Automatic Resolution

Currently, function pointers are only resolved when the user manually clicks
on a callback address in the node detail panel.

**Required:** Resolve automatically whenever:
- The inspector selects a new node (`showNodeDetail()`)
- The inspector initialises for the first time

Batch all callback addresses for the selected node into a single
`resolve_function_pointers` request and display results inline.

### 6.2 Source Location via `backtrace` (Rust Side)

The current `dladdr`-based resolver only returns the shared library path and
the symbol name — not the source file or line number.

**Enhance the Rust resolver (`resolve_function_pointer`):**

1. **Try `backtrace::resolve` first** (requires the user's binary compiled
   with `-g`).  If it returns `filename()` and `lineno()`, include them in
   `ResolvedSymbolInfo` as `source_file: Option<String>` and
   `source_line: Option<u32>`.

2. **Warn about `-g`:** If `backtrace` returns only the symbol name but no
   file/line, set a `hint` field in the response:
   `"Compile with -g for source locations"`.

3. **Minimal heuristic fallback (worst case):** If `backtrace` gives a
   symbol name but no file, do a **minimal** search of the current working
   directory for a definition of that symbol.  Constraints:
   - Only scan `.c`, `.cpp`, `.h`, `.cc`, `.m`, `.mm` files.
   - Respect `.gitignore` (use the `ignore` crate or equivalent).
   - Search pattern: `^.*\b<symbol_name>\s*\(` (find definition, not call
     sites).
   - Stop at the first match. This is a heuristic — flag it as approximate.

4. **Add `source_file` and `source_line` to `ResolvedSymbolInfo`:**
   ```rust
   pub struct ResolvedSymbolInfo {
       pub symbol_name: String,
       pub file_name: String,         // shared library path (existing)
       pub source_file: Option<String>, // source code file path (new)
       pub source_line: Option<u32>,    // line number (new)
       pub hint: Option<String>,        // e.g. "Compile with -g"
       pub approximate: bool,           // true if heuristic was used
   }
   ```

### 6.3 "Open in Editor" (JS Side)

When `source_file` and `source_line` are present, show a clickable
"Open" link next to the resolved symbol.

- **Command:** Use `open` (macOS) / `xdg-open` (Linux) / `start`
  (Windows) — **not** `code`.  This respects the user's default editor
  association.  On macOS, `open <file>` opens the file in the default
  app.
- **Alternative:** Construct a `vscode://file/<path>:<line>` URL and
  open it via `window.open()`. This works if VS Code is installed but
  doesn't require it.
- **Display:** Show the source file (last path component) and line
  number as a clickable link:
  `main.c:15` → clicking sends a request to the debug server with
  `{ op: 'open_file', file: '/full/path/main.c', line: 15 }`, which
  calls `open` on the server side.

### 6.4 Resolved Symbol Cache

Maintain a session-level cache `app.state.resolvedSymbols` mapping
`{ address → resolvedInfo }` so repeated node selections don't re-resolve
the same pointers.  Include this cache in the project export (see §2.2).

---

## 7. Editable Names — Pencil Icon

### Current State

Test names in the sidebar can be renamed via double-click, but there is no
visual affordance (no pencil icon).

### Required Changes

- Add a small pencil (✏️) icon next to every renameable item.  Clicking the
  pencil activates inline edit mode (same as the current double-click
  behaviour).
- Apply this pattern to:
  - **Test names** in the sidebar list
  - **Snapshot aliases** in the Saved Snapshots section (§1)
  - Any future user-editable labels

Use a `<span class="edit-icon" onclick="...">✏️</span>` element styled to
appear on hover (opacity 0 → 1 on `.test-item:hover .edit-icon`).

---

## 8. Previously Completed Requirements (Phase 1)

*(Kept for reference — already implemented.)*

1. CSS Properties: space-between layout, scrollable container, transparent
   border by default.
2. Accessibility section: role, aria-label, tab-index, contenteditable,
   focusable.
3. Clip Mask / Scroll nesting section.
4. Terminal inline image display (base64 PNG).
5. Slash command popup: examples array, named parameters, variant examples.
6. Test Explorer redesign: no large buttons, compact steps, editable names.
7. Add Step toolbar: left-aligned icon bar with play/pause/step/reset.
8. Add Step form: variant support, shared schema with slash commands.
9. E2E semantic failure checking: client-side and server-side runners check
   `success: false`, `found: false`, `passed: false` in API responses.
