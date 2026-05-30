# syntax=docker/dockerfile:1
#
# azul-web-base — pre-lifted WASM base image for azul web apps
# =============================================================
#
# STATUS: DRAFT (2026-05-27). See docker/web-base/README.md for the
# feasibility analysis. This image is only fully effective once the
# determinism + cache-path caveats listed there are addressed in
# dll/src/web/. Until then it warms the *IR* lift cache, which still
# saves the single most expensive per-function step (the remill-lift
# subprocess) but NOT the per-function opt/llc/wasm-ld passes.
#
# WHAT THIS IMAGE DOES
# --------------------
# azul's web backend (AZ_BACKEND=web://...) lifts the host x86_64/aarch64
# machine code of the azul library into WASM *at server startup*, via an
# embedded remill-based lifter. Lifting the whole library is slow
# (minutes). The subprocess lift path caches the raw lifted LLVM IR on
# disk, keyed by a hash of each function's machine bytes (see
# `lift_cache_path` in dll/src/web/transpiler_remill.rs).
#
# This image:
#   1. installs the lifter toolchain (remill-lift-17 + LLVM/LLD 21 +
#      wasm-ld) and a prebuilt libazul.so,
#   2. runs a one-shot "prelift" of the azul library so the IR cache in
#      $AZ_LIFT_CACHE_HOME is populated,
#   3. ships that warm cache in the image so a derived app only pays to
#      lift ITS OWN callbacks/layout fn, not the whole library.
#
# HOW A USER EXTENDS IT
# ---------------------
#   FROM ghcr.io/fschutt/azul:<version>
#   COPY my-app /usr/local/bin/my-app
#   ENV AZ_BACKEND=web://0.0.0.0:8080?allow_public=1
#   CMD ["/usr/local/bin/my-app"]
#
# The first request still triggers a lift, but library functions are
# served from the warm cache and only the app's own code is lifted.
#
# BUILD ARGS
# ----------
#   AZUL_REF        git ref / tag of azul to build libazul from (default: master)
#   PRELIFT_TARGET  path to a tiny "warm-up" binary that links libazul and
#                   runs run_web once under AZ_E2E to drive the library lift.
#                   Defaults to the in-repo prelift harness (see below).

# ---------------------------------------------------------------------
# Stage 1: toolchain + remill — build (or vendor) the lifter binaries.
# ---------------------------------------------------------------------
# NOTE: building remill+LLVM from source (scripts/build_remill.sh) takes
# ~45 min and ~10 GB. For CI we recommend caching this stage aggressively
# (GitHub Actions cache / a separate `azul-remill-toolchain` image) and
# only rebuilding it when third_party/remill or the LLVM pin changes.
FROM debian:bookworm-slim AS toolchain

