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
    /// The session's document size in paragraphs (the app-level "document
    /// size" unit this demo reports via `set_document_size`).
    doc_paragraphs: Option<usize>,
    /// Enables the INTRODUCED CRASH BUG: sessions with a big document
    /// crash mid-run with a realistic `expect` reason. Shipped (per the
    /// drill) in 1.5.0 only.
    crash_bug: bool,
    /// Parent mode: spawn N simulated users (child invocations of this
    /// binary) across three versions and exit.
    fleet: Option<usize>,
    /// Update drill: check this manifest URL at session end.
    update_manifest: Option<String>,
    rollout_drill: Option<String>,
    /// `download_automatically` for the drill (auto mode); without it the
    /// drill is manual: notify, then download+apply on simulated consent.
    update_auto: bool,
    /// Base64 minisign ROOT public key: when given, the drill stages through
    /// the SIGNED path (`download_and_verify`) exactly as a real client does.
    update_root_key: Option<String>,
    /// Release channel the drill follows ("" = stable).
    update_channel: String,
    /// Crash-mail drill: after a crash is persisted, the relaunch mails the
    /// dump to this address (with --mail-port against a local sink).
    mail_to: Option<String>,
    mail_port: Option<u16>,
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
            doc_paragraphs: None,
            crash_bug: false,
            fleet: None,
            update_manifest: None,
            rollout_drill: None,
            update_auto: false,
            update_root_key: None,
            update_channel: String::new(),
            mail_to: None,
            mail_port: None,
        };
        if let Ok(to) = std::env::var("AZ_DEMO_MAIL_TO") {
            args.mail_to = Some(to);
        }
        if let Ok(port) = std::env::var("AZ_DEMO_MAIL_PORT") {
            args.mail_port = port.parse().ok();
        }
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
                "--doc-size" => args.doc_paragraphs = value().parse().ok(),
                "--crash-bug" => args.crash_bug = true,
                "--fleet" => args.fleet = value().parse().ok(),
                "--update-manifest" => args.update_manifest = Some(value()),
                "--rollout-drill" => args.rollout_drill = Some(value()),
                "--update-auto" => args.update_auto = true,
                "--update-root-key" => args.update_root_key = Some(value()),
                "--update-channel" => args.update_channel = value(),
                "--mail-to" => args.mail_to = Some(value()),
                "--mail-port" => args.mail_port = value().parse().ok(),
                // (also readable from AZ_DEMO_MAIL_TO / AZ_DEMO_MAIL_PORT —
                // env survives the panic hook's reporter reinvoke, argv does
                // not)
                "--help" | "-h" => {
                    println!(
                        "usage: telemetry_grafana [--version V] [--channel C] \
                         [--iterations N] [--flush-every N] [--pace-ms N] [--panic-at N] \
                         [--remember] [--doc-size PARAGRAPHS] [--crash-bug] [--fleet N] \
                         [--mail-to ADDR] [--mail-port P]"
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

/// The REAL engine pipeline, exercised once per iteration when the build
/// has the rendering features: style -> solve (solver3) -> display list ->
/// CPU paint into a pixmap. This is what makes the "Sub-spans: layout" and
/// "Sub-spans: repaint" panels show genuine engine timings — a headless app
/// still lays out and repaints, and so does this demo.
#[cfg(all(
    feature = "text_layout",
    feature = "cpurender",
    feature = "xml",
    feature = "font_loading"
))]
mod engine_frame {
    use std::collections::BTreeMap;

    use azul_core::dom::{Dom, DomId};
    use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
    use azul_core::resources::{IdNamespace, ImageCache, RendererResources};
    use azul_core::styled_dom::StyledDom;
    use azul_css::props::basic::FontRef;
    use azul_layout::cpurender::{render_with_font_manager, RenderOptions};
    use azul_layout::font::loading::build_font_cache;
    use azul_layout::font_traits::{FontManager, TextLayoutCache};
    use azul_layout::glyph_cache::GlyphCache;
    use azul_layout::solver3::layout_document;
    use azul_layout::xml::DomXmlExt;
    use azul_layout::Solver3LayoutCache;

