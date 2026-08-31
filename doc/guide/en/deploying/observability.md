---
slug: deploying/observability
title: Observability
language: en
canonical_slug: deploying/observability
audience: external
maturity: wip
guide_order: 233
topic_only: false
short_desc: Consent tiers, OTLP metrics and logs, crash reports, and the collector behind them
prerequisites: [hello-world, deploying/signing-updates]
tracked_files:
  - layout/src/telemetry/mod.rs
  - layout/src/telemetry/config.rs
  - layout/src/telemetry/metrics.rs
  - layout/src/telemetry/queue.rs
  - layout/src/telemetry/crash_mail.rs
  - layout/src/probe.rs
  - layout/src/dialogs/telemetry_consent.rs
  - dll/src/desktop/app.rs
last_generated_rev: c7631154b571a2467f2fee67c4e67e72cb29be83
generated_at: 2026-08-20T00:00:00Z
default-search-keys:
  - CallbackInfo
  - SysDialogType
  - UpdateSettings
  - StringPairVec
  - AppConfig
---

# Observability

A shipped desktop app answers "did 1.4.2 regress startup RAM?" from support
tickets, badly. Azul ships an OTLP client for the question: an in-process
metric registry, a structured log buffer, a disk queue that survives an
offline backend, and an uploader.

Nothing leaves the machine unless **two** keys turn:

1. you compile the `telemetry` feature in and configure an endpoint, and
2. the user selects a consent tier at or above the data being sent.

The default tier is `off`. Linking the feature in collects nothing.

## Turning it on

```toml
[dependencies]
azul = { version = "$VERSION", features = ["telemetry"] }
```

The app's identity comes from the `UpdateSettings` block that the updater
already reads — `app_name` becomes the OTLP service name and keys the queue
directory, `current_version` becomes the `version` label:

```rust,ignore
let mut app_config = AppConfig::create();
app_config.updates.app_name = "myapp".into();
app_config.updates.current_version = env!("CARGO_PKG_VERSION").into();
// Where SysDialogType::ReportProblem and manual crash reports are mailed.
// Unset, the report dialog saves to disk instead.
app_config.report_problem = Some("support@myapp.example".into()).into();
let app = App::create(RefAny::new(data), app_config);
```

`App::create` then does the wiring itself: `telemetry::init`, the panic hook,
one `app_sessions_started_total` tick, and the background uploader thread.
Recording is lock-light and safe on the UI thread; uploads never happen on
it.

The destination is environment or config, not code — a packager can point a
build at their own collector without a rebuild:

```bash
# Local debugging loop (the Grafana stack from layout/examples/telemetry-grafana):
AZ_OBSERVE=1 ./myapp

# Production / custom collector - the granular variables:
AZ_TELEMETRY_ENDPOINT=https://otlp.example.com \
AZ_TELEMETRY_TOKEN=<ingest token> \
AZ_TELEMETRY=metrics ./myapp
```

With no endpoint configured the client warns once and uploads nothing, rather
than buffering forever.

## Consent tiers

| Tier | Allows |
| --- | --- |
| `off` | nothing. The default, always |
| `crashes` | crash and panic reports |
| `metrics` | the above plus anonymous metrics and logs |
| `full` | the above plus serialized app state on a crash |

Four layers set the tier, most specific first:

1. `AZ_TELEMETRY` — CI, corporate lockdown, tests
2. `.azul/telemetryconfig.json` beside the executable — the packager/admin
   pin. `{"tier": "off"}` here also suppresses the consent dialog
3. `{config_dir}/{app-id}/telemetry.json` — this app's user choice
4. `{config_dir}/azul/telemetry.json` — "remember for all azul apps"

They are human-readable JSON deliberately: the config is part of the
transparency story.

### Asking the user

```rust,ignore
info.invoke_system_dialog(SysDialogType::TelemetryConsent);
```

The dialog lists **every instrument the app can record** — each engine metric
with a plain-language sentence, plus any app-defined ones — with a checkbox
per metric above the four signal switches (crashes, logs, metrics, app state
on crash). Saving applies immediately at runtime and persists; the "remember
for all azul apps" box writes the machine-wide
`{config_dir}/azul/config.json` channel default instead of this app's
override.

A tier downgrade retires the `client_id` through a deletion-request ping, so
the backend learns which install asked to be forgotten.

