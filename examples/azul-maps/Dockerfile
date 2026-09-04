# syntax=docker/dockerfile:1
#
# Run azul-maps as a web app — reuse the Linux x86-64 desktop binary, lift it to
# WASM in-process (remill, via libazulwithremill.so in the base image) and serve
# it over HTTP. There is NO separate web build and no recompile from source: the
# same desktop binary you'd run locally is reused; only azul-maps's own callback /
# layout functions are lifted at container start (the azul library functions are
# pre-lifted into /cache at `docker build` time). STATUS: the prelift hook and the x86 lift are implemented; the
# image flow itself is not yet covered by CI. See
# https://azul.rs/guide/deploying-web.
#
#   docker build --build-arg AZUL_VERSION=0.2.0 -t azmaps \
#     https://raw.githubusercontent.com/fschutt/azul/0.2.0/examples/azul-maps/Dockerfile
#   docker run -p 8080:8080 azmaps          # then open http://localhost:8080
ARG AZUL_VERSION=0.2.0

# ---- prelift: warm the WASM cache at BUILD time (azul library + app fns) -----
# NB: the base image's libazulwithremill.so is built WITHOUT `debug-server`, so
# AZ_DEBUG / AZ_E2E are compiled out (no accidental open debug port in a deployed
# web app). The build-time prelift therefore uses the `web-prelift://` backend
# URL: same syntax as `web://`, but it exits once the lift cache is warm instead
# of serving, so no debug surface is involved.
FROM ghcr.io/fschutt/azul:${AZUL_VERSION} AS prelift
ARG AZUL_VERSION
ADD https://azul.rs/release/${AZUL_VERSION}/demos/azul-maps-linux /app/azul-maps.bin
RUN chmod +x /app/azul-maps.bin \
 && AZ_LIFT_CACHE_DIR=/cache AZ_BACKEND="web-prelift://0.0.0.0:8080" /app/azul-maps.bin

# ---- final: distroless + binary + in-process lifter + warm cache -------------
FROM ghcr.io/fschutt/azul:${AZUL_VERSION}
COPY --from=prelift /app/azul-maps.bin /app/azul-maps.bin
COPY --from=prelift /cache /cache
ENV AZ_BACKEND="web://0.0.0.0:8080?allow_public=1"
EXPOSE 8080
ENTRYPOINT ["/app/azul-maps.bin"]
