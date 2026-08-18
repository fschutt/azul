# Azul telemetry → local Grafana

A complete, runnable observability loop for an azul app: the demo binary does
real work, measures it with the engine's own `Probe` instrumentation, ships it
as **OTLP/HTTP JSON**, and a provisioned Grafana dashboard shows the result.

```
layout/examples/telemetry_grafana.rs        the app  (real XML/CSS parsing, real RSS reads)
        │  OTLP/HTTP JSON + Bearer token
        ▼
  otel-collector-contrib :4318              auth + re-encode  (= the ingest Worker's job)
        ├── Prometheus remote-write ──► VictoriaMetrics :8428   metrics
        └── OTLP logs ───────────────► Loki :3100                logs
                                              ▲
                                        Grafana :3000            provisioned datasources
                                                                 + dashboard
```

Nothing in the pipeline is simulated. Every histogram observation is a real
duration, every RSS gauge a real `/proc/self/statm` read (or
`K32GetProcessMemoryInfo` on Windows), and the log records come from the same
run.

---

## 1. Requirements

* Docker **or** rootless Podman (the helper script starts Podman's user socket
  and points `docker-compose` at it), plus a compose implementation:
  `docker compose`, `podman-compose`, or `docker-compose`.
* `curl` (the readiness probes use it).
* A Rust toolchain for the demo binary.
* Ports **3000**, **3100**, **4318** and **8428** free on `127.0.0.1`.
* ~700 MB of image downloads on first run.

## 2. Start the stack

```bash
cd layout/examples/telemetry-grafana
./run-stack.sh up
```

