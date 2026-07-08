# syntax=docker/dockerfile:1
#
# azul-web-base — pre-lifted WASM base image for azul web apps.
#
# azul's web backend (AZ_BACKEND=web://...) lifts the host machine code of
# libazul into WASM at server startup — slow (minutes). This image warms the
# on-disk lift cache (raw IR + objects) at an image-baked path
# (AZ_LIFT_CACHE_DIR) so a derived app skips lifting the whole library and only
# pays to lift ITS OWN callbacks/layout fn. The first request still triggers a
# lift, but library functions are served from the warm cache.
#
# Full design, caveats, "how a user extends it", and the required library-side
# changes live in docker/README.md.
#
# Build args: AZUL_REF (git ref to build libazul from; default master),
# RUST_TOOLCHAIN, LLVM_VERSION.

# ---------------------------------------------------------------------
# Stage 1: toolchain + remill — build (or vendor) the lifter binaries.
# ---------------------------------------------------------------------
# NOTE: building remill+LLVM from source takes ~45 min / ~10 GB; CI should
# cache this stage (or COPY a prebuilt toolchain image). See README.md.
FROM debian:bookworm-slim AS toolchain

ARG LLVM_VERSION=21
ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl git cmake ninja-build build-essential \
        python3 python3-pip xz-utils lsb-release wget software-properties-common \
    && rm -rf /var/lib/apt/lists/*

# LLVM/LLD/clang ${LLVM_VERSION} via apt.llvm.org — provides llc/opt/llvm-link/
# wasm-ld + clang for the cc-built azul_remill glue.
RUN curl -fsSL https://apt.llvm.org/llvm.sh -o /tmp/llvm.sh \
    && chmod +x /tmp/llvm.sh \
    && /tmp/llvm.sh ${LLVM_VERSION} all \
    && rm -f /tmp/llvm.sh
ENV PATH="/usr/lib/llvm-${LLVM_VERSION}/bin:${PATH}"

# remill-lift-17 — the AArch64/x86 → LLVM-IR lifter the web backend shells out
# to (subprocess path). Built from source here for self-containment; CI should
# COPY a prebuilt binary instead. See third_party/WEB_LIFTER_INSTALL.md.
WORKDIR /src
# remill is a pinned FORK (fschutt/remill). Clone + build it DIRECTLY at a pinned
# ref — NOT via the azul checkout — so this ~45-min build's layer cache is keyed
# on REMILL_REF alone and is NOT invalidated every time AZUL_REF (the app source,
# cloned later in Stage 2) changes. That's what keeps website iteration fast: the
# remill toolchain layer is reused across every azul rebuild until you bump
# REMILL_REF (i.e. only when the submodule commit actually moves).
ARG REMILL_REF=1d5dd7fa4cec8ed8e867b1aedc61c9746f624b13
RUN git clone https://github.com/fschutt/remill.git /src/remill \
    && cd /src/remill \
    && git checkout "${REMILL_REF}" \
    && git submodule update --init --recursive

# KNOWN BLOCKER (experimental): remill's cmake does `find_package(XED CONFIG)`,
# which needs XEDConfig.cmake — i.e. XED (and LLVM/glog/gflags) from trailofbits
# **cxx-common**, NOT a standalone XED build. Provision cxx-common for LLVM
# ${LLVM_VERSION} and pass its toolchain here (VCPKG_ROOT / CMAKE_TOOLCHAIN_FILE)
# before this succeeds. Until then the image build fails at this step — track
# https://github.com/lifting-bits/cxx-common. Left as-is so the intended build
# is documented; the web-base image is experimental and not yet green.
# Heavy (~45 min); the binary lands in /usr/local/bin so the DLL's subprocess
# lifter finds it via REMILL_LIFT_BIN (set in Stage 3).
RUN cd /src/remill \
    && cmake -G Ninja -B /tmp/remill-build . \
         -DCMAKE_BUILD_TYPE=Release \
         -DCMAKE_INSTALL_PREFIX=/opt/remill-install \
         -DREMILL_ENABLE_TESTING=OFF -DREMILL_BUILD_SPARC32=OFF \
    && cmake --build /tmp/remill-build --target remill-lift-17 \
    && cmake --install /tmp/remill-build \
    && cp /opt/remill-install/bin/remill-lift* /usr/local/bin/ 2>/dev/null || true

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

# Clone the APP source LAST (keyed on AZUL_REF) — everything above (remill +
# LLVM + rust + deps) is AZUL_REF-independent and stays cached across app
# rebuilds. Bump AZUL_REF (default master) per build via --build-arg.
ARG AZUL_REF=master
RUN git clone --depth 1 --branch "${AZUL_REF}" \
        https://github.com/fschutt/azul.git /src/azul

WORKDIR /src/azul
# Build libazul. The subprocess lift path is what we warm, so plain build-dll
# is sufficient (web-transpiler-static would only help the non-default
# in-process native path). See README.md / use_native_remill docs.
RUN cargo build --profile prod-release -p azul-dll --features="build-dll"

# Build a minimal azul app WITH the subprocess web transpiler so we can drive
# the whole-framework lift. Best-effort: if the `web` feature isn't wired in the
# built ref, this is a no-op and the cache warms on the first real request.
RUN cargo build --profile prod-release -p azul-examples --features web 2>/dev/null \
    || echo "[prelift] azul-examples[web] not buildable in this ref — cache warms on first request"

# ---------------------------------------------------------------------
# Stage 3: warm the lift cache (the actual "prelift").
# ---------------------------------------------------------------------
FROM builder AS prelift

# Pin both lift caches (raw IR + objects) to an image-baked location via
# AZ_LIFT_CACHE_DIR so the warm cache survives the multi-stage build.
ENV AZ_LIFT_CACHE_DIR=/opt/azul/lift-cache
# AZ_LIFT_CACHE=1 opts the subprocess path INTO the on-disk object cache
# (off by default); the raw-IR cache is on unless AZ_NO_LIFT_CACHE=1.
ENV AZ_LIFT_CACHE=1
RUN mkdir -p "${AZ_LIFT_CACHE_DIR}"

# Make the toolchain discoverable to the subprocess lifter.
ENV REMILL_LIFT_BIN=/usr/local/bin/remill-lift-17
ENV LLC=/usr/lib/llvm-21/bin/llc
ENV OPT=/usr/lib/llvm-21/bin/opt
ENV LLVM_LINK=/usr/lib/llvm-21/bin/llvm-link
ENV WASM_LD=/usr/lib/llvm-21/bin/wasm-ld

# Drive the one-shot whole-framework lift with the `web-prelift` backend: it runs
# the exact run_web startup (Phase A/B/D lift the eventloop + framework/callback
# fns into ${AZ_LIFT_CACHE_DIR}) then EXITS before binding the server — no harness,
# no synthetic-event hack, no hang. Best-effort; a missing binary is a no-op.
RUN AZ_BACKEND="web-prelift://127.0.0.1:0" \
    LD_LIBRARY_PATH=/src/azul/target/prod-release \
    /src/azul/target/prod-release/azul-examples 2>&1 | tail -40 \
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

# Make the warm cache discoverable: point AZ_LIFT_CACHE_DIR at the baked
# location so the library reads its raw-IR + object cache from there.
ENV AZ_LIFT_CACHE_DIR=/opt/azul/lift-cache
ENV AZ_LIFT_CACHE=1

# Derived apps set their own ENTRYPOINT/CMD. A sensible default bind:
ENV AZ_BACKEND="web://0.0.0.0:8080?allow_public=1"
EXPOSE 8080
