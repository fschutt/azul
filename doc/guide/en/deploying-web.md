---
slug: deploying-web
title: Deploying to the web
language: en
canonical_slug: deploying-web
audience: external
maturity: wip
guide_order: 300
topic_only: false
short_desc: Run the same azul binary as a web app — how the WASM lift works and how to ship it
prerequisites: [hello-world, headless-rendering]
tracked_files:
  - dll/src/web/config.rs
  - dll/src/web/mod.rs
  - dll/src/web/html_render.rs
  - dll/src/web/transpiler_remill.rs
  - docker/Dockerfile
default-search-keys:
  - AZ_BACKEND
  - web-prelift
  - run_web
  - Route
---

# Deploying to the web

**The same binary that opens a desktop window can serve itself as a web app** —
no separate WASM build, no JS rewrite, no second codebase. You set one
environment variable and azul replaces the platform window with an HTTP server:

```sh
AZ_BACKEND=web://127.0.0.1:8080 ./my-azul-app
# open http://127.0.0.1:8080
```

Your `layout` callback runs on the server and is serialized to HTML + CSS; your
*event* callbacks (`on_click`, …) run in the browser as WebAssembly that azul
produces automatically from your compiled binary. This page explains **how that
works**, then **how to ship it**.

## What "lifting" is (and when each step runs)