It waits until every service answers, then prints the URLs. Grafana is at
<http://127.0.0.1:3000> with anonymous admin access (demo convenience — see
[Security](#8-security-notes)), and the dashboard is provisioned as the home
dashboard, so the landing page already has panels.

## 3. Run the app

```bash
# from the repository root
AZ_TELEMETRY=metrics \
AZ_TELEMETRY_ENDPOINT=http://127.0.0.1:4318 \
AZ_TELEMETRY_TOKEN=azul-demo-token \
AZ_TELEMETRY_FLUSH_SECS=5 \
cargo run --release -p azul-layout --example telemetry_grafana \
    --features telemetry,probe -- --version 1.4.1 --iterations 150
```

Then run it again as a *newer* release, so the per-version comparison panels
have two things to compare — which is the whole point of the metric design:

```bash
AZ_TELEMETRY=metrics \
AZ_TELEMETRY_ENDPOINT=http://127.0.0.1:4318 \
AZ_TELEMETRY_TOKEN=azul-demo-token \
cargo run --release -p azul-layout --example telemetry_grafana \
    --features telemetry,probe -- --version 1.4.2 --iterations 150
```

Each run prints the resolved consent posture and a per-flush line:

```
azul telemetry demo
  version           1.4.2
  channel           beta
  consent tier      metrics (from Env)
  endpoint          http://127.0.0.1:4318/v1/metrics
  client_id         6f1c…-…-…
  queue             /home/you/.local/share/azul-telemetry-demo/telemetry/pending

startup 0.005s, first pass parsed 4003 nodes, rss 4 MiB
iter   10  0.0065s    4003 nodes  flush: queued_metrics=true queued_logs=1 uploaded=2 dropped=0 retained=0
iter   20  0.0112s    6003 nodes  flush: queued_metrics=true queued_logs=1 uploaded=2 dropped=0 retained=0
```

`uploaded=N` with `retained=0` means the collector accepted the payload (one
metrics ping plus one logs ping per flush).

### Useful flags

| flag | default | what it does |
|---|---|---|
| `--version <v>` | `1.4.2` | sets the `version` metric label |
| `--channel <c>` | `beta` | sets the `channel` metric label |
| `--iterations <n>` | `120` | workload iterations; `0` runs forever |
| `--flush-every <n>` | `10` | iterations between flushes |
| `--pace-ms <n>` | `200` | idle time between iterations; `0` runs flat out |
| `--panic-at <n>` | never | genuinely panics at iteration `n` |
| `--remember` | off | writes the consent choice to `{config_dir}/azul-telemetry-demo/telemetry.json`, so `client_id` is stable across runs |

`--pace-ms` exists because a run that finishes in 300 ms lands every sample in
one scrape interval, and the time-series panels then show a single spike
instead of a curve. The sleep is idle time between iterations — the durations
reported to the histograms exclude it. The defaults give a ~25 s run.

## 4. What to look at in Grafana

Open <http://127.0.0.1:3000/d/azul-telemetry>. Set the time range to
**Last 15 minutes** and the refresh to **10s**.

| Panel | What it answers | Metric behind it |
|---|---|---|
| **Sessions / Panics / Versions seen / Current RSS** | at-a-glance state | `app_sessions_started_total`, `app_panics_total`, `app_rss_bytes` |
| **Startup time by version (p50/p95)** | "is startup slower after the update?" | `app_startup_seconds_bucket` |
| **Startup RSS by version (p50)** | "did 1.4.2 raise RAM right after startup?" — the rollout gate | `app_startup_rss_bytes_bucket` |
| **Adoption — sessions by version** | "which users updated?" | `app_sessions_started_total` |
| **Panics per session by version** | release health; do not raise `rollout_percent` until the new version is at or below the old one | `app_panics_total` ÷ `app_sessions_started_total` |
| **Frame work by scope** | "did the update make rendering do more work?" | `app_frame_relayout_scope_total{scope}` |
| **Phase duration p95** | per-phase timings straight from `Probe::span` | `app_phase_seconds_bucket{phase}` |
| **Document size worked on** | an *app-defined* histogram through the public API | `demo_document_nodes_bucket` |
| **Log records** | the structured logs shipped on the same connection | Loki, `{service_name="azul-telemetry-demo"}` |

Two things worth doing by hand:

1. **Produce a real panic and watch the offline path work.**

   ```bash
   AZ_TELEMETRY=metrics AZ_TELEMETRY_ENDPOINT=http://127.0.0.1:4318 \
   AZ_TELEMETRY_TOKEN=azul-demo-token \
   cargo run --release -p azul-layout --example telemetry_grafana \
       --features telemetry,probe -- --version 1.4.2 --panic-at 25
   ```

   The panic hook counts the panic, buffers the message, and writes the queue
   to **disk** — it never uploads from a dying process. The panic count and
   the `FATAL` log record appear in Grafana only after you start the demo
   again, because the *next* launch drains the queue. That delay is the
   design, not a bug.

2. **Watch it survive the backend being down.** Stop the stack
   (`./run-stack.sh down`), run the demo — the per-flush line shows
   `retained=N` and the pending directory fills up — then bring the stack back
   and run it again. The backlog uploads.

## 5. Turning it off

The demo sends nothing unless you ask it to. With no `AZ_TELEMETRY` set, the
consent tier is `off` and the run is a pure benchmark.

```bash
AZ_TELEMETRY=off cargo run --release -p azul-layout --example telemetry_grafana \
    --features telemetry,probe
```

The tiers are `off` → `crashes` → `metrics` → `full`, and they can also be set
in files (most specific wins):

1. `AZ_TELEMETRY`
2. `.azul/telemetryconfig.json` next to the executable — the packager/admin
   pin; `{"tier":"off"}` here also suppresses the consent dialog
3. `{config_dir}/azul-telemetry-demo/telemetry.json`
4. `{config_dir}/azul/telemetry.json`

`AZ_TELEMETRY_ENDPOINT` and `AZ_TELEMETRY_TOKEN` set the destination; without
an endpoint the client prints a warning and uploads nothing.

## 6. Poking at it directly

```bash
# Every azul metric VictoriaMetrics currently knows about
curl -s 'http://127.0.0.1:8428/api/v1/label/__name__/values' | tr ',' '\n' | grep app_

# The rollout-gate query, straight from the research doc
curl -s --data-urlencode \
  'query=sum by (version) (increase(app_panics_total[1h])) / sum by (version) (increase(app_sessions_started_total[1h]))' \
  http://127.0.0.1:8428/api/v1/query

# Send a hand-made OTLP payload (should print {"partialSuccess":{}})
curl -s -X POST http://127.0.0.1:4318/v1/metrics \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer azul-demo-token' \
  -d '{"resourceMetrics":[{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":"curl"}}]},"scopeMetrics":[{"metrics":[{"name":"hand_made_total","sum":{"aggregationTemporality":2,"isMonotonic":true,"dataPoints":[{"asInt":"1","timeUnixNano":"'"$(date +%s)"'000000000"}]}}]}]}]}'

# What the client would send right now, without sending it, is also available
# in-process via telemetry::preview_payloads() — the consent preview *is* the
# payload.
```

## 7. Design notes and known limitations

**Cumulative counters + no per-user label = the ingest proxy has work to do.**
The metric design deliberately allows only four labels (`version`, `channel`,
`os`, `arch`), so two clients running the same build write to the *same*
series. Each client sends its own cumulative totals, so at the storage layer
those samples interleave and look like a counter that jumps around and resets.
`rate()`/`increase()` absorb resets, which is why the dashboard uses them, but
with many concurrent clients the arithmetic stops being exact. In the deployed
design that is the ingest proxy's job: it converts per-client cumulative
payloads into server-side aggregates before storage. This demo runs clients
sequentially, so the numbers here are exact — do not extrapolate the *storage*
choice to a fleet without that proxy.

**`increase(...[$__range])` on the startup panels.** `app_startup_seconds` gets
exactly one observation per app run. A sliding `rate(...[5m])` window is empty
whenever no process started inside it, and `histogram_quantile` of an all-zero
rate is `NaN` — the panel would read "No data" most of the time even though the
data is there. `increase()` over the picker's whole range sums across runs and
across the resets between them.

**Not implemented in this branch** (they are later phases of the plan): the
crash-reporter child process and minidumps, the RefAny state snapshot at
consent tier `full`, breadcrumbs from the undo journal, the updater, and gzip
request bodies. The consent *tiers* exist and gate behaviour; the consent
*dialogs* do not.

## 8. Security notes

This stack is a **demo**, not a deployment:

* Grafana runs with anonymous admin and no login form.
* The bearer token is the literal string `azul-demo-token`, committed here.
* Everything binds to `127.0.0.1`, and there is no TLS anywhere.

For anything real, the collector position is where authentication, per-tier
policy enforcement and scrubbing belong, and Grafana belongs behind an
identity proxy.

## 9. Troubleshooting

| Symptom | Cause / fix |
|---|---|
| `dropped=1` right after a flush | The collector returned a permanent 4xx — almost always a wrong `AZ_TELEMETRY_TOKEN` (401). The client treats that as poison and drops the ping rather than retrying forever. |
| `retained=N`, `err=…` | The backend is unreachable or returned 5xx. The pings stay on disk and go out on the next flush. This is the offline path working. |
| Panels say "No data" | Check the run printed `uploaded=`, then confirm the metric exists: `curl -s 'http://127.0.0.1:8428/api/v1/label/__name__/values' \| grep app_`. Remember the dashboard's default range is the last 30 minutes. |
| Logs panel empty but metrics work | Loki takes longer to become ready than the other services. `./run-stack.sh status` shows `200` when it is up. |
| `RSS metrics are unavailable` | The demo was built without `--features probe`; the platform RSS readers are compiled in by that feature. |
| Podman: compose cannot reach the daemon | `systemctl --user start podman.socket`, then `export DOCKER_HOST=unix://$XDG_RUNTIME_DIR/podman/podman.sock`. `run-stack.sh` does this for you. |

## 10. Cleaning up

```bash
./run-stack.sh down          # containers + the metrics volume
rm -rf ~/.local/share/azul-telemetry-demo    # the client's pending-ping queue
rm -f  ~/.config/azul-telemetry-demo/telemetry.json
```

## Query cookbook — finding crashes, slowness, memory

All Loki queries assume the stream selector `{service_name="<your app id>"}`
(this demo: `azul-telemetry-demo`). `client_id` and every `crash.*` / `sys.*`
/ `slow.*` field ride as structured metadata — filter them with `| key="…"`,
NOT with `|= "…"` body matching.

### Crashes

```logql
# every crash record (red, Severity::Error, event.kind="crash")
{service_name="azul-telemetry-demo"} | event_kind="crash"

# …the same by body prefix, when you just want to eyeball it
{service_name="azul-telemetry-demo"} |= "crash: "

# crash COUNT over time — fleet-accurate (one record per crash per client;
# the app_panics_total METRIC undercounts multi-client fleets until the
# ingest proxy aggregates per-client cumulative series)
sum(count_over_time({service_name="azul-telemetry-demo"} | event_kind="crash" [1h]))

# one user's whole story, oldest first (iterations, slow warns, crash, relaunch)
{service_name="azul-telemetry-demo"} | client_id="sim-user-016"
```

Each crash record carries: `crash.message` (the `expect` reason),
`crash.location` (`file:line`, paths stripped), `crash.scope` (the live
probe-span path, e.g. `demo.autosave`), `crash.backtrace` (paths stripped),
`app.document_size`, and the full `sys.*` snapshot.

### Slowness

```logql
# slow frames / timer ticks / spans, with the exact name and ms
{service_name="azul-telemetry-demo"} |~ "slow (total|timer|span)"

# only slow SPANS, i.e. WHICH phase was slow
{service_name="azul-telemetry-demo"} | event_kind="slow_span"
```

```promql
# frame duration p95 in ms, by scope (the smoothness number)
histogram_quantile(0.95, sum by (le, scope)
  (rate(app_frame_seconds_bucket[$__rate_interval])))

# timer-tick p95 — the clock animations ride
histogram_quantile(0.95, sum by (le, version)
  (rate(app_timer_frame_seconds_bucket[$__rate_interval])))

# slow-frame rate per minute, by scope (incl. cb:<callback> names)
sum by (scope) (rate(app_slow_frames_total[$__rate_interval])) * 60

# one NAMED app callback across versions ("my_button_click got slower in 1.5.0")
histogram_quantile(0.95, sum by (le, version)
  (rate(app_phase_seconds_bucket{phase="cb:demo_button_click"}[$__rate_interval])))
```

The FIRST slow event of a session attaches `sys.cpu_model`, `sys.cpu_count`,
`sys.ram_total_bytes`, `sys.os`, `sys.windowing`, `sys.gpu` — hardware
context ships only when something was actually slow.

### Memory vs document size

```promql
# is high RSS explained by a big document? (flat = yes)
max by (version) (app_document_rss_bytes_per_unit)

# RSS at the checkpoints the app records
max by (version, checkpoint) (app_rss_bytes)

# the raw pair: RSS delta of the open vs the document size itself
max by (version) (app_document_rss_delta_bytes)
max by (version) (app_document_size)
```

### The rollout gate

```promql
# adoption
sum by (version) (app_sessions_started_total)

# startup p50 by version (regression check after an update)
histogram_quantile(0.50, sum by (le, version)
  (increase(app_startup_seconds_bucket[$__range])))

# updater observing itself
sum by (result) (increase(app_update_check_total[$__range]))
```

### Slow (phased) rollout

Releases stagger by default. A manifest with a `release_date` and no
`slow` key gets the built-in ladder — day 1 → 10 %, day 2 → 30 %,
day 3 → 50 %, day 4 → 100 % — so there is a cooldown to inspect this
dashboard per version before the fleet moves. Explicit stages (percent →
datetime; unix seconds or `YYYY-MM-DD[THH:MM:SSZ]`) override the ladder,
and `"slow": "off"` releases to everyone at once:

```json
{ "latest": { "version": "1.6.0",
              "download_url": "https://…/app-1.6.0.bin",
              "changelog_md": "https://…/CHANGELOG.md",
              "digest": "",
              "release_date": "2026-08-17",
              "slow": { "10": "2026-08-18", "50": "2026-08-19",
                        "100": "2026-08-20" } } }
```

Each client draws a persistent cohort bucket (0–99, `update-state.json`;
`AZ_UPDATE_BUCKET` overrides for drills) and updates once its bucket
falls under the currently-open percent. Package-managed installs
(notify-only) do not even see the "please update" hint until the rollout
reaches 100 %. Gated clients record `staggered`:

```promql
# how far the rollout has actually reached, over time
sum by (result) (increase(app_update_check_total{result=~"available|staggered"}[$__range]))
```

Drill it locally (three manifests, several cohorts each):

```bash
cargo run --release -p azul-layout --example telemetry_grafana \
  --features telemetry,probe,updater -- \
  --rollout-drill http://127.0.0.1:8913/manifest.json --version 1.5.0 --iterations 1
```

### Callback span names (`cb:*`)

`Probe::span_for_fn` names callback spans via `dladdr`:
`cb:my_button_click` where the symbol is exported (`-rdynamic`, dylib
apps). Statically linked binaries keep their own functions out of
`.dynsym`, so the span falls back to the module-relative offset
(`cb:+0x588f0`) — stable across runs of the SAME binary, so distinct
callbacks stay distinguishable and per-version comparisons still work;
`addr2line -e ./app 0x588f0` maps an offset back to a name when needed.
