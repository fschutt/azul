---
slug: deploying-web
title: Deploying azul web apps with the pre-lifted base image
language: en
canonical_slug: deploying-web
audience: external
maturity: wip
guide_order: 300
topic_only: false
short_desc: Fast cold starts via a pre-lifted WASM Docker base image
prerequisites: [headless-rendering]
tracked_files:
  - dll/src/web/transpiler_remill.rs
  - dll/src/web/symbol_table.rs
  - dll/src/web/mod.rs
  - docker/Dockerfile
last_generated_rev: null
generated_at: 2026-05-27T00:00:00Z
default-search-keys:
  - AZ_BACKEND
  - run_web
---

# Deploying azul web apps with the pre-lifted base image

> **Note.** The web lift pipeline this builds on is validated end-to-end
> (aarch64/macOS + x86_64/Windows). The Docker base image
> (`ghcr.io/fschutt/azul-web-base`) is built by `.github/workflows/dockery.yml`
> on release tags. Build-time cache warming still depends on the prelift
> harness landing — see `docker/README.md`; until then the cache warms on the
> first request. Treat the speedup numbers as targets.

## Why cold starts are slow

When you run an azul app with `AZ_BACKEND=web://<host>:<port>`, azul does
**not** ship a hand-written WASM build of itself. Instead, the web backend
*lifts* the native machine code of the azul library into WebAssembly at
server startup, using an embedded [remill](https://github.com/lifting-bits/remill)-based
lifter. This is what lets the same Rust callbacks run server-side on the
desktop and client-side in the browser without a separate WASM toolchain.

The catch: lifting the *entire* library is slow — on the order of minutes
for the full layout + cascade dependency graph. That cost lands on the very
first request, which is a poor first experience for `hello-world`.

## The cache azul already keeps

The lifter caches the expensive part — the per-function lift — on disk,
keyed by a hash of each function's machine bytes. The same library function
always hashes to the same cache entry, **independent of which app is
running**, because the key is the code bytes, not the app. That is the
property that makes a *shared* cache possible: warm it once in CI, ship it,
and every app reuses it.

(Today the cache stores the lifted *LLVM IR*, so a hit skips the single
slowest step — the lift itself — but the app still runs the cheaper
optimize + WASM-link passes. See `docker/README.md` for the full
mechanics and the planned change that would persist the final WASM too.)

## Using the base image

Build your app's binary as usual, then base your container on the pre-lifted
image:

```dockerfile
FROM ghcr.io/fschutt/azul-web-base:0.1.0

# Your statically- or dynamically-linked azul app.
COPY target/release/my-app /usr/local/bin/my-app

# Bind the web backend. allow_public=1 is required to bind a non-loopback
# address (the default refuses 0.0.0.0 because the server has no auth on by
# default — add ?auth_token=... if you expose it).
ENV AZ_BACKEND="web://0.0.0.0:8080?allow_public=1"

EXPOSE 8080
CMD ["/usr/local/bin/my-app"]
```

Pull it directly to inspect:

```sh
docker pull ghcr.io/fschutt/azul-web-base:latest
```

### What happens on first request

1. The library functions your app touches are found in the **baked cache** —
   no multi-minute library lift.
2. Only **your own** code (your `LayoutCallback` and widget callbacks) is
   lifted, which is seconds, not minutes.
3. Subsequent requests reuse everything.

The image also carries the lifter toolchain (`remill-lift-17`, LLVM `llc`,
`opt`, `llvm-link`, `wasm-ld`) because step 2 still needs it to lift your
callbacks at runtime.

## How the cache is laid out in the image

The image bakes the warm cache at `/opt/azul/lift-cache` and points the
backend at it. Your derived image inherits that, so no extra configuration
is required. If you build your own variant, keep the cache location
consistent between the build-time warm-up and runtime.

## Caveats

- The base image pins a specific `libazul` build. If your app links a
  *different* azul version, the byte hashes differ and the cache misses —
  always match your app's azul version to the base image tag.
- The first lift of your own callbacks still happens on the first request.
  For latency-sensitive deployments, send one warm-up request at container
  start (a readiness probe works well).
- See `docker/README.md` for the load-order / cache-key caveat that
  must be addressed in the library before the cache hits reliably across
  arbitrary apps.

## Related

- Headless rendering: `headless-rendering`
- Web backend internals: `internals/web`
