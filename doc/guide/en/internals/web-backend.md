---
slug: internals/web-backend
title: Web Backend Internals
language: en
canonical_slug: internals/web-backend
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: WASM target - DOM-attachment and OffscreenCanvas
prerequisites: []
tracked_files:
  - dll/src/web/cb_gen.rs
  - dll/src/web/classify.rs
  - dll/src/web/config.rs
  - dll/src/web/html_render.rs
  - dll/src/web/loader_js.rs
  - dll/src/web/mini_gen.rs
  - dll/src/web/mod.rs
  - dll/src/web/server.rs
  - dll/src/web/transpiler.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:50:16Z
---

> **WIP — Phase 0**: of the five planned phases (A–E), only **D** (HTML
> pre-render) and **E** (HTTP server) are functional. Phases A–C
> (api.json classification, `azul-mini.wasm` generation, callback
> transpilation) are stubs that emit empty/minimal artifacts. All
> callbacks execute server-side via `POST /az/exec/{node_id}`.

The web backend turns an Azul application into an HTTP server: setting
`AZ_BACKEND=web://0.0.0.0:8080` makes `App::run` dispatch to
`dll/src/web/mod.rs::run_web` instead of opening a native window. The
layout callback runs natively on the server, the resulting `StyledDom`
is serialized to HTML + a per-node `#az_N { … }` stylesheet, and a
small bootstrap JavaScript wires up callback dispatch. There is **no**
client-side WASM today; that is the long-term goal that Phases A–C are
placeholders for.

## Backend selection — web://ip:port

Parsed at `dll/src/web/config.rs:18`:

```rust,ignore
pub fn parse_web_url(s: &str) -> Option<SocketAddr>
```

Accepts `web://127.0.0.1:8080`, `web://0.0.0.0:3000`,
`web://[::1]:8080`. The `web://` prefix is case-insensitive; an optional
`?query` (e.g. `?tls=cert.pem`) is stripped before
`SocketAddr::from_str`. The result is wrapped in
`AzBackend::Web(SocketAddr)` and consumed by `dll/src/desktop/run.rs`,
which calls `run_web` instead of the native shell.

## run_web — the five-phase orchestrator

`dll/src/web/mod.rs:45`:

```rust,ignore
pub fn run_web(
    app_data: RefAny,
    config: AppConfig,
    fc_cache: Arc<FcFontCache>,
    font_registry: Option<Arc<FcFontRegistry>>,
    root_window: WindowCreateOptions,
    bind_addr: SocketAddr,
) -> Result<(), WindowError>
```

- **Phase A.** Functional: decompresses embedded `api.json` and classifies. Output is unused downstream.
  - `dll/src/web/classify.rs::classify_api_functions`
- **Phase B.** Stub. Returns the 8-byte WASM header.
  - `dll/src/web/mini_gen.rs::generate_mini_wasm`
- **Phase C.** Stub. Returns `Vec::new()`.
  - `dll/src/web/cb_gen.rs::discover_and_transpile_callbacks`
- **Phase D.** Functional. Renders the initial page for each route.
  - `dll/src/web/html_render.rs::render_initial_page`
- **Phase E.** Functional.
  - `dll/src/web/server.rs::run_server`

Phase D walks `config.routes` and calls `render_initial_page` for each.
When there are no routes, the root window's layout callback is rendered
at `/`. Each `RenderOutput` carries an HTML body, a vector of
`CollectedImage`, and a vector of `CollectedFont`. Image and font IDs
are *per-render*. `dll/src/web/mod.rs` rebases them onto a global ID
space and rewrites the URLs in the HTML so different routes don't
collide.

Phase E hands the merged state to `server::run_server`, which blocks
forever serving HTTP.

## Phase A — classify

`dll/src/web/classify.rs` decompresses an embedded brotli'd `api.json`
(roughly 120 KB compressed, 3.7 MB raw) and bins every C function into one of
three categories:

```rust,ignore
pub enum FnClass {
    Framework,             // AzDom_*, AzRefAny_*, ...      → goes into mini.wasm
    ServerEntryPoint,      // AzApp_run                     → never in WASM
    ReplaceWithDomPatcher, // AzDisplayList_*, AzGl_*       → emit setStyle() instead
}
```

