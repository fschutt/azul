# syntax=docker/dockerfile:1
ARG AZUL_VERSION=0.2.0

FROM ghcr.io/fschutt/azul:${AZUL_VERSION} AS prelift
ARG AZUL_VERSION
ADD https://azul.rs/release/${AZUL_VERSION}/demos/azul-meet-linux /app/azul-meet.bin
RUN chmod +x /app/azul-meet.bin \
 && AZ_LIFT_CACHE_DIR=/cache AZ_PRELIFT=1 /app/azul-meet.bin || true

FROM ghcr.io/fschutt/azul:${AZUL_VERSION}
COPY --from=prelift /app/azul-meet.bin /app/azul-meet.bin
COPY --from=prelift /cache /cache
ENV AZ_BACKEND="web://0.0.0.0:8080?allow_public=1"
EXPOSE 8080
ENTRYPOINT ["/app/azul-meet.bin"]
