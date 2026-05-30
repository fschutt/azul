# syntax=docker/dockerfile:1
#
# ghcr.io/fschutt/azul — base image for running ANY azul app as a web app.
# =================================================================
# STATUS: DRAFT concept (see docker/web-base/README.md for the caveats).
#
# Ships **libazulwithremill.so** = libazul built with the `web-transpiler-static`
# feature, i.e. remill + LLVM + LLD linked IN-PROCESS. So an azul desktop binary
# linked against it can lift its OWN native machine code to WASM at runtime with
# no external toolchain and no separate "web build" — the same x86_64 desktop
# binary is reused (see https://azul.rs/guide/deploying-web).
#
# Published as `ghcr.io/fschutt/azul:<version>`; the per-app Dockerfiles
# in examples/<app>/Dockerfile `FROM` this. The final per-app image is distroless
# (glibc + a TCP stack, nothing else) + the binary + this .so + a pre-lifted
# /cache, so a container starts fast: only the app's own callbacks are lifted.
#
#   docker build --build-arg AZUL_REF=0.2.0 -t ghcr.io/fschutt/azul:0.2.0 .

ARG AZUL_REF=master

# ---- build libazulwithremill.so (libazul + in-process remill/LLVM/LLD) -------
FROM debian:bookworm-slim AS build
ARG AZUL_REF
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl git clang cmake ninja-build build-essential \
        libgl1-mesa-dev libx11-dev libv4l-dev pkg-config python3 xz-utils \
    && rm -rf /var/lib/apt/lists/*
RUN curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain 1.88.0
ENV PATH="/root/.cargo/bin:${PATH}"
RUN git clone --depth 1 --branch "${AZUL_REF}" \
        https://github.com/fschutt/azul.git /src \
    && cd /src && git submodule update --init --recursive third_party/remill || true
WORKDIR /src
# `web-transpiler-static` statically links remill + LLVM + LLD into the cdylib,
# so the resulting library lifts native → wasm in-process — no remill-lift / opt
# / llc / wasm-ld subprocesses, hence the final image needs no toolchain.
# (Building remill+LLVM is slow ~45 min; CI should cache this layer.)
RUN cargo build --profile prod-release -p azul-dll \
        --features "build-dll,web-transpiler-static" \
    && cp target/prod-release/libazul.so /libazulwithremill.so

# ---- base runtime: distroless + the in-process lifter -----------------------
FROM gcr.io/distroless/cc-debian12
LABEL org.opencontainers.image.source="https://github.com/fschutt/azul"
LABEL org.opencontainers.image.description="Base image for azul web apps (in-process remill lifter)"
LABEL org.opencontainers.image.licenses="MPL-2.0"
COPY --from=build /libazulwithremill.so /usr/local/lib/libazulwithremill.so
# AZ_LIFT_CACHE_DIR: where the lifted WASM cache lives (per-library + per-app),
# warmed at build time by each per-app Dockerfile's prelift stage.
ENV LD_LIBRARY_PATH=/usr/local/lib \
    AZ_LIFT_CACHE_DIR=/cache