Classification rules in `dll/src/web/classify.rs`:

```rust,ignore
fn classify_fn(name: &str) -> FnClass {
    match name {
        "AzApp_run" => FnClass::ServerEntryPoint,
        n if n.starts_with("AzDisplayList_") => FnClass::ReplaceWithDomPatcher,
        n if n.starts_with("AzGl_") => FnClass::ReplaceWithDomPatcher,
        _ => FnClass::Framework,
    }
}
```

The brotli blob is built at codegen time
(`target/codegen/api.json.br`, produced by `azul-doc codegen all`).
`classify_api_functions` is called from `run_web` for diagnostics only.
Phase 0 doesn't act on the classification.

## Phase B — mini_gen

`dll/src/web/mini_gen.rs` returns the smallest valid WASM module:

```rust,ignore
const WASM_HEADER: [u8; 8] = [
    0x00, 0x61, 0x73, 0x6D,  // \0asm magic
    0x01, 0x00, 0x00, 0x00,  // version 1
];
```

Browsers will load and parse this 8-byte module without complaint, so
the `<link rel="preload" href="/az/mini.{hash}.wasm">` hint in the
generated HTML resolves rather than 404'ing. The eventual implementation
will lift ~200 framework C functions from the running binary through
`Transpiler::lift_and_link_framework`.

## Phase C — transpiler and cb_gen

`dll/src/web/transpiler.rs` defines the trait:

```rust,ignore
pub trait Transpiler {
    fn lift_function(&self, fn_name: &str, fn_addr: usize, fn_size: usize)
        -> Result<WasmModule, TranspileError>;
    fn lift_and_link_framework(&self, functions: &[(String, usize, usize)])
        -> Result<WasmModule, TranspileError>;
    fn is_available(&self) -> bool;
    fn name(&self) -> &str;
}
```

The only implementation today is `StubTranspiler`, returned by
`default_transpiler()`. Both lift methods return `Err(TranspileError)`
and `is_available()` is `false`. The intended pipeline is:

```text
running native binary
  ─ dladdr / DWARF ─►  (fn_name, fn_addr, fn_size)
  ─ remill-rs   ─►  LLVM IR
  ─ llc -mtriple=wasm32 ─► WASM
  ─ wasm-link   ─►  module that imports `Az*` from azul-mini.wasm
```

`dll/src/web/cb_gen.rs` is the consumer. It would walk `config.routes`,
collect every callback function pointer in the resulting DOM, resolve
each pointer to a symbol via `dladdr`, and feed them into
`Transpiler::lift_function`. Today it returns `Vec::new()`, which means
the HTML emitter has no `<link rel="preload" href="/az/cb/*.wasm">`
hints to add and the server's `/az/cb/{name}.wasm` route always 404s.

## Phase D — html_render

`dll/src/web/html_render.rs::render_initial_page` produces a full HTML document. The
pipeline:

1. **Run the layout callback** with a `LayoutCallbackInfo` constructed
   from the same `RefAny` and `FullWindowState` the desktop backend
   uses. `image_cache` and `gl_context` are empty — no GPU on the
   server. Active route info is threaded through
   `LayoutCallbackInfoRefData` so route-aware layout callbacks see the
   matched pattern.

2. **Run the cascade**: `StyledDom::create_from_dom(dom)` resolves all
   conditional CSS (OS, theme, viewport, container queries, language)
   on the server, leaving only interactive pseudo-states (`:hover`,
   `:focus`, `:active`, `:focus-within`, etc.) to the browser. By the
   time HTML is emitted, every node has a fully-resolved
   `computed_values[node]` entry in the property cache.

3. **Walk the StyledDom flat arena** depth-first via
   `RenderContext::render_node_recursive`. Each node:
   - Gets a synthetic `id="az_N"` where `N` is a per-render counter.
   - Emits `<{tag} id="az_N" class="..." data-az-cb="N" ...>` —
     `data-az-cb` is present iff the node has callbacks. `data-az-ev`
     records the JS event name (e.g. `click`, `mousedown`) derived from
     the first callback's `EventFilter`.
   - Image nodes encode the bitmap to PNG via
     `azul_layout::image::encode_png`, push a `CollectedImage`, and
     rewrite the `src` to `/az/img/{id}`.
   - The `id` and `class` attributes from the DOM are preserved as
     `data-az-id` and `class=`, since `id="az_N"` is reserved for the
     synthetic node ID.