azul does not ship a hand-written WASM build of itself. Instead it **lifts** the
native machine code of the library into WebAssembly with an embedded
[remill](https://github.com/lifting-bits/remill)-based lifter. That is what lets
one set of Rust callbacks run both server-side (desktop) and client-side
(browser). Lifting is slow — minutes for the whole framework — so it matters
*when* it happens. There are four distinct moments:

| Stage | When | What happens |
|---|---|---|
| **1. Docker build** (optional, once per release) | You build the base image | The whole **framework** is lifted to WASM and written to an on-disk cache (`AZ_LIFT_CACHE_DIR`). Slow, but done **once** and baked into the image. |
| **2. Server startup** | `./my-app` starts with `AZ_BACKEND=web://…` | Loads the warm cache. With a baked cache there is **nothing left to lift** for the framework; without one, this is where the multi-minute lift lands. |
| **3. First HTTP request** | A browser hits `/` | Only code **not** already cached is lifted — in practice just **your own** callbacks (seconds). Served at `/az/cb/{name}.{hash}.wasm`. |
| **4. In the browser** | The page runs | The client just **fetches and runs** that WASM. No lifting ever happens in the browser. |

The cache key is a hash of each function's **machine bytes**, not the app — so
the same framework function always maps to the same entry regardless of which
app is running. That is what makes a *shared, pre-warmed* cache possible: lift
the framework once in CI, ship it, and every app reuses it.

> **Portability note.** The lift output is architecture-neutral WASM, so the
> framework can be lifted on one CPU and served on another. azul's CI lifts on
> **aarch64** (where lifting is validated today) and bakes the result into an
> **x86_64** image.

## Quick start — run a demo

Once the base image is published (`ghcr.io/fschutt/azul-web-base`, built by
`.github/workflows/dockery.yml` on every website release):

```sh
docker run --rm -p 8080:8080 ghcr.io/fschutt/azul-web-base:0.2.0
# open http://localhost:8080
```

(Use `podman run …` — the command is identical.)

## Deploy your own app

Build your binary as usual, then base your container on the pre-lifted image so
your `docker build` only lifts **your** callbacks, not the framework:

```dockerfile
FROM ghcr.io/fschutt/azul-web-base:0.2.0

# Your azul app binary.
COPY target/release/my-app /usr/local/bin/my-app

# Bind the web backend. allow_public=1 is REQUIRED to bind a non-loopback
# address — the default refuses 0.0.0.0 because the server ships no auth.
# Add ?auth_token=… (and a reverse proxy, below) before exposing it publicly.
ENV AZ_BACKEND="web://0.0.0.0:8080?allow_public=1"
EXPOSE 8080
CMD ["/usr/local/bin/my-app"]
```

**Match your app's azul version to the base-image tag.** The cache is keyed by
machine bytes, so a different `libazul` build hashes differently and the cache
misses — pin both to the same version.

### Warming the cache yourself (`web-prelift`)

The base image is warmed by running an azul app once with the **prelift**
backend, which runs the exact server startup (Stages 1–2 above) and then exits
*before* it starts serving:

```sh
AZ_BACKEND=web-prelift://127.0.0.1:0 ./my-app   # lifts, populates the cache, exits
```

`web-prelift://` takes the same URL and query syntax as `web://`. Point it at
`AZ_LIFT_CACHE_DIR` and you get a warm cache you can copy into any image. This is
exactly what the base-image build does; you only need it if you build your own
variant.

## `AZ_BACKEND` URL format

- `web://127.0.0.1:8080` — localhost only (the safe default).
- `web://0.0.0.0:3000?allow_public=1` — all interfaces (requires `allow_public=1`).
- `web://[::1]:8080` — IPv6 loopback.
- `web://0.0.0.0:8443?tls_cert=cert.pem&tls_key=key.pem` — built-in TLS.
- `web-prelift://127.0.0.1:0` — warm the lift cache and exit (see above).

Query options: `allow_public`, `tls_cert`/`tls_key`, `auth_token`, `max_body`,
`max_connections`. The scheme is case-insensitive; a malformed value falls back
to the desktop shell.

## What runs where

| Runs on the **server** | Runs in the **browser** |
|---|---|
| `layout` callback (builds the `Dom`) | Browser layout + paint |
| Full CSS cascade (theme/viewport/container/lang all resolved) | `:hover` / `:focus` / `:active` (plain CSS, no round-trip) |
| HTML serialization + image/font collection | Event callbacks (`on_click`) as lifted WASM |
| Route matching | Reads the hydration payload into shared WASM memory |

The cascade is fully resolved server-side, so only interactive pseudo-states
ship as CSS. Event callbacks execute client-side against a hydrated `RefAny` in
shared WASM linear memory; mutations are read back by the loader and applied to
the DOM.

> **Maturity (WIP).** `on_click` → counter works end-to-end for `hello-world`.
> Broader event wiring (`mousemove`/`keydown`/…), full DOM diff/patch, and
> arbitrary hydration payloads are in progress — see
> [internals/web](internals/web.md#whats-bypassed). Complex apps fall back to
> server-side `POST /az/exec/{node_id}` execution.

## Routes

Routes use `:param` segments, passed via `AppConfig.routes`; without any, the
root layout callback becomes `/`:

```rust,ignore
let routes = vec![
    Route { pattern: "/users/:id".into(),     layout_callback: user_page() },
    Route { pattern: "/products/:sku".into(), layout_callback: product_page() },
];
```

Matching is exact → parameterized → `/` fallback. Assets are served under
`/az/` with content-hashed, `immutable` URLs (`/az/cb/…wasm`, `/az/img/…`,
`/az/font/…`), so a rebuild invalidates browser caches automatically.

## Production

The built-in server is a standalone HTTP listener — **no auth by default, no
load balancing**. For anything public, front it with a reverse proxy and bind to
loopback:

```text
[client] ──HTTPS──▶ [Caddy / nginx] ──HTTP──▶ [azul app on 127.0.0.1:8080]
```

```text
my-app.example.com {
    reverse_proxy 127.0.0.1:8080
    encode gzip
}
```

- **Memory**: the server holds all pre-rendered HTML, images, and fonts in RAM —
  a multi-route app with large images grows quickly.
- **Concurrency**: one OS thread per connection — fine for tens of users; for
  more, run several processes behind the proxy with shared external state.
- **Browser support**: plain HTML5 + ES5; the WASM path needs
  `WebAssembly.instantiateStreaming` + `BigInt` (Safari 14+, Chrome 85+,
  Firefox 78+ — evergreen browsers since 2020).

## Related

- [Headless rendering](headless-rendering.md) — the render pipeline without a window
- [Security model](security.md) — what azul does and doesn't defend against
- [Web backend internals](internals/web.md) — the full lift/dispatch deep dive (for contributors)
