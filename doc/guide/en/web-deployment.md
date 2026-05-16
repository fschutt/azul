---
slug: web-deployment
title: Web Deployment
language: en
canonical_slug: web-deployment
audience: external
maturity: wip
guide_order: 400
topic_only: false
short_desc: Building for the browser via WASM
prerequisites: [hello-world, architecture]
tracked_files:
  - dll/src/web/classify.rs
  - dll/src/web/config.rs
  - dll/src/web/eventloop.rs
  - dll/src/web/headless.rs
  - dll/src/web/html_render.rs
  - dll/src/web/loader_js.rs
  - dll/src/web/mod.rs
  - dll/src/web/server.rs
  - dll/src/web/transpiler.rs
  - dll/src/web/transpiler_remill.rs
last_generated_rev: 38ff46cf3a85513e90205c82e4613e2a22173e3b
generated_at: 2026-05-16T20:55:00Z
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Update
  - Callback
---

# Web Deployment

## Introduction

*WIP — M8.7c.* Server-side rendering, the HTTP server, and a minimal client-side WASM dispatch path all work today. The hello-world `on_click` callback runs as remill-lifted WebAssembly in the browser and increments a counter that lives in shared wasm linear memory. More complex apps still fall back to server-side execution; the production roadmap is in [`scripts/HACKS_REVIEW_2026_05_16.md`](../../scripts/HACKS_REVIEW_2026_05_16.md).

The same binary that opens a desktop window can run as an HTTP server. Set the environment variable and azul replaces the platform shell with a TCP listener:

```sh
AZ_BACKEND=web://127.0.0.1:8080 ./my-azul-app
```

The layout callback runs natively in the server process. Its output is rendered to HTML with a `<style>` block built from the resolved CSS cascade, then served at `/`. Images and fonts are extracted and served under `/az/`. Per-callback WASM modules are lifted from the running binary's `.text` via remill and served under `/az/cb/`.

To enable client-side WASM dispatch, build the dll with the `web-transpiler` feature:

```sh
cargo build -p azul-dll --release --features "build-dll web web-transpiler"
```

Without `web-transpiler`, callbacks fall back to `POST /az/exec/{node_id}` server-side execution as in earlier releases.

## AZ_BACKEND URL format

Accepted forms:

- `web://127.0.0.1:8080`. Localhost only.
- `web://0.0.0.0:3000`. All IPv4 interfaces.
- `web://[::1]:8080`. IPv6 loopback.
- `web://0.0.0.0:443?tls=cert.pem`. Query string is ignored today.

The `web://` prefix is case-insensitive. Anything after `?` is reserved for future flags and silently dropped. A malformed value falls back to the desktop shell.

## What runs server-side, what runs client-side

- **Layout callback (`Dom` construction).** Runs on the server. Works.
- **CSS cascade.** Runs on the server. Works.
- **HTML serialization.** Runs on the server. Works.
- **Image and font collection.** Runs on the server. Works.
- **Browser layout and paint.** Runs on the client. Works (the browser does it).
- **`:hover` and `:focus` styling.** Runs on the client. Works (CSS only).
- **RefAny hydration into wasm memory.** Runs on the client at bootstrap. Works for simple data shapes (single u32 in hello-world).
- **`on_click` callback dispatch.** Runs on the client as remill-lifted WASM. Works for hello-world.
- **`mousemove` / `keydown` / `focus` / `resize` etc. dispatch.** Not yet wired (only `click` is in `azWireListeners`).
- **DOM diff + patch after `Update::RefreshDom`.** Not yet wired (loader.js uses a hardcoded `textContent =` shortcut on the counter node).

Azul's full cascade resolves all conditional rules on the server (theme, viewport, container, language). Only interactive pseudo-states remain as CSS rules in the served stylesheet. The browser handles those without a round-trip.

The wasm-side dispatch path uses a hydration payload embedded in the rendered HTML's `<head>`:

```html
<script id="az-hydrate" type="application/json">
  {"type_id":"4298653512","json":5}
</script>
```

