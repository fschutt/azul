#!/usr/bin/env python3
"""A local iroh relay for tests: iroh's own relay server (`iroh-relay`, the binary of the
iroh-relay crate) in dev mode - plain HTTP on 127.0.0.1, no TLS, no QUIC address discovery, no
external network - with its metrics, which count the bytes it relayed.

Build the binary once, outside the repository (the workspace runs iroh / iroh-relay 1.2.0; the
crate ships its own Cargo.lock, and its `server` feature is what builds the binary):

    cd /tmp && cargo install iroh-relay@1.2.0 --locked --features server --root ~/.cache/azul/iroh-relay

It lands in ~/.cache/azul/iroh-relay/bin/iroh-relay, where `find_binary` looks (after --relay-bin,
AZMEET_RELAY_BIN and PATH). On its own:

    python3 scripts/iroh_relay_dev.py [--relay-bin <path>] [--port 3340] [--metrics-port 9095]

starts the relay, prints its URL (`--relay <url> --relay-only` for AzMeet) and the bytes it relayed
every 5 seconds until Ctrl+C. scripts/azmeet_e2e.py's relay phase uses `DevRelay`.

The relay is started with `--dev --config-path <toml>`: the config binds the relay and its metrics
to 127.0.0.1 (dev mode alone binds [::]:3340 and [::]:9090, every interface). Its metrics are
OpenMetrics text on any path of the metrics port: `relayserver_bytes_recv_total` (datagram bytes
the clients sent it), `relayserver_bytes_sent_total` (what it passed on), `relayserver_accepts_total`
(client connections).
"""

import argparse
import json
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request

BUILD_COMMAND = ("cd /tmp && cargo install iroh-relay@1.2.0 --locked --features server "
                 "--root ~/.cache/azul/iroh-relay")
DEFAULT_ROOT = os.path.join("~", ".cache", "azul", "iroh-relay", "bin", "iroh-relay")
METRIC_LINE = re.compile(r"^([A-Za-z_:][A-Za-z0-9_:]*)(?:\{[^}]*\})?\s+([-+0-9.eE]+|NaN|[+-]Inf)\s*$")


class RelayError(Exception):
    pass


def find_binary(explicit=None):
    """The iroh-relay binary: `explicit`, AZMEET_RELAY_BIN, the build command's root, PATH,
    ~/.cargo/bin; None when there is none."""
    candidates = [explicit, os.environ.get("AZMEET_RELAY_BIN"), os.path.expanduser(DEFAULT_ROOT),
                  shutil.which("iroh-relay"), os.path.expanduser(os.path.join("~", ".cargo", "bin", "iroh-relay"))]
    for c in candidates:
        if c and os.path.isfile(c) and os.access(c, os.X_OK):
            return os.path.abspath(c)
    return None