## What the engine records

Metrics carry exactly four labels — `version`, `channel`, `os`, `arch` —
enforced by the label set being a struct rather than a map. The random
`client_id` that makes "crash-free *users*" and adoption dedup possible rides
on **log records only**, never on a metric.

| Metric | Kind | Answers |
| --- | --- | --- |
| `app_sessions_started_total` | counter | the denominator of every release-health ratio |
| `app_crashes_total`, `app_panics_total` | counter | release health, per version |
| `app_startup_seconds` | histogram | "is startup slower since the update?" |
| `app_startup_rss_bytes` | histogram | the rollout gate: did RAM regress? |
| `app_rss_bytes`, `app_heap_bytes` | gauge | live memory |
| `app_frame_seconds{scope}` | histogram | frame cost by how much work it caused |
| `app_timer_frame_seconds` | histogram | the clock animations ride |
| `app_frame_relayout_scope_total{scope}` | counter | `repaint` vs `relayout` vs `regenerate_dom` mix |
| `app_slow_frames_total{scope}` | counter | frames past the slow threshold (32 ms default) |
| `app_phase_seconds{phase}` | histogram | per-phase timings from `Probe::span` |
| `app_update_check_total{result}`, `app_update_apply_total{result}` | counter | updater outcomes |

Every slow frame also emits a `WARN` log record naming the scope, the
duration and the document size, and the session's first one carries a
one-shot system-info snapshot.

`app_phase_seconds` is armed automatically: `telemetry::init` calls
`enable_probe_bridge()` whenever the resolved consent tier ships metrics, so
a consenting run fills the phase histogram with no further switches. (It
used to require a separate `enable_probe_bridge()` call - the histogram was
silently empty otherwise, and an empty histogram is indistinguishable from a
fast one.) `AZ_PROFILE` is unrelated to telemetry: it arms the same recorder
for LOCAL consumers - agent/e2e debugging, `AZ_PROFILE=cpu` dumps,
cross-frame phase diffs - and unknown tokens now warn with the valid list
instead of parsing to nothing.

## Your own metrics

From any callback, in any language binding:

```rust,ignore
info.record_counter("documents_opened_total".into(), 1, StringPairVec::from_vec(vec![]));
info.record_histogram("export_seconds".into(), elapsed, labels);
info.record_gauge("open_tabs".into(), tabs as f64, labels);
```

Labels are sanitized and capped — six keys, 64-character values — and every
distinct combination counts against the global series ceiling. Names must be
fixed strings, never user-derived: a metric name built from a filename is an
unbounded series. All three are no-ops unless telemetry is compiled in and
consented to, so they are safe to leave in the code path.

## Crashes

The panic hook counts the panic, buffers the message, and writes the queue
**to disk**. Release builds are `panic = "abort"`: the process dies the moment
the hooks return, so nothing can be deferred to a background thread. With an
OTLP endpoint configured the hook therefore ships the queued crash record
**synchronously** (a 3-second budget; whatever fails stays queued for the next
launch). Without an endpoint it re-spawns the app as the crash-reporter dialog
(below). With telemetry off — below the `crashes` tier — the plain "unexpected
fatal error" message box is the only surface (`AppConfig.enable_visual_panic_hook`,
on by default; suppressed under `AZ_E2E`), and it yields whenever telemetry
owns the crash, so a user sees exactly one crash surface.

Each crash record carries `crash.message`, `crash.location` (`file:line`),
`crash.scope` (the live probe-span path, so you know what the app was doing),
`crash.backtrace`, the document size and the `sys.*` snapshot. Paths are
scrubbed: `$HOME` becomes `~`, and rustc and registry paths collapse.

When a crashed process re-spawns itself with `AZ_CRASH_DUMP=<dump.json>`,
that invocation *is* the crash reporter: azul shows the dump in a CPU-rendered
dialog instead of starting the app. This happens on every crash-tier panic
without an endpoint — no crash contact is needed to *see* the report; only
its **Send** needs one. `AppConfig.report_problem` (the support mailbox
`SysDialogType::ReportProblem` mails to) arms that contact automatically when
the `crash-mail` transport is compiled in. A dump still queued on a later
launch — the reporter was closed, or crashed with the app — re-opens the
reporter alongside the app when it can send.

### Reports the user starts

