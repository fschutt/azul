#!/usr/bin/env bash
# Bring the local observability stack up or down.
#
#   ./run-stack.sh up      start everything and wait for it to be reachable
#   ./run-stack.sh down    stop everything and delete the metrics volume
#   ./run-stack.sh logs    follow the collector's log
#   ./run-stack.sh status  show container state + a reachability probe
#
# Works with Docker or rootless Podman. With Podman it starts the user-level
# API socket, which docker-compose talks to.
set -euo pipefail

cd "$(dirname "$0")"

# ── Pick a compose implementation ───────────────────────────────────────────
COMPOSE=""
if docker compose version >/dev/null 2>&1; then
    COMPOSE="docker compose"
elif command -v podman-compose >/dev/null 2>&1; then
    COMPOSE="podman-compose"
elif command -v docker-compose >/dev/null 2>&1; then
    COMPOSE="docker-compose"
else
    echo "error: need one of 'docker compose', 'podman-compose' or 'docker-compose'" >&2
    exit 1
fi

# ── Rootless Podman needs its API socket for the docker-compose clients ─────
# Only when there is no real Docker daemon socket and none was configured:
# on such a machine `docker` is usually podman's CLI shim, so probing `docker
# info` would succeed and tell us nothing.
if [ -z "${DOCKER_HOST:-}" ] && [ ! -S /var/run/docker.sock ] &&
   command -v podman >/dev/null 2>&1; then
    sock="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/podman/podman.sock"
    if [ ! -S "$sock" ]; then
        echo "starting rootless podman API socket ..."
        systemctl --user start podman.socket 2>/dev/null || true
    fi
    if [ -S "$sock" ]; then
        export DOCKER_HOST="unix://$sock"
        echo "using DOCKER_HOST=$DOCKER_HOST"
    fi
fi

wait_for() {
    local name="$1" url="$2" tries="${3:-60}"
    printf 'waiting for %-16s' "$name"
    for _ in $(seq "$tries"); do
        if curl -fsS -o /dev/null "$url" 2>/dev/null; then
            echo " ok"
            return 0
        fi
        printf '.'
        sleep 1
    done
    echo " TIMEOUT ($url)"
    return 1
}

case "${1:-up}" in
up)
    $COMPOSE up -d
    echo
    wait_for "VictoriaMetrics" "http://127.0.0.1:8428/health" || true
    wait_for "Loki" "http://127.0.0.1:3100/ready" 90 || true
    wait_for "Grafana" "http://127.0.0.1:3000/api/health" 90 || true
    # The collector's OTLP port answers 405 to a GET (it wants POST) and 401
    # without the token, so any HTTP response at all means it is listening.
    printf 'waiting for %-16s' "OTLP collector"
    for _ in $(seq 60); do
        if curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:4318/v1/metrics 2>/dev/null | grep -qE '^[1-5]'; then
            echo " ok"
            break
        fi
        printf '.'
        sleep 1
    done
    echo
    echo "Grafana:          http://127.0.0.1:3000  (anonymous admin, no login)"
    echo "Dashboard:        http://127.0.0.1:3000/d/azul-telemetry"
    echo "OTLP endpoint:    http://127.0.0.1:4318   token: azul-demo-token"
    echo "VictoriaMetrics:  http://127.0.0.1:8428"
    ;;
down)
    $COMPOSE down -v
    ;;
logs)
    $COMPOSE logs -f otel-collector
    ;;
status)
    $COMPOSE ps
    echo
    for probe in \
        "VictoriaMetrics http://127.0.0.1:8428/health" \
        "Loki http://127.0.0.1:3100/ready" \
        "Grafana http://127.0.0.1:3000/api/health"; do
        set -- $probe
        printf '%-16s %s\n' "$1" "$(curl -fsS -o /dev/null -w '%{http_code}' "$2" 2>/dev/null || echo unreachable)"
    done
    ;;
*)
    echo "usage: $0 {up|down|logs|status}" >&2
    exit 1
    ;;
esac
