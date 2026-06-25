# syntax=docker/dockerfile:1
#
# hello-world.c in a browser — the "C hello-world running on the web, one command".
#
#   docker build https://azul.rs/ui/release/0.2.0/hello-world.Dockerfile -t azul-hello-world
#   docker run -p 8080:8080 azul-hello-world          # then open http://localhost:8080
#
# How it works: azul's web backend (AZ_BACKEND=web://) lifts the NATIVE x86-64
# machine code of hello-world's callbacks to WebAssembly at server start
# (remill → LLVM-IR → wasm) and serves the page + per-callback wasm over HTTP.
# There is NO separate "web build" and no JS/Emscripten port: the SAME C program
# you would run on the desktop is reused. Only hello-world's own on_click/layout
# functions are lifted on the first request; the azul library functions are served
# from the base image's warm /opt/azul/lift-cache. See
# https://azul.rs/guide/web-deployment and docker/README.md.
ARG AZUL_VERSION=0.2.0

# ---- build: compile hello-world.c against libazul.so --------------------------
# The runtime base is distroless (no compiler), so compile in a throwaway stage.
FROM debian:bookworm-slim AS build
ARG AZUL_VERSION
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl gcc libc6-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
# The C header + the upstream hello-world.c (mirrored on azul.rs / the git tag).
ADD https://azul.rs/ui/release/${AZUL_VERSION}/azul.h     /app/azul.h
ADD https://raw.githubusercontent.com/fschutt/azul/${AZUL_VERSION}/examples/c/hello-world.c /app/hello-world.c
# libazul.so comes straight from the runtime base image so the compile- and run-
# time libraries are byte-identical (no dependence on a separately-uploaded
# release artifact). BuildKit pulls the image once; the runtime stage reuses it.
COPY --from=ghcr.io/fschutt/azul:${AZUL_VERSION} /usr/local/lib/libazul.so /usr/local/lib/libazul.so
# -g keeps the .symtab function symbols the lifter resolves callbacks by name with
# (it does NOT strip); link dynamically against libazul.
RUN gcc -O2 -g /app/hello-world.c -I/app -L/usr/local/lib -lazul \
        -Wl,-rpath,/usr/local/lib -o /app/hello-world

# ---- runtime: pre-lifted web base (subprocess remill lifter + warm cache) -----
# ghcr.io/fschutt/azul — the shared web base image the azul-* demo Dockerfiles
# also FROM — ships libazul.so + the remill-lift/LLVM/wasm-ld toolchain + a warm
# library lift cache, so the container only lifts hello-world's OWN callbacks.
# Published by CI (docker/Dockerfile via .github/workflows/docker-base.yml); to
# build it yourself first:  docker build -t ghcr.io/fschutt/azul:0.2.0 docker/
FROM ghcr.io/fschutt/azul:${AZUL_VERSION}
COPY --from=build /app/hello-world /app/hello-world
# Bind all interfaces inside the container; allow_public opts past the localhost guard.
ENV AZ_BACKEND="web://0.0.0.0:8080?allow_public=1"
EXPOSE 8080
ENTRYPOINT ["/app/hello-world"]