`SysDialogType::ReportProblem` is the path that does not wait for a crash: a
message box with an optional screenshot of the current window and an optional
system-info block. The screenshot is captured in-process at invoke time, so it
shows the situation the user is complaining about. Nothing leaves the machine
before they press Send, and the report goes to `AppConfig.report_problem` — or
to disk when that is unset.

### Without a backend

For a deployment with no collector at all — tier `crashes`, metrics off, a
support mailbox — the `crash-mail` feature drains the dumps into one email
over plain SMTP, the dump as a JSON attachment and the user's message as the
body. It is manual by design: the app calls it from its own "the app crashed
last time, send a report?" dialog on the next launch.

## Where the data goes

Payloads are OTLP/HTTP **JSON** to `<endpoint>/v1/metrics` and
`<endpoint>/v1/logs` with a bearer token. A flush reports what happened to
each ping:

- `uploaded` — the collector took it.
- `retained` — unreachable or 5xx. The ping stays on disk and goes out next
  flush. This is the offline path working.
- `dropped` — a permanent 4xx, almost always a wrong token. Retrying a
  rejected payload forever is a worse failure than losing it.

The collector position is where authentication, per-tier policy and scrubbing
belong; Grafana belongs behind an identity proxy.

**Plan for an aggregating ingest proxy.** Four labels and no per-user label
means two clients on the same build write to the *same* series, each sending
its own cumulative totals — at the storage layer those interleave and look
like a counter that resets. `rate()` and `increase()` absorb resets, which is
why the queries below use them, but with a real fleet the arithmetic stops
being exact unless the proxy converts per-client cumulative payloads into
server-side aggregates first.

### A stack you can run

`layout/examples/telemetry-grafana/` is a complete loop — collector →
VictoriaMetrics + Loki → a provisioned Grafana dashboard — with a demo app
doing real work:

```bash
cd layout/examples/telemetry-grafana && ./run-stack.sh up
```

Its README covers the dashboard panels, the offline and panic paths, and a
query cookbook. Nothing in it is simulated; every histogram observation is a
real duration.

## Queries that gate a rollout

```promql
# panics per session, by version — do not raise rollout_percent until the new
# version is at or below the old one
sum by (version) (increase(app_panics_total[1h]))
  / sum by (version) (increase(app_sessions_started_total[1h]))

# startup p95 by version
histogram_quantile(0.95,
  sum by (version, le) (increase(app_startup_seconds_bucket[$__range])))
```

```logql
# every crash record, and one install's whole story
{service_name="myapp"} | event_kind="crash"
{service_name="myapp"} | client_id="…"
```

`increase(…[$__range])` rather than a sliding `rate()`: startup is observed
once per run, so a five-minute window is empty whenever no process started in
it, and a quantile over an all-zero rate is `NaN`.

## Environment variables

| Variable | Effect |
| --- | --- |
| `AZ_OBSERVE` | ONE-VAR local loop: `1`/`local` = metrics tier + `http://127.0.0.1:4318` + the local stack's token + 5s flushes; `<url>` targets another collector. Granular vars below override pieces. |
| `AZ_TELEMETRY` | consent tier: `off`, `crashes`, `metrics`, `full` |
| `AZ_TELEMETRY_ENDPOINT` | OTLP base URL; signal paths are appended |
| `AZ_TELEMETRY_TOKEN` | bearer token for the ingest endpoint |
| `AZ_TELEMETRY_FLUSH_SECS` | uploader flush interval |
| `AZ_TELEMETRY_CHANNEL` | the `channel` label (default `default`) |
| `AZ_TELEMETRY_CLIENT_ID` | pins the client id, for test determinism |
| `AZ_CRASH_DUMP` | set on the re-spawn; makes this run the crash reporter |
| `AZ_PROFILE` | also enables `Probe` recording, hence `app_phase_seconds` |

An unknown `AZ_TELEMETRY` value is rejected rather than guessed, so a typo is
visible instead of silently collecting or silently not.

## Cross-references

- [Signed Updates](signing-updates.md): the `UpdateSettings` block this page
  reads its identity from, and the release channels the metrics compare.
- [Profiling](../debugging/profiling.md): the same `Probe` spans, read
  locally instead of shipped.
- [Debugging](../debugging.md): overlays and structured logging on the
  developer's own machine.