def free_port():
    """A TCP port nothing listens on at 127.0.0.1 right now."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def parse_metrics(text):
    """{metric name: value} of an OpenMetrics / Prometheus text page (labelled series summed)."""
    found = {}
    for line in text.splitlines():
        if not line or line.startswith("#"):
            continue
        m = METRIC_LINE.match(line.strip())
        if not m:
            continue
        try:
            value = float(m.group(2))
        except ValueError:
            continue
        found[m.group(1)] = found.get(m.group(1), 0.0) + value
    return found


def metric(found, stem):
    """The value of the metric named `stem` in any group (`relayserver_bytes_recv_total` for
    `bytes_recv`), 0 when absent."""
    pattern = re.compile(r"(?:^|_)%s(?:_total)?$" % re.escape(stem))
    return sum(v for name, v in found.items() if pattern.search(name))


class DevRelay:
    """One iroh-relay in dev mode on 127.0.0.1, its stderr in `<logs>/iroh-relay.log`."""

    def __init__(self, binary, logs, port=None, metrics_port=None):
        self.tag = "iroh-relay"
        self.binary = binary
        self.port = port or free_port()
        self.metrics_port = metrics_port or free_port()
        while self.metrics_port == self.port:
            self.metrics_port = free_port()
        self.url = "http://127.0.0.1:%d" % self.port
        self.metrics_url = "http://127.0.0.1:%d/metrics" % self.metrics_port
        self.config_path = os.path.join(logs, "iroh-relay.toml")
        self.log_path = os.path.join(logs, "iroh-relay.log")
        self.process = None
        self.version = None

    def start(self, deadline):
        """Starts the relay and waits (until `deadline`) for its health and its metrics."""
        with open(self.config_path, "w", encoding="utf-8") as f:
            f.write(
                "# A local relay for tests (scripts/iroh_relay_dev.py): plain HTTP, 127.0.0.1 only.\n"
                "enable_relay = true\n"
                "http_bind_addr = \"127.0.0.1:%d\"\n"
                "enable_quic_addr_discovery = false\n"
                "enable_metrics = true\n"
                "metrics_bind_addr = \"127.0.0.1:%d\"\n" % (self.port, self.metrics_port)
            )
        env = dict(os.environ)
        env.setdefault("RUST_LOG", "info")
        self.process = subprocess.Popen(
            [self.binary, "--dev", "--config-path", self.config_path],
            env=env, stdin=subprocess.DEVNULL, stdout=open(self.log_path, "wb"),
            stderr=subprocess.STDOUT, start_new_session=True,
        )
        last = None
        while time.time() < deadline:
            if self.process.poll() is not None:
                raise RelayError("iroh-relay exited (%s) at start:\n%s" % (self.process.returncode, self.tail()))
            try:
                with urllib.request.urlopen(self.url + "/healthz", timeout=2) as response:
                    health = json.loads(response.read().decode("utf-8") or "{}")
                if health.get("status") == "ok":
                    self.version = health.get("version")
                    self.metrics()
                    return self
            except (OSError, ValueError, urllib.error.URLError) as e:
                last = e
            time.sleep(0.25)
        raise RelayError("iroh-relay did not answer at %s/healthz (last error: %s):\n%s"
                         % (self.url, last, self.tail()))

    def metrics(self):
        """The relay's metrics now, {name: value}."""
        with urllib.request.urlopen(self.metrics_url, timeout=3) as response:
            return parse_metrics(response.read().decode("utf-8", errors="replace"))

    def relayed(self, since=None):
        """(bytes the clients sent it, bytes it passed on, client connections accepted), counted
        from `since` (an earlier `metrics()`) when given."""
        now = self.metrics()
        since = since or {}
        return tuple(metric(now, stem) - metric(since, stem) for stem in ("bytes_recv", "bytes_sent", "accepts"))

    def alive(self):
        return self.process is not None and self.process.poll() is None

    def tail(self, lines=30):
        try:
            with open(self.log_path, "r", encoding="utf-8", errors="replace") as f:
                return "".join(f.readlines()[-lines:])
        except OSError:
            return "(no output)"

    def stop(self):
        if not self.alive():
            return
        try:
            os.killpg(self.process.pid, signal.SIGTERM)
            self.process.wait(timeout=3)
        except (OSError, subprocess.TimeoutExpired):
            try:
                os.killpg(self.process.pid, signal.SIGKILL)
            except OSError:
                pass


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--relay-bin")
    parser.add_argument("--port", type=int, default=3340)
    parser.add_argument("--metrics-port", type=int, default=9095)
    parser.add_argument("--logs", default=None, help="where the config and the log go (default: a temp folder)")
    args = parser.parse_args()
    binary = find_binary(args.relay_bin)
    if not binary:
        print("no iroh-relay binary (--relay-bin / AZMEET_RELAY_BIN); build it once with:\n    %s" % BUILD_COMMAND)
        return 2
    logs = args.logs or os.path.join("/tmp", "iroh-relay-dev-%d" % os.getpid())
    os.makedirs(logs, exist_ok=True)
    relay = DevRelay(binary, logs, args.port, args.metrics_port)
    try:
        relay.start(time.time() + 20)
        print("iroh-relay %s at %s (metrics %s, log %s)" % (relay.version, relay.url, relay.metrics_url, relay.log_path))
        print("AzMeet: --relay %s --relay-only" % relay.url, flush=True)
        start = relay.metrics()
        while relay.alive():
            time.sleep(5)
            got, passed, accepts = relay.relayed(start)
            print("relayed: %.0f KiB in, %.0f KiB out, %d connections" % (got / 1024, passed / 1024, accepts), flush=True)
        print("iroh-relay exited:\n%s" % relay.tail())
        return 1
    except RelayError as e:
        print(e)
        return 1
    except KeyboardInterrupt:
        return 0
    finally:
        relay.stop()


if __name__ == "__main__":
    sys.exit(main())
