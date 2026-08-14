//! Live telemetry demo — real work, real OTLP/HTTP JSON, real Grafana panels.
//!
//! This binary does actual azul work (XML + CSS parsing over a document that
//! grows and shrinks), measures it with the same `Probe` instrumentation the
//! engine uses everywhere else, and ships the result to an OTLP endpoint as
//! spec-encoded JSON. Nothing here is simulated: every histogram observation
//! is a real duration, every RSS gauge is a real `/proc/self/statm` read.
//!
//! The companion stack (OpenTelemetry Collector -> VictoriaMetrics + Loki ->
//! Grafana) lives next door in `telemetry-grafana/`. Start it first:
//!
//! ```text
//! cd layout/examples/telemetry-grafana && ./run-stack.sh up
//! ```
//!
//! then run this:
//!
//! ```text
//! AZ_TELEMETRY=metrics \
//! AZ_TELEMETRY_ENDPOINT=http://127.0.0.1:4318 \
//! AZ_TELEMETRY_TOKEN=azul-demo-token \
//! cargo run --release -p azul-layout --example telemetry_grafana \
//!     --features telemetry,probe
//! ```
//!
//! Run it twice with different `--version` values to make the release
//! comparison panels ("did 1.4.2 regress startup RAM vs 1.4.1?") light up —
//! that comparison is the entire point of the metric design.
//!
//! Flags (all optional):
//!
//! | flag | default | meaning |
//! |---|---|---|
//! | `--version <v>` | `1.4.2` | the `version` metric label |
//! | `--channel <c>` | `beta` | the `channel` metric label |
//! | `--iterations <n>` | `120` | workload iterations, `0` = run forever |
//! | `--flush-every <n>` | `10` | iterations between flushes |
//! | `--pace-ms <n>` | `200` | idle time between iterations (`0` = flat out) |
//! | `--panic-at <n>` | never | genuinely panic at iteration `n` |
//! | `--remember` | off | persist the consent choice, so `client_id` is stable |
//!
//! With no `AZ_TELEMETRY` set the tier is `off` and the run is a pure
//! benchmark that sends nothing — which is the point of the default.

use std::time::Instant;

use azul_layout::telemetry::{self, AppMeta, Severity};

/// One paragraph of the synthetic document.
const PARAGRAPH: &str = "<p class=\"body\">The quick brown fox jumps over the lazy dog, \
                         then pauses to consider the kerning of the ligature it just \
                         stepped on.</p>";