4. **Emit CSS rules**: `emit_css_from_cache` produces:
   - `#az_N { property: value; … }` for the base computed values.
   - `#az_N:hover { … }` / `:focus` / `:active` / etc. for properties
     that the property cache marks as state-dependent. The
     `pseudo_state_to_css` function in `dll/src/web/html_render.rs` maps
     `PseudoStateType::Dragging` to `:active`. That's the closest CSS
     equivalent because the browser has no "dragging" pseudo-class.

5. **Bundle fonts** as `@font-face` rules pointing at `/az/font/{id}`,
   then concatenate everything into a single `<style>` block.

6. **Inject the loader JS** via
   `loader_js::generate_loader_js("stub", &cb_wasms)`.

`RenderOutput` carries the assembled HTML plus the collected image and
font vectors that the server will serve under `/az/img/` and
`/az/font/`.

### Per-route ID rebasing

`dll/src/web/mod.rs` rewrites image and font URLs after a route renders so
that route 0's `/az/img/3` becomes route 1's `/az/img/8` (or whatever
the offset is). The simple `.replace(&old, &new)` is safe because the
URLs include a leading `/` and unambiguous numeric suffix.

## Phase E — server

`dll/src/web/server.rs::run_server`:

```rust,ignore
pub fn run_server(bind_addr: SocketAddr, state: WebServerState)
    -> Result<(), String>
```

A `TcpListener` accept loop spawning a `std::thread` per connection.
Zero external dependencies. The request line and headers are parsed
inline via `BufReader::read_line`. The 16 MB body cap in
`dll/src/web/server.rs` is the only DoS guard.

### Routes

- `GET /az/loader.js` returns the bootstrap JS string. Immutable cache OK, not cached today.
- `GET /az/mini.{hash}.wasm` returns `state.mini_wasm`, an 8-byte stub. Cached, immutable.
- `GET /az/cb/{name}.{hash}.wasm` returns per-callback WASM. Always 404 in Phase 0.
- `GET /az/img/{id}` returns the encoded image. Cached, immutable.
- `GET /az/font/{id}` returns font bytes. Cached, immutable.
- `POST /az/exec/{node_id}` runs server-side callback dispatch in Phase 0.
- `GET /favicon.ico` returns 204 No Content.
- `GET /<route-pattern>` returns pre-rendered HTML for the matching route.

Image, font, and WASM responses include
`Cache-Control: public, max-age=31536000, immutable` because their URLs
embed a content hash. HTML responses are not cached.

### POST /az/exec/{node_id} — Phase 0 callback dispatch

In `dll/src/web/server.rs`, the current implementation is a placeholder:

```rust,ignore
("POST", p) if p.starts_with("/az/exec/") => {
    let _node_id_str = p.strip_prefix("/az/exec/").unwrap_or("0");
    if content_length > 0 {
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body).map_err(...)?;
    }
    let html = re_render_body(state);
    send_response(&mut stream, 200, "text/html; charset=utf-8", html.as_bytes())
}
```

The node ID is parsed but unused. The body is read but discarded.
`re_render_body` re-runs the layout callback with the current
`app_data` and returns the entire new HTML page. The browser replaces
its document with the response. **No actual callback runs**. Every
POST behaves like a forced re-layout. The intended dispatch path
(parse `node_id`, look up the registered callback, invoke it with the
deserialized `CallbackInfo`, feed the resulting `Update` back through
the layout system) is unimplemented.

### Route matching

In `dll/src/web/server.rs`, the three-stage fallback for `GET /<path>` is:

1. Direct lookup in `state.rendered_routes` keyed by the literal path.
2. Loop through registered routes and call
   `azul_core::resources::match_route(&pattern, path)` — for
   parameterized patterns like `/users/{id}` this finds a template, but
   Phase 0 serves the un-parameterized template HTML rather than
   re-rendering with the captured params.
3. Fall back to the `/` route, then to any registered route, then to
   `404 No routes configured`.