    /// Everything a real window retains between frames.
    pub struct EngineState {
        font_manager: FontManager<FontRef>,
        layout_cache: Solver3LayoutCache,
        text_cache: TextLayoutCache,
        glyph_cache: GlyphCache,
        renderer_resources: RendererResources,
        image_cache: ImageCache,
        styled: StyledDom,
    }

    impl EngineState {
        /// Builds the persistent engine state (font discovery happens here,
        /// once — same as a real app's startup).
        pub fn create(document: &str) -> Option<Self> {
            let font_manager = FontManager::new(build_font_cache()).ok()?;
            Some(Self {
                font_manager,
                layout_cache: Solver3LayoutCache::default(),
                text_cache: TextLayoutCache::new(),
                glyph_cache: GlyphCache::new(),
                renderer_resources: RendererResources::default(),
                image_cache: ImageCache::default(),
                styled: Dom::from_xml_string(document),
            })
        }

        /// Replaces the document (the "user edited / resized the doc" path —
        /// the next frame reconciles against the new tree).
        pub fn set_document(&mut self, document: &str) {
            self.styled = Dom::from_xml_string(document);
        }

        /// One frame: solve layout at `viewport_width`, then CPU-paint the
        /// display list. All solver/raster `Probe` spans fire inside —
        /// `drain_probe_events()` turns them into `app_phase_seconds`.
        /// Returns the painted pixmap's byte count (and keeps the work
        /// observable so nothing is optimised away).
        pub fn frame(&mut self, viewport_width: f32) -> usize {
            let viewport = LogicalRect {
                origin: LogicalPosition::zero(),
                size: LogicalSize::new(viewport_width, 600.0),
            };
            let mut debug_messages = None;
            let display_list = match layout_document(
                &mut self.layout_cache,
                &mut self.text_cache,
                &self.styled,
                viewport,
                &self.font_manager,
                &BTreeMap::new(),
                &BTreeMap::new(),
                &mut debug_messages,
                None,
                &self.renderer_resources,
                IdNamespace(0),
                DomId::ROOT_ID,
                false,
                Vec::new(),
                None,
                &self.image_cache,
                None,
                None,
                azul_core::task::GetSystemTimeCallback {
                    cb: azul_core::task::get_system_time_libstd,
                },
                &[],
            ) {
                Ok(dl) => dl,
                Err(_) => return 0,
            };
            let pixmap = render_with_font_manager(
                &display_list,
                &self.renderer_resources,
                &self.font_manager,
                RenderOptions {
                    width: viewport_width,
                    height: 600.0,
                    dpi_factor: 1.0,
                },
                &mut self.glyph_cache,
            );
            pixmap.map_or(0, |p| std::hint::black_box(p.data().len()))
        }
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
    if let Some(users) = args.fleet {
        run_fleet(users, &args);
        return;
    }

    // ── Consent ─────────────────────────────────────────────────────────
    // Reads AZ_TELEMETRY plus the layered config files. Tier is `off` unless
    // something explicitly opted in, and nothing below sends anything at
    // tier off.
    let config = telemetry::init(
        "azul-telemetry-demo",
        AppMeta::new(&args.version, &args.channel),
    );
    telemetry::install_panic_hook();
    // The app registers its support mailbox right after init — it must exist
    // both in the normal launch (arming the reinvoke-reporter) and in the
    // reporter process itself (init also gives the mail subject its
    // app-name + version).
    register_crash_contact(&args);
    // Crash-REPORTER mode: the panic hook of a previous (endpoint-less)
    // process respawned us with AZ_CRASH_DUMP. We are not the app now — show
    // the dump and offer submission. A real shell does this as a small
    // CPU-rendered window (`AzApp::run` checks the same env var); this
    // headless demo prints it and submits with a canned user message.
    if run_crash_reporter_if_spawned(&args) {
        return;
    }
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
    println!(
        "  consent tier      {} (from {:?})",
        config.tier.as_str(),
        config.tier_source
    );
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
    let mut paragraphs = args.doc_paragraphs.unwrap_or(2_000);
    // Slow frames (and probe spans) at or above this warn — and the FIRST
    // slow event of the session carries the machine's system info.
    telemetry::set_slow_frame_threshold_ms(25.0);

