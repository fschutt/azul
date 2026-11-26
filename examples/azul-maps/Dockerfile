# syntax=docker/dockerfile:1
ARG AZUL_VERSION=0.2.0

# The demo is a release asset (AzMaps-linux.tar.gz: scripts/demo_apps.toml names it); a stage of its own fetches and unpacks it.
FROM alpine:3.20 AS fetch
ARG AZUL_VERSION
ADD https://github.com/fschutt/azul/releases/download/${AZUL_VERSION}/AzMaps-linux.tar.gz /tmp/demo.tar.gz
RUN mkdir /app && tar -xzf /tmp/demo.tar.gz -C /app && mv /app/AzMaps-linux /app/azul-maps.bin && chmod +x /app/azul-maps.bin

FROM ghcr.io/fschutt/azul:${AZUL_VERSION} AS prelift
ARG AZUL_VERSION
COPY --from=fetch /app/azul-maps.bin /app/azul-maps.bin
RUN AZ_LIFT_CACHE_DIR=/cache AZ_PRELIFT=1 /app/azul-maps.bin || true

FROM ghcr.io/fschutt/azul:${AZUL_VERSION}
COPY --from=prelift /app/azul-maps.bin /app/azul-maps.bin
COPY --from=prelift /cache /cache
ENV AZ_BACKEND="web://0.0.0.0:8080?allow_public=1"
EXPOSE 8080
ENTRYPOINT ["/app/azul-maps.bin"]