const STYLESHEET: &str = "
    .body { font-size: 12px; line-height: 1.4; color: #222; margin: 0 0 8px 0; }
    .heading { font-size: 24px; font-weight: bold; margin: 16px 0; }
    .frame { display: flex; flex-direction: column; padding: 24px; width: 800px; }
    .frame > .body:hover { color: #06c; }
";

struct Args {
    version: String,
    channel: String,
    iterations: u64,
    flush_every: u64,
    pace_ms: u64,
    panic_at: Option<u64>,
    remember: bool,
}

impl Args {
    fn parse() -> Self {
        let mut args = Self {
            version: std::env::var("AZ_DEMO_VERSION").unwrap_or_else(|_| "1.4.2".to_owned()),
            channel: std::env::var("AZ_DEMO_CHANNEL").unwrap_or_else(|_| "beta".to_owned()),
            iterations: 120,
            flush_every: 10,
            pace_ms: 200,
            panic_at: None,
            remember: false,
        };
        let mut argv = std::env::args().skip(1);
        while let Some(flag) = argv.next() {
            let mut value = || argv.next().unwrap_or_default();
            match flag.as_str() {
                "--version" => args.version = value(),
                "--channel" => args.channel = value(),
                "--iterations" => args.iterations = value().parse().unwrap_or(120),
                "--flush-every" => args.flush_every = value().parse().unwrap_or(10).max(1),
                "--pace-ms" => args.pace_ms = value().parse().unwrap_or(200),
                "--panic-at" => args.panic_at = value().parse().ok(),
                "--remember" => args.remember = true,
                "--help" | "-h" => {
                    println!(
                        "usage: telemetry_grafana [--version V] [--channel C] \
                         [--iterations N] [--flush-every N] [--pace-ms N] [--panic-at N] \
                         [--remember]"
                    );
                    std::process::exit(0);
                }
                other => eprintln!("ignoring unknown flag {other:?}"),
            }
        }
        args
    }
}

/// Current RSS in bytes, or `None` when the `probe` feature is off.
///
/// `probe` is what compiles the platform RSS readers in at all; without it
/// there is nothing to read and reporting a zero would be worse than
/// reporting nothing.
fn current_rss() -> Option<u64> {
    #[cfg(feature = "probe")]
    {
        let (rss, _virtual) = azul_layout::probe::current_rss_bytes();
        (rss != 0).then_some(rss)
    }
    #[cfg(not(feature = "probe"))]
    {
        None
    }
}

/// Builds a document of `paragraphs` paragraphs.
fn build_document(paragraphs: usize) -> String {
    let mut doc = String::with_capacity(paragraphs * PARAGRAPH.len() + 256);
    doc.push_str("<div class=\"frame\"><h1 class=\"heading\">Telemetry demo</h1>");
    for _ in 0..paragraphs {
        doc.push_str(PARAGRAPH);
    }
    doc.push_str("</div>");
    doc
}

/// Runs one iteration of real parsing work and returns the node count.
fn run_workload(document: &str) -> usize {
    // These spans are ordinary `Probe` spans — the same mechanism the layout
    // solver uses. `telemetry::drain_probe_events()` turns them into
    // `app_phase_seconds{phase}` observations, so the Grafana panel shows
    // genuine per-phase timings rather than anything this example invented.
    let node_count = {
        let _span = azul_layout::probe::Probe::span("demo.parse_xml");
        azul_layout::xml::parse_xml_string(document).map_or(0, |nodes| count_nodes(&nodes))
    };

    {
        let _span = azul_layout::probe::Probe::span("demo.parse_css");
        let (css, _warnings) = azul_css::parser2::new_from_str(STYLESHEET);
        // Keep the parsed stylesheet observably alive so the work is not
        // optimised away.
        std::hint::black_box(&css);
    }

    node_count
}

fn count_nodes(nodes: &[azul_core::xml::XmlNodeChild]) -> usize {
    let mut count = nodes.len();
    for node in nodes {
        if let azul_core::xml::XmlNodeChild::Element(element) = node {
            count += count_nodes(element.children.as_ref());
        }
    }
    count
}

fn main() {
    let process_start = Instant::now();
    let args = Args::parse();

    // ── Consent ─────────────────────────────────────────────────────────
    // Reads AZ_TELEMETRY plus the layered config files. Tier is `off` unless
    // something explicitly opted in, and nothing below sends anything at
    // tier off.
    let config = telemetry::init(
        "azul-telemetry-demo",
        AppMeta::new(&args.version, &args.channel),
    );
    telemetry::install_panic_hook();
    // Opt into the Probe -> metrics bridge. Without this, `Probe` stays
    // dormant unless AZ_PROFILE is set and the per-phase histogram would be
    // empty — which reads exactly like "everything is fast".
    telemetry::enable_probe_bridge();
    // This demo ships INFO records so the Loki panel has something in it; the
    // library default is WARN and above.
    telemetry::set_min_log_severity(Severity::Info);

    // "Remember this choice": writes {config_dir}/azul-telemetry-demo/
    // telemetry.json, so the client_id survives across runs — which is what
    // makes crash-free-*users* and adoption dedup possible at all. Off by
    // default because a demo should not quietly leave consent state behind.
    if args.remember {
        match telemetry::config::save_user_choice(
            "azul-telemetry-demo",
            telemetry::ConsentScope::ThisApp,
        ) {
            Ok(path) => println!("  remembered choice in {}", path.display()),
            Err(e) => println!("  could not remember choice: {e}"),
        }
    }

    println!("azul telemetry demo");
    println!("  version           {}", args.version);
    println!("  channel           {}", args.channel);
    println!("  consent tier      {} (from {:?})", config.tier.as_str(), config.tier_source);
    println!(
        "  endpoint          {}",
        config
            .signal_url("metrics")
            .unwrap_or_else(|| "<none — nothing will be uploaded>".to_owned())
    );
    println!(
        "  client_id         {}",
        config.client_id.as_deref().unwrap_or("<none>")
    );
    println!(
        "  queue             {}",
        telemetry::ping_queue().map_or_else(
            || "<no data dir>".to_owned(),
            |q| q.dir().display().to_string()
        )
    );
    if config.pinned_off {
        println!("  NOTE: a pin (env or .azul/telemetryconfig.json) forces tier off.");
    }
    if current_rss().is_none() {
        println!("  NOTE: built without --features probe, so RSS metrics are unavailable.");
    }
    if !telemetry::probe_bridge_enabled() {
        println!(
            "  NOTE: Probe is not recording (build with --features probe), so the \
             app_phase_seconds panel will stay empty."
        );
    }
    println!();

    // ── Session + startup ───────────────────────────────────────────────
    telemetry::record_session_start();

    // Big enough that a parse takes single-digit milliseconds — small enough
    // that 120 iterations finish in a couple of seconds. Sub-microsecond work
    // would pile every observation into the first histogram bucket and the
    // latency panels would be a flat line at zero.
    let mut paragraphs = 2_000_usize;
    let mut document = build_document(paragraphs);
    let first_nodes = run_workload(&document);
    let startup_secs = process_start.elapsed().as_secs_f64();
    telemetry::record_startup(startup_secs, current_rss().unwrap_or(0));
    println!(
        "startup {startup_secs:.3}s, first pass parsed {first_nodes} nodes, \
         rss {} MiB",
        current_rss().unwrap_or(0) / (1024 * 1024)
    );

    // ── Workload loop ───────────────────────────────────────────────────
    let mut iteration: u64 = 0;
    let mut slowest = 0.0_f64;
    loop {
        iteration += 1;
        if args.iterations != 0 && iteration > args.iterations {
            break;
        }
        if args.panic_at == Some(iteration) {
            // A genuine panic. The hook counts it, buffers the message, and
            // writes the queue to disk; the *next* run uploads it. That
            // "never upload from a dying process" ordering is the design.
            panic!("--panic-at {iteration}: deliberate demo panic");
        }

        // Resize the document every 12 iterations. The size change is what
        // makes the frame "relayout" rather than "repaint" — the scope label
        // reflects work that actually happened, it is not a dice roll.
        let resized = iteration % 12 == 0;
        if resized {
            paragraphs = if paragraphs >= 6_000 { 2_000 } else { paragraphs + 1_000 };
            document = build_document(paragraphs);
        }

        // Pacing, not fake work: a real app does periodic work with idle time
        // between, and a run that finishes in 300 ms puts every sample in one
        // scrape interval — the time-series panels would show a spike instead
        // of a curve. The measurement below excludes this sleep.
        if args.pace_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(args.pace_ms));
        }

        let started = Instant::now();
        let nodes = run_workload(&document);
        let elapsed = started.elapsed().as_secs_f64();
        slowest = slowest.max(elapsed);

        telemetry::record_relayout_scope(if resized { "relayout" } else { "repaint" });
        telemetry::observe("demo_document_nodes", nodes as f64);
        telemetry::count("demo_iterations_total", 1);
        if let Some(rss) = current_rss() {
            telemetry::record_memory(rss, 0);
        }
        // Probe spans -> app_phase_seconds{phase}. This is the bridge the
        // plan calls for: the profiler's buffer, drained into metrics.
        telemetry::drain_probe_events();

        // Real, load-bearing log lines: the WARN is the one you would want to
        // find in Loki sitting next to a latency spike.
        if elapsed > 0.020 {
            telemetry::log(
                Severity::Warn,
                format!("slow iteration {iteration}: {elapsed:.4}s for {nodes} nodes"),
            );
        }

        if iteration % args.flush_every == 0 {
            telemetry::log(
                Severity::Info,
                format!(
                    "iteration {iteration}: {nodes} nodes in {elapsed:.4}s \
                     ({paragraphs} paragraphs)"
                ),
            );
            let outcome = telemetry::flush();
            println!(
                "iter {iteration:>4}  {elapsed:.4}s  {nodes:>6} nodes  \
                 flush: queued_metrics={} queued_logs={} uploaded={} dropped={} retained={}{}",
                outcome.queued_metrics,
                outcome.queued_logs,
                outcome.upload.uploaded,
                outcome.upload.dropped,
                outcome.upload.retained,
                outcome
                    .upload
                    .last_error
                    .as_deref()
                    .map_or_else(String::new, |e| format!(" err={e}"))
            );
        }
    }

    // ── Clean shutdown ──────────────────────────────────────────────────
    telemetry::log(
        Severity::Info,
        format!("demo finished after {iteration} iterations, slowest {slowest:.3}s"),
    );
    let outcome = telemetry::shutdown();
    println!();
    println!(
        "shutdown flush: skipped={} queued_metrics={} queued_logs={} uploaded={} retained={}",
        outcome.skipped,
        outcome.queued_metrics,
        outcome.queued_logs,
        outcome.upload.uploaded,
        outcome.upload.retained
    );
    if let Some(queue) = telemetry::ping_queue() {
        let left = queue.len();
        if left > 0 {
            println!(
                "{left} ping(s) still queued at {} — they will be uploaded on the next run \
                 (this is the offline path working, not an error)",
                queue.dir().display()
            );
        }
    }
}
