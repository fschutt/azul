# M8.7 — HeadlessApp hydration plan (2026-05-16)

**Drafted in response to user direction** (verbatim quote):

> we need to write up the proper plan: window state + app config can be
> transmitted as json, as can "fc font cache". layout_cb_fn_addr can be
> resolved to a String by using dlsym on the server (which resolves the
> question of "what layout callback do we need to fetch so that we're
> ready for actually handling the RefreshDom). That leaves only
> StyledDom (which can be serialized too, see how the AZ_DEBUG inspector
> does that) and of course our RefAny: we should require a JSON callback
> here and then rebuild it on the client. I.e. we will make some debug
> output and "check on server startup" that the RefAny is serializable
> and deserializable to / from JSON. We cannot just dump the server
> memory as bytes because what happens if the initial RefAny bytes
> contains a Vec<> -> then the wasm client would reference bytes that
> are still on the server. So JSON serialization is the only option. We
> probably also need some "HeadlessApp" wrapper, I don't think that
> exists yet.

This document is the architecture spec. Code follows after sign-off.

## The HeadlessApp wrapper

There's an existing `HeadlessWindow` in
`dll/src/desktop/shell2/headless/mod.rs` that holds the desktop
headless-mode state (CommonWindowState, CpuBackend, config, font
registry, etc.). The **web** backend needs a parallel wrapper —
`HeadlessApp` — that owns the same logical state PLUS the bits needed
for client-side dispatch (lifted layout-cb URL, cb fn-addr → URL map,
etc.). This wrapper lives in `dll/src/web/headless.rs` (new file).

```rust
pub struct HeadlessApp {
    /// User's RefAny — the root app data.
    pub app_data: RefAny,
    /// App-level config (font_loading flags, accessibility, etc.).
    pub config: AppConfig,
    /// FcFontCache: same shape as desktop, but on the server side
    /// `path` fields stay as filesystem paths; the JSON serializer
    /// rewrites them to `/az/font/<id>` URLs for the wasm side.
    pub font_cache: Arc<FcFontCache>,
    /// Window state (size, title, decorations, etc.).
    pub window_state: FullWindowState,
    /// Current StyledDom — the rendered DOM tree.
    pub current_dom: StyledDom,
    /// Layout callback fn-addr (server-side raw value). Server
    /// dlsym-resolves this to a symbol NAME at hydration time; the
    /// name is what the wasm client uses to fetch the lifted
    /// `/az/layout/<name>.<hash>.wasm`.
    pub layout_cb_fn_addr: usize,
}
```

## Server-side startup validation

Before serving any HTTP, `run_web` validates:

1. **RefAny has a registered JSON serializer.** Call
   `AzRefAny_serializeToJson(&app_data)`. If it returns `None`, abort
   the web server with a clear error:
   ```
   [azul-web] FATAL: web backend requires the root RefAny to have a
   JSON serializer registered via AZ_REFLECT_JSON. Got AzRefAny with
   no toJson fn-ptr. Cannot hydrate state on the wasm client.
   See dll/azul.h's AZ_REFLECT_JSON macro for how to register.
   ```
2. **Roundtrip the JSON.** Serialize → deserialize → compare. If the
   roundtrip diverges, log a debug warning so users catch their own
   serializer bugs early (`Wrote {...}, read back {...}, fields
   differ`). Non-fatal — some user serializers may intentionally drop
   fields (transient cache state, etc.).
3. **Layout callback is dladdr-resolvable.** Call
   `resolve_fn_ptr(root_window.window_state.layout_callback.cb)`. If
   the name is `cb_<addr>` (fallback), warn — the wasm client won't be
   able to associate the lifted `/az/layout/<name>.<hash>.wasm` with
   anything user-meaningful. Non-fatal; the URL still works.

All validation happens once at `run_web` startup, before any HTTP
serving.

## JSON shape: the hydration payload

Embedded in the HTML head as:

```html
<script id="az-state" type="application/json">{...}</script>
```

Shape:

```json
{
  "version": 1,
  "config": { /* serde-serialized AppConfig */ },
  "window_state": { /* serde-serialized FullWindowState (subset of fields the wasm side actually uses) */ },
  "font_cache": {
    "fonts": [
      { "id": 0, "url": "/az/font/0", "family": "Inter", "weight": 400 },
      ...
    ]
  },
  "layout_cb": "layout",
  "layout_cb_hash": "9c4f784aa5ce135f",
  "refany": { /* user-supplied JSON from MyDataModel_toJson */ },
  "styled_dom": {
    /* render-tree shape — see styled_dom_to_render_tree in
       dll/src/desktop/shell2/common/debug_server.rs:4622 */
    "root": {
      "tag": "body",
      "id": "az_0",
      "callbacks": [],
      "children": [
        { "tag": "div", "id": "az_1", "text": "5", ... },
        {
          "tag": "button",
          "id": "az_3",
          "callbacks": [
            { "event": "click", "cb_fn_addr": 4291837952 }
          ],
          ...
        }
      ]
    }
  }
}
```

For each clickable node we emit `cb_fn_addr` (the native fn-pointer
address, resolved at server-side render time). The wasm client uses
this to look up the per-callback wasm via JS-side
`__az_resolve_callback`.

## WASM-side hydration

`AzStartup_init(json_ptr: u32, json_len: u32) -> state_ptr: u32`
will:

1. Parse the JSON bytes via a wasm-side JSON parser. **Open question**:
   which parser? Options:
   - `serde_json` lifted — fights the same SROA/memcpy issues we've
     hit before. Probably won't work cleanly.
   - `json` crate — pure-rust, no_std-friendly, probably lifts
     better but still untested.
   - Hand-written tiny JSON parser — minimal enough to lift cleanly,
     but limited to the subset we emit.
   - **Recommended**: hand-written for v1; swap to a library once
     M8.9 (framework-call routing) is mature enough to bring the
     allocator + Vec ops into scope.
2. Reconstruct each piece:
   - `AppConfig` — deserialize fields directly.
   - `FullWindowState` — deserialize fields.
   - `FcFontCache` — build from font list (urls instead of paths;
     fetch via JS shim on first font use).
   - `RefAny` — call the user's `<Type>_fromJson` via
     `__az_call_indirect` against the cb table slot the loader.js
     reserved for it. (This is the JSON-deserializer registration
     path; see [`AzStartup_registerStateDeserializer`].)
   - `StyledDom` — recursively walk the render-tree JSON, build
     `NodeData` + `NodeHierarchyItem` + `StyledNode` Vecs. Each
     `callbacks: [{event, cb_fn_addr}]` becomes an entry on the
     node's `NodeData.callbacks`.
   - `layout_cb` — store the fn-ptr as a `u64` (used as the
     `__az_resolve_callback` key for the lifted layout WASM).
3. Build a `HeadlessApp` Box, store its pointer in `EventloopState`.
4. Return state ptr to JS.

The deserializer is the BIG chunk of new wasm-side code. Estimate:
~500-1000 LOC depending on the StyledDom subset we support.

## Dispatch flow (post-hydration)

`AzStartup_dispatchEvent(state, kind, evt_ptr, evt_len, out_len_ptr)
-> patches_ptr`:

1. Decode event bytes (coords, button, modifiers — NO node_idx; JS
   doesn't do hit-test).
2. Walk `state.headless_app.current_dom` to find the node containing
   `(x, y)`. For MOUSE events, this is a real hit-test
   (bbox-recursion); for keyboard/focus events the "hit" is the
   currently-focused node.
3. Look up the cb fn-addr from the hit node's `callbacks` array for
   the matching event kind.
4. `__az_resolve_callback(cb_fn_addr)` → JS returns table index.
5. `__az_call_indirect(idx, refany.lo, refany.hi, info_ptr)`.
6. Process the `Update` result:
   - `DoNothing`: return 0.
   - `RefreshDom`: invoke layout-cb via the same call_indirect path
     (layout-cb table index is in `state.layout_cb_table_idx`).
     Diff new StyledDom against `state.current_dom`. Emit TLV
     patches. Update `state.current_dom`.
7. Return patches ptr + write length to `*out_len_ptr`.

## Iteration breakdown

1. **M8.7a — HeadlessApp + startup validation** (no wasm work yet).
   Define the struct. Wire startup checks. Verify hello-world's
   RefAny passes validation.
2. **M8.7b — JSON serializer**.
   `pub fn headless_app_to_json(&self) -> serde_json::Value`. Includes
   render-tree walk (port from debug_server). Server emits in HTML.
3. **M8.7c — JS load + dummy WASM init**.
   loader.js reads `<script id="az-state">`, allocates wasm memory,
   copies JSON bytes, calls `AzStartup_init(ptr, len)`. WASM-side
   stub: just remembers the ptr+len; doesn't parse yet.
4. **M8.7d — WASM hand-written JSON parser** (the hard part).
   Minimal tokenizer + recursive descent for the shapes we emit.
   Bump-allocator-backed `Vec` for arrays + recursive node trees.
5. **M8.7e — WASM-side `HeadlessApp` reconstruction**.
   Deserializers for AppConfig, WindowState, FontCache, StyledDom,
   RefAny (via user's `_fromJson` cb).
6. **M8.5c — Hit-test against the hydrated StyledDom**.
   `dispatchEvent` reads coords + walks node bboxes. Drops the
   `node_idx` parameter from the event-bytes format.
7. **M8.5d — Reconciliation + TLV patches**.
   After cb returns RefreshDom, invoke the lifted layout cb →
   diff → emit patches. Update current_dom.

Estimated total: ~10-15h focused work, 7 commits. Step 4 (WASM JSON
parser) is the biggest risk + the bottleneck — everything after
depends on it.

## Why JSON, not raw bytes

User direction:
> We cannot just dump the server memory as bytes because what happens
> if the initial RefAny bytes contains a Vec<> -> then the wasm client
> would reference bytes that are still on the server.

Confirmed. The `RefAny`'s internal pointers (refcount + data) point
into native heap. Even copying the contents through wouldn't be
enough — anything the user data references (Vec<>, String, nested
RefAnys, etc.) lives at native addresses that are meaningless to the
wasm linear memory.

JSON serialization is value-level: only the data itself crosses the
boundary, and the wasm side rebuilds the structure with its own
addresses.

## Why we still need M8.9 (framework-call routing)

The user's RefAny deserializer (`MyDataModel_fromJson`) calls
framework functions: `AzJson_asInt`, `AzRefAny_newC` (via the
AZ_REFLECT macro's `_upcast`), etc. These need to actually work
on the wasm side, not be noop'd by M7's intercept pass.

So M8.9 is a prerequisite for M8.7e (RefAny deserialization). Order:
M8.7a → M8.7b → M8.7c → M8.7d → M8.9 (or a subset of it: lift the
framework fns needed by `_fromJson` and route them) → M8.7e → M8.5c
→ M8.5d.

## Open architectural questions for the user

1. **JSON parser choice**. Hand-written (~200 LOC, lifts cleanly,
   limited shape support) vs. a crate (`json` / `serde_json` /
   `nanoserde`) which fights the lift.
2. **FcFontCache URL scheme**. `/az/font/<id>` is the natural choice
   (server already serves these). Anything more sophisticated?
3. **Server-side validation severity**. RefAny without serializer →
   FATAL (abort) per spec, or WARN + fall back to serving the static
   render with no interactivity?
4. **HeadlessWindow vs HeadlessApp**. The desktop side has
   `HeadlessWindow`. Should the web `HeadlessApp` just wrap a
   `HeadlessWindow` (DRY) or be its own type (decoupled)?
5. **StyledDom serialization completeness**. The debug_server walk
   emits tag/text/classes/children. Do we need style computed values
   in the JSON too, or are the existing per-node CSS rules in the
   HTML head sufficient for the wasm side to NOT re-run layout
   until RefreshDom fires?

End of plan. Awaiting user sign-off before any code.