The bootstrap reads it, allocates a wasm-side `AzRefAny` via `AzStartup_hydrate`, and passes the resulting pointer to every callback invocation. Mutations the cb makes to the user data are observable from JS via `DataView(memory.buffer).getUint32(modelPtr)` and applied back to the DOM. Today the JSON payload is restricted to a single number (hello-world's counter); arbitrary user data needs the lifted `_fromJson` path (M8.8 work — see [internals](internals/web.md#whats-bypassed)).

## Routes

Routes use `:param` segments. Pass them via `AppConfig.routes`:

```rust,ignore
let routes = vec![
    Route { pattern: "/users/:id".into(), layout_callback: user_page() },
    Route { pattern: "/products/:sku".into(), layout_callback: product_page() },
];
```

Without any routes, the root window's layout callback becomes `/`. The server tries exact match first, then parameterized match, then falls back to `/`.

## Asset URL layout

Every URL under `/az/` is generated and served by the framework:

- `/` and route patterns. Pre-rendered HTML, no cache header.
- `/az/loader.js`. Bootstrap script (inline-embedded in the HTML today).
- `/az/mini.{hash}.wasm`. Mini wasm (~5 KB, six `AzStartup_*` entry points), `immutable, max-age=1y`.
- `/az/cb/{name}.{hash}.wasm`. Per-callback wasm (~14 KB for hello-world; size scales with the transitive lift closure), `immutable, max-age=1y`.
- `/az/layout/{name}.{hash}.wasm`. Per-layout-callback wasm. Preloaded today but not yet instantiated by the loader.
- `/az/img/{id}`. PNG-encoded image, `immutable, max-age=1y`.
- `/az/font/{id}`. TTF font bytes, `immutable, max-age=1y`.
- `POST /az/exec/{node_id}`. Server-side callback dispatch fallback. Not invoked by the current loader (the wasm path is always tried first).

`{hash}` is a content hash so a build refresh invalidates the browser's cache automatically. POST bodies are capped at 16 MB; oversized payloads return `413 Payload Too Large`.

## CSS emission

Azul runs the cascade server-side and emits one rule per node:

```css
#az_0 { display: flex; flex-direction: column; padding: 12px; }
#az_1 { color: #333; font-size: 14px; }
#az_1:hover { color: #000; }
```

Base styles come from the resolved cascade (every condition already resolved). Pseudo-state overrides (`:hover`, `:focus`, `:active`, `:checked`, `:disabled`) are emitted as separate selectors. The browser does the rest.

Bundled fonts in `AppConfig.bundled_fonts` are emitted as `@font-face` rules pointing at `/az/font/{id}`. The server only emits fonts it has the bytes for; system font fallback is not exposed to the browser.

## The loader script

The bootstrap (`loader_js.rs::generate_m8_loader`, inline-embedded
in the HTML rather than served separately) does:

1. **Mini wasm bootstrap.** Fetches `/az/mini.<hash>.wasm`,
   instantiates with a shared `WebAssembly.Table` for indirect
   callback dispatch, calls `AzStartup_init` to allocate the
   eventloop state.
2. **Hydration.** Reads `<script id="az-hydrate">`, allocates the
   user data + calls `AzStartup_hydrate` to build a wasm-side
   `AzRefAny` in shared linear memory. Saves the resulting
   pointer (`azRefAnyPtr`) for callback invocation.
3. **Per-callback instantiation.** For each `[data-az-cb]
   [data-az-wasm]` element, fetches the cb wasm and instantiates
   it with `env.memory = mini.exports.memory` so both modules
   share linear memory.
4. **Click wiring.** Installs a `click` listener on `document.body`
   that walks the target's id chain for `az_N`, looks up the
   matching cb fn, invokes
   `cbFn(BigInt(azRefAnyPtr), 0n, infoPtr)`. If the cb returns
   `Update::RefreshDom` (1), reads the mutated counter from wasm
   memory and applies a `textContent =` update to the matching
   DOM node.
5. **SPA-style navigation.** Internal `<a href="/...">` clicks are
   intercepted and routed through `fetch()` plus
   `history.pushState`. Asset URLs (`/az/...`) are excluded.
6. **Back/forward.** A `popstate` listener re-fetches
   `location.pathname`.

The DOM diff/patch, mousemove dispatch, and full event chain via
`AzStartup_dispatchEvent` are next-session work — see
[internals/web.md](internals/web.md#whats-bypassed).

## Server-side fallback

The `POST /az/exec/{node_id}` route from earlier releases is still
implemented and runs the cb natively, returning a full HTML
replacement. The current loader does **not** invoke it (the wasm
path always wins on supported builds). It remains available for:

- Apps that need to bypass the wasm path on a per-cb basis.
- Browsers that fail to instantiate the cb wasm (the loader logs
  a warning and the cb becomes a silent no-op rather than falling
  back automatically).
- Manual testing via `curl -X POST http://localhost:8080/az/exec/3`.

A future bootstrap version will fall back to POST automatically
when wasm instantiation fails.

## Deploying

The server has no built-in TLS, no auth, and no load balancing. It is a standalone HTTP listener with `Connection: close` semantics. For a public deployment, front it with a reverse proxy (nginx, Caddy, traefik):

```text
[client] ──HTTPS──> [Caddy/nginx] ──HTTP──> [azul app on 127.0.0.1:8080]
```

A minimal Caddyfile:

```text
my-app.example.com {
    reverse_proxy 127.0.0.1:8080
    encode gzip
}
```

Bind to `127.0.0.1` (not `0.0.0.0`) when behind a proxy on the same host. Bind to `0.0.0.0` only inside a container or behind an infrastructure firewall. Port 0 is not supported.

Resource considerations:

- **Memory**: the server keeps the entire pre-rendered HTML, all collected images, and all collected fonts in RAM. A multi-route app with large images can grow quickly.
- **Concurrency**: one OS thread per connection. Fine for tens of concurrent users; for more, run multiple processes behind the proxy and share state externally.
- **State**: callback-heavy workloads serialize through a single mutex on the shared app data.

Browser support: the served HTML is plain HTML5 + ES5; the wasm
path additionally requires `WebAssembly.instantiateStreaming` +
`BigInt` (Safari 14+, Chrome 85+, Firefox 78+ — all evergreen
browsers from 2020 onward).

## Coming Up Next

- [Headless Rendering](headless-rendering.md) — Running the pipeline without a window
- [Security Model](security.md) — What azul does and doesn't defend against
- [Code Generation](code-generation.md) — How `azul-doc` regenerates bindings from `api.json`
