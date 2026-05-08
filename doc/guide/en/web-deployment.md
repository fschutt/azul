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
generated_at: 2026-05-02T12:00:00Z
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Update
  - Callback
---

# Web Deployment

## Introduction

*WIP.* Server-side rendering and the HTTP server work today. Client-side WASM transpilation is stubbed — every callback currently round-trips to the server. The asset URLs, route mapping, and JS loader are stable and won't change shape when WASM lands.

The same binary that opens a desktop window can run as an HTTP server. Set the environment variable and azul replaces the platform shell with a TCP listener:

```sh
AZ_BACKEND=web://127.0.0.1:8080 ./my-azul-app
```

The layout callback runs natively in the server process. Its output is rendered to HTML with a `<style>` block built from the resolved CSS cascade, then served at `/`. Images and fonts are extracted and served under `/az/`.

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
- **Event callbacks.** Runs on the server via POST round-trip. Phase 0.
- **Framework WASM.** Runs on the client. Stubbed.
- **User-callback WASM.** Runs on the client. Stubbed.

Azul's full cascade resolves all conditional rules on the server (theme, viewport, container, language). Only interactive pseudo-states remain as CSS rules in the served stylesheet. The browser handles those without a round-trip.

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
- `/az/loader.js`. Bootstrap script, no cache header.
- `/az/mini.{hash}.wasm`. Framework WASM, `immutable, max-age=1y`.
- `/az/cb/{name}.{hash}.wasm`. Per-callback WASM, `immutable, max-age=1y`.
- `/az/img/{id}`. PNG-encoded image, `immutable, max-age=1y`.
- `/az/font/{id}`. TTF font bytes, `immutable, max-age=1y`.
- `POST /az/exec/{node_id}`. Runs the callback and returns new HTML, no cache header.

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

`/az/loader.js` attaches three behaviors to the served HTML:

1. **Callback wiring**. For each element with `data-az-cb` and `data-az-ev`, install an event listener that POSTs to `/az/exec/{cbId}` and replaces the document with the response.
2. **SPA-style navigation**. Internal `<a href="/...">` clicks are intercepted and routed through `fetch()` plus `history.pushState`. Asset URLs (`/az/...`) are excluded.
3. **Back/forward**. A `popstate` listener re-fetches `location.pathname`.

## Server-side callback execution

Today, every event runs on the server:

```text
[browser]  click → POST /az/exec/3 + JSON event payload
[server]   re-run layout, render HTML
[server]   200 text/html
[browser]  swap document
```

Plan for upcoming phases:

1. Dispatch `node_id` to the registered callback on the server, pass through the JSON event payload, and apply the resulting `Update::RefreshDom` before re-render.
2. Ship a real framework WASM bundle so DOM construction, cascade, and hit-testing happen in the browser. POSTs disappear for pure-style updates.
3. Lift user callbacks to WASM. Callbacks that lift cleanly run client-side; the rest fall back to POST.

Until lifting is functional, every callback runs server-side.

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

Browser support: the served HTML is plain HTML5 with no WASM requirement today. Anything that runs ES5 plus `fetch()` works, which covers all evergreen browsers.

## Coming Up Next

- [Headless Rendering](headless-rendering.md) — Running the pipeline without a window
- [Security Model](security.md) — What azul does and doesn't defend against
- [Code Generation](code-generation.md) — How `azul-doc` regenerates bindings from `api.json`
