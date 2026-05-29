# syntax=docker/dockerfile:1
#
# Run azul-vault as a web app — reuse the Linux x86-64 desktop binary, lift it to
# WASM in-process (remill, via libazulwithremill.so in the base image) and serve
# it over HTTP. There is NO separate web build and no recompile from source: the
# same desktop binary you'd run locally is reused; only azul-vault's own callback /
# layout functions are lifted at container start (the azul library functions are
# pre-lifted into /cache at `docker build` time). STATUS: DRAFT concept — see
# https://azul.rs/guide/deploying-web.
#
#   docker build --build-arg AZUL_VERSION=0.2.0 -t azul-vault \
#     https://raw.githubusercontent.com/fschutt/azul/0.2.0/examples/azul-vault/Dockerfile
#   docker run -p 8080:8080 azul-vault          # then open http://localhost:8080
ARG AZUL_VERSION=0.2.0

# ---- prelift: warm the WASM cache at BUILD time (azul library + app fns) -----
# NB: the base image's libazulwithremill.so is built WITHOUT `debug-server`, so
# AZ_DEBUG / AZ_E2E are compiled out (no accidental open debug port in a deployed
# web app). The build-time prelift therefore uses a dedicated, non-debug
# lift-and-exit hook (AZ_PRELIFT) rather than the debug-server E2E runner. [TODO:
# AZ_PRELIFT hook is not implemented yet — until then the cache warms on the first
# request instead of at build time.]
FROM ghcr.io/fschutt/azul-web-base:${AZUL_VERSION} AS prelift
ARG AZUL_VERSION
ADD https://azul.rs/release/${AZUL_VERSION}/demos/azul-vault-linux /app/azul-vault.bin
RUN chmod +x /app/azul-vault.bin \
 && AZ_LIFT_CACHE_DIR=/cache AZ_PRELIFT=1 /app/azul-vault.bin || true

# ---- final: distroless + binary + in-process lifter + warm cache -------------
FROM ghcr.io/fschutt/azul-web-base:${AZUL_VERSION}
COPY --from=prelift /app/azul-vault.bin /app/azul-vault.bin
COPY --from=prelift /cache /cache
ENV AZ_BACKEND="web://0.0.0.0:8080?allow_public=1"
EXPOSE 8080
ENTRYPOINT ["/app/azul-vault.bin"]
