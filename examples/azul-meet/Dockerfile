# syntax=docker/dockerfile:1
ARG AZUL_VERSION=0.2.0

# The demo is a release asset (AzMeet-linux.tar.gz: scripts/demo_apps.toml names it); a stage of its own fetches and unpacks it.
FROM alpine:3.20 AS fetch
ARG AZUL_VERSION
ADD https://github.com/fschutt/azul/releases/download/${AZUL_VERSION}/AzMeet-linux.tar.gz /tmp/demo.tar.gz
RUN mkdir /app && tar -xzf /tmp/demo.tar.gz -C /app && mv /app/AzMeet-linux /app/azul-meet.bin && chmod +x /app/azul-meet.bin

FROM ghcr.io/fschutt/azul:${AZUL_VERSION} AS prelift
ARG AZUL_VERSION
COPY --from=fetch /app/azul-meet.bin /app/azul-meet.bin
RUN AZ_LIFT_CACHE_DIR=/cache AZ_PRELIFT=1 /app/azul-meet.bin || true

FROM ghcr.io/fschutt/azul:${AZUL_VERSION}
COPY --from=prelift /app/azul-meet.bin /app/azul-meet.bin
COPY --from=prelift /cache /cache
ENV AZ_BACKEND="web://0.0.0.0:8080?allow_public=1"
EXPOSE 8080
ENTRYPOINT ["/app/azul-meet.bin"]