### WebServerState

```rust,ignore
pub struct WebServerState {
    pub app_data: Arc<Mutex<RefAny>>,
    pub config: AppConfig,
    pub fc_cache: Arc<FcFontCache>,
    pub font_registry: Option<Arc<FcFontRegistry>>,
    pub window_state: FullWindowState,
    pub mini_wasm: Vec<u8>,
    pub cb_wasms: Vec<CallbackWasm>,
    pub layout_callback: LayoutCallback,
    pub rendered_routes: HashMap<String, RenderedRoute>,
    pub images: Vec<CollectedImage>,
    pub fonts: Vec<CollectedFont>,
}
```

`Arc<Mutex<RefAny>>` is the only synchronization point. `re_render_body`
locks it, calls into `render_initial_page`, and drops the lock.
Concurrent requests serialize through this mutex. There is no
per-connection state.

## loader_js — bootstrap script

`dll/src/web/loader_js.rs::generate_loader_js` returns a fixed JavaScript string. Three
things happen on `DOMContentLoaded`:

1. **Callback wiring**: every element with `data-az-cb` gets an event
   listener bound to its `data-az-ev` event type. The listener POSTs
   to `/az/exec/{cb-id}` with `{x, y, button, key}` JSON and replaces
   the document with the response via
   `document.open() / document.write() / document.close()`.

2. **Link interception**: every `<a href="/...">` (excluding `/az/`)
   becomes an SPA navigation. `azNavigate(path)` does
   `fetch(path) → document.open/write/close + history.pushState`.

3. **`popstate` handler**: browser back/forward triggers a
   `fetch(location.pathname)` and the same document-replacement dance.

`document.write` after `document.open` is a documented anti-pattern
because it tears down the script that called it. Phase 0 gets away
with it because each response is a complete page. For incremental
client-side updates the eventual replacement is `morphdom`-style
diffing or `documentElement.innerHTML = …`.

The `_mini_wasm_hash` and `_callbacks` parameters are accepted but
ignored in the Phase 0 generator. The `<link rel="preload">` hints
are emitted by `html_render`, not by `loader_js`.

### loader_js_hash

FNV-1a 64-bit hash of the loader JS, used for cache-busting URLs.
Mirrors `html_render::content_hash` exactly (same constants:
`0xcbf29ce484222325` offset basis, `0x100000001b3` prime). Both could
be unified into a `util::content_hash` if the duplication grows.

## Asset URL summary

```text
GET /                         → pre-rendered HTML
GET /az/loader.js             → bootstrap (always served from /az/loader.js)
GET /az/mini.{hash}.wasm      → framework wasm (8 bytes today)
GET /az/cb/{name}.{hash}.wasm → callback wasm (404 today)
GET /az/img/{id}              → image bytes
GET /az/font/{id}             → font bytes
POST /az/exec/{node_id}       → callback dispatch → returns full HTML
```

The `/az/` prefix is the only reserved namespace. Any other path is
matched against registered routes.

## Cross-references

- `dll/src/web/mod.rs::run_web` is the orchestrator.
- `dll/src/web/server.rs::run_server` is the accept loop and dispatch.
- `dll/src/web/html_render.rs::render_initial_page` produces the HTML.
- `dll/src/web/loader_js.rs::generate_loader_js` returns the bootstrap JS.
- `dll/src/web/transpiler.rs` defines the `Transpiler` trait.
- `dll/src/desktop/run.rs` has the four call sites that route into
  `run_web` based on `AzBackend::Web(addr)`.
- `target/codegen/api.json.br` is the embedded brotli'd API descriptor.
- [DOM Internals](dom.md) — the `Dom` / `NodeData` / `NodeType` model
  the renderer walks.
- [Cascade, Inheritance, Restyle](cascade.md) — the `StyledDom` and
  property cache the renderer reads.
- [Event System Internals](event-system.md) — the `EventFilter` enum
  mapped to JS event names.

## Coming Up Next

- [Rendering Pipeline](rendering-pipeline.md) — From `StyledDom` to pixels
- [WebRender Bridge](webrender-bridge.md) — How azul talks to WebRender
- [FFI Codegen](build-and-codegen.md) — How `cargo build` cascades and the codegen pass