ARG LLVM_VERSION=21
ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl git cmake ninja-build build-essential \
        python3 python3-pip xz-utils lsb-release wget software-properties-common \
    && rm -rf /var/lib/apt/lists/*

# LLVM/LLD/clang ${LLVM_VERSION} via the official apt.llvm.org script.
# This provides `llc`, `opt`, `llvm-link`, `wasm-ld` (as
# `wasm-ld-${LLVM_VERSION}`) and clang for the cc-built azul_remill glue.
RUN curl -fsSL https://apt.llvm.org/llvm.sh -o /tmp/llvm.sh \
    && chmod +x /tmp/llvm.sh \
    && /tmp/llvm.sh ${LLVM_VERSION} all \
    && rm -f /tmp/llvm.sh
ENV PATH="/usr/lib/llvm-${LLVM_VERSION}/bin:${PATH}"

# remill-lift-17 — the AArch64/x86 → LLVM-IR lifter the web backend shells
# out to on the default (subprocess) path. Two options:
#   (a) build from source with scripts/build_remill.sh (slow; needs the
#       third_party/remill submodule + Trail-of-Bits cxx-common bundle), or
#   (b) COPY a prebuilt remill-lift-17 produced by a separate CI job.
# We use (a) here for self-containment; CI should prefer (b) for speed.
WORKDIR /src
ARG AZUL_REF=master
RUN git clone --depth 1 --branch "${AZUL_REF}" \
        https://github.com/fschutt/azul.git azul \
    && cd azul \
    && git submodule update --init --recursive third_party/remill || true

# Build remill (idempotent; produces third_party/remill-install/install).
# If this is too heavy for your CI, replace with a COPY from a cached
# toolchain image and delete this RUN.
RUN cd /src/azul \
    && REMILL_INSTALL_DIR=/opt/remill-install bash scripts/build_remill.sh \
    && cp /opt/remill-install/install/bin/remill-lift* /usr/local/bin/ 2>/dev/null || true

# ---------------------------------------------------------------------
# Stage 2: build libazul + the prelift harness with Rust.
# ---------------------------------------------------------------------
FROM toolchain AS builder

ARG RUST_TOOLCHAIN=1.88.0
RUN curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain ${RUST_TOOLCHAIN}
ENV PATH="/root/.cargo/bin:${PATH}"

# System deps libazul needs at build time on Linux (matches rust.yml).
RUN apt-get update && apt-get install -y --no-install-recommends \
        clang libgl1-mesa-dev libx11-dev libv4l-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src/azul
# Build the dynamic library. The default (subprocess) lift path is what we
# warm here, so the standard build-dll feature is sufficient — we do NOT
# need web-transpiler-static for the SUBPROCESS cache (it would only help
# if we warmed the in-process native path, which is non-default and
# currently miscompiles the full library; see use_native_remill docs).
RUN cargo build --profile prod-release -p azul-dll --features="build-dll"

# Build the prelift harness: a minimal azul app that constructs a trivial
# DOM and starts run_web once, which triggers the full EVENTLOOP_SYMBOLS +
# layout-callback library lift, populating the IR cache. The harness exits
# after one synthetic request via AZ_E2E.
#
# DRAFT NOTE: this binary does not exist yet. See README.md "Required code
# change #2" — add `examples/rust/prelift.rs` (or a `cargo run -p azul-dll
# --bin az-prelift`) that runs run_web with a tiny static DOM under
# AZ_E2E so the lift runs headless and terminates. Until that exists, the
# line below is a placeholder and the prelift stage is a no-op shell.
RUN cargo build --profile prod-release -p azul --example prelift 2>/dev/null \
    || echo "[prelift] example not present yet — see README.md change #2"

# ---------------------------------------------------------------------
# Stage 3: warm the lift cache (the actual "prelift").
# ---------------------------------------------------------------------
FROM builder AS prelift

# Pin the lift IR cache to a stable, image-baked location. The library
# code currently hardcodes `std::env::temp_dir().join("az-lift-cache")`
# (see lift_cache_path). For the cache to survive into the final image
# AND be found at app runtime, TMPDIR must point at the same baked dir in
# both this stage and any derived image.
#
# DRAFT NOTE: see README.md "Required code change #1" — add an
# AZ_LIFT_CACHE_DIR env override to lift_cache_path so we don't have to
# hijack TMPDIR. Using TMPDIR here as the only currently-available lever.
ENV AZ_LIFT_CACHE_HOME=/opt/azul/lift-cache
ENV TMPDIR=${AZ_LIFT_CACHE_HOME}
RUN mkdir -p "${AZ_LIFT_CACHE_HOME}"

# Make the toolchain discoverable to the subprocess lifter.
ENV REMILL_LIFT_BIN=/usr/local/bin/remill-lift-17
ENV LLC=/usr/lib/llvm-21/bin/llc
ENV OPT=/usr/lib/llvm-21/bin/opt
ENV LLVM_LINK=/usr/lib/llvm-21/bin/llvm-link
ENV WASM_LD=/usr/lib/llvm-21/bin/wasm-ld

# Drive the one-shot library lift. AZ_E2E makes run_web render + dispatch a
# single synthetic event then exit, so the build does not hang on a live
# server. The lift populates ${AZ_LIFT_CACHE_HOME}/az-lift-cache/*.lifted.ll.
#
# DRAFT NOTE: requires the prelift harness from change #2. Placeholder:
RUN AZ_BACKEND="web://127.0.0.1:0" \
    AZ_E2E="/src/azul/scripts/m9_e2e/prelift-warm.json" \
    LD_LIBRARY_PATH=/src/azul/target/prod-release \
    /src/azul/target/prod-release/examples/prelift 2>&1 | tail -40 \
    || echo "[prelift] harness not present — cache will warm on first real request"

# ---------------------------------------------------------------------
# Stage 4: slim runtime image — toolchain + libazul + warm cache.
# ---------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

ARG LLVM_VERSION=21
LABEL org.opencontainers.image.source="https://github.com/fschutt/azul"
LABEL org.opencontainers.image.description="Pre-lifted WASM base image for azul web apps (AZ_BACKEND=web)"
LABEL org.opencontainers.image.licenses="MPL-2.0"

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates libgl1-mesa-glx libx11-6 \
    && rm -rf /var/lib/apt/lists/*

# The lifter toolchain must be present at app runtime too, because the
# derived app still lifts ITS OWN callbacks/layout fn on first request.
COPY --from=toolchain /usr/lib/llvm-${LLVM_VERSION}/bin/llc        /usr/lib/llvm-${LLVM_VERSION}/bin/llc
COPY --from=toolchain /usr/lib/llvm-${LLVM_VERSION}/bin/opt        /usr/lib/llvm-${LLVM_VERSION}/bin/opt
COPY --from=toolchain /usr/lib/llvm-${LLVM_VERSION}/bin/llvm-link  /usr/lib/llvm-${LLVM_VERSION}/bin/llvm-link
COPY --from=toolchain /usr/lib/llvm-${LLVM_VERSION}/bin/wasm-ld    /usr/lib/llvm-${LLVM_VERSION}/bin/wasm-ld
COPY --from=toolchain /usr/local/bin/remill-lift-17                /usr/local/bin/remill-lift-17

# Prebuilt libazul + the warm IR cache.
COPY --from=builder  /src/azul/target/prod-release/libazul.so      /usr/local/lib/libazul.so
COPY --from=prelift  /opt/azul/lift-cache                          /opt/azul/lift-cache

ENV PATH="/usr/lib/llvm-${LLVM_VERSION}/bin:${PATH}"
ENV LD_LIBRARY_PATH="/usr/local/lib:${LD_LIBRARY_PATH}"
ENV REMILL_LIFT_BIN=/usr/local/bin/remill-lift-17
ENV LLC=/usr/lib/llvm-${LLVM_VERSION}/bin/llc
ENV OPT=/usr/lib/llvm-${LLVM_VERSION}/bin/opt
ENV LLVM_LINK=/usr/lib/llvm-${LLVM_VERSION}/bin/llvm-link
ENV WASM_LD=/usr/lib/llvm-${LLVM_VERSION}/bin/wasm-ld

# Make the warm cache discoverable. The library reads the cache from
# $TMPDIR/az-lift-cache, so point TMPDIR at the baked location. Once
# AZ_LIFT_CACHE_DIR lands (README change #1) prefer that and leave TMPDIR
# to the OS default.
ENV TMPDIR=/opt/azul/lift-cache
ENV AZ_LIFT_CACHE_HOME=/opt/azul/lift-cache

# Derived apps set their own ENTRYPOINT/CMD. A sensible default bind:
ENV AZ_BACKEND="web://0.0.0.0:8080?allow_public=1"
EXPOSE 8080
