# syntax=docker/dockerfile:1
ARG AZUL_VERSION=0.2.0

# The demo is a release asset (AzWriter-linux.tar.gz: scripts/demo_apps.toml names it); a stage of its own fetches and unpacks it.
FROM alpine:3.20 AS fetch
ARG AZUL_VERSION
ADD https://github.com/fschutt/azul/releases/download/${AZUL_VERSION}/AzWriter-linux.tar.gz /tmp/demo.tar.gz
RUN mkdir /app && tar -xzf /tmp/demo.tar.gz -C /app && mv /app/AzWriter-linux /app/azul-writer.bin && chmod +x /app/azul-writer.bin

FROM ghcr.io/fschutt/azul:${AZUL_VERSION} AS prelift
ARG AZUL_VERSION
COPY --from=fetch /app/azul-writer.bin /app/azul-writer.bin
RUN AZ_LIFT_CACHE_DIR=/cache AZ_PRELIFT=1 /app/azul-writer.bin || true

FROM ghcr.io/fschutt/azul:${AZUL_VERSION}
COPY --from=prelift /app/azul-writer.bin /app/azul-writer.bin
COPY --from=prelift /cache /cache
ENV AZ_BACKEND="web://0.0.0.0:8080?allow_public=1"
EXPOSE 8080
ENTRYPOINT ["/app/azul-writer.bin"]