    // "Open the document": RSS before, build + parse (the resident copy IS
    // the open document — it stays alive for the whole session so RSS
    // genuinely scales with document size), RSS after + delta-per-unit.
    let doc_open = telemetry::record_document_open_begin();
    let mut document = build_document(paragraphs);
    let resident_doc: Vec<String> = vec![document.clone()];
    let first_nodes = run_workload(&document);
    // The REAL pipeline (style -> solve -> display list -> CPU paint), when
    // the build has the rendering features. A headless app still lays out
    // and repaints every frame — so does this demo.
    #[cfg(all(
        feature = "text_layout",
        feature = "cpurender",
        feature = "xml",
        feature = "font_loading"
    ))]
    let mut engine = engine_frame::EngineState::create(&document);
    #[cfg(not(all(
        feature = "text_layout",
        feature = "cpurender",
        feature = "xml",
        feature = "font_loading"
    )))]
    println!(
        "  NOTE: built without text_layout+cpurender+xml+font_loading — no real \
         layout/paint per frame, so the layout/repaint sub-span panels stay empty."
    );
    telemetry::record_document_opened(doc_open, paragraphs as f64);
    let startup_secs = process_start.elapsed().as_secs_f64();
    telemetry::record_startup(startup_secs, current_rss().unwrap_or(0));
    println!(
        "startup {:.1} ms, opened a {paragraphs}-paragraph document \
         ({first_nodes} nodes), rss {:.1} MiB",
        startup_secs * 1_000.0,
        current_rss().unwrap_or(0) as f64 / (1024.0 * 1024.0)
    );
    assert!(!resident_doc.is_empty()); // the resident copy must stay alive

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
        // THE INTRODUCED CRASH BUG (drill): 1.5.0 sessions with a big
        // document die mid-session on a realistic `expect`. The hook turns
        // this into a red Loki record + a queued crash dump with the
        // message, location, stripped backtrace, live span scope and system
        // info — which is exactly what the fleet run exists to verify.
        if args.crash_bug && paragraphs > 5_200 && iteration == args.iterations / 2 {
            let _guard = azul_layout::probe::Probe::span("demo.autosave");
            // The bug: the "cache lookup" comes back empty for huge
            // documents and the expect fires. Opaque to clippy on purpose —
            // a literal `None.expect()` would be linted away.
            let glyph_page: Option<u32> = [7_u32].iter().copied().find(|_| paragraphs < 5_200);
            let _ =
                glyph_page.expect("glyph cache page must exist for documents over 5200 paragraphs");
        }

        // Resize the document every 12 iterations. The size change is what
        // makes the frame "relayout" rather than "repaint" — the scope label
        // reflects work that actually happened, it is not a dice roll.
        let resized = iteration % 12 == 0;
        if resized {
            paragraphs = if paragraphs >= 6_000 {
                2_000
            } else {
                paragraphs + 1_000
            };
            document = build_document(paragraphs);
            #[cfg(all(
                feature = "text_layout",
                feature = "cpurender",
                feature = "xml",
                feature = "font_loading"
            ))]
            if let Some(engine) = engine.as_mut() {
                engine.set_document(&document);
            }
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
        // Real layout + CPU repaint. The viewport wobbles every third frame
        // (a resize), so some frames re-flow and some hit the engine's
        // caches — both paths are the truth the panels should show.
        #[cfg(all(
            feature = "text_layout",
            feature = "cpurender",
            feature = "xml",
            feature = "font_loading"
        ))]
        if let Some(engine) = engine.as_mut() {
            let viewport_width = 800.0 + ((iteration / 3) % 8) as f32 * 20.0;
            let painted = engine.frame(viewport_width);
            std::hint::black_box(painted);
        }
        let elapsed = started.elapsed().as_secs_f64();
        slowest = slowest.max(elapsed);

        // An app callback timed under its own SYMBOL NAME (dladdr): shows
        // up as span `cb:demo_button_click` in the per-phase panels — the
        // "my_button_click is slower on 1.5.0" comparison.
        {
            let _cb = azul_layout::probe::Probe::span_for_fn(demo_button_click as usize);
            demo_button_click(nodes as u64);
        }

        // Per-frame DURATION histograms (query these in ms): the whole
        // iteration as scope `total`, and the same duration again as a TIMER
        // tick — this demo's loop stands in for the animation timer, so a
        // slow iteration is exactly a slow animation frame.
        telemetry::record_frame("total", elapsed);
        telemetry::record_timer_frame(elapsed);
        telemetry::record_relayout_scope(if resized { "relayout" } else { "repaint" });
        telemetry::observe("demo_document_nodes", nodes as f64);
        telemetry::count("demo_iterations_total", 1);
        if let Some(rss) = current_rss() {
            telemetry::record_memory(rss, 0);
        }
        // Probe spans -> app_phase_seconds{phase}. This is the bridge the
        // plan calls for: the profiler's buffer, drained into metrics.
        // Bridge Probe spans into app_phase_seconds; the returned count is
        // only informative here.
        let _ = telemetry::drain_probe_events();

        // Real, load-bearing log lines: the WARN is the one you would want to
        // find in Loki sitting next to a latency spike.
        if elapsed > 0.020 {
            telemetry::log(
                Severity::Warn,
                format!(
                    "slow iteration {iteration}: {:.1} ms for {nodes} nodes",
                    elapsed * 1_000.0
                ),
            );
        }

        if iteration % args.flush_every == 0 {
            telemetry::log(
                Severity::Info,
                format!(
                    "iteration {iteration}: {nodes} nodes in {:.1} ms \
                     ({paragraphs} paragraphs)",
                    elapsed * 1_000.0
                ),
            );
            let outcome = telemetry::flush();
            println!(
                "iter {iteration:>4}  {:>7.1} ms  {nodes:>6} nodes  \
                 flush: queued_metrics={} queued_logs={} uploaded={} dropped={} retained={}{}",
                elapsed * 1_000.0,
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

    // ── Update drill (localhost manifest; auto + manual modes) ─────────
    #[cfg(feature = "updater")]
    if let Some(manifest) = &args.rollout_drill {
        run_rollout_drill(manifest, &args.version, &args.update_channel);
    }
    #[cfg(feature = "updater")]
    if let Some(manifest) = &args.update_manifest {
        run_update_drill(
            manifest,
            &args.version,
            &args.update_channel,
            args.update_auto,
            args.update_root_key.as_deref(),
        );
    }
    #[cfg(not(feature = "updater"))]
    if args.rollout_drill.is_some() || args.update_manifest.is_some() {
        eprintln!("update drills need --features updater");
    }

    // ── Clean shutdown ──────────────────────────────────────────────────
    telemetry::log(
        Severity::Info,
        format!(
            "demo finished after {iteration} iterations, slowest {:.1} ms",
            slowest * 1_000.0
        ),
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

/// Deterministic tiny LCG so the fleet is reproducible without pulling in a
/// rand crate (and without `SystemTime` seeding, which would make two runs
/// incomparable).
fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state >> 33
}

/// Parent mode: simulate `users` real users as CHILD INVOCATIONS of this
/// binary across three versions.
///
/// Each user gets a stable `AZ_TELEMETRY_CLIENT_ID` (`sim-user-NNN`), a
/// version from a rollout-shaped weighting (50% 1.4.2, 30% 1.4.3, 20%
/// 1.5.0), and a document size from a skewed distribution. 1.5.0 carries
/// the INTRODUCED CRASH BUG (`--crash-bug`): its users with documents over
/// 5200 paragraphs crash mid-session. Crashed users are relaunched once —
/// the real "next launch" that uploads the persisted crash — and, when
/// `--mail-to` is set, the relaunch also mails the crash dump.
fn run_fleet(users: usize, args: &Args) {
    let exe = std::env::current_exe().expect("own path");
    let versions = ["1.4.2", "1.4.3", "1.5.0"];
    let mut crashed: Vec<(usize, String, usize)> = Vec::new();
    let mut running: Vec<(usize, std::process::Child)> = Vec::new();
    let max_parallel = 6usize;
    let mut spawned = 0usize;
    let mut finished = 0usize;

    println!("fleet: {users} users across {versions:?} (crash bug shipped in 1.5.0)");
    let spawn = |i: usize| -> (std::process::Child, String, usize, bool) {
        let mut seed = 0x00C0_FFEE ^ (i as u64).wrapping_mul(0x9E37_79B9);
        let v = match lcg(&mut seed) % 10 {
            0..=4 => versions[0],
            5..=7 => versions[1],
            _ => versions[2],
        };
        // 1500..=7500 paragraphs, skewed small (min of two draws) so most
        // users have modest documents and a tail has huge ones.
        let d1 = 1_500 + (lcg(&mut seed) % 6_000) as usize;
        let d2 = 1_500 + (lcg(&mut seed) % 6_000) as usize;
        let doc = d1.min(d2);
        let iters = 14 + (lcg(&mut seed) % 16);
        let has_bug = v == "1.5.0";
        let mut cmd = std::process::Command::new(&exe);
        cmd.env("AZ_TELEMETRY_CLIENT_ID", format!("sim-user-{i:03}"))
            .arg("--version")
            .arg(v)
            .arg("--channel")
            .arg(&args.channel)
            .arg("--iterations")
            .arg(iters.to_string())
            .arg("--pace-ms")
            .arg("40")
            .arg("--flush-every")
            .arg("5")
            .arg("--doc-size")
            .arg(doc.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if has_bug {
            cmd.arg("--crash-bug");
        }
        (
            cmd.spawn().expect("spawn child"),
            v.to_owned(),
            doc,
            has_bug,
        )
    };

    let mut meta: std::collections::HashMap<usize, (String, usize)> =
        std::collections::HashMap::new();
    while finished < users {
        while spawned < users && running.len() < max_parallel {
            let (child, v, doc, _bug) = spawn(spawned);
            meta.insert(spawned, (v, doc));
            running.push((spawned, child));
            spawned += 1;
        }
        let mut still: Vec<(usize, std::process::Child)> = Vec::new();
        for (i, mut child) in running.drain(..) {
            match child.try_wait() {
                Ok(Some(status)) => {
                    finished += 1;
                    let (v, doc) = meta.get(&i).cloned().unwrap_or_default();
                    if !status.success() {
                        println!("  user {i:03} v{v} doc={doc}: CRASHED ({status})");
                        crashed.push((i, v, doc));
                    }
                    if finished % 20 == 0 {
                        println!("  {finished}/{users} sessions done");
                    }
                }
                _ => still.push((i, child)),
            }
        }
        running = still;
        std::thread::sleep(std::time::Duration::from_millis(30));
    }

    // The "next launch" of every crashed user: uploads the persisted crash
    // (metrics + the red Loki record), and optionally mails the dump.
    println!(
        "fleet: {} of {users} users crashed — relaunching each once (the drain)",
        crashed.len()
    );
    for (i, v, doc) in &crashed {
        let mut cmd = std::process::Command::new(&exe);
        cmd.env("AZ_TELEMETRY_CLIENT_ID", format!("sim-user-{i:03}"))
            .arg("--version")
            .arg(v)
            .arg("--channel")
            .arg(&args.channel)
            .arg("--iterations")
            .arg("6")
            .arg("--pace-ms")
            .arg("20")
            .arg("--flush-every")
            .arg("3")
            .arg("--doc-size")
            .arg(doc.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        drop(cmd.status());
    }
    println!("fleet: done ({} crash drains)", crashed.len());
}

/// Registers the support mailbox — which ARMS the reinvoke-reporter: a
/// panic with no OTLP endpoint then writes its dump to a temp file and
/// respawns this executable with AZ_CRASH_DUMP set.
#[cfg(feature = "crash-mail")]
fn register_crash_contact(args: &Args) {
    let Some(to) = &args.mail_to else { return };
    let mut config = telemetry::crash_mail::CrashMailConfig::new(
        to.clone(),
        "crash-reporter@azul-demo.invalid",
        "azul-demo.invalid",
    )
    .with_tls(false);
    if let Some(port) = args.mail_port {
        config = config.with_ports(vec![port]);
    }
    telemetry::crash_mail::set_crash_contact(config);
}

#[cfg(not(feature = "crash-mail"))]
fn register_crash_contact(_args: &Args) {}

/// The reporter half: when AZ_CRASH_DUMP is set, this process exists only
/// to show the previous process's crash and offer submission. Returns true
/// when it ran (the caller must exit instead of starting the app).
#[cfg(feature = "crash-mail")]
fn run_crash_reporter_if_spawned(_args: &Args) -> bool {
    let Some(dump) = telemetry::crash_dump_from_env() else {
        return false;
    };
    println!("┌─ azul crash reporter ────────────────────────────────");
    println!("│ the application crashed:");
    println!("│   {}", dump.message);
    println!("│ at    {}", dump.location);
    println!("│ scope {}", dump.scope);
    println!("│ backtrace (paths stripped):");
    for line in dump.backtrace.lines().take(8) {
        println!("│   {line}");
    }
    println!("└──────────────────────────────────────────────────────");
    match telemetry::crash_mail::crash_contact() {
        Some(contact) => {
            // The real UI collects the message from the user here.
            let user_message = "simulated user message: it crashed while I was typing";
            match telemetry::crash_mail::send_dump_file(contact, &dump.path, user_message) {
                Ok(()) => println!("crash-reporter: dump mailed to {}", contact.to),
                Err(e) => println!("crash-reporter: {e} (dump kept at {})", dump.path.display()),
            }
        }
        None => println!(
            "crash-reporter: no contact registered — dump kept at {}",
            dump.path.display()
        ),
    }
    true
}

#[cfg(not(feature = "crash-mail"))]
fn run_crash_reporter_if_spawned(_args: &Args) -> bool {
    false
}

/// The SLOW-ROLLOUT drill: runs the REAL check path (HTTP + manifest +
/// persisted state + cohort gate) as several cohort buckets against one
/// manifest, so "10% today, 50% tomorrow" is observable: low buckets get
/// the release, high buckets read `staggered` (= UpToDate for now), and
/// the notify-only audience stays quiet until the rollout completes.
#[cfg(feature = "updater")]
fn run_rollout_drill(manifest_url: &str, current_version: &str, channel: &str) {
    use azul_layout::updater as up;

    println!("rollout: drilling {manifest_url} as several cohort buckets");
    let state_dir = std::env::temp_dir().join("azul-rollout-demo");
    drop(std::fs::remove_dir_all(&state_dir)); // fresh cohorts per drill

    for bucket in [5u8, 25, 42, 75, 95] {
        // AZ_UPDATE_BUCKET forces the cohort — exactly what a fleet of
        // machines would each draw once and persist.
        std::env::set_var("AZ_UPDATE_BUCKET", bucket.to_string());
        let mut state = up::UpdateState::load(&state_dir);
        let verdict = up::check_for_updates_blocking(
            manifest_url,
            current_version,
            channel,
            &mut state,
            up::UpdateAudience::AutoUpdate,
        );
        let text = match &verdict {
            up::UpdateCheckResult::Available(r) => {
                format!("AVAILABLE {} (this cohort is open)", r.version.as_str())
            }
            up::UpdateCheckResult::UpToDate => {
                "staggered/up-to-date (cohort not open yet)".to_owned()
            }
            up::UpdateCheckResult::Error(e) => format!("ERROR {}", e.as_str()),
        };
        println!("rollout: bucket {bucket:>2} auto   -> {text}");
    }

    // The system-installed audience: no notification until 100%.
    std::env::set_var("AZ_UPDATE_BUCKET", "0");
    let mut state = up::UpdateState::load(&state_dir);
    let verdict = up::check_for_updates_blocking(
        manifest_url,
        current_version,
        channel,
        &mut state,
        up::UpdateAudience::NotifyOnly,
    );
    let text = match &verdict {
        up::UpdateCheckResult::Available(r) => {
            format!("NOTIFY {} (rollout complete)", r.version.as_str())
        }
        up::UpdateCheckResult::UpToDate => {
            "quiet (rollout not at 100% yet - system installs wait)".to_owned()
        }
        up::UpdateCheckResult::Error(e) => format!("ERROR {}", e.as_str()),
    };
    println!("rollout: notify-only audience -> {text}");
    std::env::remove_var("AZ_UPDATE_BUCKET");
}

/// A stand-in for an app's own `extern "C"` UI callback — timed via
/// `Probe::span_for_fn`, so its RESOLVED NAME becomes the span/phase.
#[unsafe(no_mangle)]
extern "C" fn demo_button_click(nodes: u64) -> u64 {
    // A little real work proportional to the document, so versions with
    // bigger documents genuinely spend more time "in the callback".
    let mut acc = 0u64;
    for i in 0..(nodes / 8).max(1) {
        acc = acc.wrapping_add(i).rotate_left(3) ^ 0x5A5A;
    }
    acc
}

/// The UPDATE drill: check → notify → (auto-stage | consent-download) →
/// APPLY onto a scratch "installed app" copy, proving the whole chain incl.
/// resume. A real app runs the check via `CallbackInfo::check_for_updates`
/// (async on an azul Thread) and shows the UpdateVersion dialog instead of
/// printing.
#[cfg(feature = "updater")]
fn run_update_drill(
    manifest_url: &str,
    current_version: &str,
    channel: &str,
    auto: bool,
    root_key: Option<&str>,
) {
    use azul_layout::updater as up;

    let install = up::InstallKind::detect();
    let mode = up::effective_mode(up::UpdateMode::SelfUpdate, &install);
    println!("update: install={install:?} effective_mode={mode:?}");

    let state_dir = std::env::temp_dir().join("azul-update-demo");
    let mut state = up::UpdateState::load(&state_dir);
    let result = up::check_for_updates_blocking(
        manifest_url,
        current_version,
        channel,
        &mut state,
        up::UpdateAudience::AutoUpdate,
    );
    state.save(&state_dir);

    let release = match result {
        up::UpdateCheckResult::UpToDate => {
            println!("update: {current_version} is up to date");
            return;
        }
        up::UpdateCheckResult::Error(e) => {
            println!("update: check failed: {}", e.as_str());
            return;
        }
        up::UpdateCheckResult::Available(r) => r,
    };
    println!(
        "update: {} -> {} available ({} mode)",
        current_version,
        release.version.as_str(),
        if auto { "auto" } else { "manual" }
    );

    // The changelog the UpdateVersion dialog would render (Markdown).
    if !release.changelog_md_url.as_str().is_empty() {
        if let Ok(resp) = azul_layout::http::http_get_with_config(
            release.changelog_md_url.as_str(),
            &azul_layout::http::HttpRequestConfig::new(),
        ) {
            let md = String::from_utf8_lossy(resp.body.as_ref());
            println!("update: changelog ({} lines):", md.lines().count());
            for line in md.lines().take(4) {
                println!("    | {line}");
            }
        }
    }

    let staging = state_dir.join("staging");
    // With a root key the drill takes the SIGNED path a real client takes:
    // digest pin + root-delegated signature, checked on THIS call.
    if let Some(root) = root_key {
        match up::download_and_verify(&release, &staging, root, &mut state) {
            Ok(o) => println!(
                "update: staged AND VERIFIED {} ({} bytes, cached={}) — signature chain OK, \
                 key generation now {}",
                o.path.display(),
                o.bytes_written,
                o.used_cached,
                state.key_generation
            ),
            Err(e) => println!("update: REFUSED by verification: {e}"),
        }
        state.save(&state_dir);
        return;
    }
    if auto {
        // AUTO: stage in the background; consent still gates the swap.
        match up::download_update(&release, &staging) {
            Ok(o) => println!(
                "update: auto-staged {} ({} bytes this call, resumed_from={}, cached={}, range-resume-honored={})",
                o.path.display(), o.bytes_written, o.resumed_from_bytes, o.used_cached,
                o.server_supports_resume
            ),
            Err(e) => println!("update: staging failed: {e}"),
        }
        println!("update: waiting for user consent to install (auto mode ends here)");
        return;
    }

    // MANUAL: simulate the dialog's "Install now" consent, then download +
    // apply onto a scratch installed-app copy (never the running binary in
    // a demo).
    println!("update: [dialog] user consents to install");
    let outcome = match up::download_update(&release, &staging) {
        Ok(o) => o,
        Err(e) => {
            println!("update: download failed: {e}");
            return;
        }
    };
    println!(
        "update: downloaded {} ({} bytes this call, resumed_from={}, range-resume-honored={})",
        outcome.path.display(),
        outcome.bytes_written,
        outcome.resumed_from_bytes,
        outcome.server_supports_resume
    );
    let fake_install = state_dir.join("installed-app.bin");
    drop(std::fs::write(&fake_install, b"OLD-VERSION-BINARY"));
    match up::apply_update(&outcome.path, &fake_install) {
        Ok(()) => {
            let new_len = std::fs::metadata(&fake_install).map_or(0, |m| m.len());
            println!(
                "update: APPLIED — {} is now {} bytes (was 18: the old binary)",
                fake_install.display(),
                new_len
            );
        }
        Err(e) => println!("update: apply failed: {e}"),
    }
}
