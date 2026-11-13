//! `azul-doc gen-e2e <txt> <out-dir>` — fan out a fleet of Claude agents
//! that turn a ONE-LINE test description into a real e2e JSON test file.
//!
//! It GENERATES tests, it does not run them (`azul-doc reftest` / the debug
//! server's E2E runner execute them later).
//!
//! The corpus (`scripts/E2E_TESTS.txt`) is one test per line, each tagged
//! `[category/sub] description…`. One line → one agent → one
//! `<out-dir>/<NNNNN>-<slug>.json`.
//!
//! Everything the agent is told about the schema is DERIVED FROM THE CODE
//! (`layout/src/e2e/full.rs`) at run time — the op
//! names, their parameters and which of those are required are parsed out of
//! the `DebugEvent` enum and the `evaluate_assertion` dispatch, never recalled
//! from memory. The same parse is the mechanical validation gate: a generated
//! file that does not parse as JSON, or that references an op / omits a
//! required parameter that the engine does not actually have, is DELETED and
//! counted as a failure.
//!
//! The gate also checks what the ENGINE would refuse at run time, so a scenario
//! that could only fail for a reason of its own making is never written:
//!   * the step is read as the runner reads it (`parse_debug_event`): wrong types and unknown enum
//!     variants fail here, not at step N;
//!   * a string param with a closed set of values (`which`, `frame`, ...) holds one of them - the
//!     engine's own table, not a copy;
//!   * `mount` has html, and it parses (`mounted_nodes`, the parser `mount` uses);
//!   * a numeric `node_id` is the node the op takes: it exists, was not deleted, and is an element
//!     (not `<html>`, `<body>` or a whitespace run), a text node for `set_node_text`, an `<img>` for
//!     `set_node_image`;
//!   * a step that names a node comes after the `wait_frame` that applies the `mount` (before it
//!     the document does not exist: "Node 1 not found");
//!   * an op that names its node by `selector` OR `node_id` is given exactly one.
//!
//! Every artifact's `name` is made unique by appending `_<8 hex of _source_hash>`, and a review
//! batch pairs results with FILES, never with names (`pair_results`).
//!
//! Hard-won lessons inherited from earlier E2E fleets:
//!   1. A RATE-LIMITED `claude -p` exits 0 and answers with the limit message as PLAIN TEXT. That
//!      must never be written out as a test.
//!   2. RESUME: a 13k-line run *will* be interrupted. Keep a done-list.
//!   3. `--dry-run` prints the work list and launches nothing.
//!   4. Only mark a line done when the artifact actually landed AND validated.
//!
//! INCREMENTAL, and CONTENT-ADDRESSED
//! ---------------------------------
//! The corpus is machine-generated (`scripts/gen_e2e_cases.py`); lines get
//! inserted and reordered, so a line NUMBER is not a stable id. The done-key is
//! therefore the HASH OF THE DESCRIPTION LINE. Each generated artifact carries
//! its own `_source_hash` / `_source`, so the out-dir alone is a complete
//! resume record — the `.done-gen-e2e` list is only a cache.
//!
//! Done-ness is resolved as: an artifact with this line's hash exists on disk
//! AND still passes the validation gate. Anything else is work:
//!   * no artifact                      -> generate
//!   * artifact exists but FAILS the gate -> regenerate (overwrite)
//!   * artifact whose hash is no longer in the corpus -> STALE ORPHAN, reported, deleted only with
//!     `--prune`
//!     `--limit N` means "generate N MORE", i.e. it truncates the not-yet-done list
//!     (after `--filter`), never the corpus.
//!
//! THE REVIEW LOOP (`--review-batch N`)
//! -----------------------------------
//! Fanning 6 agents at a 13k-line corpus before the prompt is trustworthy
//! produces 13k plausible-looking files that certify nothing, and every one of
//! them then has to be re-read by a human. `--review-batch N` is the slow lane:
//!
//!   generate N  ->  RUN exactly those N  ->  ONE review agent  ->  STOP
//!
//! and the operator then edits `build_prompt` (below), rebuilds, and asks for
//! the next N. The rebuild is a DELIBERATE GATE, not an oversight: the prompt
//! stays in Rust source precisely so that changing it is a reviewed, committed,
//! `git blame`-able act. The reviewer PROPOSES a prompt diff; it is never
//! applied automatically.
//!
//! The run step is what makes the review worth anything: the mechanical gate
//! (`validate`) never executes a scenario, so up to now nothing distinguished a
//! test that expresses its corpus line from one that parses. Running the batch
//! also separates the two failure kinds the loop cares about — the ENGINE is
//! wrong (a genuine find; keep the test) versus the headless RUNNER cannot drive
//! it (a port task; see `Runner::unsupported` in `layout/src/e2e/runner.rs`).
//!
//! IMAGES ARE TRIAGE, NOT AN ARTIFACT. No generated scenario is required to
//! capture a PNG: that would tax all 13k of them for an image nobody opens, and
//! for the idle half of the corpus the capture is blank by construction.
//! Instead the REVIEWER captures one when it is investigating a FAILURE — it
//! copies the scenario into `TRIAGE_DIR`, adds `capture_damage_png` steps THERE,
//! runs the copy and looks at the result (`triage_doc`). The committed scenario
//! stays byte-identical, and `target/` is gitignored, so no image ever reaches
//! git.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
};

use anyhow::{bail, Context, Result};
use azul_layout::e2e::{
    closed_param_values, load_e2e_tests, mounted_nodes_with, parse_debug_event, render_report,
    DebugEvent, E2eTest, E2eTestResult, MountedKind, MountedNode, CLOSED_STRING_PARAMS,
    DEAD_CONTROL_MARKER, STALE_SCREEN_MARKER,
};

/// Relative path of the file that DEFINES the e2e schema. Single source of truth.
// MUST be the copy that `azul-doc e2e` actually EXECUTES (azul_layout::e2e), not
// the DLL's. The server was ported into azul-layout and the two copies have
// already drifted (12,187 vs 12,252 lines); generating against the DLL's schema
// would emit tests whose ops the runner does not have — a silent false-green
// across the whole corpus. De-duplicating the DLL copy is tracked separately;
// until then, this path is the single source of truth for generation.
const FULL_RS: &str = "layout/src/e2e/full.rs";
/// Relative path of the worked example handed to every agent.
const EXAMPLE_JSON: &str = "tests/e2e/mount_damage_smoke.json";

/// The only op that writes an INSPECTABLE IMAGE FILE headlessly: the frame
/// masked to the damage region. NOT required in generated scenarios — it is the
/// review agent's failure-triage instrument (see `triage_doc`).
///
/// (`take_screenshot` also renders headlessly but answers with a base64 data URI
/// in the step response, i.e. nothing to open; `take_native_screenshot` needs a
/// host hook that nothing installs and always fails — see `OP_POLICY`.)
const PNG_OP: &str = "capture_damage_png";
/// Every capture lands under here: one gitignored tree (`.gitignore:2 target/`),
/// so a triage image can never reach git, and a model-authored `path` can never
/// escape it. Repo-root-relative, like the rest of the scenario format.
const PNG_DIR: &str = "target/e2e/";
/// Where the review agent may write COPIES of failing scenarios plus their
/// captures. Under `PNG_DIR`, so it inherits both properties above.
const TRIAGE_DIR: &str = "target/e2e/_triage/";
/// Model defaults, for BOTH the generator fleet and the reviewer.
///
/// TOKEN COST IS NOT THE CONSTRAINT ON THIS PROJECT — TEST QUALITY IS. A weak
/// generator does not fail loudly; it writes a plausible-looking scenario that
/// quietly tests something ELSE than its corpus line, which is precisely the
/// false-green class this whole corpus exists to eliminate, and which no
/// mechanical gate can catch (`validate` checks shape and op names, never
/// meaning). A cheap batch that has to be found and regenerated costs far more
/// than generating it well once — and each line is generated ONCE and then lives
/// in the repo as a gate, so the cost is amortised over every future CI run.
/// Likewise a cheap reviewer rubber-stamps, and a review that always approves is
/// worse than no review.
///
/// `--model` / `--effort` (and `--review-model` / `--review-effort`) remain as
/// overrides for anyone who wants to economise; the DEFAULT is the good model.
/// The CLI's effort scale is low|medium|high|xhigh|max; `medium` is the chosen
/// point.
const MODEL: &str = "opus";
/// See `MODEL`.
const EFFORT: &str = "medium";
/// The reviewer is a separate knob from the generator (one careful reader vs. a
/// fleet), but it defaults to the same good model for the same reason.
const REVIEW_MODEL: &str = MODEL;
/// See `MODEL`.
const REVIEW_EFFORT: &str = EFFORT;
/// The substring `Runner::unsupported` puts in the error of every step that
/// failed because the HEADLESS RUNNER cannot drive it (as opposed to the engine
/// being wrong). This is what makes harness-vs-engine attribution mechanical
/// instead of a matter of the reviewer's opinion.
const HARNESS_MARKER: &str = "is not supported by the headless runner";
/// Shorter than this and the "review" is a refusal, a truncation or a limit
/// message — not something to append to a report as analysis.
const MIN_REVIEW_CHARS: usize = 200;

// ===========================================================================
// Options
// ===========================================================================

#[derive(Debug, Clone)]
pub struct GenE2eOptions {
    pub txt: PathBuf,
    pub out_dir: PathBuf,
    pub jobs: usize,
    pub model: String,
    pub effort: String,
    pub dry_run: bool,
    pub redo: bool,
    pub limit: Option<usize>,
    pub filter: Option<String>,
    /// Delete artifacts whose source line no longer exists in the corpus.
    pub prune: bool,
    /// `--review-batch N`: generate at most N tests, RUN exactly those N, then
    /// have ONE agent review the batch and write `_review-<id>.md`. Implies
    /// `--limit N`. See the `REVIEW LOOP` section at the top of this file.
    pub review_batch: Option<usize>,
    /// Model for the single review agent — a knob of its own (a fleet of writers
    /// vs. one careful reader), with the same good default. See `MODEL`.
    pub review_model: String,
    /// Effort for the review agent.
    pub review_effort: String,
}

impl GenE2eOptions {
    pub fn parse(args: &[&str]) -> Result<Self> {
        let mut positional: Vec<&str> = Vec::new();
        let mut opts = Self {
            txt: PathBuf::new(),
            out_dir: PathBuf::new(),
            jobs: 6,
            model: MODEL.to_string(),
            effort: EFFORT.to_string(),
            dry_run: false,
            redo: false,
            limit: None,
            filter: None,
            prune: false,
            review_batch: None,
            review_model: REVIEW_MODEL.to_string(),
            review_effort: REVIEW_EFFORT.to_string(),
        };

        let mut i = 0;
        while i < args.len() {
            match args[i] {
                "--jobs" => {
                    opts.jobs = next(args, &mut i, "--jobs")?.parse()?;
                }
                "--model" => opts.model = next(args, &mut i, "--model")?.to_string(),
                "--effort" => opts.effort = next(args, &mut i, "--effort")?.to_string(),
                "--limit" => opts.limit = Some(next(args, &mut i, "--limit")?.parse()?),
                "--filter" => opts.filter = Some(next(args, &mut i, "--filter")?.to_string()),
                "--dry-run" => opts.dry_run = true,
                "--redo" => opts.redo = true,
                "--prune" => opts.prune = true,
                "--review-batch" => {
                    opts.review_batch = Some(next(args, &mut i, "--review-batch")?.parse()?);
                }
                "--review-model" => {
                    opts.review_model = next(args, &mut i, "--review-model")?.to_string();
                }
                "--review-effort" => {
                    opts.review_effort = next(args, &mut i, "--review-effort")?.to_string();
                }
                other if other.starts_with("--") => bail!("gen-e2e: unknown flag '{other}'"),
                other => positional.push(other),
            }
            i += 1;
        }

        match positional.as_slice() {
            [txt, out] => {
                opts.txt = PathBuf::from(txt);
                opts.out_dir = PathBuf::from(out);
            }
            _ => bail!("{USAGE}"),
        }
        if opts.jobs == 0 {
            bail!("gen-e2e: --jobs must be >= 1");
        }
        // `--review-batch N` IS the batch size, i.e. it is `--limit N` plus the
        // run+review tail. Accepting both would leave two knobs fighting over
        // one number — and silently taking the smaller is exactly the kind of
        // "helpful" behaviour that makes an operator mis-read the batch they
        // just reviewed.
        if let Some(n) = opts.review_batch {
            if n == 0 {
                bail!("gen-e2e: --review-batch must be >= 1");
            }
            if opts.limit.is_some() {
                bail!(
                    "gen-e2e: --review-batch N already limits the batch to N — pass one or the \
                     other, not both"
                );
            }
            opts.limit = Some(n);
        }
        Ok(opts)
    }
}

/// One usage string, shared by the parse error and `main`'s help.
pub const USAGE: &str = "usage: azul-doc gen-e2e <txt-file> <out-dir> [--jobs N] [--model M] \
                         [--effort E] [--limit N] [--filter <tag>] [--dry-run] [--redo] [--prune] \
                         [--review-batch N [--review-model M] [--review-effort E]]";

fn next<'a>(args: &[&'a str], i: &mut usize, flag: &str) -> Result<&'a str> {
    *i += 1;
    args.get(*i)
        .copied()
        .with_context(|| format!("gen-e2e: {flag} needs a value"))
}

// ===========================================================================
// Schema, parsed out of full.rs
// ===========================================================================

#[derive(Debug, Clone)]
struct OpDef {
    /// The `op` string as it appears in JSON (snake_case).
    name: String,
    /// (param, required)
    params: Vec<(String, bool)>,
    /// The first paragraph of the enum variant's doc on one line, if it
    /// carried one (see [`DocParagraph`]).
    doc: Option<String>,
}

#[derive(Debug)]
pub struct Schema {
    /// Debug-server ops (`DebugEvent` variants) usable as timeline steps.
    ops: Vec<OpDef>,
    /// Assertion ops (`evaluate_assertion` dispatch) + the params they read.
    asserts: Vec<OpDef>,
    /// Ops handled directly by the E2E step loop (not `DebugEvent` variants),
    /// with the params that block reads — see `step_loop_op_params`.
    extra: Vec<OpDef>,
    /// `DebugEvent` variants that ACTUALLY HAVE A MATCH ARM in the dispatch.
    /// A declared variant missing from this set is a ZOMBIE: it falls through to
    /// the catch-all, which logs "Unhandled" and answers `ok` — so a test using
    /// it PASSES WHILE DOING NOTHING. See `Schema::zombies`.
    handled: BTreeSet<String>,
}

impl Schema {
    fn known_op(&self, op: &str) -> Option<&OpDef> {
        self.ops
            .iter()
            .chain(self.asserts.iter())
            .find(|o| o.name == op)
    }
    fn is_known(&self, op: &str) -> bool {
        self.known_op(op).is_some() || self.extra.iter().any(|e| e.name == op)
    }
    /// Every op the engine has, in one list (timeline ops, step-loop ops, asserts).
    fn all_op_names(&self) -> impl Iterator<Item = &str> {
        self.ops
            .iter()
            .chain(self.asserts.iter())
            .map(|o| o.name.as_str())
            .chain(self.extra.iter().map(|o| o.name.as_str()))
    }
    /// Ops the engine has that NOBODY classified — a new `DebugEvent` variant.
    /// These are denied by the gate and must be surfaced loudly, never ignored.
    pub fn unclassified(&self) -> Vec<&str> {
        self.all_op_names()
            .filter(|o| classify(o) == OpClass::Unclassified)
            .collect()
    }
    /// Is this op DECLARED in `DebugEvent` but UNHANDLED by the dispatch?
    ///
    /// An op with no match arm is not a real op: the catch-all logs "Unhandled"
    /// and returns `ok`, so the harness reports SUCCESS FOR WORK IT DID NOT DO
    /// — a vacuously-green test, which is worse than no test because it counts
    /// as coverage. Derived from the code, never hardcoded: the moment somebody
    /// gives the variant a real match arm, it stops being a zombie and becomes
    /// usable again, with no change to `OP_POLICY` and no change here.
    pub fn is_zombie(&self, op: &str) -> bool {
        self.ops.iter().any(|o| o.name == op) && !self.handled.contains(op)
    }
    /// Every declared-but-unhandled op, in enum order.
    pub fn zombies(&self) -> Vec<&str> {
        self.ops
            .iter()
            .map(|o| o.name.as_str())
            .filter(|o| !self.handled.contains(*o))
            .collect()
    }
    /// Classified entries that the engine no longer has — a stale table row.
    pub fn stale_policy_entries(&self) -> Vec<&'static str> {
        OP_POLICY
            .iter()
            .map(|(n, _)| *n)
            .filter(|n| !self.is_known(n))
            .collect()
    }
}

// ===========================================================================
// OP CLASSIFICATION — the test surface, carved out of the debug protocol
// ===========================================================================
//
// `DebugEvent` is the DEBUG / VISUAL-EDITOR protocol. It is NOT a test surface,
// and handing all of it to a generator produces self-defeating tests.
//
// What these tests ARE: HEADLESS BEHAVIOUR tests over the cpurender path —
//     MOCK INPUT EVENT -> engine -> CORRECT DAMAGE PATCH / correct behaviour.
// Real OS input is out of scope (manual testing owns it). Layout/geometry
// correctness is out of scope (`azul-doc reftest` owns it). Everything below
// the OS boundary — every Callback API path — is in scope.
//
// The table below classifies EVERY op. It is the law: the prompt is rendered
// from the ALLOWED half (a model cannot use what it is not shown), and
// the validation gate rejects the DENIED half (the prompt is advisory, the gate
// is law). An op that appears in `DebugEvent` but NOT in this table is
// UNCLASSIFIED: it is reported loudly and treated as denied, so a newly added
// op can never be silently allowed nor silently swallowed.
//
// ORTHOGONAL to this table, and NOT expressible in it, is the ZOMBIE check
// (`Schema::is_zombie`): an op DECLARED in `DebugEvent` but with NO MATCH ARM
// falls through to the dispatch's catch-all, which answers `ok` without doing
// anything — so a test using it is vacuously green. That is derived from the
// code, not from this table, precisely so that an op stops being a zombie the
// instant it is implemented, with no edit here. An op must be BOTH allowed and
// non-zombie to reach the generator.

/// Why an op may not appear in a generated behaviour test. `None` == allowed.
pub type DenyReason = &'static str;

/// THE CLASSIFICATION TABLE. `(op, None)` = allowed, `(op, Some(reason))` = denied.
/// Keyed by the snake_case `op` string as it appears in the JSON.
#[rustfmt::skip]
const OP_POLICY: &[(&str, Option<DenyReason>)] = &[
    // DENY: the op NAME is defined by the application, not the engine, so the
    // generator has nothing valid to generate. Every scenario it invented
    // would name an op no handler recognises and fail with handled=false —
    // which is the op behaving correctly, and useless as a generated test.
    ("custom_op", Some("app-defined op name; the engine cannot know any valid one")),
    // DENY: its answer is whatever the application registered, so a generated
    // test has nothing engine-defined to assert on.
    ("list_app_ops", Some("lists the application's own ops; nothing engine-defined to assert")),
    // DENY: both are human-debugging aids that write to the log and assert
    // NOTHING. A generated scenario that reaches for one has substituted
    // "show me" for "check this", and passes no matter what the engine did —
    // `assert_response` is the assertion form of print_response.
    ("print",          Some("writes to the log and asserts nothing; a generated test needs an assertion")),
    ("print_response", Some("prints the last response instead of asserting on it; use assert_response")),
    // ALLOW: a read-only snapshot. Denying it would also block HAND-WRITTEN
    // scenarios from using it, and asserting a memory budget from a scenario
    // is the entire reason it exists.
    ("get_profile_report", None),
    // ALLOW: read-only, and the ONLY way a scenario can observe a transition.
    ("get_animations", None),
    // ALLOW: a deterministic stepper. It exists precisely so a generated
    // scenario can advance an animation without sampling the wall clock, which
    // is what would otherwise make every mid-flight assertion flaky.
    ("tick_animations", None),
    // ALLOW: queues the answers the engine's host requests (file dialogs,
    // http, audio/video devices) will receive, so a scenario that opens a
    // file or fetches a URL is deterministic instead of blocking on a real
    // dialog. Same reasoning as tick_animations: it exists so a generated
    // scenario can drive a flow that would otherwise depend on the host.
    ("mock", None),
    // -- ALLOW: MOCK INPUT — the primary drive surface ----------------------
    ("mouse_move",                None),
    ("mouse_down",                None),
    ("mouse_up",                  None),
    ("click",                     None),
    ("click_node",                None),
    ("double_click",              None),
    ("scroll",                    None),
    ("wheel",                     None),
    // Content-chokepoint image ops (overlay refactor O1): mutate through
    // LayoutWindow::apply_content_change exactly like a callback would.
    ("set_node_image",            None),
    ("add_image_to_cache",        None),
    ("remove_image_from_cache",   None),
    ("key_down",                  None),
    // Global hotkeys through the window's manager on the SIMULATED backend
    // (AZ_BACKEND=headless): press a declared accelerator, program the
    // backend's answer to the next grab (taken / refused / pending), and
    // settle a pending one. Deterministic input, like key_down.
    ("global_hotkey",             None),
    ("global_hotkey_answer",      None),
    ("global_hotkey_settle",      None),
    // `<webview>`: the scenario plays the browser - the headless backend and
    // the in-crate runner share one recording backend - and lists the views
    // the engine keeps. Deterministic input, like key_down.
    ("list_webviews",                  None),
    ("simulate_webview_navigation",    None),
    ("simulate_webview_load_finished", None),
    ("simulate_webview_title",         None),
    ("simulate_webview_load_failed",   None),
    // DENY: a notification click / button / dismissal queued into the
    // mailbox. Only the dll's notification service (AZ_BACKEND=headless)
    // routes it to a callback; the in-crate runner generated tests run in
    // has none, so nothing a generated scenario asserts afterwards can see
    // it. Hand-written AZ_E2E scenarios use it (examples/azul-widgets/e2e/).
    ("notification_event",        Some("delivered by the dll's notification service \
                                        (AZ_BACKEND=headless); the in-crate runner has none, \
                                        so its callback never runs - red on arrival")),
    ("key_up",                    None),
    ("text_input",                None),
    // A user's paste with the content handed in (no OS clipboard): the
    // focused node's Paste callbacks, then the engine's paste. Deterministic
    // input, like text_input; it errors by name without a focused node.
    ("paste",                     None),
    ("touch_start",               None),
    ("touch_move",                None),
    ("touch_end",                 None),
    ("touch_cancel",              Some("FullWindowState.touch_state has no cancel channel — a \
                                        cancelled touch is the same state delta as a lifted one, \
                                        so no TouchCancel event can be determined and the op \
                                        refuses by name; use touch_end")),
    ("pen_down",                  None),
    ("pen_move",                  None),
    ("pen_up",                    None),
    ("swipe",                     None),
    ("pinch",                     None),
    ("rotate",                    None),
    ("long_press",                None),
    ("resize",                    None),
    ("move",                      None),
    ("dpi_changed",               None),
    ("hit_test",                  None),
    ("focus",                     None),
    ("blur",                      None),
    // DOM focus, as opposed to the two WINDOW-focus ops above. `text_input`
    // hard-errors without a focused node and 18 of the 24 corpus widgets have
    // no focusable node, so every keyboard-editing line needed a precondition
    // it had no op to express. Click-to-focus is not a substitute: it needs a
    // coordinate, which a generated test may not guess.
    ("focus_node",                None),
    // Assistive technology as a drive surface. Not a synonym for `click`: it
    // addresses a node directly the way AT-SPI `do_action` / UIA Invoke /
    // `accessibilityActivate` do, and it exercises the action -> EventFilter
    // mapping no pointer op ever touches. Refuses by name on an unknown action,
    // a missing payload, or a node no screen reader can reach, so it cannot go
    // green on nothing.
    ("accessibility_action",      None),

    // -- ALLOW: APP-CALLBACK API — a real app mutates the DOM from a callback,
    //    so this is a legitimate second drive surface. ----------------------
    ("set_node_text",             None),
    ("set_node_css_override",     None),
    ("set_node_classes",          None),
    ("insert_node",               None),
    ("delete_node",               None),
    ("set_app_state",             None),
    ("scroll_node_to",            None),
    ("scroll_node_by",            None),
    ("scroll_into_view",          None),
    ("commit_undo_snapshot",      None),
    ("undo_app_state",            None),
    ("redo_app_state",            None),
    // `CallbackInfo::add_timer` / `remove_timer` — the API every real azul app
    // schedules work with (examples/azul-maps, azul-gamepad, rust/anim.rs, …).
    // A scenario cannot hand over the Rust `TimerCallback` fn pointer a
    // `CallbackChange::AddTimer` carries, so these two ops supply a canned
    // callback (rewrite one text node with the run count appended) and push it
    // through the real `CallbackInfo` methods. NOT in the `redraw`/`relayout`
    // deny class: those forge the repaint the engine was supposed to schedule
    // by itself, whereas these ask the engine to schedule something and then
    // let it decide, so the assertions still measure engine behaviour — and the
    // removal half is a genuine leak detector.
    ("add_timer",                 None),
    ("remove_timer",              None),
    // `CallbackInfo::set_mode` / `set_theme` - an app's own light / dark and
    // theme toggles (AzWidgets' toolbar, AzBuilder's page through these ops).
    // The engine then decides what the switch costs: a restyle for the mode,
    // a DOM rebuild for the theme.
    ("set_mode",                  None),
    ("set_theme",                 None),

    // -- ALLOW: HARNESS CONTROL --------------------------------------------
    ("mount",                     None),
    ("unmount",                   None),
    ("tick_ms",                   None),
    ("wait",                      None),
    ("wait_frame",                None),
    // Waits for the window to come to rest (no slide, transition, easing or
    // fade in flight) - harness control; it forges nothing.
    ("wait_settled",              None),
    ("reset_frame_counters",      None),
    ("snapshot_frame",            None),
    ("snapshot_resources",        None),
    ("snapshot_managers",         None),
    ("get_frame_report",          None),
    ("capture_damage_png",        None),
    ("take_screenshot",           None),

    // -- ALLOW: OBSERVATION (state queries; they carry no geometry) ---------
    ("get_state",                 None),
    ("get_app_state",             None),
    ("get_dom",                   None),
    ("get_dom_tree",              None),
    // ALLOW: read-only enumeration of the live DOMs, the same shape as
    // get_dom_tree. A multi-DOM scenario has no other way to learn the ids it
    // then addresses.
    ("list_doms",                 None),
    // ALLOW: read-only enumeration of the windows the debug server reaches
    // (a dialog, a menu the app opened). A multi-window scenario has no other
    // way to learn the `window_id`s it then addresses.
    ("list_windows",              None),
    ("get_node_hierarchy",        None),
    ("get_html_string",           None),
    ("get_node_css_properties",   None),
    ("get_node_dataset",          None),
    ("get_focus_state",           None),
    ("get_cursor_state",          None),
    // ALLOW: read-only, the accessibility tree the platform adapter holds.
    ("get_accessibility_tree",    None),
    // The app's mode choice and what the window shows; the app theme.
    ("get_mode",                  None),
    ("get_theme",                 None),
    ("get_selection_state",       None),
    ("dump_selection_manager",    None),
    ("get_scroll_states",         None),
    ("get_scrollable_nodes",      None),
    ("get_scrollbar_info",        None),
    ("get_virtual_view_states",   None),
    ("get_drag_state",            None),
    ("get_drag_context",          None),
    ("find_node_by_text",         None),

    // -- DENY 1: THE CRITICAL ONES -----------------------------------------
    // No real caller can reach these; they exist for the debugger, and they
    // MANUFACTURE THE VERY EFFECT UNDER TEST. `set_node_text` -> `redraw` ->
    // `assert_changed` passes even when the invalidation path is completely
    // broken — i.e. it masks the exact stale-screen bug this suite exists to
    // catch. The engine must decide to redraw/relayout BY ITSELF.
    ("redraw",   Some("debugger-only: forces the repaint the test is supposed to prove the engine \
                       schedules by itself — masks a broken invalidation path")),
    ("relayout", Some("debugger-only: forces the relayout the test is supposed to prove the engine \
                       schedules by itself — masks a broken invalidation path")),

    // -- DENY 2: the component / IDE family — out of scope entirely ---------
    ("create_component",           Some("visual-editor/IDE surface, not engine behaviour")),
    ("delete_component",           Some("visual-editor/IDE surface, not engine behaviour")),
    ("update_component",           Some("visual-editor/IDE surface, not engine behaviour")),
    ("update_component_render_fn", Some("visual-editor/IDE surface, not engine behaviour")),
    ("get_component_preview",      Some("visual-editor/IDE surface, not engine behaviour")),
    ("get_component_registry",     Some("visual-editor/IDE surface, not engine behaviour")),
    ("get_component_render_tree",  Some("visual-editor/IDE surface, not engine behaviour")),
    ("get_component_source",       Some("visual-editor/IDE surface, not engine behaviour")),
    ("create_library",             Some("visual-editor/IDE surface, not engine behaviour")),
    ("delete_library",             Some("visual-editor/IDE surface, not engine behaviour")),
    ("get_libraries",              Some("visual-editor/IDE surface, not engine behaviour")),
    ("get_library_components",     Some("visual-editor/IDE surface, not engine behaviour")),
    ("import_component_library",   Some("visual-editor/IDE surface, not engine behaviour")),
    ("export_component_library",   Some("visual-editor/IDE surface, not engine behaviour")),
    // AzBuilder's document ops (layout/src/e2e/builder.rs): they REPLACE the
    // app's DOM with the builder's document — pinned by the builder_tests
    // module, never a generated behaviour test.
    ("builder_get_document",       Some("visual-editor/IDE surface, not engine behaviour")),
    ("builder_insert",             Some("visual-editor/IDE surface, not engine behaviour")),
    ("builder_move",               Some("visual-editor/IDE surface, not engine behaviour")),
    ("builder_delete",             Some("visual-editor/IDE surface, not engine behaviour")),
    ("builder_set_attribute",      Some("visual-editor/IDE surface, not engine behaviour")),
    ("builder_undo",               Some("visual-editor/IDE surface, not engine behaviour")),
    ("builder_redo",               Some("visual-editor/IDE surface, not engine behaviour")),
    ("builder_reset",              Some("visual-editor/IDE surface, not engine behaviour")),
    ("builder_convert_to_component", Some("visual-editor/IDE surface, not engine behaviour")),
    // B5: the document's own stylesheet (a builder edit that remounts the
    // document; pinned by builder_tests / export_tests / project_tests).
    ("builder_get_stylesheet",     Some("visual-editor/IDE surface, not engine behaviour")),
    ("builder_set_stylesheet",     Some("visual-editor/IDE surface, not engine behaviour")),
    // B5: maps a window point to a builder document node through the
    // builder's own marker classes - IDE plumbing (a drop onto the window
    // picture); `hit_test` is the engine-facing query.
    ("builder_hit_test",           Some("visual-editor/IDE surface, not engine behaviour")),
    // B5: document edits and the document file (the format project_save
    // writes) - pinned by builder_tests.
    ("builder_duplicate",          Some("visual-editor/IDE surface, not engine behaviour")),
    ("builder_save_document",      Some("visual-editor/IDE surface, not engine behaviour")),
    ("builder_load_document",      Some("visual-editor/IDE surface, not engine behaviour")),
    ("get_component_thumbnail",    Some("visual-editor/IDE surface, not engine behaviour")),
    // AzBuilder's project folder ops (layout/src/e2e/project.rs): they read
    // and write the user's disk — pinned by the project_tests module, never a
    // generated behaviour test.
    ("project_info",               Some("visual-editor/IDE surface (project files), not engine behaviour")),
    ("project_open",               Some("visual-editor/IDE surface (project files), not engine behaviour")),
    ("project_close",              Some("visual-editor/IDE surface (project files), not engine behaviour")),
    ("project_list",               Some("visual-editor/IDE surface (project files), not engine behaviour")),
    ("project_read_file",          Some("visual-editor/IDE surface (project files), not engine behaviour")),
    ("project_write_file",         Some("visual-editor/IDE surface (project files), not engine behaviour")),
    ("project_create",             Some("visual-editor/IDE surface (project files), not engine behaviour")),
    ("project_rename",             Some("visual-editor/IDE surface (project files), not engine behaviour")),
    ("project_delete",             Some("visual-editor/IDE surface (project files), not engine behaviour")),
    ("project_save",               Some("visual-editor/IDE surface (project files), not engine behaviour")),
    ("project_load",               Some("visual-editor/IDE surface (project files), not engine behaviour")),
    ("project_export_zip",         Some("visual-editor/IDE surface (project files), not engine behaviour")),
    ("project_import_zip",         Some("visual-editor/IDE surface (project files), not engine behaviour")),
    ("export_code",                Some("codegen surface, not engine behaviour")),
    ("export_code_zip",            Some("codegen surface, not engine behaviour")),
    // AzBuilder's quick exports (layout/src/e2e/export.rs), pinned by the
    // export_tests module.
    ("get_codegen_languages",      Some("codegen surface, not engine behaviour")),
    ("get_css_rules",              Some("codegen surface, not engine behaviour")),
    ("compile_css",                Some("codegen surface, not engine behaviour")),
    ("html_to_code",               Some("codegen surface, not engine behaviour")),
    ("export_subtree_code",        Some("codegen surface, not engine behaviour")),
    ("export_component_code",      Some("codegen surface, not engine behaviour")),
    ("resolve_function_pointers",  Some("editor/codegen plumbing, not engine behaviour")),
    ("run_e2e_tests",              Some("the test runner itself — a test may not recurse into it")),
    ("get_logs",                   Some("debug-server tooling, asserts nothing about the engine")),
    // Writes an instrumented build's PGO counters (crate::pgo) - build
    // tooling; `false` and a no-op in every build a scenario runs in.
    ("dump_profile",               Some("PGO build tooling (writes profile counters), asserts nothing \
                                         about the engine")),
    // Routes through `e2e::hooks::take_native_screenshot_base64`, whose default
    // is `None` — and NOTHING in the workspace calls `set_host_hooks`, so the op
    // returns "native screenshot unavailable (no e2e host hook installed)" in
    // every runner we have. Verified by running it. Offering it would generate
    // tests that are red on arrival for a reason that has nothing to do with the
    // engine — the same false-signal class as a zombie op, in the other
    // direction. `take_screenshot` (CPU render, no hook) stays allowed.
    ("take_native_screenshot",     Some("needs an e2e host hook that nothing installs — always \
                                         fails headlessly, so a test using it is red on arrival")),
    ("open_file",                  Some("editor/host file I/O, outside the headless engine")),
    ("close",                      Some("tears the window down — ends the timeline, tests nothing")),

    // -- DENY 3: geometry queries ------------------------------------------
    // `azul-doc reftest` owns layout/geometry correctness. These are the side
    // door through which a generator smuggles a geometry assertion back in.
    ("get_node_layout",        Some("geometry — `azul-doc reftest` owns layout correctness")),
    ("get_all_nodes_layout",   Some("geometry — `azul-doc reftest` owns layout correctness")),
    ("get_layout_tree",        Some("geometry — `azul-doc reftest` owns layout correctness")),
    ("get_display_list",       Some("geometry — `azul-doc reftest` owns layout correctness")),
    ("get_virtual_view_layout",Some("geometry — `azul-doc reftest` owns layout correctness")),

    // -- ALLOW: the manager / composition / damage-soundness assertions ------
    // E2E_PLAN §(c)/(g1)/(g2)/(g3). Classified explicitly rather than left to
    // the `assert_*`-is-allowed fallback, because these four are the ones the
    // corpus was WRITTEN against and a silent reclassification would be
    // invisible. All four are real reads of `LayoutWindow` state that can fail;
    // none of them is a stub. `assert_composition` additionally needs the
    // per-step stage trace, so it only means anything inside a scenario run.
    ("assert_state_machines_idle", None),
    ("assert_manager_invariants",  None),
    ("assert_composition",         None),
    ("assert_damage_sound",        None),

    // -- DENY: assertions that leave the behaviour surface ------------------
    ("assert_layout",     Some("geometry — `azul-doc reftest` owns layout correctness")),
    ("assert_screenshot", Some("needs a reference PNG the generator cannot have; assert \
                                RELATIVELY, vs. an earlier snapshot")),
];

/// The verdict for one op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpClass {
    Allowed,
    Denied(DenyReason),
    /// In `DebugEvent`, but nobody classified it. Reported loudly; denied, so it
    /// can never be silently smuggled into a generated test.
    Unclassified,
}

/// Classify one op. `assert_*` ops are observation by construction (they read
/// engine state and can only fail a test), so any assertion not explicitly
/// denied above is allowed.
pub fn classify(op: &str) -> OpClass {
    match OP_POLICY.iter().find(|(n, _)| *n == op) {
        Some((_, None)) => OpClass::Allowed,
        Some((_, Some(why))) => OpClass::Denied(why),
        None if op.starts_with("assert_") => OpClass::Allowed,
        None => OpClass::Unclassified,
    }
}

fn snake(camel: &str) -> String {
    let mut out = String::new();
    for (i, c) in camel.chars().enumerate() {
        if c.is_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The longest an op's doc is in the prompt's schema section, in characters.
const OP_DOC_MAX_CHARS: usize = 300;

/// The doc of one `DebugEvent` variant as the prompt shows it: the `///` lines
/// up to the first blank one, joined into a single line, at most
/// [`OP_DOC_MAX_CHARS`] characters ([`cap_doc`]).
///
/// It used to be the first `///` LINE, which cut the sentences that matter:
/// `wheel`'s sign convention sat on the sixth line of its doc, `tick_ms` stopped
/// at "- WITHOUT", `wait_settled` and `mock` mid-sentence, and `scroll` had no
/// doc at all. A scenario that wheel-scrolled with `delta_y: +120` scrolled
/// nothing - down is -120 - and was green on a list that never moved (batch 3,
/// line 11).
#[derive(Debug, Default)]
struct DocParagraph {
    text: String,
    /// A blank `///` line has ended the paragraph; what follows is not shown.
    closed: bool,
}

impl DocParagraph {
    /// One `///` line, without the slashes.
    fn push(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            // Blank lines before the text are nothing; one after it ends the
            // paragraph.
            self.closed |= !self.text.is_empty();
            return;
        }
        if self.closed {
            return;
        }
        if !self.text.is_empty() {
            self.text.push(' ');
        }
        self.text.push_str(line);
    }

    /// The paragraph so far, capped, and an empty slate for the next variant.
    fn take(&mut self) -> Option<String> {
        let text = std::mem::take(&mut self.text);
        self.closed = false;
        (!text.is_empty()).then(|| cap_doc(&text, OP_DOC_MAX_CHARS))
    }
}

/// `doc` in at most `max` characters: whole when it fits, otherwise cut after
/// the last full sentence that fits when that keeps at least half the room,
/// else at the last whole word, with an ellipsis. Never in the middle of a word.
fn cap_doc(doc: &str, max: usize) -> String {
    if doc.chars().count() <= max {
        return doc.to_string();
    }
    // Room for the ellipsis.
    let end = doc
        .char_indices()
        .nth(max.saturating_sub(1))
        .map_or(doc.len(), |(i, _)| i);
    let head = &doc[..end];
    if let Some(dot) = head.rfind(". ") {
        if head[..=dot].chars().count() >= max / 2 {
            return head[..=dot].to_string();
        }
    }
    let words = head.rfind(' ').map_or(head, |space| &head[..space]);
    format!("{}…", words.trim_end())
}

/// Parse `DebugEvent` + the assertion dispatch out of `full.rs`.
///
/// This is deliberately a dumb line scanner rather than a `syn` parse: it only
/// needs variant names, field names and whether a field is optional
/// (`#[serde(default)]` or `Option<…>`), and it must keep working while the
/// enum grows.
pub fn parse_schema(project_root: &Path) -> Result<Schema> {
    let path = project_root.join(FULL_RS);
    let src = fs::read_to_string(&path)
        .with_context(|| format!("gen-e2e: cannot read the schema source {}", path.display()))?;

    // ---- 1. the DebugEvent enum -------------------------------------------
    let enum_start = src
        .find("pub enum DebugEvent {")
        .context("gen-e2e: `pub enum DebugEvent` not found in full.rs")?;
    let body = &src[enum_start..];

    let mut ops: Vec<OpDef> = Vec::new();
    let mut depth: i32 = 0;
    let mut cur: Option<OpDef> = None;
    let mut pending_doc = DocParagraph::default();
    let mut pending_default = false;
    let mut pending_rename: Option<String> = None;

    for line in body.lines().skip(1) {
        let t = line.trim();

        if depth == 0 {
            // Between variants: collect the doc comment + serde attrs.
            if let Some(d) = t.strip_prefix("///") {
                pending_doc.push(d);
                continue;
            }
            if t.starts_with("#[") || t.is_empty() || t.starts_with("//") {
                continue;
            }
            if t == "}" {
                break; // end of enum
            }
            // `Variant,` (unit) or `Variant {` (struct)
            let ident: String = t
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if ident.is_empty() || !ident.starts_with(char::is_uppercase) {
                continue;
            }
            let def = OpDef {
                name: snake(&ident),
                params: Vec::new(),
                doc: pending_doc.take(),
            };
            if t.contains('{') {
                depth = 1;
                cur = Some(def);
            } else {
                ops.push(def);
            }
            continue;
        }

        // Inside a struct variant: fields.
        if t.starts_with("#[serde") {
            if t.contains("default") {
                pending_default = true;
            }
            if let Some(r) = t
                .split("rename = \"")
                .nth(1)
                .and_then(|s| s.split('"').next())
            {
                pending_rename = Some(r.to_string());
            }
            continue;
        }
        if t.starts_with("///") || t.starts_with("//") || t.starts_with("#[") || t.is_empty() {
            continue;
        }
        if t.starts_with('}') {
            depth = 0;
            if let Some(c) = cur.take() {
                ops.push(c);
            }
            pending_default = false;
            pending_rename = None;
            continue;
        }
        if let Some((name, ty)) = t.split_once(':') {
            let name = pending_rename
                .take()
                .unwrap_or_else(|| name.trim().to_string());
            let optional = pending_default || ty.trim_start().starts_with("Option<");
            pending_default = false;
            if let Some(c) = cur.as_mut() {
                c.params.push((name, !optional));
            }
        }
    }

    // ---- 2. the assertion dispatch ----------------------------------------
    // `"assert_foo" => eval_assert_foo(params, …)` — then read the params the
    // eval fn actually looks at (`params.get("x")`).
    let mut asserts: Vec<OpDef> = Vec::new();
    for line in src.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("\"assert_") else {
            continue;
        };
        let Some(name) = rest.split('"').next() else {
            continue;
        };
        if !t.contains("=>") {
            continue;
        }
        let op = format!("assert_{name}");
        if asserts.iter().any(|a| a.name == op) {
            continue;
        }
        let params = eval_fn_params(&src, &format!("eval_{op}"));
        asserts.push(OpDef {
            name: op,
            params,
            doc: None,
        });
    }
    if asserts.is_empty() {
        bail!("gen-e2e: no assert_* ops found in full.rs — the dispatch shape changed");
    }

    // ---- 2b. WHICH VARIANTS ACTUALLY HAVE A MATCH ARM ----------------------
    // The dispatch is `match request.event { DebugEvent::Foo {..} => {…} … _ =>
    // { log("Unhandled"); send_ok() } }`. A variant with no arm hits that
    // catch-all and answers OK WITHOUT DOING ANYTHING — a test against it is
    // vacuously green. Detect it exactly the way the enum itself is detected:
    // an arm head is a line whose first token is `DebugEvent::<Variant>` (the
    // head may then continue over several lines for a wide field list, so we
    // must NOT require `=>` on the same line).
    let mut handled: BTreeSet<String> = BTreeSet::new();
    for line in src.lines() {
        let Some(rest) = line.trim_start().strip_prefix("DebugEvent::") else {
            continue;
        };
        let ident: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !ident.is_empty() {
            handled.insert(snake(&ident));
        }
    }
    // Sanity: if the catch-all ever disappears (every variant handled, or the
    // dispatch restructured), say so rather than silently trusting the scan.
    if !src.contains("Unhandled:") {
        eprintln!(
            "!! [gen-e2e] the `_ => Unhandled` catch-all is gone from {FULL_RS}; re-check the \
             zombie-op scan in gene2e.rs::parse_schema"
        );
    }
    if handled.is_empty() {
        bail!(
            "gen-e2e: no `DebugEvent::` match arms found in full.rs — the dispatch shape changed"
        );
    }

    // ---- 3. ops the E2E step loop handles itself (not DebugEvent variants) --
    // `assert_response` is dispatched inside the step loop (it reads the previous
    // step's response payload), not as a DebugEvent arm — so the schema scanner
    // never sees it. Without it here, any GENERATED test using the get_* / query
    // family (which pairs a query op with `assert_response`) fails validation.
    let extra: Vec<OpDef> = [
        "commit_undo_snapshot",
        "undo_app_state",
        "redo_app_state",
        "assert_response",
        // Holds the window idle for N frames in ONE step: it moves the clock and waits
        // for frames, which an `eval_assert_*` fn cannot (E2E-M2).
        "assert_idle_frames",
    ]
    .into_iter()
    .filter(|o| src.contains(&format!("\"{o}\"")))
    .map(|o| OpDef {
        name: o.to_string(),
        params: step_loop_op_params(&src, o),
        doc: None,
    })
    .collect();

    // Blind-spot alarm. `schema_doc` renders "(no params)" for anything this
    // scanner came up empty on, and the prompt says params not listed do not
    // exist — so an assertion whose eval fn CLEARLY reads params but reported
    // none is a scanner gap that silently un-narrows a whole assertion class
    // (this is exactly how `assert_manager_invariants` lost `managers`/`cross`).
    // Shout instead of shipping the lie.
    for a in &asserts {
        if scanner_missed_params(&src, a) {
            eprintln!(
                "!! [gen-e2e] `{}` reads `params` in full.rs but the schema scan extracted NONE — \
                 the prompt will advertise it as `(no params)` and the model cannot narrow it. \
                 Extend gene2e.rs::eval_fn_params.",
                a.name
            );
        }
    }

    Ok(Schema {
        ops,
        asserts,
        extra,
        handled,
    })
}

/// The blind-spot alarm's test: the scan extracted no params for assertion
/// `a`, yet its eval fn reads `params` for more than its guard.
///
/// Only the fn's BODY counts (its signature always names `params`), and the
/// `reject_unknown_params(.., params, &[..])` guard does not: when the scan
/// found nothing, the guard's list is empty (`reject_guard_keys` reads it), and
/// an empty guard is the fn declaring that it takes no params - so the empty
/// scan is right (`assert_no_unmocked_requests`).
fn scanner_missed_params(src: &str, a: &OpDef) -> bool {
    if !a.params.is_empty() {
        return false;
    }
    let Some(body) = top_level_fn_body(src, &format!("eval_{}", a.name)) else {
        return false;
    };
    let Some(open) = body.find('{') else {
        return false;
    };
    let mut rest = &body[open + 1..];
    let mut outside_guards = String::new();
    while let Some(p) = rest.find("reject_unknown_params(") {
        outside_guards.push_str(&rest[..p]);
        // The guard's `params` argument sits before its `&[..]` allow-list.
        let guard = &rest[p..];
        rest = &guard[guard.find(']').map_or(guard.len(), |e| e + 1)..];
    }
    outside_guards.push_str(rest);
    outside_guards.contains("params")
}

/// The body of the top-level `fn <name>(…)` in `src`, or `None`.
///
/// "Top-level" = declared at column 0; the body ends at the first line that is
/// exactly `}` at column 0.
fn top_level_fn_body<'a>(src: &'a str, fn_name: &str) -> Option<&'a str> {
    let start = src.find(&format!("\nfn {fn_name}("))?;
    let body = &src[start + 1..];
    let end = body.find("\n}\n").map_or(body.len(), |e| e + 2);
    Some(&body[..end])
}

/// Every `<recv>.get("…")` key in `body`, in source order, deduplicated.
fn literal_get_keys(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = body;
    while let Some(p) = rest.find(".get(\"") {
        rest = &rest[p + 6..];
        if let Some(k) = rest.split('"').next() {
            if !out.iter().any(|n| n == k) {
                out.push(k.to_string());
            }
        }
    }
    out
}

/// Param keys an eval fn reads INDIRECTLY, through a local reader closure.
///
/// `eval_assert_manager_invariants` is the motivating case:
///
/// ```ignore
/// let list = |key: &str, default: &[&str]| { match params.get(key) { … } };
/// let managers = list("managers", KNOWN_MANAGERS);
/// let cross    = list("cross",    KNOWN_CROSS);
/// ```
///
/// There is no `params.get("…")` literal anywhere in that function, so the
/// literal scan returns NOTHING and the prompt advertises the assertion as
/// `(no params)` — the model then cannot narrow it to a manager or an
/// invariant and emits the broadest possible form, every time.
///
/// So: find each `let <ident> = |<first>: …` closure whose body reads
/// `params.get(<first>)`, then harvest the string literal each `<ident>("…"`
/// call site passes.
fn closure_relayed_keys(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (i, _) in body.match_indices("let ") {
        let after = &body[i + 4..];
        let Some(eq) = after.find(" = |") else {
            continue;
        };
        let name = after[..eq].trim();
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        // First closure parameter name: `|key: &str, …|` → `key`.
        let params_start = eq + 4;
        let Some(bar) = after[params_start..].find('|') else {
            continue;
        };
        let arg = after[params_start..params_start + bar]
            .split(',')
            .next()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("")
            .trim();
        if arg.is_empty() {
            continue;
        }
        // Does the closure actually relay that argument into `params.get()`?
        if !after[params_start + bar..].contains(&format!("params.get({arg})")) {
            continue;
        }
        // Harvest the literal first argument of every call to it.
        let call = format!("{name}(\"");
        let mut rest = body;
        while let Some(p) = rest.find(&call) {
            rest = &rest[p + call.len()..];
            if let Some(k) = rest.split('"').next() {
                if !out.iter().any(|n| n == k) {
                    out.push(k.to_string());
                }
            }
        }
    }
    out
}

/// The allow-list an eval fn declares by calling
/// `reject_unknown_params("assert_x", params, &["a", "b"])` (`full.rs`).
///
/// That list IS the assertion's param surface — the guard fails the assertion on
/// anything else — but several of those keys are read through a HELPER rather
/// than in the eval fn's own body (`damage_of` reads `which` / `frame`), so the
/// `params.get("…")` scan never saw them and the prompt advertised a narrower
/// set than the engine accepts. Reading the guard is exact by construction:
/// accepted-at-runtime and advertised-to-the-model are the same list.
fn reject_guard_keys(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = body;
    while let Some(p) = rest.find("reject_unknown_params(") {
        rest = &rest[p + "reject_unknown_params(".len()..];
        let Some(open) = rest.find("&[") else { break };
        let Some(close) = rest[open..].find(']') else {
            break;
        };
        let list = &rest[open..open + close];
        for key in list.split('"').skip(1).step_by(2) {
            if !out.iter().any(|n| n == key) {
                out.push(key.to_string());
            }
        }
        rest = &rest[open + close..];
    }
    out
}

/// Param keys that are not fixed names at all: the eval fn walks
/// `params.as_object()` and looks each key up in a map built by a `collect_*`
/// helper (`eval_assert_resource_counts` × `collect_resource_counts`). The
/// assertion's real param surface is that map's KEY SET, which lives in the
/// helper's `insert("…"` calls.
fn collected_map_keys(src: &str, body: &str) -> Vec<String> {
    if !body.contains("params.as_object()") {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (i, _) in body.match_indices("collect_") {
        let suffix: String = body[i + 8..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let name = format!("collect_{suffix}");
        let Some(helper) = top_level_fn_body(src, &name) else {
            continue;
        };
        let mut rest = helper;
        while let Some(p) = rest.find(".insert(\n") {
            // `out.insert(\n    "key".to_string(),` — the literal is on the
            // next line.
            rest = &rest[p + 9..];
            let Some(q) = rest.find('"') else { break };
            if let Some(k) = rest[q + 1..].split('"').next() {
                if !out.iter().any(|n: &String| n == k) {
                    out.push(k.to_string());
                }
            }
        }
        let mut rest = helper;
        while let Some(p) = rest.find(".insert(\"") {
            rest = &rest[p + 9..];
            if let Some(k) = rest.split('"').next() {
                if !out.iter().any(|n: &String| n == k) {
                    out.push(k.to_string());
                }
            }
        }
    }
    out
}

/// Every param key `fn <name>(…)` reads — directly, through a local reader
/// closure, or as a dynamic key looked up in a `collect_*` map.
///
/// A key this misses is a key the generator NEVER SEES: `schema_doc` renders
/// the extracted list verbatim into the prompt, and "Params NOT listed here do
/// not exist — do not invent any" is right underneath it. So an under-reported
/// assertion cannot be narrowed at all.
fn eval_fn_params(src: &str, fn_name: &str) -> Vec<(String, bool)> {
    let Some(body) = top_level_fn_body(src, fn_name) else {
        return Vec::new();
    };

    let mut keys = literal_get_keys(body);
    for k in closure_relayed_keys(body) {
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    for k in collected_map_keys(src, body) {
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    for k in reject_guard_keys(body) {
        if !keys.contains(&k) {
            keys.push(k);
        }
    }

    keys.into_iter()
        .map(|k| {
            // every assertion param is read with `if let Some(..)` = optional,
            // except `vs`/`selector`/`expected`, which the eval fns hard-require.
            let required = matches!(k.as_str(), "selector" | "expected" | "reference");
            (k, required)
        })
        .collect()
}

/// Param keys read by a block the E2E STEP LOOP handles itself, i.e.
/// `if op == "<name>" { … step.params.get("…") … }`. Those ops have no
/// `DebugEvent` variant and no `eval_*` fn, so neither of the two scanners
/// above ever sees them — `assert_response` (`type` / `contains`), which every
/// `get_*` query in the corpus is paired with, was advertised as
/// `(no params)`.
fn step_loop_op_params(src: &str, op: &str) -> Vec<(String, bool)> {
    let Some(start) = src.find(&format!("if op == \"{op}\" {{")) else {
        return Vec::new();
    };
    let body = &src[start..];
    // The block is inside the step loop, so it is NOT at column 0; bound the
    // scan at the matching closing brace by tracking depth.
    let mut depth = 0i32;
    let mut end = body.len();
    for (i, c) in body.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &body[..end];
    literal_get_keys(body)
        .into_iter()
        .filter(|k| k != "op" && k != "screenshot")
        .map(|k| (k, false))
        .collect()
}

// ===========================================================================
// The prompt
// ===========================================================================

/// `(no params)`, or `a, b?, c?` — required first-class, optional with `?`.
fn render_params(params: &[(String, bool)]) -> String {
    if params.is_empty() {
        return "(no params)".to_string();
    }
    params
        .iter()
        .map(|(n, req)| if *req { n.clone() } else { format!("{n}?") })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The schema section of the agent prompt — rendered from the parsed `full.rs`,
/// FILTERED THROUGH `OP_POLICY` (only ALLOWED ops are ever shown) and through the
/// ZOMBIE scan (an op with no match arm does nothing, so it is not offered
/// either). A model cannot use what it is not shown; the gate then enforces
/// both rules for real (see `validate`).
fn schema_doc(schema: &Schema) -> String {
    let mut s = String::new();
    s.push_str("### TIMELINE OPS (`{\"op\": \"<name>\", …}`)\n");
    for op in schema
        .ops
        .iter()
        .filter(|o| classify(&o.name) == OpClass::Allowed && !schema.is_zombie(&o.name))
    {
        let params = render_params(&op.params);
        match &op.doc {
            Some(d) => s.push_str(&format!("- {} : {}   // {}\n", op.name, params, d)),
            None => s.push_str(&format!("- {} : {}\n", op.name, params)),
        }
    }
    for e in schema
        .extra
        .iter()
        .filter(|e| classify(&e.name) == OpClass::Allowed)
    {
        s.push_str(&format!("- {} : {}\n", e.name, render_params(&e.params)));
    }
    s.push_str("\n### ASSERTIONS\n");
    for a in schema
        .asserts
        .iter()
        .filter(|a| classify(&a.name) == OpClass::Allowed)
    {
        s.push_str(&format!("- {} : {}\n", a.name, render_params(&a.params)));
    }
    s.push_str(
        "\n`?` = optional. Params NOT listed here do not exist — do not invent any.\nThe op list \
         above is EXHAUSTIVE: an op you do not see above is REJECTED by the validator, and your \
         test is thrown away. In particular there is NO op that forces a repaint or a relayout — \
         the engine must decide to do that BY ITSELF in response to the input/mutation you \
         perform; that decision is exactly what these tests measure.\n`vs` always names a \
         snapshot created EARLIER in the same timeline by `snapshot_frame {\"as\": …}` (pixels), \
         `snapshot_resources {\"as\": …}` (resource counters) or `snapshot_managers {\"as\": …}` \
         (every manager's state, for `assert_only_managers_changed`).\n",
    );
    s
}

fn build_prompt(schema_doc: &str, example: &str, line: &str) -> String {
    format!(
        r#"You are writing ONE azul e2e test file, as JSON, from a one-line description.
Print ONLY the JSON object. No prose, no markdown fences, no explanation.

## THE TEST TO WRITE
{line}

## WHAT AN AZUL E2E TEST IS
A JSON object: {{"name", "description", "setup", "steps"}}.
- "name": a short snake_case identifier of the form <behaviour>_<property>_<fixture> that
  names what THIS line asserts, of what, on which fixture (for example
  mouse_move_converges_image_flex_row). Add no number or hash: a suffix that makes the name
  unique is appended for you.
- "description": one sentence — restate the one-liner.
- "setup": {{"window_width": 400, "window_height": 300, "dpi": 96}}
- "steps": a TIMELINE of ops, executed in order, against a real headless azul window.

The first step is almost always `mount`, which installs an inline HTML+CSS document
as the window's DOM. `html` and `css` are ARRAYS OF LINES (one JSON string per source
line) so the test stays human-readable — NOT one escaped mega-string.

## THE SCHEMA (this is the complete, actual op set — nothing else exists)
{schema_doc}

## A REAL TEST (the ground truth for the FORMAT — copy its shape, not its bounds)
```json
{example}
```

## A FAILING TEST IS A SUCCESS — DO NOT TUNE FOR GREEN
You are writing a SPECIFICATION of correct engine behaviour, not a description of
what the engine currently does. Nothing runs your test before it is accepted: the
checker reads the file (its shape, the ops and params, the values and node ids the
engine would refuse outright) and never executes it. So you will never be told whether
it passed, and you must not try to guess.

Assert what the engine SHOULD do. If the engine is buggy, your test fails — that is
the POINT of this corpus and a genuinely useful result. A test that passes because
you softened it until it could not fail is WORSE THAN NO TEST: it hides the bug and
lends false confidence forever after.

Concretely:
- Pick the bound the one-liner IMPLIES, not a safe-looking one. "does not re-run
  layout" is `exact_layout_passes: 0` — never 1 "just in case". "returns to idle" is
  `assert_idle_stable` with damage `none` — never "some small damage is probably OK".
- AN UPPER BOUND ALONE IS SATISFIED BY ZERO. `max_relayouts: 2` is true of an engine
  that dropped the input on the floor and did nothing at all. Whenever the line says
  the step DID something, pin the lower end too, and copy the line's own quantifier:
  "at least one" is `min_*: 1` and nothing else; "at most one" is `max_*: 1` (plus `min_*`
  only if the line also says it happened); "exactly one" or "once" is `exact_*: 1`. Never
  upgrade "at least" to `exact_*` and never add an upper bound the line did not give.
- WHEN THE LINE ASSERTS AN ABSENCE, ADD THE CONTROL IT ASKS FOR. "no damage", "no
  layout pass", "the counter did not move" is also exactly what a dead engine
  reports. Most such one-liners spell the control out ("... then change the node's
  background-color as a POSITIVE CONTROL and assert THAT does produce damage"). Put
  it in the SAME timeline, after the absence assertion — never in a second file — and
  MEASURE IT ON ITS OWN: right before the control's mutation call `reset_frame_counters`
  and take a fresh `snapshot_frame` (`ctl_before`), and assert the control against THAT
  snapshot. Without the reset the control's counters and damage still hold what the
  absence step left behind, and a control that did nothing could pass on that.
- A CONTROL THAT CANNOT MOVE A PIXEL PROVES NOTHING: its `assert_changed` fails as a DEAD
  CONTROL, the test's fault and not the engine's. Nothing will run your file, so prove each
  control from your own stylesheet BEFORE you write it: name the pixels that change and why.
  If you cannot, change the FIXTURE, not the property. The fixture the line names is a
  minimum: add a child, a sibling, a second flex item, a background or a backdrop colour to
  make the control visible. What each property needs:
    - background-color: the new colour differs from the old one and from what is behind the
      node (a body background paints the whole window: white-on-white and red-on-red change
      nothing).
    - width / height: a FIXED-size painted box on a backdrop of another colour; never a
      `flex-grow:1` item (it ignores an overridden `width`), never a box that already fills
      the window.
    - border-width: `border-style: solid` and a border colour unlike the box background.
    - padding / margin: a painted child the change moves or resizes, or a painted box
      smaller than the window.
    - display:none / visibility:hidden: the node must PAINT something itself (a background);
      an `<img>` paints nothing of its own (step 1 below).
    - overflow: build exactly this shape. A window-sized frame (`#frame {{ width: 400px; height:
      300px; background: white; }}`) holds the node under test at a SMALLER fixed size (`#box {{
      width: 200px; height: 100px; background: red; }}`), and the node holds a painted child that
      is LARGER than it in at least one axis (`#kid {{ width: 100px; height: 200px; background:
      blue; flex-shrink: 0; }}`), so `overflow: visible` shows the child hanging out of the box and
      `overflow: hidden` cuts it off. A node as large as its frame, or whose child lies wholly
      inside it, shows no change in either direction.
    - position (static -> absolute): a painted sibling FOLLOWS the node in the flow, so it
      jumps into the gap. An absolute box with an explicit size and no offsets stays where
      it was.
    - justify-content / align-items / flex-direction / flex-grow: at least TWO painted flex
      items that do not fill the container along the axis the property acts on (with one
      item, `space-between` equals `flex-start`).
  If the line names a control property that NO node of ANY fixture you can build shows, add the
  top-level key "pending": "control cannot be made visible: <why>" and write the closest honest
  test. Before you give up, move the override to the node that CAN show the property: the flex
  row or a wrapper you add around the named node. An `<img>` has no children, but the row that
  holds it does: put `overflow` on the row and let a taller sibling stick out of it.
- The numbers in the recipes below are ILLUSTRATIVE placeholders. Derive each bound
  from the sentence you were given; do not copy them by default.
- Never widen a bound, drop an assertion, or downgrade `eq` to `le` to make the test
  feel safer. If the description implies an exact count, assert equality.
- Never add an assertion the line does not ask for merely to make the file look
  substantial. Each assertion must trace to the one-liner. A control asserts exactly what
  the line names: "produces damage and changes pixels" is `assert_changed` alone; "spends a
  layout pass" is `assert_work_bounded {{"min_layout_passes": 1}}` alone. Do not append
  coverage, soundness or work-counter assertions the line does not name, and never
  `min_layout_passes` on a paint-only change.
- If the described behaviour cannot be expressed with the ops above, write the
  closest HONEST test and let it fail — do NOT substitute a weaker property that
  happens to hold.

## SCOPE — THE ONE RULE YOU MUST NOT BREAK
Assert BEHAVIOUR: damage, redraw, repaint liveness/soundness, settling to idle,
bounded work, resource counts, focus/scroll/selection state, "nothing panics".
NEVER assert geometry or layout correctness — no "node X is at (10,20)", no
"width == 60". `assert_layout` is FORBIDDEN in generated tests (`azul-doc reftest`
owns layout correctness). You must NOT need to know, compute or guess any expected
pixel coordinate, size, colour or screenshot: every assertion must be about the
ENGINE's behaviour, expressed RELATIVELY (vs. a snapshot you took earlier in the
same timeline). `assert_screenshot` is likewise forbidden — it needs a reference PNG
you cannot have.

NEVER force the effect you are testing. A test may DRIVE the engine (mock input, or a
DOM/state mutation of the kind an app callback performs) and then OBSERVE what the
engine decided to do. It may never tell the engine to repaint or to re-layout: that
decision IS the thing under test, and forcing it would make the test pass even with the
invalidation path completely broken. Only the ops listed above exist; use nothing else.

## HOW TO TURN THE ONE-LINER INTO A TIMELINE
1. `mount` the DOM the line describes (invent plausible, minimal HTML+CSS for it).
   `html` must contain at least one element: an "empty body" is ["<body></body>"], never []
   (the checker rejects [], and `mount` fails with "'html' is empty"). Give every element
   the line talks about an `id` (`<div id="box">`): you will name it by selector.
   "Filling the window" means explicit pixels equal to the window (width:400px;height:300px)
   or `html, body {{ margin:0; width:100%; height:100% }}` together with width/height 100% on
   the root; a root with height:100% under an html/body that has no height collapses to 0px
   and paints nothing. A `<body>` background is propagated to the whole window, so on an
   empty body the only visible control is recolouring `<html>`/`<body>`; changing the body's
   width changes nothing. `<img>` mounts as a real image node, but its placeholder raster is
   transparent: an `<img>` paints NOTHING until it has a `background-color` or you call
   `set_node_image` on it (after the `wait_frame` that follows the `mount`). A page whose
   only content is an `<img>` renders blank white, and nothing about such an image can move
   a pixel.
   A VIRTUALVIEW (a list too long to build) is a key of the `mount`, never markup:
   `<virtual-view>`, `<virtualview>`, `<iframe>` and `data-virtual-*`/`rows` attributes mount as
   a plain `<div>` with the attributes dropped. Write the host as an EMPTY element, size it in the
   `css`, and name it in `virtual_views` (the mount is refused at its step when the selector
   matches no element or several, or the element has content):
       {{"op":"mount","collapse_whitespace":true,"html":["<div class=\"list\"></div>"],
         "css":[".list {{ width:300px; height:200px; overflow:auto; }}"],
         "virtual_views":[{{"selector":".list","rows":5000,"row_height":24,
                           "row_html":["<div class=\"label\">Row {{n}}</div>"],
                           "row_css":[".label {{ font-size:14px; }}"]}}]}}
   Row `i` (`{{i}}` counts from 0, `{{n}}` from 1) sits in `<div class="e2e-row" id="row-{{i}}">` of
   `row_height` px. Only the visible rows and a screen of rows either side exist - a few dozen
   nodes, not 5000 - and the view builds more as it scrolls near an edge of what it built
   (`scroll_node_by` or `wheel` on the host). The rows are the VIEW's document: the page's `css`
   does not reach them (that is what `row_css` is for), a `selector` finds the host and never a
   row, and the host is the node to scroll and to give a `set_node_css_override`. Look at the
   view with `get_virtual_view_states` and `assert_response` `contains`: its
   `virtual_scroll_size_height` is the whole document (rows x row_height), its
   `scroll_size_height` the rows it has built (a few dozen x row_height, never the 5000) and
   `was_invoked` says its callback ran.
   FONTS: for any text, use only the built-in mock fonts — `Azul Mock Mono`
   (0.5em advance) or `Azul Mock Wide` (1.0em advance). They are registered
   automatically, always resolve, and need no @font-face. If a case needs N
   DISTINCT families, invent N distinct names but ALWAYS end the stack with a mock
   font, e.g. `font-family: MyFakeFamilyA, "Azul Mock Mono";`. NEVER name a real
   system font (Arial, Helvetica, Times, Courier, Verdana): on the CI box they match
   nothing and collapse onto one shared FontId, which makes font-identity and
   leak/font assertions vacuously green.
2. `wait_frame`, then `wait {{"ms": 100}}` to let the first frame settle. A `mount` only
   takes effect at the next frame: every step that names a node (`set_node_*`, `insert_node`,
   `delete_node`, `click`, `focus_node`, `mouse_move` with a selector, ...) comes AFTER that
   `wait_frame`, or the op fails with "Node N not found" (the checker rejects the file).
3. `reset_frame_counters`, then `snapshot_frame {{"as": "before"}}` and, if you will
   assert resource counts, `snapshot_resources {{"as": "baseline"}}`.
4. Perform the interaction / mutation / CSS change the line describes (click,
   scroll, key_down, text_input, resize, set_node_css_override, insert_node,
   delete_node, set_node_classes, a second `mount` with changed markup, …).
   KEYBOARD EDITING NEEDS DOM FOCUS FIRST. `text_input` hard-errors with "No focused
   node - text input requires focus on contenteditable" unless some node holds DOM
   focus, and the `focus` / `blur` ops are WINDOW focus — they set `window_focused`
   and nothing else. Use `focus_node {{"selector": ".editor"}}` immediately before
   typing. Do NOT substitute a click: a click needs a coordinate, and you may not
   guess coordinates.
5. `wait_frame` + `wait` (and `tick_ms` for anything time-driven: momentum,
   fade, blink, animation — `tick_ms` advances the engine clock WITHOUT sleeping).
   A window of "identical frames" must not overlap time-driven state: take the first
   snapshot only AFTER it has finished.
   A wheel, a drag or a hover over a scroller makes its overlay scrollbar visible, and that
   STARTS A FADE: 500 ms visible, then 200 ms fading. A box that declares
   `-azul-scrollbar-fade-delay` and/or `-azul-scrollbar-fade-duration` fades by them instead (each
   alone: a box that sets only the delay keeps the 200 ms; a delay of 0 is a bar that never fades,
   a duration of 0 a bar that is gone the moment the delay is up), and `scrollbar-width`/
   `scrollbar-color` configure no fade at all. So between such an input and the first
   `reset_frame_counters`/`snapshot_frame` of an idle chain ALWAYS put four `tick_ms` 250 ms, each
   followed by `wait_frame` (together more than the box's delay + duration when it declares
   them). That preamble is not one of the steps the line counts: "wheel-scroll, then advance the
   clock in 6 steps of 150ms" still gets it, and the 6 steps follow it.
   `wait_settled` is the other way to that rest: it moves the clock itself, a step at a time (a
   scrollbar still to fade out counts, and the step runs to the end of its fade), until nothing in
   the window moves any more, or `timeout_ms` of ENGINE time (default 3000) is spent and the step
   fails naming what still moved. No `wait_frame` is needed after it.
   SCROLLING. `wheel` is a RAW hardware notch: a NEGATIVE `delta_y` scrolls the content DOWN
   (`{{"op":"wheel","x":100,"y":100,"delta_x":0,"delta_y":-120}}` leaves the offset at 120 once it
   has glided); a positive `delta_y` scrolls UP, which at the top of a list moves nothing.
   `scroll` and `scroll_node_by` are programmatic and take a POSITIVE delta for "down";
   `scroll_node_to` takes the absolute offset. A wheel glides on a timer: follow it with
   `tick_ms` + `wait_frame` before you read the offset. A timeline that needs a list to have
   scrolled must PROVE it with `assert_scroll {{"selector": ".list", "y": 120}}` after the glide,
   or it stays green on a list that never moved.
6. Assert what the line asks for:
   - "the pixels change" / liveness      -> assert_changed  {{"vs": "before", "min_damage_rects": 1}}
   - "damage covers the change" / sound  -> assert_damage_covers_changes {{"vs": "before"}}
   - "a patch, not a full redraw"        -> assert_damage_incremental {{"max_area_ratio": 0.5}}
   - "returns to idle / zero damage"     -> tick_ms, wait, then assert_idle_stable {{"vs": "<a snapshot_frame taken after the change>"}}
   - "EVERY frame identical to the PREVIOUS one" / "stays idle for N frames" / "identical at
     frames 1, 10, 20 and 40"
     -> ONE step, NOT a snapshot / tick / wait_frame / assert quadruple per frame:
            {{"op":"assert_idle_frames","frames":40,"step_ms":16}}
        It starts the frame counters afresh, then N times moves the engine clock by `step_ms`
        (default 16), waits for the frame that follows and holds it to the window as the step
        FOUND it (or to `"vs": "<a snapshot_frame>"`; `"threshold"` is a per-channel pixel
        tolerance, default 0): no paint or present damage so far, a full repaint with the same
        pixels, and a frame rendered for the turn at all. Every frame is held to the SAME
        picture, so an engine that goes A -> B -> A -> B fails at the first B (a lone
        `assert_idle_stable vs f0` after the last tick would pass it). It names the first
        frame that fails ("frame 7 of 40 (+112 ms)"); the frames at 1, 10, 20 and 40 of a line
        are `"frames": 40` - nothing is sampled. Take the window to rest first (step 5), and
        follow it with `assert_work_bounded` when the line also says no layout ran: the
        counters it reads are those of exactly these frames.
   - "settles WITHIN N frames"           -> assert_idle_stable {{"vs": "<snapshot>", "max_frames": N}}
     `max_frames` counts frames since the `reset_frame_counters` of step 3, so put
     that reset immediately before the interaction and the budget is real. Without
     it "within 5 ticks" is prose: nothing stops a 20-frame settle from passing.
   - "damage covers the change AND is not a full redraw"
                                         -> assert_damage_sound {{"vs": "before", "forbid_full": true}}
     The ONLY assertion that checks soundness (every changed pixel is inside a damage
     rect) and tightness (it is not a whole-window repaint) in one call. Prefer it
     whenever the line claims both, which "the repaint is sound and tight" does.
   - "every state machine settled / nothing is latched"
                                         -> assert_state_machines_idle {{}}
     One sweep over: no active drag, no un-ended gesture session, no running scroll
     animation, `scrollbar_fade_active` cleared, no latched `display_list_dirty`, no
     orphan caret blink, no unresolved focus request.
     Strictly stronger than `assert_idle_stable` for "did every manager notice the
     interaction ended?".

   ### THE THREE WORK COUNTERS — pick the RIGHT one, they measure different things
   `assert_work_bounded` reads three independent counters. Choosing the wrong one is
   the single most common way to write a test that cannot fail:

   | counter          | what it counts                          | idle | inert pointer event | set_node_css_override        | resize |
   |------------------|-----------------------------------------|------|---------------------|------------------------------|--------|
   | `relayouts`      | EVENT PASSES (`process_window_events`)  | 0    | 1                   | 0                            | 1      |
   | `dom_regens`     | runs of the layout callback             | 0    | 0                   | 0                            | 1      |
   | `layout_passes`  | times LAYOUT ACTUALLY RAN               | 0    | 0                   | 0 paint-only / 1 geometry    | 1      |
   | `virtual_view_size_passes` | extra passes content-sized VirtualViews cost | 0 | 0            | 0                            | 0      |

   - `relayouts` is NOT a layout counter, despite the name. It is the recursion depth
     of the event pass. `0` means "no state delta was processed" — what an IDLE frame
     looks like. Any input that changes window state runs exactly ONE pass, so
     `max_relayouts: 0` after a click is a test that ALWAYS FAILS against a correct
     engine. `> 1` is the engine's own invalidation-loop signal.
   - `layout_passes` is the one that sees a CALLBACK-API mutation. `set_node_css_override`,
     `set_node_text` and `set_node_classes` never enter the event pass at all, so
     `relayouts` stays `0` for them.
   - What a CSS override costs in `layout_passes` depends on the PROPERTY and on whether the
     value changed. A paint-only property (color, background-color, background-*,
     opacity, transform, border-*-color/-style/-radius, box-shadow, text-shadow, filter,
     cursor) rebuilds the display list only and costs 0 layout passes, correctly, so never
     put `min_layout_passes` on a recolour. Any other property (width, height, margin,
     padding, border-width, display, position, overflow, visibility, flex-*,
     justify-content, align-items, font-*) runs layout: one pass per override, like
     `set_node_classes`, `mount` and `resize`.
   - Re-setting the value a node already resolves to (the stylesheet's `width: 200px`
     written again as 200px) is a no-op: the write is recorded, nothing is laid out and
     nothing is repainted. So "a no-op CSS write must not re-run layout" is
     `exact_layout_passes: 0`: write it exactly like that. The positive control "does spend
     a layout pass" is `min_layout_passes: 1` and nothing more: never `exact_layout_passes: 1`
     or `max_layout_passes: 1` unless the line itself says "exactly one" / "at most one".
   - `set_node_classes` re-matches the stylesheet against the node's new classes: the rules
     of the classes it has now apply and those of the classes it lost do not (a colour-only
     swap repaints, so it is a valid positive control, and a `.b:hover` rule is the one the
     pointer finds afterwards). The window lays out once. Writing the classes (and id) the
     node already has is a no-op like the re-set value above: zero layout passes, no damage.
   - `virtual_view_size_passes` counts the ONE extra pass a content-sized VirtualView
     (a status-bar label, an icon view - no stated width/height) costs when its
     callback reports a size the solver has not seen: on its first frame, and on a
     re-render whose content changed size. Steady state is `0`; a re-render that keeps
     the size is `0`; one per keystroke is a view that never converges.
   - Rows: "does not re-run layout"      -> assert_work_bounded {{"exact_layout_passes": 0}}
           "the label re-render re-sized its box once" -> assert_work_bounded {{"exact_virtual_view_size_passes": 1}}
           "costs exactly one layout pass"-> assert_work_bounded {{"exact_layout_passes": 1}}
           "one event pass, no re-entry" -> assert_work_bounded {{"exact_relayouts": 1, "max_dom_regens": 0}}
           "bounded, and it DID run"     -> assert_work_bounded {{"min_relayouts": 1, "max_relayouts": 2}}
           "never trips the depth cap"   -> any assert_work_bounded (it fails on `hit_depth_cap` unless `allow_depth_cap`)

   - "no leak / counters return"         -> assert_resource_counts {{"vs": "baseline", "parsed_fonts": "eq", "font_chain_cache": "eq", "font_hash_to_families": "eq"}}
   - "the counter PROVABLY moved" (a control): the control must ADD to what the baseline
     holds. A re-mount that REPLACES the baseline's text with text in another font stack
     releases the old font, so `parsed_fonts` and `fonts` stay flat (or fall) and a `gt` can
     never pass. Mount the baseline's nodes PLUS one new node whose text uses a font stack the
     baseline never used (when the baseline has no text, any text node is new),
     `wait_frame`, `wait {{"ms": 100}}`, then
     assert_resource_counts {{"vs": "baseline", "parsed_fonts": "gt", "fonts": "gt"}}.
     `gt` / `lt` are what make a leak test mean something: an `eq` on a counter that never
     moved is `0 == 0`, and holds on an engine that was deleted.
   - COUNTERS THAT MOVE, AND THE TWO THAT NEVER DO. All but two counters move in this
     headless runner: `parsed_fonts`, `font_chain_cache` and `font_hash_to_families` (the font
     manager's tables), and `fonts`, `font_hash_map`, `images` and `image_key_map`, which count
     what the display lists on screen draw: one font per font the text is drawn in, one image
     per decoded picture (a `set_node_image` picture, an `add_image_to_cache` picture that a
     box's `background-image: url(id)` names). A picture drawn by ten boxes is ONE image, an
     `<img>` that holds no picture and a registered id that no box names are none, and a font
     or picture the page no longer draws is gone from them at once (no frames of grace, as a
     real renderer keeps them). `font_id_map` and `font_families_map` are legacy tables no
     shell writes any more: they are 0 whatever happens, so never assert them (an `eq` on
     them is `0 == 0`).
   - A LEAK TEST HAS TWO SAMPLES AND BOTH READ THE SAME COUNTERS. After the mount that adds the
     font or the picture: `gt` against the baseline (`parsed_fonts` and `fonts` for a font,
     `images` for a picture) - it really was loaded, so the leak half cannot go green for the
     wrong reason. After the mount that removes it: `eq` to the baseline (`lt` against the
     loaded sample proves the release path ran). A fixture with no text and no image has
     nothing to leak: add one text node in a mock font, or one `<img>` and a
     `set_node_image`, so the counters have something to hold.
   - A picture counts when it is ON SCREEN: `<img class="pic">` plus
     `set_node_image {{"selector": ".pic", "width": 40, "height": 40, "color": "red"}}`, or a
     registered id (`add_image_to_cache`) that a box names in `background-image: url("id")`.
     A swap (`set_node_image` again) replaces the picture, so `images` stays at the same
     number: it does not grow, and a `gt` control must come from a NEW node or a new id.
   - "damage kind / no full redraw"      -> assert_damage {{"kind": "rects", "max_area_ratio": 0.5}}  (kind is "none" | "rects" | "full")
   - DAMAGE ASSERTIONS READ AN ACCUMULATOR. `assert_damage`, `assert_changed` and
     `assert_damage_covers_changes` look at the damage accumulated since the last
     `reset_frame_counters`, not at the last frame, unless you pass "frame": "last". "The last
     frame reports zero paint damage" is therefore {{"op":"assert_damage","kind":"none","frame":"last"}};
     a plain `assert_damage none` after an input that legitimately repainted once (a :hover
     restyle, a scrollbar that appeared) fails on the repaint that was supposed to happen.
   - `which` is exactly "paint" (default: the rects re-rasterised) or "present" (what changed
     on screen) and `frame` is exactly "accumulated" (default) or "last"; any other value is
     refused, and "last" is a value of `frame`, never of `which`. A box displaced by a
     neighbour's size change is moved by a blit: it appears in PRESENT damage and not in PAINT
     damage, so for that case use "which":"present" on `assert_damage_covers_changes`, on
     `assert_damage_sound` (the change must then lie inside what was presented; its tightness
     bound and `forbid_full` still read the paint damage) and on `assert_changed` (a control
     that only MOVES boxes is `assert_changed {{"vs": "ctl_before", "which": "present"}}`).
   - `assert_changed` means "damage is not none AND pixels differ": never write it straight
     after an assertion that damage is none, and `min_damage_rects: 0` is meaningless.
   - "nothing panics"                    -> the steps running at all IS the assertion; still end with a liveness or idle assertion.
   - structure survived a mutation       -> assert_exists / assert_not_exists / assert_node_count / assert_text
7. If the line mentions a NodeId-renumbering mutation (insert/delete/reorder a
   sibling), do the mutation and then assert the DOM still holds
   (assert_node_count / assert_exists) and the window settles (assert_idle_stable).

NAME EVERY NODE BY A SELECTOR. The DOM-mutation ops (`set_node_css_override`,
`set_node_classes`, `set_node_text`, `set_node_image`, `delete_node`; `insert_node` by its
`parent_selector`) and `mouse_move`, `focus_node`, `click`, `scroll_node_by` take a CSS `selector`:
ALWAYS use it, never a numeric `node_id`, and give the elements your markup mounts an `id` so
that `#box` names exactly the node you mean. The mutation ops and `mouse_move` refuse a selector
that matches no node or several (an `#id` matches one); `click` and `focus_node` take the first
match, so a selector that is not unique clicks a node you did not mean. `set_node_text` with a
selector names the ELEMENT (`#label`, a `<p>`) and writes the one text inside it, so keep one
text per element. `set_node_image` works only on an `<img>`, after the `wait_frame` that
follows the `mount`.

WHY NOT A NUMBER: a `node_id` counts EVERY node of the mounted document, and it is not "the
root is 0 and its children are 1, 2, 3". `mount` wraps your `html` in `<html>` (node 0) and
`<body>` (node 1), the ids then run depth first over every node, and every newline or indent
between two tags is a whitespace text node that takes an id of its own. The number you count is
almost never the box you mean (in a first batch every numeric id landed on `<html>`, `<body>` or
a whitespace run), and the checker REJECTS such an id. The few ops that have no `selector` and
need a number (`add_timer`, which takes the id of a TEXT node, `click_node`, `get_node_dataset`)
take the id of the node they name: count it over the whole mounted document, whitespace runs
included - unless the `mount` says `"collapse_whitespace": true`, which drops the line break and
the indent between two tags (JSX's rule; a space on one line and the text inside an element stay):
no run then takes an id, the ids count only the elements and their text, and the checker counts
the same way. To recolour or resize the page itself, give the selector `body` or `html`.

Output the JSON object now — nothing else."#
    )
}

// ===========================================================================
// Validation gate
// ===========================================================================

/// Phrases a rate-limited / errored `claude -p` answers with, as PLAIN TEXT,
/// while still exiting 0. Never write such a reply out as a test.
const LIMIT_MARKERS: &[&str] = &[
    "rate limit",
    "rate-limit",
    "usage limit",
    "quota",
    "too many requests",
    "try again",
    "overloaded",
    "insufficient",
    "credit balance",
    "please run /login",
];

fn looks_rate_limited(raw: &str) -> bool {
    let low = raw.to_lowercase();
    LIMIT_MARKERS.iter().any(|m| low.contains(m))
}

/// Strip a ```json fence / leading prose and return the outermost JSON object.
fn extract_json(raw: &str) -> Option<&str> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(&raw[start..=end])
}

// ===========================================================================
// The document a scenario addresses
// ===========================================================================
//
// A numeric `node_id` counts EVERY node of the mounted document - `<html>` is
// 0, `<body>` is 1, each newline or indent between two tags is a whitespace
// text node of its own - so the number a model counts is rarely the element it
// means (44 of 44 id-addressed scenarios of the first batch addressed `<html>`,
// `<body>` or a whitespace run). The gate therefore resolves every id with the
// parser `mount` itself uses (`azul_layout::e2e::mounted_nodes`) and follows
// the document through the scenario: a `mount` replaces it, `insert_node` adds
// a node at the end, `delete_node` tombstones one.
//
// It also knows WHEN the document exists. `mount` only queues the new document;
// it takes effect at the next frame, and a step that addresses a node before
// that finds the old (or no) document - "Node 1 not found".

/// A node that is no longer in the document, and what took it out.
struct Gone {
    /// The `delete_node` step.
    step: usize,
    /// The node that step deleted: the node itself, or an ancestor of it.
    root: usize,
}

/// One node of the document as the gate follows it.
struct GateNode {
    node: MountedNode,
    /// Set when a `delete_node` took it out. The engine tombstones: the id
    /// stays, the node is gone - and so is everything that hung below it (its
    /// children keep pointing at the tombstone, but nothing reaches them).
    deleted_by: Option<Gone>,
}

/// The document the steps address, and the `mount` still waiting to land.
#[derive(Default)]
struct GateDom {
    /// What the steps see: the last `mount` that has landed, with the
    /// scenario's `insert_node` / `delete_node` applied. `None`: no document.
    live: Option<Vec<GateNode>>,
    /// A `mount` (its step, its document) or `unmount` (its step, `None`) that
    /// the next frame applies.
    pending: Option<(usize, Option<Vec<GateNode>>)>,
}

/// The steps that give the frame loop a turn, so a queued `mount` lands before
/// the step after them: `wait_frame` and `wait` (the two the prompt prescribes
/// after a `mount`) and `tick_ms` (which repaints).
const FRAME_STEPS: &[&str] = &["wait_frame", "wait", "tick_ms"];

/// The keys that name a node: a step with one addresses the document.
const NODE_KEYS: &[&str] = &["selector", "node_id", "parent_selector", "parent_id"];

/// What kind of node an op's numeric id has to be.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Needs {
    /// An element (`<html>` / `<body>` excepted): the box the scenario is
    /// about. Most ops.
    Element,
    /// A text node: `set_node_text` writes it, `add_timer` rewrites it.
    Text,
    /// An `<img>`: the node `set_node_image` swaps.
    Image,
}

fn needs(op: &str) -> Needs {
    match op {
        "set_node_text" | "add_timer" => Needs::Text,
        "set_node_image" => Needs::Image,
        _ => Needs::Element,
    }
}

/// `` `<div id="a" class="box">` ``, the text of a text run, or what else a
/// node is, for a message.
fn describe(n: &MountedNode) -> String {
    match n.kind {
        MountedKind::Html => "`<html>`".to_string(),
        MountedKind::Body => "`<body>`".to_string(),
        MountedKind::Whitespace => {
            "a whitespace text run (the newline or indent between two tags)".to_string()
        }
        MountedKind::Text => format!("the text {:?}", n.text.chars().take(24).collect::<String>()),
        MountedKind::Element | MountedKind::Image => {
            let mut s = format!("<{}", n.tag);
            if let Some(id) = &n.css_id {
                s.push_str(&format!(" id=\"{id}\""));
            }
            if !n.classes.is_empty() {
                s.push_str(&format!(" class=\"{}\"", n.classes.join(" ")));
            }
            s.push('>');
            format!("`{s}`")
        }
    }
}

/// A selector that finds `n` again: its `#id`, else `tag.class`, else the tag.
fn selector_for(n: &MountedNode) -> String {
    if let Some(id) = &n.css_id {
        return format!("#{id}");
    }
    match n.classes.first() {
        Some(class) => format!("{}.{class}", n.tag),
        None => n.tag.clone(),
    }
}

/// The elements of a document by id, for the end of a message: where the
/// numbers the scenario meant actually are.
fn element_ids(nodes: &[GateNode]) -> String {
    let named: Vec<String> = nodes
        .iter()
        .enumerate()
        .filter(|(_, g)| {
            g.deleted_by.is_none()
                && matches!(g.node.kind, MountedKind::Element | MountedKind::Image)
        })
        .take(8)
        .map(|(id, g)| format!("{id} {}", describe(&g.node)))
        .collect();
    if named.is_empty() {
        "(none)".to_string()
    } else {
        named.join(", ")
    }
}

impl GateDom {
    /// A `mount` step: its document exists from the next frame on.
    fn mount(&mut self, step: usize, nodes: Vec<MountedNode>) {
        let doc = nodes
            .into_iter()
            .map(|node| GateNode {
                node,
                deleted_by: None,
            })
            .collect();
        self.pending = Some((step, Some(doc)));
    }

    /// An `unmount` step: from the next frame on there is no document to address.
    fn unmount(&mut self, step: usize) {
        self.pending = Some((step, None));
    }

    /// A step that gave the frame loop a turn: a queued mount lands.
    fn land(&mut self) {
        if let Some((_, doc)) = self.pending.take() {
            self.live = doc;
        }
    }

    /// The document a node-addressing step sees - or why there is none yet.
    fn addressable(&self, i: usize, op: &str) -> Result<&[GateNode]> {
        if let Some((at, _)) = &self.pending {
            bail!(
                "step {i}: `{op}` addresses a node, but the `mount` at step {at} has not been \
                 applied yet: a `mount` takes effect at the next frame, so a `wait_frame` has to \
                 come between it and the first step that addresses its nodes (before that the \
                 document does not exist and the op fails with \"Node N not found\")"
            );
        }
        match &self.live {
            Some(nodes) => Ok(nodes),
            None => bail!(
                "step {i}: `{op}` addresses a node, but nothing is mounted: start the scenario \
                 with a `mount` and a `wait_frame`"
            ),
        }
    }

    /// What a step that passed its checks does to the document.
    fn apply(&mut self, i: usize, op: &str, step: &serde_json::Map<String, serde_json::Value>) {
        let Some(nodes) = self.live.as_mut() else {
            return;
        };
        match op {
            // The engine appends the new node at the end of the document.
            "insert_node" => {
                let node_type = step
                    .get("node_type")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                let (kind, tag, text) = match node_type.strip_prefix("text:") {
                    Some(t) if t.trim().is_empty() => {
                        (MountedKind::Whitespace, String::new(), t.to_string())
                    }
                    Some(t) => (MountedKind::Text, String::new(), t.to_string()),
                    None => (
                        MountedKind::Element,
                        node_type.to_lowercase(),
                        String::new(),
                    ),
                };
                nodes.push(GateNode {
                    node: MountedNode {
                        kind,
                        tag,
                        css_id: step
                            .get("id")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string),
                        classes: step
                            .get("classes")
                            .and_then(serde_json::Value::as_array)
                            .map(|c| {
                                c.iter()
                                    .filter_map(|c| c.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        text,
                        parent: step
                            .get("parent_id")
                            .and_then(serde_json::Value::as_u64)
                            .and_then(|p| usize::try_from(p).ok()),
                    },
                    deleted_by: None,
                });
            }
            "delete_node" => {
                let deleted = step
                    .get("node_id")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|id| usize::try_from(id).ok())
                    .filter(|&id| id < nodes.len());
                if let Some(root) = deleted {
                    // The node and everything below it: walk each node's parents.
                    let below: Vec<usize> = (0..nodes.len())
                        .filter(|&n| {
                            let mut cur = Some(n);
                            for _ in 0..=nodes.len() {
                                match cur {
                                    Some(c) if c == root => return true,
                                    Some(c) => cur = nodes[c].node.parent,
                                    None => return false,
                                }
                            }
                            false
                        })
                        .collect();
                    for n in below {
                        nodes[n].deleted_by.get_or_insert(Gone { step: i, root });
                    }
                }
            }
            _ => {}
        }
    }
}

/// Resolve the numeric ids of a step against the document.
///
/// An id that does not exist, that was deleted, or that is not the kind of node
/// the op takes is refused - and the refusal says where the elements ARE, so the
/// fix is one read away. `selector_form` says whether the op can name the node
/// by a selector instead (the advice differs).
fn check_node_ids(
    i: usize,
    op: &str,
    step: &serde_json::Map<String, serde_json::Value>,
    nodes: &[GateNode],
    selector_form: bool,
) -> Result<()> {
    for key in ["node_id", "parent_id"] {
        let Some(id) = step.get(key).and_then(serde_json::Value::as_u64) else {
            continue;
        };
        let Some(g) = usize::try_from(id).ok().and_then(|id| nodes.get(id)) else {
            bail!(
                "step {i}: `{op}` {key} {id} does not exist: the mounted document has {} nodes \
                 (ids 0..={}). Elements: {}",
                nodes.len(),
                nodes.len().saturating_sub(1),
                element_ids(nodes)
            );
        };
        if let Some(gone) = &g.deleted_by {
            if usize::try_from(id).is_ok_and(|id| id == gone.root) {
                bail!(
                    "step {i}: `{op}` {key} {id} was deleted by step {}",
                    gone.step
                );
            }
            bail!(
                "step {i}: `{op}` {key} {id} hung below node {}, which step {} deleted",
                gone.root,
                gone.step
            );
        }
        let n = &g.node;
        let advice = if selector_form {
            "Name the element with a `selector` instead (a CSS selector that matches exactly \
             one node)"
        } else {
            "Use the id of the node it means"
        };
        let wrong = |what: &str| -> Result<()> {
            bail!(
                "step {i}: `{op}` {key} {id} is {}, {what}. A numeric id counts EVERY node of the \
                 mounted document (`<html>` is 0, `<body>` is 1, each newline or indent between \
                 two tags is a whitespace text node of its own), so the number you counted is \
                 rarely the box you mean. {advice}. Elements: {}",
                describe(n),
                element_ids(nodes)
            )
        };
        match (needs(op), n.kind) {
            (Needs::Element, MountedKind::Element | MountedKind::Image) => {}
            (Needs::Element, MountedKind::Html | MountedKind::Body) => {
                return wrong("the page itself, not the element the scenario is about");
            }
            (Needs::Element, MountedKind::Whitespace) => {
                return wrong("not an element");
            }
            (Needs::Element, MountedKind::Text) => {
                let owner = n
                    .parent
                    .and_then(|p| nodes.get(p))
                    .map_or_else(String::new, |p| {
                        format!(
                            " (its element is node {} {})",
                            n.parent.unwrap_or(0),
                            describe(&p.node)
                        )
                    });
                return wrong(&format!("a text node, not an element{owner}"));
            }
            (Needs::Text, MountedKind::Text) => {}
            (Needs::Text, MountedKind::Whitespace) => {
                return wrong("a whitespace run with no text to set");
            }
            (Needs::Text, _) => {
                let text_child = nodes.iter().enumerate().find(|(_, c)| {
                    c.node.parent == usize::try_from(id).ok()
                        && c.deleted_by.is_none()
                        && c.node.kind == MountedKind::Text
                });
                let hint = text_child.map_or_else(String::new, |(t, _)| {
                    format!(" (the text inside it is node {t})")
                });
                return wrong(&format!(
                    "an element, but `{op}` addresses the TEXT node inside it{hint}"
                ));
            }
            (Needs::Image, MountedKind::Image) => {}
            (Needs::Image, _) => {
                return wrong("not an `<img>`");
            }
        }
    }
    Ok(())
}

/// A string param with a closed set of values holds one of them.
///
/// The set is the engine's own (`azul_layout::e2e::closed_param_values`, the
/// table the ops themselves refuse with), so what the gate allows and what the
/// engine accepts cannot drift apart. `"last"` is a value of `frame`, not of
/// `which`: the first batch wrote `{"which": "last"}`, the engine read it as the
/// default, and the assertion judged the paint damage of the whole run - so the
/// refusal also says which param a value belongs to.
fn check_closed_values(
    i: usize,
    op: &str,
    step: &serde_json::Map<String, serde_json::Value>,
) -> Result<()> {
    for (key, value) in step {
        let Some(allowed) = closed_param_values(key) else {
            continue;
        };
        let text = value.as_str();
        if text.is_some_and(|t| allowed.contains(&t)) {
            continue;
        }
        let belongs_to = text.and_then(|t| {
            CLOSED_STRING_PARAMS
                .iter()
                .find(|(param, values)| param != key && values.contains(&t))
                .map(|(param, _)| *param)
        });
        bail!(
            "step {i}: `{op}` param `{key}` is {value}, which is not one of {}{}. The engine \
             refuses it too; before it did, an unknown value was read as the default and the step \
             judged something other than what it says.",
            allowed.join(", "),
            belongs_to.map_or_else(String::new, |p| format!(" ({value} is a value of `{p}`)"))
        );
    }
    Ok(())
}

/// The step as the runner reads it, for an op the engine dispatches as a
/// `DebugEvent` (assertions and step-loop ops are read by their own
/// evaluators). The runner strips the harness's `screenshot` flag and lifts
/// `dom_id` out before it parses; so does this.
fn engine_reading(
    schema: &Schema,
    i: usize,
    op: &str,
    step: &serde_json::Map<String, serde_json::Value>,
) -> Result<Option<DebugEvent>> {
    if !schema.ops.iter().any(|o| o.name == op) {
        return Ok(None);
    }
    let mut cmd = step.clone();
    cmd.remove("screenshot");
    cmd.remove("dom_id");
    parse_debug_event(serde_json::Value::Object(cmd))
        .map(Some)
        .map_err(|why| anyhow::anyhow!("step {i}: `{op}` is not read by the engine: {why}"))
}

/// Is `key` given in the step, with a value?
fn given(step: &serde_json::Map<String, serde_json::Value>, key: &str) -> bool {
    step.get(key).is_some_and(|v| !v.is_null())
}

/// How an op names its target, and that it does so in the one way the rule allows.
///
/// The rule is the ENGINE's: `azul_layout::e2e::targeting(op)` says whether the op
/// takes exactly one of a `selector` and a `node_id` (the DOM-mutation family,
/// `focus_node`, with `parent_selector` / `parent_id` for `insert_node`), a position
/// or a selector (`mouse_move`), or has no single rule (`click`: a position, a text,
/// a selector, an id - the engine resolves them in a fixed order; `assert_scroll`: a
/// REQUIRED `selector` and the offsets to compare). The ops resolve their target by the
/// same table, and the refusal is the same words. This gate used to DERIVE the rule
/// from the NAMES of an op's params, which let an op that named its target by another
/// pair through until the engine refused it at run time (`FIX-87b0f2b8c-1`).
/// The schema cannot say "exactly one" itself: both params are optional.
fn check_targeting(
    i: usize,
    op: &str,
    step: &serde_json::Map<String, serde_json::Value>,
) -> Result<()> {
    azul_layout::e2e::targeting(op)
        .check(op, &|param| given(step, param))
        .map_err(|why| anyhow::anyhow!("step {i}: {why}"))
}

/// The MECHANICAL GATE. Every failure here means: delete the artifact, count a
/// FAIL, do not mark the line done.
pub fn validate(schema: &Schema, json: &str) -> Result<()> {
    let v: serde_json::Value = serde_json::from_str(json).context("output is not valid JSON")?;
    let obj = v.as_object().context("top level is not a JSON object")?;

    if !obj.get("name").is_some_and(serde_json::Value::is_string) {
        bail!("missing string field `name`");
    }
    let steps = obj
        .get("steps")
        .and_then(|s| s.as_array())
        .context("missing array field `steps`")?;
    if steps.is_empty() {
        bail!("`steps` is empty");
    }

    let mut snapshots: BTreeSet<String> = BTreeSet::new();
    let mut asserted = false;
    let mut dom = GateDom::default();

    for (i, step) in steps.iter().enumerate() {
        let s = step
            .as_object()
            .with_context(|| format!("step {i} is not an object"))?;
        let op = s
            .get("op")
            .and_then(|o| o.as_str())
            .with_context(|| format!("step {i} has no string `op`"))?;

        if !schema.is_known(op) {
            bail!("step {i}: unknown op `{op}` (not in full.rs)");
        }
        // ZOMBIE — declared in `DebugEvent`, but no match arm: the dispatch's
        // catch-all logs "Unhandled" and answers `ok`. A test using it PASSES
        // WHILE DOING NOTHING. That is the same false-green class as `redraw`,
        // and it is derived from the code, so it heals itself the moment the op
        // is implemented.
        if schema.is_zombie(op) {
            bail!(
                "step {i}: op `{op}` is declared in DebugEvent but has no match arm — it silently \
                 returns ok and does nothing; implement it or remove it before it may appear in a \
                 test"
            );
        }
        // SCOPE — the classification table is the law (`OP_POLICY`). The prompt
        // only SHOWS the allowed ops; this is what ENFORCES it.
        match classify(op) {
            OpClass::Allowed => {}
            OpClass::Denied(why) => {
                bail!("step {i}: op `{op}` is DENIED for generated behaviour tests — {why}");
            }
            OpClass::Unclassified => {
                bail!(
                    "step {i}: op `{op}` is UNCLASSIFIED — it exists in DebugEvent but no row of \
                     gene2e.rs::OP_POLICY covers it. Classify it (allow or deny, with a reason) \
                     before it may appear in a test."
                );
            }
        }
        if let Some(def) = schema.known_op(op) {
            for (p, required) in &def.params {
                if *required && !s.contains_key(p) {
                    bail!("step {i}: op `{op}` is missing required param `{p}`");
                }
            }
        }
        // UNKNOWN PARAM. This gate used to check only that REQUIRED params were
        // PRESENT — never that the params supplied actually exist. So a step
        // could ask for a bound the evaluator does not implement
        // (`min_relayouts` before it existed, `max_relayout` with the `s`
        // dropped), be accepted here, be ignored at run time, and report green
        // while asserting nothing. The evaluators now reject unknown keys at run
        // time (`full.rs::reject_unknown_params`); rejecting them at GENERATION
        // time costs nothing and stops the artifact from being written at all.
        //
        // `screenshot` is a step-level flag owned by the harness, not a param.
        if let Some(def) = schema
            .known_op(op)
            .or_else(|| schema.extra.iter().find(|e| e.name == op))
        {
            for key in s.keys() {
                if key == "op" || key == "screenshot" {
                    continue;
                }
                if !def.params.iter().any(|(p, _)| p == key) {
                    let known: Vec<&str> = def.params.iter().map(|(p, _)| p.as_str()).collect();
                    bail!(
                        "step {i}: op `{op}` has no param `{key}` (known: {}). An unknown param \
                         is dropped by the runner, so the step would assert LESS than it says.",
                        if known.is_empty() {
                            "(none)".to_string()
                        } else {
                            known.join(", ")
                        }
                    );
                }
            }
        }
        // TARGET. An op that names its node by `selector` OR by `node_id` takes
        // exactly one of them, and the schema cannot say so: both are optional.
        check_targeting(i, op, s)?;
        // CLOSED SETS. A string param that takes one of a few values refuses any
        // other: the engine used to read an unknown one as the default.
        check_closed_values(i, op, s)?;
        // `mount` html must be the pretty ARRAY-OF-LINES form (a string is also
        // read) - said plainly before the typed read below says it in serde's
        // words.
        if op == "mount" {
            let html = s.get("html").context("mount: missing `html`")?;
            if !html.is_array() && !html.is_string() {
                bail!("step {i}: mount `html` must be an array of lines");
            }
        }
        // THE ENGINE'S OWN READING. The step is read as the runner reads it
        // (`parse_debug_event`): a value of the wrong type, an enum variant that
        // does not exist, a field the op needs - each fails there, at step N,
        // after the steps before it ran, when it could have failed here.
        let event = engine_reading(schema, i, op, s)?;
        if op.starts_with("assert_") {
            asserted = true;
        }
        match op {
            "snapshot_frame" | "snapshot_resources" | "snapshot_managers" => {
                let name = s
                    .get("as")
                    .and_then(|n| n.as_str())
                    .with_context(|| format!("step {i}: `{op}` needs a string `as`"))?;
                snapshots.insert(name.to_string());
            }
            _ => {
                if let Some(vs) = s.get("vs").and_then(|n| n.as_str()) {
                    if !snapshots.contains(vs) {
                        bail!(
                            "step {i}: `{op}` references snapshot `{vs}`, which no earlier \
                             snapshot_frame/snapshot_resources/snapshot_managers created"
                        );
                    }
                }
            }
        }
        // THE DOCUMENT. `mount` is read with the parser the `mount` op uses; its
        // nodes are what a numeric id will mean once it has landed.
        if let Some(DebugEvent::Mount {
            html,
            css,
            collapse_whitespace,
            virtual_views,
        }) = &event
        {
            if html.is_empty() {
                bail!(
                    "step {i}: `mount` has an empty `html` ({}): the runtime refuses it (\"mount: \
                     'html' is empty\") and the scenario fails at step {i} having tested nothing. \
                     An empty page is [\"<body></body>\"], never []",
                    s.get("html").map_or_else(String::new, ToString::to_string)
                );
            }
            let nodes = mounted_nodes_with(html, css, *collapse_whitespace)
                .map_err(|e| anyhow::anyhow!("step {i}: {e}"))?;
            // The views land on the document the mount parses: one that cannot (no
            // element, several, one with content, a row that is no markup) is the
            // refusal the `mount` op would answer with.
            if !virtual_views.is_empty() {
                azul_layout::e2e::check_mount_views(html, css, *collapse_whitespace, virtual_views)
                    .map_err(|e| anyhow::anyhow!("step {i}: {e}"))?;
            }
            dom.mount(i, nodes);
        }
        if op == "unmount" {
            dom.unmount(i);
        }
        // A step that names a node needs a document that exists (G5), and an id
        // that is the element it means (G4). An assertion only READS, and `assert_not_exists`
        // before a mount is a question, not an address.
        if !op.starts_with("assert_") && NODE_KEYS.iter().any(|k| given(s, k)) {
            let nodes = dom.addressable(i, op)?;
            let selector_form = schema.known_op(op).is_some_and(|d| {
                d.params
                    .iter()
                    .any(|(p, _)| p == "selector" || p == "parent_selector")
            });
            check_node_ids(i, op, s, nodes, selector_form)?;
        }
        dom.apply(i, op, s);
        if FRAME_STEPS.contains(&op) {
            dom.land();
        }
        // A scenario's PNG `path` is the one model-authored string this process
        // hands to `fs::write`. Confine it: inside `target/e2e/`, relative, no
        // `..`, `.png`. Without this a generated scenario could name any path in
        // the repo and the runner would overwrite it. NOTE this does not REQUIRE
        // a capture step — it only constrains one that is there (see below).
        if op == PNG_OP {
            let path = s
                .get("path")
                .and_then(|p| p.as_str())
                .with_context(|| format!("step {i}: `{PNG_OP}` needs a string `path`"))?;
            if !path.starts_with(PNG_DIR)
                || !path.ends_with(".png")
                || path.contains("..")
                || path.len() <= PNG_DIR.len() + 4
            {
                bail!(
                    "step {i}: `{PNG_OP}` path {path:?} must be a `{PNG_DIR}<name>.png` path \
                     inside the repo (relative, no `..`)"
                );
            }
        }
    }

    if !asserted {
        bail!("the test contains no assert_* step");
    }
    // DELIBERATELY NOT REQUIRED: a `capture_damage_png` step in every scenario.
    // A PNG per test is an artifact tax on all 13k of them for an image almost
    // nobody will open — and for the idle/no-input half of the corpus the
    // capture is a blank patch by construction. Images are TRIAGE material for
    // the review agent instead: when a scenario FAILS, the reviewer copies it
    // into a scratch dir, adds capture steps THERE and looks at the result
    // (`triage_doc`). The committed scenario stays byte-identical and no image
    // is ever committed.
    Ok(())
}

// ===========================================================================
// Run
// ===========================================================================

/// One corpus line, resolved into a unit of work.
#[derive(Debug, Clone)]
pub struct Work {
    /// 1-based line number in the corpus. Cosmetic ONLY (it names the file);
    /// it is NOT the identity of the test — `hash` is.
    pub index: usize,
    /// Content address of the description line: the done-key. Survives the
    /// corpus being regenerated with lines inserted above / reordered.
    pub hash: String,
    pub tag: String,
    pub line: String,
    /// Where the artifact for this line SHOULD live (`<NNNNN>-<slug>.json`).
    pub out: PathBuf,
}

/// An artifact already on disk, identified by the hash it carries.
#[derive(Debug, Clone)]
pub struct Artifact {
    pub path: PathBuf,
    /// `None` = the file does not identify its source line (hand-written, or
    /// produced before content-addressing). Treated as an orphan.
    pub hash: Option<String>,
    /// Does it still pass the validation gate?
    pub valid: bool,
}

/// The outcome of planning: what `--dry-run` prints and what the pool executes.
#[derive(Debug, Default)]
pub struct Plan {
    /// Corpus lines considered (after `--filter`).
    pub total: usize,
    /// Lines whose artifact exists AND validates.
    pub already_done: usize,
    /// Lines whose artifact exists but FAILED the gate — they are in `todo`.
    pub invalid: usize,
    /// Lines to generate, in corpus order, `--limit`ed.
    pub todo: Vec<Work>,
    /// Lines to generate BEFORE `--limit` was applied.
    pub todo_total: usize,
    /// Artifacts on disk that no corpus line claims (`--prune` deletes these).
    pub orphans: Vec<PathBuf>,
}

/// FNV-1a 64. A content address, not a security primitive: it only has to be
/// stable across runs and across corpus regenerations, and it must not pull a
/// crypto dependency into the doc tool.
pub fn line_hash(line: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in line.trim().as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// The two self-identifying fields spliced into every artifact we write.
const HASH_KEY: &str = "_source_hash";
const SOURCE_KEY: &str = "_source";

/// Splice `_source_hash` / `_source` in as the FIRST keys of the object, by text
/// (a serde round-trip would sort the keys and wreck the array-of-lines layout
/// the format exists for).
fn stamp(json: &str, w: &Work) -> String {
    let json = json.trim();
    let rest = json.strip_prefix('{').unwrap_or(json);
    format!(
        "{{\n  \"{HASH_KEY}\": {},\n  \"{SOURCE_KEY}\": {},{}\n",
        serde_json::Value::String(w.hash.clone()),
        serde_json::Value::String(w.line.clone()),
        rest
    )
}

/// How many hex digits of the source hash a scenario's name is suffixed with.
const NAME_HASH_LEN: usize = 8;

/// The end of the JSON string whose content starts at `from` (the index of its
/// closing quote), or `None` when it never ends.
fn json_string_end(bytes: &[u8], mut from: usize) -> Option<usize> {
    while from < bytes.len() {
        match bytes[from] {
            b'\\' => from += 2,
            b'"' => return Some(from),
            _ => from += 1,
        }
    }
    None
}

/// Where the CONTENT (between the quotes) of the string value of the TOP-LEVEL
/// key `key` of the JSON object `json` sits, or `None`.
///
/// Scanned by text, because the reply is spliced by text (a serde round-trip
/// sorts the keys and wrecks the array-of-lines layout the format exists for),
/// and a `"name"` key can also sit deeper in a step.
fn top_level_string_span(json: &str, key: &str) -> Option<std::ops::Range<usize>> {
    let bytes = json.as_bytes();
    let (mut depth, mut i) = (0usize, 0usize);
    while i < bytes.len() {
        match bytes[i] {
            b'{' | b'[' => {
                depth += 1;
                i += 1;
            }
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b'"' => {
                let start = i + 1;
                let end = json_string_end(bytes, start)?;
                // A string at depth 1 followed by `:` is a key of the object.
                let mut j = end + 1;
                while bytes.get(j).is_some_and(u8::is_ascii_whitespace) {
                    j += 1;
                }
                if depth == 1 && bytes.get(j) == Some(&b':') && &json[start..end] == key {
                    j += 1;
                    while bytes.get(j).is_some_and(u8::is_ascii_whitespace) {
                        j += 1;
                    }
                    if bytes.get(j) != Some(&b'"') {
                        return None; // the value is not a string
                    }
                    let value_end = json_string_end(bytes, j + 1)?;
                    return Some(j + 1..value_end);
                }
                i = end + 1;
            }
            _ => i += 1,
        }
    }
    None
}

/// The scenario with `_<first 8 hex of its source hash>` appended to its `name`.
///
/// The model names a scenario after the property and the fixture its line is
/// about, and 6 of the first 50 lines (all `noop_css_override_no_layout`) chose
/// the same name. Results are paired with files by FILE now, but the verdict
/// report prints names, an `expect` marker pairs by name and `azul-doc e2e` of
/// the whole corpus refuses two tests of one name: every artifact carries a name
/// of its own, and the hash is the one thing that is its own. Idempotent, and a
/// reply without a string `name` is left alone (the gate rejects it anyway).
fn with_unique_name(json: &str, hash: &str) -> String {
    let suffix = format!("_{}", &hash[..hash.len().min(NAME_HASH_LEN)]);
    let Some(span) = top_level_string_span(json, "name") else {
        return json.to_string();
    };
    if json[span.clone()].ends_with(&suffix) {
        return json.to_string();
    }
    format!("{}{suffix}{}", &json[..span.end], &json[span.end..])
}

/// What is written for a reply that passed the gate: its name made unique,
/// stamped with the line it answers.
fn artifact_text(json: &str, w: &Work) -> String {
    stamp(&with_unique_name(json, &w.hash), w)
}

/// Read one artifact off disk: which corpus line does it claim, and is it still
/// valid? Anything unreadable / unparseable is an invalid, unidentified file.
pub fn read_artifact(schema: &Schema, path: &Path) -> Artifact {
    let Ok(src) = fs::read_to_string(path) else {
        return Artifact {
            path: path.to_path_buf(),
            hash: None,
            valid: false,
        };
    };
    let hash = serde_json::from_str::<serde_json::Value>(&src)
        .ok()
        .and_then(|v| {
            let o = v.as_object()?;
            // Prefer the recorded hash; fall back to re-hashing the recorded
            // source line, so an artifact stamped by an older format still
            // resolves.
            o.get(HASH_KEY)
                .and_then(|h| h.as_str())
                .map(str::to_string)
                .or_else(|| o.get(SOURCE_KEY).and_then(|s| s.as_str()).map(line_hash))
        });
    Artifact {
        path: path.to_path_buf(),
        valid: validate(schema, &src).is_ok(),
        hash,
    }
}

/// Every `*.json` in `out_dir`, read once.
pub fn scan_artifacts(schema: &Schema, out_dir: &Path) -> Vec<Artifact> {
    let Ok(rd) = fs::read_dir(out_dir) else {
        return Vec::new();
    };
    let mut out: Vec<Artifact> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "json"))
        .map(|p| read_artifact(schema, &p))
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// PURE. Given the corpus work list, what is already on disk, and the cached
/// done-list, decide what to generate. This is the whole incremental contract,
/// and it is unit-tested.
///
/// `work` must be the FILTERED list; `corpus_hashes` must be the UNFILTERED set
/// (an orphan is a file no corpus line claims — `--filter` must not turn the
/// rest of the corpus into orphans).
pub fn plan(
    work: Vec<Work>,
    corpus_hashes: &BTreeSet<String>,
    artifacts: &[Artifact],
    done_list: &BTreeSet<String>,
    redo: bool,
    limit: Option<usize>,
) -> Plan {
    let mut by_hash: BTreeMap<&str, &Artifact> = BTreeMap::new();
    for a in artifacts {
        if let Some(h) = &a.hash {
            // A valid artifact always wins over a duplicate invalid one.
            match by_hash.get(h.as_str()) {
                Some(prev) if prev.valid || !a.valid => {}
                _ => {
                    by_hash.insert(h.as_str(), a);
                }
            }
        }
    }

    let mut p = Plan {
        total: work.len(),
        ..Plan::default()
    };

    for w in work {
        let art = by_hash.get(w.hash.as_str());
        // The done-list is only a CACHE: it can say "done" all it likes, if the
        // artifact is gone or broken the line is not done. Conversely a valid
        // artifact IS done even with no done-list at all.
        let done = !redo && art.is_some_and(|a| a.valid);
        if done {
            p.already_done += 1;
            continue;
        }
        if art.is_some_and(|a| !a.valid) {
            p.invalid += 1;
        }
        p.todo.push(w);
    }
    // The done-list carries no authority; it is read only so a stale entry can
    // be reported/ignored rather than trusted.
    let _ = done_list;

    p.todo_total = p.todo.len();
    if let Some(n) = limit {
        p.todo.truncate(n);
    }

    // Orphans: on disk, but no corpus line (in the WHOLE corpus) claims them.
    //
    // ONLY files this generator produced are eligible. `--prune` deletes orphans,
    // and the out-dir is now the shared `e2e/` corpus, which also holds the
    // hand-written scenarios (`op-*.json`, `bug-*.json`, `mock-*.json`). Those
    // carry no `_source_hash` — they predate content-addressing and no corpus
    // line will ever claim them — so the old "unclaimed => orphan" rule made all
    // 21 of them prune candidates. A single `--prune` would have silently deleted
    // the entire hand-written suite, including the regression tests guarding bugs
    // fixed today. A foreign file is not ours to reclassify, let alone remove.
    for a in artifacts {
        if !is_generated_artifact(&a.path) {
            continue;
        }
        let claimed = a
            .hash
            .as_ref()
            .is_some_and(|h| corpus_hashes.contains(h.as_str()));
        if !claimed {
            p.orphans.push(a.path.clone());
        }
    }
    p
}

/// Whether `path` is an artifact THIS generator wrote.
///
/// Generated artifacts are named `<NNNNN>-<slug>.json` (see the `out` path built
/// in `plan`), so a leading 5-digit index followed by `-` is the signature. Any
/// other `*.json` in the out-dir belongs to someone else — hand-written
/// scenarios, fixtures, scratch files — and must never be pruned or renumbered.
fn is_generated_artifact(path: &std::path::Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let Some((index, rest)) = name.split_once('-') else {
        return false;
    };
    // The slug itself must be non-empty: `00001-.json` is malformed, and an
    // ambiguous name is not worth a deletion. Err toward NOT ours.
    index.len() == 5
        && index.bytes().all(|b| b.is_ascii_digit())
        && !rest.trim_start_matches(".json").is_empty()
        && rest != ".json"
}

fn slug(tag: &str, desc: &str) -> String {
    let base = format!("{tag} {desc}");
    let mut s = String::new();
    for c in base.chars() {
        if c.is_ascii_alphanumeric() {
            s.extend(c.to_lowercase());
        } else if !s.ends_with('-') {
            s.push('-');
        }
        if s.len() >= 60 {
            break;
        }
    }
    s.trim_matches('-').to_string()
}

/// Corpus text -> work items. Blank / `#` lines are skipped. PURE (given the
/// out-dir), so the id + filename scheme is unit-testable.
pub fn parse_corpus(corpus: &str, out_dir: &Path) -> Vec<Work> {
    let mut work = Vec::new();
    for (i, raw) in corpus.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let tag = line
            .strip_prefix('[')
            .and_then(|r| r.split(']').next())
            .unwrap_or("untagged")
            .to_string();
        let desc = line.split_once(']').map_or(line, |(_, d)| d.trim());
        let index = i + 1;
        work.push(Work {
            index,
            hash: line_hash(line),
            out: out_dir.join(format!("{:05}-{}.json", index, slug(&tag, desc))),
            tag,
            line: line.to_string(),
        });
    }
    work
}

pub fn run(project_root: &Path, opts: &GenE2eOptions) -> Result<()> {
    let txt = resolve(project_root, &opts.txt);
    let out_dir = resolve(project_root, &opts.out_dir);

    let schema = parse_schema(project_root)?;
    let corpus = fs::read_to_string(&txt)
        .with_context(|| format!("gen-e2e: cannot read {}", txt.display()))?;

    // --- work list -------------------------------------------------------
    // Parse the WHOLE corpus first: `--filter` must not make the rest of the
    // corpus look like orphaned artifacts.
    let all = parse_corpus(&corpus, &out_dir);
    if all.is_empty() {
        bail!("gen-e2e: empty work list ({})", txt.display());
    }
    let corpus_hashes: BTreeSet<String> = all.iter().map(|w| w.hash.clone()).collect();
    let work: Vec<Work> = all
        .iter()
        .filter(|w| {
            opts.filter
                .as_ref()
                .is_none_or(|f| w.tag.contains(f.as_str()))
        })
        .cloned()
        .collect();
    if work.is_empty() {
        bail!(
            "gen-e2e: --filter {:?} matched no corpus line",
            opts.filter.as_deref().unwrap_or("")
        );
    }

    // --- resume ----------------------------------------------------------
    fs::create_dir_all(&out_dir)?;
    let done_file = out_dir.join(".done-gen-e2e");
    let done_list: BTreeSet<String> = fs::read_to_string(&done_file)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_string)
        .collect();

    // CORPUS DRIFT: a line that moved keeps its artifact (same hash) but the
    // artifact's `<NNNNN>-` prefix is now wrong. Rename it into place, so the
    // human-friendly numbering tracks the corpus and the file is not mistaken
    // for a duplicate later.
    let mut artifacts = scan_artifacts(&schema, &out_dir);
    let expected: BTreeMap<&str, &Path> = all
        .iter()
        .map(|w| (w.hash.as_str(), w.out.as_path()))
        .collect();
    let mut renamed = 0usize;
    for a in &mut artifacts {
        let Some(h) = a.hash.clone() else { continue };
        let Some(want) = expected.get(h.as_str()) else {
            continue;
        };
        if a.path != *want && !want.exists() && !opts.dry_run
            && fs::rename(&a.path, want).is_ok() {
                a.path = want.to_path_buf();
                renamed += 1;
            }
    }

    let p = plan(
        work,
        &corpus_hashes,
        &artifacts,
        &done_list,
        opts.redo,
        opts.limit,
    );

    println!(
        "[gen-e2e] corpus={} total={} already-done={} to-generate={} (of {} outstanding, {} \
         invalid) stale-orphans={} model={} effort={} jobs={}",
        txt.display(),
        p.total,
        p.already_done,
        p.todo.len(),
        p.todo_total,
        p.invalid,
        p.orphans.len(),
        opts.model,
        opts.effort,
        opts.jobs
    );
    let allowed = schema
        .all_op_names()
        .filter(|o| classify(o) == OpClass::Allowed && !schema.is_zombie(o))
        .count();
    let denied = schema
        .all_op_names()
        .filter(|o| matches!(classify(o), OpClass::Denied(_)))
        .count();
    println!(
        "[gen-e2e] schema: {} ops + {} assertions + {} step-loop ops (parsed from {})",
        schema.ops.len(),
        schema.asserts.len(),
        schema.extra.len(),
        FULL_RS
    );
    let zombies = schema.zombies();
    println!(
        "[gen-e2e] policy: {allowed} allowed / {denied} denied (gene2e.rs::OP_POLICY) / {} zombie \
         (declared but unhandled in full.rs)",
        zombies.len()
    );

    // ZOMBIE OPS: declared in `DebugEvent`, no match arm — the dispatch's
    // catch-all logs "Unhandled" and returns ok. A test against one is
    // VACUOUSLY GREEN: it passes while doing nothing, and reports as coverage.
    // Not offered to the generator, rejected by the gate, shouted about here.
    if !zombies.is_empty() {
        eprintln!(
            "\n!! [gen-e2e] {} ZOMBIE OP(S): declared in DebugEvent but with NO MATCH ARM in the \
             dispatch. The catch-all logs \"Unhandled\" and answers `ok`, so a test using one \
             PASSES WHILE DOING NOTHING:",
            zombies.len()
        );
        for o in &zombies {
            eprintln!("!!   {o}");
        }
        eprintln!(
            "!! They are NOT shown to the generator and are REJECTED by the gate. Implement them \
             in {FULL_RS} (or delete the variant); they light up again automatically — this scan \
             is derived from the code, not from a list.\n"
        );
    }

    // A NEW `DebugEvent` variant must never be silently allowed nor silently
    // denied — it is reported here, and the gate rejects it until classified.
    let unclassified = schema.unclassified();
    if !unclassified.is_empty() {
        eprintln!(
            "\n!! [gen-e2e] {} UNCLASSIFIED OP(S) in DebugEvent — no OP_POLICY row covers them:",
            unclassified.len()
        );
        for o in &unclassified {
            eprintln!("!!   {o}");
        }
        eprintln!(
            "!! They are NOT shown to the generator and are REJECTED by the gate. Add a row to \
             gene2e.rs::OP_POLICY (allow, or deny with a one-line reason).\n"
        );
    }
    for o in schema.stale_policy_entries() {
        eprintln!("!! [gen-e2e] OP_POLICY classifies `{o}`, which no longer exists in full.rs");
    }
    if renamed > 0 {
        println!("[gen-e2e] {renamed} artifact(s) renumbered after corpus drift.");
    }

    // --- stale orphans ----------------------------------------------------
    for o in &p.orphans {
        if opts.prune && !opts.dry_run {
            let _ = fs::remove_file(o);
            println!("[prune] removed stale {}", o.display());
        } else {
            println!(
                "[stale] {} — no corpus line claims this (use --prune)",
                o.display()
            );
        }
    }

    if opts.dry_run {
        let mut by_tag: BTreeMap<&str, usize> = BTreeMap::new();
        for w in &p.todo {
            *by_tag.entry(w.tag.as_str()).or_default() += 1;
            if p.todo.len() <= 50 {
                println!(
                    "[dry] {:05} {} [{}] -> {}",
                    w.index,
                    w.hash,
                    w.tag,
                    w.out.display()
                );
            }
        }
        if p.todo.len() > 50 {
            for (tag, n) in &by_tag {
                println!("[dry] {n:6} x [{tag}]");
            }
            println!(
                "[dry] first: {:05} -> {}",
                p.todo[0].index,
                p.todo[0].out.display()
            );
            let last = &p.todo[p.todo.len() - 1];
            println!("[dry] last:  {:05} -> {}", last.index, last.out.display());
        }
        println!(
            "[dry-run] total={} already-done={} to-generate={} stale-orphans={}. Nothing launched.",
            p.total,
            p.already_done,
            p.todo.len(),
            p.orphans.len()
        );
        if opts.review_batch.is_some() {
            println!(
                "[dry-run] --review-batch: would then RUN those {} scenario(s) in-process and ask \
                 ONE reviewer ({} / {}) for {}/_review-{}.md",
                p.todo.len(),
                opts.review_model,
                opts.review_effort,
                out_dir.display(),
                batch_id(&p.todo),
            );
        }
        return Ok(());
    }
    if p.todo.is_empty() {
        println!("[gen-e2e] nothing left to do — every line already generated and valid.");
        if opts.review_batch.is_some() {
            println!(
                "[gen-e2e] --review-batch reviews the batch it GENERATED, and this run generated \
                 nothing — use --redo (or a corpus with outstanding lines) to get a batch to \
                 review."
            );
        }
        return Ok(());
    }
    let work = p.todo;

    which_claude()?;

    let example = fs::read_to_string(project_root.join(EXAMPLE_JSON))
        .with_context(|| format!("gen-e2e: cannot read {EXAMPLE_JSON}"))?;
    let schema_txt = schema_doc(&schema);
    let corpus_name = txt.display().to_string();
    let g = Generator {
        project_root,
        opts,
        schema: &schema,
        schema_txt: &schema_txt,
        example: &example,
        corpus: &corpus_name,
        out_dir: &out_dir,
    };

    let done_out = Mutex::new(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&done_file)?,
    );
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(opts.jobs)
        .build()?;
    // `map` + `collect` (not `for_each` + counters): rayon keeps an indexed
    // parallel iterator's ORDER, so the outcomes line up with `work` positionally
    // and `--review-batch` can hand each corpus line its own artifact. `None` =
    // generated and validated, `Some(e)` = rejected, artifact deleted.
    let outcomes: Vec<Option<String>> = pool.install(|| {
        use rayon::prelude::*;
        work.par_iter()
            .map(|w| match generate_one(&g, w) {
                Ok(()) => {
                    // ONLY now is the line done: the artifact landed and
                    // validated. The key is the CONTENT HASH, not the line
                    // number, so the list survives a corpus regeneration.
                    if let Ok(mut f) = done_out.lock() {
                        let _ = writeln!(
                            f,
                            "{}\t{:05}\t{}",
                            w.hash,
                            w.index,
                            w.out.file_name().unwrap_or_default().to_string_lossy()
                        );
                    }
                    println!("[ok]   {:05} {}", w.index, w.out.display());
                    None
                }
                Err(e) => {
                    let _ = fs::remove_file(&w.out); // never leave an invalid artifact
                    println!(
                        "[fail] {:05} [{}] — {e:#}  (not marked done)",
                        w.index, w.tag
                    );
                    Some(format!("{e:#}"))
                }
            })
            .collect()
    });

    let fail = outcomes.iter().filter(|o| o.is_some()).count();
    let ok = outcomes.len() - fail;
    println!(
        "\n[gen-e2e] {ok} generated, {fail} failed -> {}",
        out_dir.display()
    );
    if fail > 0 {
        println!("[gen-e2e] re-run the same command to retry the failures (resume is automatic).");
    }

    // --- the review checkpoint -------------------------------------------
    if opts.review_batch.is_some() {
        return review_batch(&g, &work, &outcomes);
    }
    Ok(())
}

fn generate_one(g: &Generator<'_>, w: &Work) -> Result<()> {
    let prompt = build_prompt(g.schema_txt, g.example, &w.line);
    let raw = claude(&prompt, &g.opts.model, &g.opts.effort)?;

    // LESSON 1: a rate-limited agent exits 0 and answers in PLAIN TEXT. It must
    // never be written out as a test.
    let json = match extract_json(&raw) {
        Some(j) if !looks_rate_limited(&raw[..raw.find('{').unwrap_or(0)]) => j,
        _ => {
            let head: String = raw.chars().take(120).collect();
            bail!(
                "no JSON in the reply (rate-limited / refusal?): {:?}",
                head.trim()
            );
        }
    };

    validate(g.schema, json)?;

    // Write the agent's JSON VERBATIM (only `_source_hash`/`_source` spliced in
    // front): serde_json's Map is a BTreeMap here, so a re-emit would sort the
    // keys ("css" before "op") and wreck the readability that the
    // array-of-lines format exists for. The stamp is what makes the out-dir a
    // self-contained resume record — the done-list is only a cache.
    let stamped = artifact_text(json, w);
    debug_assert!(validate(g.schema, &stamped).is_ok());
    fs::write(&w.out, stamped).with_context(|| format!("cannot write {}", w.out.display()))?;
    Ok(())
}

// ===========================================================================
// THE REVIEW LOOP  (`--review-batch N`)
// ===========================================================================
//
// generate N -> run exactly those N -> ONE review agent -> report -> STOP.
//
// Everything below is PURE except `review_batch` itself (which runs the
// scenarios and spawns the agent), so the report can be unit-tested without a
// single token being spent.

/// One corpus line, the artifact generated for it, and what happened when it ran.
#[derive(Debug, Clone)]
pub struct ReviewEntry {
    /// The corpus line the test was generated FROM — the thing the JSON is
    /// supposed to express. Without it a reviewer can only judge the JSON
    /// against itself.
    pub line: String,
    /// Where the artifact is (or would have been).
    pub path: PathBuf,
    /// The generated JSON, verbatim. `None` when generation itself failed.
    pub json: Option<String>,
    /// Why generation failed (gate rejection, rate limit, unreadable artifact).
    pub gen_error: Option<String>,
    /// The run outcome — `None` only when there was nothing to run.
    pub run: Option<E2eTestResult>,
}

impl ReviewEntry {
    /// Damage-PNG paths this scenario writes. Empty = no visual artifact.
    fn png_paths(&self) -> Vec<String> {
        let Some(json) = self.json.as_deref() else {
            return Vec::new();
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
            return Vec::new();
        };
        v.get("steps")
            .and_then(|s| s.as_array())
            .map(|steps| {
                steps
                    .iter()
                    .filter(|s| s.get("op").and_then(|o| o.as_str()) == Some(PNG_OP))
                    .filter_map(|s| s.get("path").and_then(|p| p.as_str()))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The name the scenario carries: the `name` of its JSON, else the name of
    /// the run that came back for it.
    fn scenario_name(&self) -> Option<String> {
        self.json
            .as_deref()
            .and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
            .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(str::to_string))
            .or_else(|| self.run.as_ref().map(|r| r.name.clone()))
    }

    /// Steps that failed, as `(index, op, error)`.
    fn failed_steps(&self) -> Vec<(usize, &str, &str)> {
        self.run.as_ref().map_or_else(Vec::new, |r| {
            r.steps
                .iter()
                .filter(|s| s.status == "fail")
                .map(|s| {
                    (
                        s.step_index,
                        s.op.as_str(),
                        s.error.as_deref().unwrap_or("(no error text)"),
                    )
                })
                .collect()
        })
    }

    /// Did this scenario fail because the RUNNER cannot drive it?
    ///
    /// Mechanical, not editorial: `Runner::unsupported` stamps every such error
    /// with `HARNESS_MARKER`, so the attribution the review report makes for
    /// these is a fact. Anything else that failed is left UNATTRIBUTED for the
    /// agent to argue about — silently calling those "engine bugs" would be the
    /// same false confidence this whole corpus exists to avoid.
    fn harness_gaps(&self) -> Vec<&str> {
        self.failed_steps()
            .into_iter()
            .filter(|(_, _, e)| e.contains(HARNESS_MARKER))
            .map(|(_, _, e)| e)
            .collect()
    }

    /// Did the scenario run and fail? A `pending` scenario (it says what it
    /// waits for - the prompt tells the model to write one for a line the ops
    /// cannot express) and a skipped one ran no step: they are parked, not red.
    fn is_red(&self) -> bool {
        self.run
            .as_ref()
            .is_some_and(|r| !matches!(r.status.as_str(), "pass" | "pending" | "skip"))
    }

    /// The reason a scenario ran no step, if it is parked.
    fn parked(&self) -> Option<(&str, &str)> {
        let r = self.run.as_ref()?;
        matches!(r.status.as_str(), "pending" | "skip").then(|| {
            (
                r.status.as_str(),
                r.skip_reason.as_deref().unwrap_or("(no reason given)"),
            )
        })
    }

    /// `generated` / `gate-rejected`, and `pass` / `fail` / `pending` / `not run`.
    fn status(&self) -> (&'static str, &'static str) {
        let gen = if self.json.is_some() {
            "ok"
        } else {
            "REJECTED"
        };
        let run = match self.run.as_ref().map(|r| r.status.as_str()) {
            Some("pass") => "pass",
            Some("pending") => "PENDING",
            Some("skip") => "skip",
            Some(_) => "FAIL",
            None => "not run",
        };
        (gen, run)
    }
}

/// One batch: what was generated, what it did, and how to name the report.
#[derive(Debug, Clone)]
pub struct ReviewBatch {
    /// Stable id derived from the batch's content (see `batch_id`).
    pub id: String,
    /// Corpus the lines came from, for the report header.
    pub corpus: String,
    /// Where the artifacts and the report live.
    pub out_dir: PathBuf,
    pub entries: Vec<ReviewEntry>,
    /// The cargo-test-style verdict block EXACTLY as
    /// `azul_layout::e2e::render_report` rendered it (ANSI and all — the markdown
    /// renderer strips it). Not re-derived here: the report must agree with
    /// `azul-doc e2e` down to the wording.
    pub verdict_block: String,
}

/// Content address of a batch: which lines it covered. Deterministic, so
/// reviewing the same batch twice replaces its report instead of littering the
/// out-dir with near-duplicates.
#[must_use]
pub fn batch_id(work: &[Work]) -> String {
    let joined = work
        .iter()
        .map(|w| w.hash.as_str())
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{:05}-n{}-{}",
        work.first().map_or(0, |w| w.index),
        work.len(),
        &line_hash(&joined)[..8]
    )
}

/// Drop ANSI SGR escapes so `render_report`'s output can be embedded in
/// markdown. (Only `ESC [ … m` is ever emitted.)
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        // ESC [ <params> m
        for c in chars.by_ref() {
            if c.is_ascii_alphabetic() {
                break;
            }
        }
    }
    out
}

/// A markdown table cell: no pipes, no newlines, bounded length.
fn cell(s: &str, max: usize) -> String {
    let flat: String = s
        .chars()
        .map(|c| {
            if c == '|' {
                '/'
            } else if c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();
    if flat.chars().count() <= max {
        return flat;
    }
    flat.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}

/// Whose fault the RUNNER says a red step is, when the error itself carries it.
///
/// `assert_changed` found no damage and compared a fresh render with the
/// snapshot (`azul_layout::e2e` writes the two markers, this reads them): pixels
/// that differ are an `ENGINE` fault (the window changed and nothing was
/// repainted), pixels that do not are a `BAD TEST` (a control that cannot move
/// one). A fact of the run, not an opinion; any other red step stays open.
fn runner_attribution(error: &str) -> Option<&'static str> {
    if error.contains(STALE_SCREEN_MARKER) {
        Some("ENGINE")
    } else if error.contains(DEAD_CONTROL_MARKER) {
        Some("BAD TEST")
    } else {
        None
    }
}

/// THE MACHINE-CHECKED HALF OF THE REPORT. PURE.
///
/// Everything here is a fact the agent cannot talk its way out of: what ran,
/// what failed, which failures the runner itself confessed to, and which
/// scenarios produced no PNG. It is written even when the agent's half is
/// missing, so a rate-limited review still leaves something usable behind.
#[must_use]
pub fn render_facts(b: &ReviewBatch) -> String {
    let n = b.entries.len();
    let generated = b.entries.iter().filter(|e| e.json.is_some()).count();
    let passed = b
        .entries
        .iter()
        .filter(|e| e.run.as_ref().is_some_and(|r| r.status == "pass"))
        .count();
    let failed = b.entries.iter().filter(|e| e.is_red()).count();
    let parked = b.entries.iter().filter(|e| e.parked().is_some()).count();
    let with_png = b
        .entries
        .iter()
        .filter(|e| !e.png_paths().is_empty())
        .count();
    // Generated, loaded and no result came back for it: nothing may hide here.
    let unrun = b
        .entries
        .iter()
        .filter(|e| e.json.is_some() && e.run.is_none())
        .count();

    let mut s = String::new();
    s.push_str(&format!("# gen-e2e batch review — `{}`\n\n", b.id));
    s.push_str(
        "Written by `azul-doc gen-e2e --review-batch N`. The **FACTS** section is \
         machine-generated from the actual run and cannot be argued with. The **REVIEW** section \
         is one agent's reading of it, and is advisory: nothing in it is applied \
         automatically.\n\n",
    );
    s.push_str(&format!(
        "- corpus: `{}`\n- out-dir: `{}`\n- batch: **{n}** line(s) — {generated} generated, {} \
         rejected by the gate\n- run: **{passed} passed, {failed} failed**{}{}\n- run mode: every \
         red step of a scenario is reported (keep-going), results are paired with their FILES, \
         not with names\n- scenarios that capture an image: {with_png}/{generated} (NOT \
         required — images are triage material, see below)\n\n",
        b.corpus,
        b.out_dir.display(),
        n - generated,
        if parked == 0 {
            String::new()
        } else {
            format!(", {parked} parked (pending / skipped: no step ran)")
        },
        if unrun == 0 {
            String::new()
        } else {
            format!(", **{unrun} NOT RUN** (loaded, but no result came back)")
        },
    ));

    s.push_str("## FACTS\n\n### Batch\n\n");
    s.push_str("| # | corpus line | artifact | gen | run | asserts | PNG |\n");
    s.push_str("|---|---|---|---|---|---|---|\n");
    for (i, e) in b.entries.iter().enumerate() {
        let (gen, run) = e.status();
        let asserts = e.json.as_deref().map_or(0, count_assertions);
        let png = if e.png_paths().is_empty() {
            "**none**"
        } else {
            "yes"
        };
        s.push_str(&format!(
            "| {} | {} | `{}` | {gen} | {run} | {asserts} | {png} |\n",
            i + 1,
            cell(&e.line, 90),
            cell(
                &e.path
                    .file_name()
                    .map_or_else(String::new, |f| f.to_string_lossy().into_owned()),
                60
            ),
        ));
    }

    // The runner's own words, not a re-derivation — but as markdown, so the
    // colour escapes come out here (rendering, not the caller's business).
    s.push_str("\n### Run verdict (verbatim from `azul_layout::e2e::render_report`)\n\n```\n");
    s.push_str(strip_ansi(&b.verdict_block).trim_start_matches('\n'));
    s.push_str("```\n");

    // ---- failures, attributed -------------------------------------------
    let failures: Vec<&ReviewEntry> = b.entries.iter().filter(|e| e.is_red()).collect();
    s.push_str("\n### Failures, attributed\n\n");
    if failures.is_empty() {
        s.push_str(
            "None. (A batch with zero failures is not automatically a good batch — see the review \
             below: a test can pass because it asserts nothing.)\n",
        );
    } else {
        for e in failures {
            let gaps = e.harness_gaps();
            let verdict = if gaps.is_empty() {
                "**UNATTRIBUTED** — engine bug or bad test? the review must decide"
            } else {
                "**HARNESS** — the headless runner cannot drive this (port task, NOT an engine bug)"
            };
            s.push_str(&format!("- `{}`\n  - {verdict}\n", cell(&e.line, 120)));
            for (k, (idx, op, err)) in e.failed_steps().into_iter().enumerate() {
                // The first red step is the cause; the later ones ran on what it
                // left behind and may be its echo.
                let also = if k == 0 {
                    ""
                } else {
                    " (also red; may follow from the first)"
                };
                // What the runner settled itself: `assert_changed` compared a fresh
                // render with the snapshot (see `runner_attribution`).
                let attributed = runner_attribution(err).map_or_else(String::new, |who| {
                    format!(" **[{who}, attributed by the runner]**")
                });
                s.push_str(&format!(
                    "  - step {idx} `{op}`{also}{attributed}: {}\n",
                    cell(err, 400)
                ));
            }
        }
    }

    // ---- parked: pending / skipped ---------------------------------------
    // A scenario that says what it waits for (`pending`) is not a failure and
    // not a pass - and WHAT it waits for is the reviewer's best evidence of a
    // missing op.
    let parked: Vec<&ReviewEntry> = b.entries.iter().filter(|e| e.parked().is_some()).collect();
    if !parked.is_empty() {
        s.push_str("\n### Parked (ran no step)\n\n");
        for e in parked {
            if let Some((status, why)) = e.parked() {
                s.push_str(&format!(
                    "- `{}`\n  - {status}: {}\n",
                    cell(&e.line, 120),
                    cell(why, 300)
                ));
            }
        }
    }

    // ---- images ----------------------------------------------------------
    // Deliberately NOT a coverage gate. A PNG per test is 13k images nobody
    // opens, and for the idle half of the corpus the capture is blank by
    // construction. Images are what the REVIEWER uses to investigate a failure.
    s.push_str(&format!(
        "\n### Images\n\nNo scenario is required to capture one. Triage images the reviewer wrote \
         live under `{TRIAGE_DIR}` — inside `target/`, which `.gitignore` already excludes, so \
         they are never committed. Look there, not in git.\n\n"
    ));
    for e in b.entries.iter().filter(|e| !e.png_paths().is_empty()) {
        s.push_str(&format!(
            "- captures its own image(s) `{}`: `{}`\n",
            e.png_paths().join("`, `"),
            cell(&e.line, 100)
        ));
    }
    let mut pngs: Vec<String> = b.entries.iter().flat_map(ReviewEntry::png_paths).collect();
    pngs.sort();
    let dupes: Vec<&String> = pngs
        .windows(2)
        .filter(|w| w[0] == w[1])
        .map(|w| &w[0])
        .collect();
    if !dupes.is_empty() {
        s.push_str(
            "\n**PNG PATH COLLISION** — scenarios run in parallel, so these race and the \
             surviving file is whichever finished last:\n",
        );
        for d in dupes {
            s.push_str(&format!("- `{d}`\n"));
        }
    }

    // ---- gate rejections -------------------------------------------------
    let rejected: Vec<&ReviewEntry> = b.entries.iter().filter(|e| e.json.is_none()).collect();
    if !rejected.is_empty() {
        s.push_str("\n### Rejected before it ever ran (generation gate)\n\n");
        for e in rejected {
            s.push_str(&format!(
                "- `{}`\n  - {}\n",
                cell(&e.line, 120),
                cell(e.gen_error.as_deref().unwrap_or("unknown"), 400)
            ));
        }
    }

    // ---- duplicate names -------------------------------------------------
    // By the name each ARTIFACT carries, whether or not a result came back for
    // it: counting only the entries that received a run made this notice unable
    // to fire when the pairing lost them.
    let mut names: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, e) in b.entries.iter().enumerate() {
        if let Some(name) = e.scenario_name() {
            names.entry(name).or_default().push(i + 1);
        }
    }
    let clashes: Vec<(&String, &Vec<usize>)> =
        names.iter().filter(|(_, rows)| rows.len() > 1).collect();
    if !clashes.is_empty() {
        s.push_str(
            "\n### Duplicate scenario names\n\nThis run paired every result with its own FILE, so \
             it is attributable. But the verdict report prints names, and an `expect` marker \
             pairs by name: scenarios that share one read as one there. `gen-e2e` appends \
             `_<8 hex of the source hash>` to every name it writes; these artifacts predate that \
             (or were written by hand).\n",
        );
        for (name, rows) in clashes {
            let rows: Vec<String> = rows.iter().map(|r| format!("#{r}")).collect();
            s.push_str(&format!("- `{name}`: rows {}\n", rows.join(", ")));
        }
    }

    s
}

/// Number of `assert_*` steps in a scenario — a crude but honest proxy for "does
/// this test check anything at all", shown per row so a 1-assertion test stands
/// out next to its corpus line.
fn count_assertions(json: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|v| {
            Some(
                v.get("steps")?
                    .as_array()?
                    .iter()
                    .filter(|s| {
                        s.get("op")
                            .and_then(|o| o.as_str())
                            .is_some_and(|o| o.starts_with("assert_"))
                    })
                    .count(),
            )
        })
        .unwrap_or(0)
}

/// The DENIED half of `OP_POLICY`, rendered for the reviewer. The generator only
/// ever sees the ALLOWED half; the reviewer needs both, because "the model
/// worked around a denied op" is one of the findings we are paying for.
fn policy_doc(schema: &Schema) -> String {
    let mut s = String::from("### OPS THE GENERATOR MAY NOT USE (`gene2e.rs::OP_POLICY`)\n");
    for (op, why) in OP_POLICY.iter().filter(|(_, w)| w.is_some()) {
        s.push_str(&format!("- {op}: {}\n", why.unwrap_or("")));
    }
    let zombies = schema.zombies();
    s.push_str(&format!(
        "\n### DECLARED BUT UNIMPLEMENTED (`{}` zombie op(s): a match arm is missing, so the \
         dispatch answers `ok` without doing anything — hidden from the generator and rejected by \
         the gate)\n",
        zombies.len()
    ));
    for z in &zombies {
        s.push_str(&format!("- {z}\n"));
    }
    let unclassified = schema.unclassified();
    if !unclassified.is_empty() {
        s.push_str(
            "\n### UNCLASSIFIED (in DebugEvent, no OP_POLICY row — denied until classified)\n",
        );
        for u in &unclassified {
            s.push_str(&format!("- {u}\n"));
        }
    }
    s
}

/// How the reviewer LOOKS AT a failing scenario instead of guessing about it.
///
/// The op signature is rendered FROM THE PARSED SCHEMA, never from memory: if
/// `CaptureDamagePng` gains or loses a field, this instruction follows without
/// an edit here. Everything the agent writes goes to `TRIAGE_DIR` — inside
/// `target/`, which `.gitignore` already excludes — and the committed scenario
/// is never touched, so triage cannot leak into the corpus or into git.
fn triage_doc(schema: &Schema) -> String {
    let Some(op) = schema.known_op(PNG_OP) else {
        return format!(
            "(this engine has no `{PNG_OP}` op, so there is no way to capture an image — \
             attribute failures from the error text alone.)\n"
        );
    };
    let params = op
        .params
        .iter()
        .map(|(n, req)| if *req { n.clone() } else { format!("{n}?") })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "You have TOOLS (bash, read, write) and your working directory is the REPO ROOT. For any \
         scenario you cannot attribute from its error text alone — and for any failure you are \
         about to call an ENGINE bug — LOOK AT WHAT RENDERED instead of guessing:\n\n1. COPY the \
         scenario JSON to `{TRIAGE_DIR}<name>.json`. NEVER edit the artifact in the out-dir: it \
         is the committed test, it must stay byte-identical, and triage steps that leak into it \
         would be committed noise.\n2. In the COPY, insert `{PNG_OP}` steps ({params}) at the \
         interesting points — right after the mount settles, and again after the interaction — \
         writing to `{TRIAGE_DIR}<name>-<step>.png`. It writes the frame MASKED TO THE DAMAGE \
         REGION (transparent elsewhere), which is exactly the picture that settles \"did the \
         engine repaint the right area?\". `which` selects `paint` (default) or `present` damage; \
         `crop` trims to the damage bounding box. A 1x1 transparent PNG means THERE WAS NO DAMAGE \
         — that is a finding, not a broken capture.\n3. Run the copy: `./target/release/azul-doc \
         e2e {TRIAGE_DIR}<name>.json`, then open the PNGs you wrote and say what you see.\n4. Do \
         NOT use `take_native_screenshot` (no host hook is installed anywhere — it fails in every \
         runner we have) and do not bother with `take_screenshot` (it answers with base64 in the \
         step response, it writes no file you can open).\n5. Images are YOUR WORKING MATERIAL, \
         not an artifact. `{TRIAGE_DIR}` is under `target/`, which `.gitignore` excludes — leave \
         them there, never copy one into the repo, never `git add` anything.\n"
    )
}

/// The review agent's prompt. PURE, so it is unit-testable.
///
/// `current_prompt` must be the ACTUAL output of `build_prompt` — the reviewer's
/// most valuable job is proposing a fix to a specific paragraph of it, and it
/// cannot do that from a paraphrase.
#[must_use]
pub fn build_review_prompt(
    b: &ReviewBatch,
    current_prompt: &str,
    policy: &str,
    triage: &str,
) -> String {
    let mut scenarios = String::new();
    for (i, e) in b.entries.iter().enumerate() {
        scenarios.push_str(&format!(
            "\n----- SCENARIO {} of {} -----\nCORPUS LINE: {}\nARTIFACT: {}\n",
            i + 1,
            b.entries.len(),
            e.line,
            e.path.display()
        ));
        match e.json.as_deref() {
            Some(j) => scenarios.push_str(&format!("GENERATED JSON:\n{j}\n")),
            None => scenarios.push_str(&format!(
                "NOT GENERATED — rejected by the mechanical gate: {}\n",
                e.gen_error.as_deref().unwrap_or("unknown")
            )),
        }
        match e.run.as_ref() {
            Some(r) => {
                scenarios.push_str(&format!(
                    "RUN RESULT: {} ({}/{} steps passed, {} ms)\n",
                    r.status, r.steps_passed, r.step_count, r.duration_ms
                ));
                for (idx, op, err) in e.failed_steps() {
                    scenarios.push_str(&format!("  FAILING STEP {idx} `{op}`: {err}\n"));
                }
            }
            None => scenarios.push_str("RUN RESULT: not run\n"),
        }
    }

    format!(
        r#"You are reviewing ONE BATCH of machine-generated azul e2e tests, as the checkpoint in a
deliberately slow loop: generate N tests -> run them -> you review -> a HUMAN edits the
generator prompt -> rebuild -> generate the next N. The point of the loop is to catch a bad
prompt after 10 wasted tests instead of after 13,000.

Print MARKDOWN only. Your output is appended verbatim under a machine-generated facts section.
Start with a single line `VERDICT: ACCEPT` or `VERDICT: REJECT` — REJECT if these tests should
NOT be trusted as a template for the next few thousand. A review that always accepts is worse
than no review; if the batch is weak, say so on line 1 and say exactly why.

## WHAT THESE TESTS ARE FOR
Headless BEHAVIOUR tests: mock input / a callback-style DOM mutation goes in, and the test
asserts what the ENGINE DECIDED to do — damage, repaint liveness and soundness, settling to
idle, bounded work, resource counts, focus/scroll/selection state. Layout and geometry are
explicitly NOT their job (`azul-doc reftest` owns that). A test is a SPECIFICATION of correct
behaviour: a test that FAILS because the engine is buggy is a SUCCESS for this corpus. A test
that passes because it was softened until it could not fail is worse than no test at all.

## WHAT THE GENERATOR WAS TOLD (verbatim `build_prompt` output, one representative line)
<<<GENERATOR_PROMPT
{current_prompt}
GENERATOR_PROMPT

## POLICY THE REVIEWER NEEDS (the generator was shown only the ALLOWED ops)
{policy}

## HOW TO INVESTIGATE A FAILURE — LOOK, DO NOT GUESS
{triage}

## THE BATCH — machine-checked facts (this is also the head of the report you are appending to)
{facts}

## THE BATCH — full scenarios
{scenarios}

## WRITE EXACTLY THESE SECTIONS

### 1. Per-test verdict
One bullet per scenario: `<corpus line>` -> FAITHFUL / DRIFTED / VACUOUS / SOFTENED, one
sentence of justification. Judge the JSON against ITS CORPUS LINE, not against itself. Flag
specifically: a test that quietly tests something ELSE than the line asks for; an assertion
that cannot fail (a bound so wide nothing violates it, `le` where the line implies `eq`, an
`assert_exists` on a node that was never touched); a test whose only assertion is that
mounting worked; and a green test that is green for the wrong reason.

### 2. Missing ops — THE HIGHEST-VALUE SECTION
Where did the model need something the op set does not offer? Look for: a scenario that
expresses its line only approximately; a workaround (driving via `mount` twice because there
is no op for the real interaction, faking a gesture with raw mouse ops, asserting a proxy
property); a line whose intent needs a DENIED op (say so, and say whether the denial is right);
an op that exists but is a zombie. Output a table: `intent | what was written instead | op
needed | exists? (no / denied / zombie)`. This drives what gets ported next, so be concrete —
name the op you would add and the params it needs. If nothing is missing, say so plainly.

### 3. Harness vs engine, for EVERY failure
For each failing scenario decide: ENGINE (the engine really is wrong — keep the test, it is a
find), HARNESS (the headless runner cannot drive it — a port task, the test may be fine) or BAD
TEST (it asserts something the line never claimed). The facts section already marks the
mechanical ones: any error containing "{HARNESS_MARKER}" is a runner gap
by construction, and names the `CallbackChange` arm to port from
`dll/src/desktop/shell2/common/event.rs::apply_user_change`. The runner also settles a missing
repaint itself: an `assert_changed` error containing "{STALE_SCREEN_MARKER}" means no damage was
reported although a fresh render differs from the snapshot (ENGINE), and one containing
"{DEAD_CONTROL_MARKER}" means the fresh render equals the snapshot, so nothing the scenario did
could change a pixel (BAD TEST: the control is dead); the facts tag those steps. For everything
else, USE THE TRIAGE
PROCEDURE ABOVE — copy the scenario, add `{PNG_OP}` steps, run it, and look at the image — before
you commit to an answer, and say what the image showed. Do not label a failure ENGINE just
because it is not marked HARNESS; "I could not tell, and here is the experiment that would" is
an acceptable and useful answer.

### 4. Prompt defects + a concrete proposed replacement
This is what the human acts on. For each defect: quote the EXACT paragraph of the generator
prompt above that caused it, then give a drop-in replacement as a fenced block, then one line
on which scenario in this batch proves the defect. Nothing you write is applied automatically —
a human pastes it into `build_prompt` in `doc/src/gene2e.rs` and rebuilds — so the replacement
must be complete, self-contained prose, not an instruction like "mention X". If the prompt is
fine, say "no prompt change" rather than inventing one.

### 5. Visual triage
What you captured, where you put it (paths under `{TRIAGE_DIR}`, which is gitignored and stays
uncommitted), and what each image actually showed — including the ones that told you nothing.
If you triaged nothing because nothing failed, say exactly that; do NOT capture images for
passing scenarios just to fill the section, and do NOT propose adding capture steps to the
committed tests — a PNG per test is 13k images nobody opens.

### 6. Verdict rationale
Two or three sentences: would you let this prompt run against thousands of lines? What is the
single highest-leverage change?
"#,
        facts = render_facts(b),
    )
}

/// Did the reviewer ACCEPT (`Some(false)`), REJECT (`Some(true)`), or ignore the
/// format it was given (`None`)?
///
/// Read out of the HEAD of the reply only, and tolerant of a stray preamble or
/// markdown emphasis around the line — but never of a verdict buried on page
/// three, which would let an unrelated quotation flip the gate.
#[must_use]
pub fn verdict_of(review: &str) -> Option<bool> {
    let head: String = review.chars().take(400).collect();
    if head.contains("VERDICT: REJECT") {
        Some(true)
    } else if head.contains("VERDICT: ACCEPT") {
        Some(false)
    } else {
        None
    }
}

/// Is this reply a REVIEW, or the plain-text limit/refusal message a `claude -p`
/// answers with while still exiting 0 (LESSON 1)?
///
/// The rate-limit scan runs on the HEAD ONLY, and is skipped entirely once the
/// reply opens with the verdict it was asked for. A review is prose about tests
/// being weak: "insufficient", "try again" and "overloaded" are all things a
/// GENUINE review says, and scanning the whole document for them would throw
/// away exactly the reports worth reading.
#[must_use]
pub fn is_usable_review(reply: &str) -> bool {
    if reply.trim().len() <= MIN_REVIEW_CHARS {
        return false;
    }
    let head: String = reply.chars().take(400).collect();
    verdict_of(reply).is_some() || !looks_rate_limited(&head)
}

/// Facts + the agent's markdown, or facts + a loud placeholder.
#[must_use]
pub fn assemble_report(facts: &str, agent: Option<&str>) -> String {
    let mut s = String::from(facts);
    s.push_str("\n---\n\n## REVIEW (agent)\n\n");
    match agent {
        Some(a) => s.push_str(a.trim()),
        None => s.push_str(
            "**THE REVIEW AGENT PRODUCED NOTHING USABLE** (rate-limited, refused, or empty). The \
             facts above are complete and were not affected. Re-run the same command to retry the \
             review — generation is incremental, so the batch above will not be regenerated.",
        ),
    }
    s.push('\n');
    s
}

/// Everything the generation pass and the review pass both need, resolved once.
struct Generator<'a> {
    project_root: &'a Path,
    opts: &'a GenE2eOptions,
    schema: &'a Schema,
    /// The rendered, policy-filtered schema section of the agent prompt.
    schema_txt: &'a str,
    /// The worked example handed to every agent.
    example: &'a str,
    /// Corpus path, for the report header.
    corpus: &'a str,
    out_dir: &'a Path,
}

/// Give every entry the result of ITS OWN test.
///
/// `results[k]` is the result of the k-th test that was loaded, and `owner[k]`
/// the entry (the artifact FILE) it came from. A file of several tests gives its
/// entry the first one's result.
fn pair_results(entries: &mut [ReviewEntry], owner: &[usize], results: &[E2eTestResult]) {
    for (r, &i) in results.iter().zip(owner) {
        if let Some(entry) = entries.get_mut(i) {
            if entry.run.is_none() {
                entry.run = Some(r.clone());
            }
        }
    }
}

/// Run the batch that was just generated, have ONE agent review it, write the
/// report, and STOP.
fn review_batch(g: &Generator<'_>, work: &[Work], outcomes: &[Option<String>]) -> Result<()> {
    let opts = g.opts;
    // Scenario paths (`capture_damage_png`) are REPO-ROOT-relative, exactly as
    // when CI runs `azul-doc e2e` from the root — but `main()` chdir'd into
    // `doc/`, so running from here would scatter PNGs into `doc/target/`.
    // Absolutise everything we still need first, then move.
    let out_dir = fs::canonicalize(g.out_dir).unwrap_or_else(|_| g.out_dir.to_path_buf());
    let mut entries: Vec<ReviewEntry> = Vec::with_capacity(work.len());
    for (w, outcome) in work.iter().zip(outcomes) {
        let path = fs::canonicalize(&w.out).unwrap_or_else(|_| w.out.clone());
        let json = match outcome {
            Some(_) => None,
            None => fs::read_to_string(&path).ok(),
        };
        entries.push(ReviewEntry {
            line: w.line.clone(),
            path,
            gen_error: outcome
                .clone()
                .or_else(|| json.is_none().then(|| "artifact unreadable".to_string())),
            json,
            run: None,
        });
    }
    std::env::set_current_dir(g.project_root)
        .with_context(|| format!("cannot enter project root {}", g.project_root.display()))?;

    // --- run exactly this batch, through the SAME runner `azul-doc e2e` uses ---
    let mut tests: Vec<E2eTest> = Vec::new();
    let mut owner: Vec<usize> = Vec::new(); // tests[i] came from entries[owner[i]]
    for (i, e) in entries.iter_mut().enumerate() {
        if e.json.is_none() {
            continue;
        }
        match load_e2e_tests(&e.path) {
            Ok(loaded) => {
                for t in loaded {
                    tests.push(t);
                    owner.push(i);
                }
            }
            Err(err) => e.gen_error = Some(err),
        }
    }

    let verdict_block = if tests.is_empty() {
        println!("[review] nothing runnable in this batch — every line was rejected by the gate.");
        "(nothing ran: every line in this batch was rejected by the generation gate)\n".to_string()
    } else {
        // Every red step of every scenario, not only the first: a review that
        // sees one assertion of a scenario that has five hides four findings.
        crate::e2erun::keep_going(&mut tests);
        println!(
            "[review] running {} generated scenario(s) (keep-going) …",
            tests.len()
        );
        // IN ORDER: `results[k]` is the result of `tests[k]`, which came from
        // `entries[owner[k]]`. The pairing is by position - by FILE - because a
        // name is not an identity (six scenarios of the first batch shared one,
        // and every result landed on the first of them).
        let results = crate::e2erun::run_tests_in_order(&tests, Some(opts.jobs))?;
        pair_results(&mut entries, &owner, &results);
        let (report, v) = render_report(&tests, &results);
        eprint!("{report}");
        println!(
            "[review] {} passed, {} failed, {} xfail, {} xpass",
            v.passed, v.failed, v.xfail, v.xpass
        );
        report
    };

    let batch = ReviewBatch {
        id: batch_id(work),
        corpus: g.corpus.to_string(),
        out_dir: out_dir.clone(),
        entries,
        verdict_block,
    };
    let facts = render_facts(&batch);

    // --- ONE review agent -------------------------------------------------
    // The generator prompt VERBATIM, for a representative line of this batch:
    // the reviewer's job includes proposing a fix to a specific paragraph of it,
    // which it cannot do from a paraphrase.
    let representative = work.first().map_or("", |w| w.line.as_str());
    let current_prompt = build_prompt(g.schema_txt, g.example, representative);
    // The reviewer writes scenario COPIES and their captures here. Created up
    // front so an agent that only has `write` never has to reason about mkdir.
    if let Err(e) = fs::create_dir_all(TRIAGE_DIR) {
        eprintln!("!! [review] cannot create the triage dir {TRIAGE_DIR}: {e}");
    }
    let prompt = build_review_prompt(
        &batch,
        &current_prompt,
        &policy_doc(g.schema),
        &triage_doc(g.schema),
    );

    println!(
        "[review] asking ONE reviewer ({} / {}) about {} scenario(s) — {} KB of prompt",
        opts.review_model,
        opts.review_effort,
        batch.entries.len(),
        prompt.len() / 1024,
    );
    // LESSON 1 applies to the reviewer too: a rate-limited `claude -p` exits 0
    // and answers with the limit message as plain text. A report whose analysis
    // section is "You've reached your usage limit" must not read as a review.
    let agent = match claude(&prompt, &opts.review_model, &opts.review_effort) {
        Ok(a) if is_usable_review(&a) => Some(a),
        Ok(a) => {
            eprintln!(
                "!! [review] the reviewer's reply is not a review ({} chars, rate-limited or \
                 refused): {:?}",
                a.trim().len(),
                a.chars().take(120).collect::<String>().trim()
            );
            None
        }
        Err(e) => {
            eprintln!("!! [review] the review agent failed: {e:#}");
            None
        }
    };

    let report_path = out_dir.join(format!("_review-{}.md", batch.id));
    fs::write(&report_path, assemble_report(&facts, agent.as_deref()))
        .with_context(|| format!("cannot write {}", report_path.display()))?;

    let rejected = agent.as_deref().and_then(verdict_of).unwrap_or(false);
    if agent.as_deref().is_some_and(|a| verdict_of(a).is_none()) {
        eprintln!(
            "!! [review] the reviewer did not open with `VERDICT: ACCEPT|REJECT` — the report is \
             written, but READ IT before assuming this batch is good."
        );
    }

    let bar = "=".repeat(78);
    println!(
        "\n{bar}\n  STOP — DELIBERATE REVIEW GATE. Nothing more will be generated.\n{bar}\n\
         \n  This loop walks the corpus SLOWLY on purpose: a small batch, actually run, actually \
         reviewed, prompt improved, rebuilt — instead of mass-generating thousands of files and \
         sifting the wreckage afterwards.\n\
         \n  1. READ    {report}\n\
         \x20 2. EDIT    `build_prompt` in doc/src/gene2e.rs — apply the proposals YOU agree with.\n\
         \x20            The review never edits it: the prompt lives in Rust source so that every \
         change to it is reviewed, committed and blameable.\n\
         \x20 3. REBUILD cargo build --release -p azul-doc --bin azul-doc\n\
         \x20 4. REPEAT  re-run this exact command for the next batch — generation is incremental, \
         so the batch above is done and will not be regenerated.\n\
         \n  Triage images (uncommitted, gitignored): {TRIAGE_DIR}\n{bar}",
        report = report_path.display(),
    );

    if agent.is_none() {
        bail!(
            "gen-e2e: the batch ran but the REVIEW agent produced nothing usable — the facts \
             section of {} is complete, the analysis is missing. Re-run to retry just the review.",
            report_path.display()
        );
    }
    if rejected {
        bail!(
            "gen-e2e: the reviewer REJECTED this batch — do not generate more until the prompt is \
             fixed. See {}",
            report_path.display()
        );
    }
    Ok(())
}

/// The one place `claude -p` is spawned. Both the generator fleet and the single
/// reviewer go through it, so the flags (and the rate-limit hazard) cannot drift
/// apart.
fn claude(prompt: &str, model: &str, effort: &str) -> Result<String> {
    let out = Command::new("claude")
        .arg("-p")
        .arg(prompt)
        .arg("--model")
        .arg(model)
        .arg("--effort")
        .arg(effort)
        .arg("--permission-mode")
        .arg("bypassPermissions")
        .arg("--output-format")
        .arg("text")
        .stdin(Stdio::null())
        .output()
        .context("failed to spawn `claude`")?;

    if !out.status.success() {
        bail!("claude exited with {}", out.status);
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn which_claude() -> Result<()> {
    let ok = Command::new("claude")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        bail!("gen-e2e: the `claude` CLI is not on PATH");
    }
    Ok(())
}

/// `main()` chdir's into `doc/`, so a relative path from the user's shell has to
/// be resolved against the project root as well.
fn resolve(project_root: &Path, p: &Path) -> PathBuf {
    if p.is_absolute() || p.exists() {
        p.to_path_buf()
    } else {
        project_root.join(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf()
    }

    /// `--prune` deletes orphans, and the out-dir is the shared `e2e/` corpus
    /// that also holds the hand-written scenarios. Those carry no `_source_hash`
    /// and no corpus line will ever claim them, so if they counted as artifacts a
    /// single `--prune` would delete the whole hand-written suite. Only files
    /// this generator NAMED (`<NNNNN>-<slug>.json`) may be reclassified.
    #[test]
    fn prune_only_ever_considers_files_this_generator_wrote() {
        for generated in [
            "00001-idle-stability-mount-a-red-flexbox.json",
            "13223-some-tag-trailing.json",
            "00000-a.json",
        ] {
            assert!(
                is_generated_artifact(std::path::Path::new(generated)),
                "{generated} is a generated artifact and must stay prunable",
            );
        }

        // Every hand-written scenario shipped in e2e/, plus near-miss shapes.
        for foreign in [
            "op-tab-focus-next.json",
            "bug-dom-mutation-no-damage.json",
            "mock-font-exact-metrics.json",
            "0001-too-short-an-index.json",
            "000001-too-long-an-index.json",
            "abcde-not-digits.json",
            "00001.json",
            "00001-.json",
        ] {
            assert!(
                !is_generated_artifact(std::path::Path::new(foreign)),
                "{foreign} is NOT ours — pruning it would delete someone else's test",
            );
        }
    }

    /// The real corpus directory must survive a prune untouched.
    #[test]
    fn the_hand_written_e2e_corpus_is_never_prunable() {
        let dir = root().join("e2e");
        let mut checked = 0;
        for entry in std::fs::read_dir(&dir).expect("e2e/ must exist") {
            let path = entry.expect("readable dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let name = path.file_name().unwrap().to_str().unwrap();
            // Generated artifacts may legitimately live here too; only assert
            // about the hand-written ones.
            if is_generated_artifact(&path) {
                continue;
            }
            checked += 1;
            assert!(
                !name.chars().take(5).all(|c| c.is_ascii_digit()),
                "{name} looks generated but was classified as hand-written",
            );
        }
        assert!(
            checked > 0,
            "no hand-written scenarios found in {}",
            dir.display()
        );
    }

    #[test]
    fn schema_parses_the_real_full_rs() {
        let s = parse_schema(&root()).unwrap();
        for op in [
            "mount",
            "unmount",
            "tick_ms",
            "reset_frame_counters",
            "snapshot_frame",
            "snapshot_resources",
            "snapshot_managers",
            "get_frame_report",
            "capture_damage_png",
            "click",
            "wait",
            "wait_frame",
            "assert_damage",
            "assert_changed",
            "assert_damage_covers_changes",
            "assert_damage_incremental",
            "assert_idle_stable",
            "assert_work_bounded",
            "assert_resource_counts",
        ] {
            assert!(s.is_known(op), "op `{op}` not parsed out of full.rs");
        }
        assert!(!s.is_known("assert_nonexistent_thing"));
        let mount = s.known_op("mount").unwrap();
        assert_eq!(
            mount.params,
            vec![("html".into(), true), ("css".into(), false)]
        );
    }

    #[test]
    fn the_worked_example_passes_the_gate() {
        let s = parse_schema(&root()).unwrap();
        let ex = fs::read_to_string(root().join(EXAMPLE_JSON)).unwrap();
        validate(&s, &ex).unwrap();
    }

    #[test]
    fn the_gate_rejects_junk() {
        let s = parse_schema(&root()).unwrap();
        // rate-limit reply
        assert!(extract_json("You've reached your usage limit. Try again later.").is_none());
        // unknown op
        let bad = r#"{"name":"x","steps":[{"op":"teleport"},{"op":"assert_damage"}]}"#;
        assert!(validate(&s, bad).is_err());
        // geometry assertion — out of scope
        let geo = r##"{"name":"x","steps":[{"op":"assert_layout","selector":"#a",
            "property":"width","expected":60}]}"##;
        assert!(validate(&s, geo).is_err());
        // dangling snapshot reference
        let dangling = r#"{"name":"x","steps":[{"op":"assert_changed","vs":"before"}]}"#;
        assert!(validate(&s, dangling).is_err());
        // missing required param
        let missing = r#"{"name":"x","steps":[{"op":"tick_ms"},{"op":"assert_damage"}]}"#;
        assert!(validate(&s, missing).is_err());
        // no assertion at all
        let inert = r#"{"name":"x","steps":[{"op":"wait_frame"}]}"#;
        assert!(validate(&s, inert).is_err());
    }

    /// A param the op does not have is DROPPED by the runner, so a step that
    /// carries one asserts less than it says. The gate used to check only that
    /// REQUIRED params were PRESENT and never that the supplied ones exist, so
    /// `max_relayout` (the `s` dropped) was accepted here, ignored at run time,
    /// and reported green while asserting nothing.
    #[test]
    fn the_gate_rejects_a_param_the_op_does_not_have() {
        let s = parse_schema(&root()).unwrap();

        let typo = r#"{"name":"x","steps":[{"op":"wait_frame"},
            {"op":"assert_work_bounded","max_relayout":0}]}"#;
        let e = validate(&s, typo).unwrap_err().to_string();
        assert!(e.contains("max_relayout"), "{e}");
        assert!(e.contains("has no param"), "{e}");

        // …and the correctly spelled ones, including every bound added for the
        // min/exact half and the params only the run-time guard names, still
        // pass — the advertised surface and the accepted surface are the SAME
        // list by construction (`reject_guard_keys`).
        let good = r#"{"name":"x","steps":[
            {"op":"snapshot_frame","as":"before"},
            {"op":"wait_frame"},
            {"op":"assert_work_bounded","max_relayouts":1,"min_relayouts":1,
             "exact_dom_regens":0,"max_layout_passes":1,"allow_depth_cap":false},
            {"op":"assert_idle_stable","vs":"before","max_frames":5},
            {"op":"assert_damage","kind":"none","which":"paint","frame":"last"}]}"#;
        validate(&s, good).unwrap();
    }

    // -----------------------------------------------------------------------
    // Selectors: the schema is DERIVED, so the new params arrive by themselves
    // -----------------------------------------------------------------------

    /// The DOM-mutation ops and `mouse_move` name their target by CSS
    /// `selector` (`insert_node`: `parent_selector`). Nothing in this file lists
    /// those params: they are read out of the `DebugEvent` enum, rendered into
    /// the prompt's schema section, and checked by the gate - so a scenario
    /// that uses them validates, and the model is shown them.
    #[test]
    fn the_selector_params_of_the_mutation_ops_reach_the_schema_without_hand_edits() {
        let s = parse_schema(&root()).unwrap();
        let doc = schema_doc(&s);
        let line = |op: &str| -> String {
            doc.lines()
                .find(|l| l.starts_with(&format!("- {op} :")))
                .unwrap_or_else(|| panic!("the prompt does not offer `{op}`"))
                .to_string()
        };
        let optional = |op: &str, param: &str| {
            let def = s
                .known_op(op)
                .unwrap_or_else(|| panic!("`{op}` is not parsed"));
            assert!(
                def.params.contains(&(param.to_string(), false)),
                "`{op}` must take an OPTIONAL `{param}`, got {:?}",
                def.params
            );
        };

        for op in [
            "set_node_css_override",
            "set_node_classes",
            "set_node_text",
            "set_node_image",
            "delete_node",
        ] {
            optional(op, "selector");
            optional(op, "node_id");
            let l = line(op);
            assert!(l.contains("selector?") && l.contains("node_id?"), "{l}");
            assert!(
                l.contains("exactly one"),
                "the one-line doc names the rule: {l}"
            );
        }
        optional("insert_node", "parent_selector");
        optional("insert_node", "parent_id");
        let l = line("insert_node");
        assert!(
            l.contains("parent_selector?") && l.contains("parent_id?"),
            "{l}"
        );
        assert!(
            s.known_op("insert_node")
                .unwrap()
                .params
                .contains(&("node_type".to_string(), true)),
            "insert_node still needs its node_type"
        );
        for p in ["selector", "x", "y"] {
            optional("mouse_move", p);
        }
        let l = line("mouse_move");
        assert!(l.contains("selector?"), "{l}");
    }

    /// Every row of the engine's targeting table names params the op really has:
    /// a param renamed in `DebugEvent` must take its row with it, or the rule
    /// would silently stop applying to the op.
    #[test]
    fn every_row_of_the_engines_targeting_table_names_params_the_op_has() {
        use azul_layout::e2e::{Targeting, OP_TARGETING};

        let s = parse_schema(&root()).unwrap();
        for (op, rule) in OP_TARGETING {
            let def = s
                .known_op(op)
                .unwrap_or_else(|| panic!("OP_TARGETING has a row for `{op}`, which is no op"));
            let names: Vec<&str> = match rule {
                Targeting::SelectorOrId { selector, id } => vec![selector, id],
                Targeting::PlaceOrSelector => vec!["selector", "x", "y"],
                Targeting::Free => Vec::new(),
            };
            assert!(
                !names.is_empty(),
                "`{op}` has a row that says nothing: a Free op needs none"
            );
            for name in names {
                assert!(
                    def.params.contains(&(name.to_string(), false)),
                    "`{op}`: OP_TARGETING names the optional param `{name}`, which the op does \
                     not have: {:?}",
                    def.params
                );
            }
        }
    }

    /// The other direction: an op that names its target by an optional
    /// `selector` / `node_id` pair (or `parent_selector` / `parent_id`) and by
    /// nothing else (no optional `text`, no `x`) has a row. The gate no longer
    /// derives the rule from these names - this is what tells a NEW such op that
    /// it needs a row, instead of letting it through the gate until the engine
    /// refuses it.
    #[test]
    fn an_op_that_names_its_target_by_a_selector_or_an_id_has_a_targeting_row() {
        use azul_layout::e2e::{targeting, Targeting};

        let s = parse_schema(&root()).unwrap();
        let mut seen = 0;
        for name in s.all_op_names() {
            let Some(def) = s.known_op(name) else {
                continue;
            };
            let has = |p: &str| def.params.iter().any(|(n, _)| n == p);
            let optional_text = def
                .params
                .iter()
                .any(|(n, required)| n == "text" && !required);
            for (selector, id) in [("selector", "node_id"), ("parent_selector", "parent_id")] {
                if has(selector) && has(id) && !optional_text && !has("x") {
                    seen += 1;
                    assert_eq!(
                        targeting(name),
                        Targeting::SelectorOrId { selector, id },
                        "`{name}` takes `{selector}` / `{id}` and nothing else to name its node: \
                         give it a row in OP_TARGETING (layout/src/e2e/full.rs)"
                    );
                }
            }
        }
        assert!(
            seen >= 7,
            "only {seen} ops name their target by a selector / id pair: the scan broke"
        );
        // An op with more ways to name its node, or a required selector, has no one rule.
        for op in [
            "click",
            "double_click",
            "scroll_node_by",
            "assert_scroll",
            "wheel",
        ] {
            assert_eq!(targeting(op), Targeting::Free, "`{op}`");
        }
    }

    /// A scenario that mounts a small document, lets it land, and runs `step`.
    /// The ids are countable: html 0, body 1, `#root` 2, `#a` 3, `#t` 4, its
    /// text 5.
    fn on_a_mounted_dom(step: &str) -> String {
        format!(
            r#"{{"name":"x","steps":[
                {{"op":"mount","html":["<body><div id=\"root\"><div id=\"a\"></div><p id=\"t\">hi</p></div></body>"]}},
                {{"op":"wait_frame"}},
                {{"op":"snapshot_frame","as":"before"}},
                {step},
                {{"op":"assert_changed","vs":"before"}}]}}"#
        )
    }

    /// `selector` and `node_id` are both optional in the schema, so the
    /// required-param check cannot say "exactly one" - and the engine refuses a
    /// step with both or neither at run time. The gate says so first, from the
    /// ENGINE's table (`azul_layout::e2e::targeting`), in the engine's words: the
    /// two owners of the rule read one definition (`FIX-87b0f2b8c-1`).
    #[test]
    fn the_gate_wants_exactly_one_way_of_naming_the_node() {
        let s = parse_schema(&root()).unwrap();

        for step in [
            // selector xor node_id
            r##"{"op":"set_node_css_override","selector":"#a","property":"width","value":"1px"}"##,
            r##"{"op":"set_node_css_override","node_id":3,"property":"width","value":"1px"}"##,
            r##"{"op":"set_node_classes","selector":"#a","classes":["x"]}"##,
            r##"{"op":"set_node_text","selector":"#t","text":"bye"}"##,
            r##"{"op":"set_node_image","selector":"#a","width":4,"height":4,"color":"red"}"##,
            r##"{"op":"delete_node","selector":"#a"}"##,
            r##"{"op":"focus_node","selector":"#a"}"##,
            // the parent of an insert
            r##"{"op":"insert_node","parent_selector":"#root","node_type":"div"}"##,
            r##"{"op":"insert_node","parent_id":2,"node_type":"div"}"##,
            // a pointer: a position, or a selector
            r##"{"op":"mouse_move","x":10,"y":20}"##,
            r##"{"op":"mouse_move","selector":"#a"}"##,
        ] {
            validate(&s, &on_a_mounted_dom(step))
                .unwrap_or_else(|e| panic!("the gate refused {step}: {e}"));
        }

        for (step, why) in [
            (
                r##"{"op":"set_node_css_override","property":"width","value":"1px"}"##,
                "needs a `selector`",
            ),
            (
                r##"{"op":"set_node_css_override","selector":"#a","node_id":3,"property":"width","value":"1px"}"##,
                "exactly one of `selector` / `node_id`",
            ),
            (
                r##"{"op":"set_node_classes","classes":["x"]}"##,
                "needs a `selector`",
            ),
            (
                r##"{"op":"set_node_text","text":"bye"}"##,
                "needs a `selector`",
            ),
            (
                r##"{"op":"set_node_image","width":4,"height":4,"color":"red"}"##,
                "needs a `selector`",
            ),
            (r##"{"op":"delete_node"}"##, "needs a `selector`"),
            (r##"{"op":"focus_node"}"##, "needs a `selector`"),
            (
                r##"{"op":"focus_node","selector":"#a","node_id":3}"##,
                "exactly one of `selector` / `node_id`",
            ),
            (
                r##"{"op":"insert_node","node_type":"div"}"##,
                "needs a `parent_selector`",
            ),
            (
                r##"{"op":"insert_node","parent_selector":"#root","parent_id":2,"node_type":"div"}"##,
                "exactly one of `parent_selector` / `parent_id`",
            ),
            (r##"{"op":"mouse_move"}"##, "needs `x` + `y`"),
            (
                r##"{"op":"mouse_move","x":10}"##,
                "needs `x` + `y` (together)",
            ),
            (
                r##"{"op":"mouse_move","selector":"#a","x":10,"y":20}"##,
                "`selector` or `x` + `y`, not both",
            ),
        ] {
            let e = validate(&s, &on_a_mounted_dom(step))
                .expect_err(&format!("the gate let {step} through"))
                .to_string();
            assert!(e.contains(why), "{step}: wanted {why:?}, got: {e}");
        }

        // An op with MORE ways to name its node (a position, a text) is the
        // engine's to resolve, not the gate's: `click` with a selector, an id,
        // a text or a position all pass.
        for step in [
            r##"{"op":"click","selector":"#a"}"##,
            r##"{"op":"click","x":10,"y":20}"##,
            r##"{"op":"click","text":"hi"}"##,
        ] {
            validate(&s, &on_a_mounted_dom(step))
                .unwrap_or_else(|e| panic!("the gate refused {step}: {e}"));
        }
    }

    /// Defect 4 of the batch-2 review. `check_targeting` read "`selector` +
    /// `x` + `y`, no `node_id`" as `mouse_move`'s "a position OR a selector"
    /// and applied that to `assert_scroll`, which has all three: its `selector`
    /// is REQUIRED (the engine refuses the step without one) and `x` / `y` are
    /// the offsets it compares (at least one is required). The step the engine
    /// wants - `{selector, y}` - was refused as "both a `selector` and `x` /
    /// `y`", and a step without the selector was refused by the engine, so no
    /// form passed both and every scroll scenario was unwritable (line 11 of
    /// the batch).
    #[test]
    fn defect4_assert_scroll_takes_a_selector_and_an_offset() {
        let s = parse_schema(&root()).unwrap();
        let def = s.known_op("assert_scroll").unwrap();
        assert!(
            def.params.contains(&("selector".to_string(), true)),
            "assert_scroll REQUIRES its selector: {:?}",
            def.params
        );
        for offset in ["x", "y"] {
            assert!(
                def.params.contains(&(offset.to_string(), false)),
                "`{offset}` is an optional offset of assert_scroll: {:?}",
                def.params
            );
        }

        let after_a_wheel = |step: &str| {
            format!(
                r##"{{"name":"x","steps":[
                    {{"op":"mount","html":["<body><div id=\"list\"></div></body>"],
                      "css":["#list {{ height: 100px; overflow: auto; }}"]}},
                    {{"op":"wait_frame"}},
                    {{"op":"wheel","x":10,"y":10,"delta_x":0,"delta_y":-120}},
                    {{"op":"wait_frame"}},
                    {step}]}}"##
            )
        };
        // The forms the engine reads (`eval_assert_scroll`): the selector, and
        // an offset on at least one axis.
        for step in [
            r##"{"op":"assert_scroll","selector":"#list","y":120}"##,
            r##"{"op":"assert_scroll","selector":"#list","x":0}"##,
            r##"{"op":"assert_scroll","selector":"#list","x":0,"y":120,"tolerance":1}"##,
        ] {
            validate(&s, &after_a_wheel(step))
                .unwrap_or_else(|e| panic!("the gate refused {step}: {e}"));
        }
        // The engine's own refusal of a missing selector is not the gate's to
        // hide: the required-param check says it first.
        refused(
            &s,
            &after_a_wheel(r#"{"op":"assert_scroll","y":120}"#),
            "missing required param `selector`",
        );

        // `mouse_move`'s selector IS the alternative to its position, and a
        // step that gives both is still refused.
        let e = validate(
            &s,
            &on_a_mounted_dom(r##"{"op":"mouse_move","selector":"#a","x":10,"y":20}"##),
        )
        .expect_err("mouse_move {selector, x, y} must stay refused")
        .to_string();
        assert!(e.contains("`selector` or `x` + `y`, not both"), "{e}");
    }

    /// The same rule against the corpus the gate must never contradict: every
    /// `assert_scroll` step of a scenario under `e2e/` - hand-written, and
    /// running green - passes the gate's targeting, and so does the scenario
    /// (the nine that assert a scroll offset use only allowed ops, name their
    /// nodes by selector, and none was written for the gate: it must let them
    /// through).
    #[test]
    fn defect4_every_committed_scroll_scenario_passes_the_gate() {
        /// Every object with `"op": "assert_scroll"`, wherever the scenario
        /// keeps its steps.
        fn scroll_steps<'a>(
            v: &'a serde_json::Value,
            out: &mut Vec<&'a serde_json::Map<String, serde_json::Value>>,
        ) {
            match v {
                serde_json::Value::Object(o) => {
                    if o.get("op").and_then(serde_json::Value::as_str) == Some("assert_scroll") {
                        out.push(o);
                    }
                    o.values().for_each(|v| scroll_steps(v, out));
                }
                serde_json::Value::Array(a) => a.iter().for_each(|v| scroll_steps(v, out)),
                _ => {}
            }
        }

        let s = parse_schema(&root()).unwrap();
        let (mut scenarios, mut seen) = (0, 0);
        let mut refused = Vec::new();
        for entry in fs::read_dir(root().join("e2e")).expect("e2e/ must exist") {
            let path = entry.expect("readable dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let text = fs::read_to_string(&path).unwrap();
            let json: serde_json::Value = serde_json::from_str(&text)
                .unwrap_or_else(|e| panic!("{} is not JSON: {e}", path.display()));
            let mut steps = Vec::new();
            scroll_steps(&json, &mut steps);
            if steps.is_empty() {
                continue;
            }
            scenarios += 1;
            for step in steps {
                seen += 1;
                if let Err(e) = check_targeting(0, "assert_scroll", step) {
                    refused.push(format!("{}: {e}", path.display()));
                }
            }
            if let Err(e) = validate(&s, &text) {
                refused.push(format!("{}: {e}", path.display()));
            }
        }
        assert!(
            refused.is_empty(),
            "the gate refuses committed scroll scenarios:\n{}",
            refused.join("\n")
        );
        assert!(
            scenarios >= 9 && seen >= 10,
            "only {seen} assert_scroll steps in {scenarios} scenarios found under e2e/ - the scan \
             lost them"
        );
    }

    // -----------------------------------------------------------------------
    // What the gate checks against the engine (G2 - G5)
    // -----------------------------------------------------------------------

    /// A scenario that mounts `html` (an array of lines), lets it land, runs
    /// `steps` (each ending in a comma-free list of steps) and asserts.
    fn on_dom(html: &str, steps: &str) -> String {
        format!(
            r#"{{"name":"x","steps":[
                {{"op":"mount","html":{html}}},
                {{"op":"wait_frame"}},
                {{"op":"snapshot_frame","as":"before"}},
                {steps},
                {{"op":"assert_changed","vs":"before"}}]}}"#
        )
    }

    /// An indented fragment, the way a model writes one. `mount` wraps it in
    /// `<html><body>`, and every newline and indent becomes a text node that
    /// takes an id:
    ///
    /// | id | node |
    /// |----|------|
    /// | 0 | `<html>` |
    /// | 1 | `<body>` |
    /// | 2 | whitespace (after `<body>`) |
    /// | 3 | `#root` |
    /// | 4 | whitespace (`\n  `) |
    /// | 5 | `#box` |
    /// | 6 | whitespace (before `</div>`) |
    /// | 7 | whitespace (before `</body>`) |
    const INDENTED: &str = r##"["<div id=\"root\">","  <div id=\"box\"></div>","</div>"]"##;

    fn refused(schema: &Schema, scenario: &str, why: &str) {
        let e = validate(schema, scenario)
            .expect_err(&format!("the gate let this through:\n{scenario}"))
            .to_string();
        assert!(
            e.contains(why),
            "wanted {why:?} in: {e}\nscenario:\n{scenario}"
        );
    }

    /// G2. `html: []` passed the gate and `mount` then failed with "'html' is
    /// empty" at step 0: 88 steps never ran.
    #[test]
    fn the_gate_rejects_a_mount_with_no_html() {
        let s = parse_schema(&root()).unwrap();
        for html in [r#"[]"#, r#"[""]"#, r#"["  ", "\n"]"#, r#""""#, r#""   ""#] {
            let json = format!(
                r#"{{"name":"x","steps":[{{"op":"mount","html":{html}}},{{"op":"wait_frame"}},
                    {{"op":"assert_exists","selector":"body"}}]}}"#
            );
            refused(&s, &json, "empty `html`");
            refused(&s, &json, "<body></body>");
        }
        // An empty PAGE is a body with nothing in it.
        validate(
            &s,
            r#"{"name":"x","steps":[{"op":"mount","html":["<body></body>"]},{"op":"wait_frame"},
                {"op":"assert_exists","selector":"body"}]}"#,
        )
        .unwrap();
        // ...and html that is not text at all is not read.
        refused(
            &s,
            r#"{"name":"x","steps":[{"op":"mount","html":[1,2]},{"op":"assert_exists","selector":"body"}]}"#,
            "not read by the engine",
        );
    }

    /// `mount` refuses markup that does not parse; so does the gate, with the
    /// runtime's own words.
    #[test]
    fn the_gate_rejects_a_mount_whose_markup_does_not_parse() {
        let s = parse_schema(&root()).unwrap();
        refused(
            &s,
            r##"{"name":"x","steps":[{"op":"mount","html":["<div id=\"a>x</div>"]},
                {"op":"wait_frame"},{"op":"assert_exists","selector":"div"}]}"##,
            "mount: XML parse error",
        );
    }

    /// G3. `{"which": "last"}` - `"last"` is a value of `frame` - was read as the
    /// paint damage; the set is the engine's (`closed_param_values`).
    #[test]
    fn the_gate_rejects_a_value_outside_a_closed_set() {
        let s = parse_schema(&root()).unwrap();
        let with = |step: &str| {
            format!(r#"{{"name":"x","steps":[{{"op":"snapshot_frame","as":"b"}},{step}]}}"#)
        };

        refused(
            &s,
            &with(r#"{"op":"assert_damage","kind":"none","which":"last"}"#),
            "\"last\" is a value of `frame`",
        );
        refused(
            &s,
            &with(r#"{"op":"assert_damage","kind":"none","which":"last"}"#),
            "paint, present",
        );
        refused(
            &s,
            &with(r#"{"op":"assert_changed","vs":"b","frame":"newest"}"#),
            "accumulated, last",
        );
        refused(
            &s,
            &with(r#"{"op":"assert_damage_incremental","which":true}"#),
            "param `which` is true",
        );
        refused(
            &s,
            &with(r#"{"op":"capture_damage_png","path":"target/e2e/x.png","which":"newest"}"#),
            "paint, present",
        );

        // every value of every set passes
        for (which, frame) in [
            ("paint", "accumulated"),
            ("present", "accumulated"),
            ("paint", "last"),
            ("present", "last"),
        ] {
            validate(
                &s,
                &with(&format!(
                    r#"{{"op":"assert_damage","kind":"none","which":"{which}","frame":"{frame}"}}"#
                )),
            )
            .unwrap_or_else(|e| panic!("{which}/{frame}: {e}"));
        }
        // the sets are the engine's, not a list of this file's
        for (param, values) in azul_layout::e2e::CLOSED_STRING_PARAMS {
            assert!(
                s.ops
                    .iter()
                    .chain(s.asserts.iter())
                    .any(|o| o.params.iter().any(|(p, _)| p == param)),
                "`{param}` is a closed set of the engine that no op of the schema has: {values:?}"
            );
        }
    }

    /// The step is read as the runner reads it: a wrong type or a variant that
    /// does not exist fails here, not at step N of a run.
    #[test]
    fn the_gate_reads_a_step_as_the_engine_does() {
        let s = parse_schema(&root()).unwrap();
        let with = |step: &str| {
            format!(
                r#"{{"name":"x","steps":[{{"op":"snapshot_frame","as":"b"}},{step},
                {{"op":"assert_changed","vs":"b"}}]}}"#
            )
        };
        refused(
            &s,
            &with(r#"{"op":"tick_ms","ms":"16"}"#),
            "not read by the engine",
        );
        refused(
            &s,
            &with(r#"{"op":"tick_ms","ms":-1}"#),
            "not read by the engine",
        );
        refused(
            &s,
            &with(r#"{"op":"click","x":1,"y":2,"button":"primary"}"#),
            "unknown variant `primary`",
        );
        validate(&s, &with(r#"{"op":"click","x":1,"y":2,"button":"right"}"#)).unwrap();
        validate(&s, &with(r#"{"op":"tick_ms","ms":16}"#)).unwrap();
    }

    /// G4. The numbers a model counts are wrong: the ids are resolved with the
    /// parser `mount` uses, and an id that is the page, a whitespace run or a
    /// node that is not there is refused, with the elements named.
    #[test]
    fn the_gate_resolves_a_node_id_with_the_parser_mount_uses() {
        let s = parse_schema(&root()).unwrap();
        let css = |id: u32| {
            on_dom(
                INDENTED,
                &format!(
                    r#"{{"op":"set_node_css_override","node_id":{id},"property":"width","value":"1px"}}"#
                ),
            )
        };

        refused(&s, &css(0), "is `<html>`");
        refused(&s, &css(1), "is `<body>`");
        refused(&s, &css(2), "whitespace text run");
        refused(&s, &css(4), "whitespace text run");
        refused(&s, &css(6), "whitespace text run");
        // the refusal says where the elements are, and what to do instead
        refused(&s, &css(1), "3 `<div id=\"root\">`");
        refused(&s, &css(1), "5 `<div id=\"box\">`");
        refused(&s, &css(1), "`selector`");
        refused(&s, &css(99), "does not exist");
        refused(&s, &css(99), "8 nodes");
        // the elements the model meant
        validate(&s, &css(3)).unwrap();
        validate(&s, &css(5)).unwrap();

        // every op that takes a node id, not only the mutation ones
        refused(
            &s,
            &on_dom(
                INDENTED,
                r#"{"op":"scroll_node_by","node_id":1,"delta_x":0,"delta_y":10}"#,
            ),
            "is `<body>`",
        );
        refused(
            &s,
            &on_dom(INDENTED, r#"{"op":"focus_node","node_id":0}"#),
            "is `<html>`",
        );
        // the parent of an insert, too
        refused(
            &s,
            &on_dom(
                INDENTED,
                r#"{"op":"insert_node","parent_id":1,"node_type":"div"}"#,
            ),
            "is `<body>`",
        );
        validate(
            &s,
            &on_dom(
                INDENTED,
                r#"{"op":"insert_node","parent_id":3,"node_type":"div"}"#,
            ),
        )
        .unwrap();
        // ...while a SELECTOR may name the page itself: the model said so.
        validate(
            &s,
            &on_dom(
                INDENTED,
                r#"{"op":"set_node_css_override","selector":"body","property":"background","value":"red"}"#,
            ),
        )
        .unwrap();
    }

    /// `set_node_text` writes the TEXT node, `set_node_image` an `<img>`,
    /// `add_timer` a text node: an id of the wrong kind is refused with the
    /// right one named.
    #[test]
    fn the_gate_knows_which_kind_of_node_an_op_takes() {
        let s = parse_schema(&root()).unwrap();
        // html0 body1 #p2 "hi"3 #pic4
        let dom = r##"["<body><p id=\"p\">hi</p><img id=\"pic\" /></body>"]"##;
        let on = |step: &str| on_dom(dom, step);

        validate(
            &s,
            &on(r#"{"op":"set_node_text","node_id":3,"text":"bye"}"#),
        )
        .unwrap();
        refused(
            &s,
            &on(r#"{"op":"set_node_text","node_id":2,"text":"bye"}"#),
            "addresses the TEXT node inside it (the text inside it is node 3)",
        );
        refused(
            &s,
            &on(r#"{"op":"set_node_text","node_id":1,"text":"bye"}"#),
            "is `<body>`",
        );
        validate(
            &s,
            &on(r##"{"op":"set_node_text","selector":"#p","text":"bye"}"##),
        )
        .unwrap();

        validate(
            &s,
            &on(r#"{"op":"set_node_image","node_id":4,"width":4,"height":4,"color":"red"}"#),
        )
        .unwrap();
        refused(
            &s,
            &on(r#"{"op":"set_node_image","node_id":2,"width":4,"height":4,"color":"red"}"#),
            "not an `<img>`",
        );

        validate(
            &s,
            &on(r#"{"op":"add_timer","timer_id":1000,"interval_ms":16,"node_id":3,"text":"t"}"#),
        )
        .unwrap();
        refused(
            &s,
            &on(r#"{"op":"add_timer","timer_id":1000,"interval_ms":16,"node_id":2,"text":"t"}"#),
            "TEXT node",
        );
    }

    /// The document is followed through the scenario: an insert adds a node at
    /// the end, a delete leaves a tombstone, a second `mount` replaces it.
    #[test]
    fn the_gate_follows_the_document_through_the_scenario() {
        let s = parse_schema(&root()).unwrap();
        // html0 body1 #a2 #b3
        let dom = r##"["<body><div id=\"a\"></div><div id=\"b\"></div></body>"]"##;
        let steps = |tail: &str| {
            format!(
                r#"{{"name":"x","steps":[{{"op":"mount","html":{dom}}},{{"op":"wait_frame"}},
                    {{"op":"snapshot_frame","as":"b"}},{tail},{{"op":"assert_changed","vs":"b"}}]}}"#
            )
        };

        // the inserted node is node 4, and can be addressed
        validate(
            &s,
            &steps(
                r#"{"op":"insert_node","parent_selector":"body","node_type":"div","id":"n"},
                   {"op":"set_node_css_override","node_id":4,"property":"width","value":"1px"}"#,
            ),
        )
        .unwrap();
        refused(
            &s,
            &steps(
                r#"{"op":"insert_node","parent_selector":"body","node_type":"div"},
                   {"op":"set_node_css_override","node_id":5,"property":"width","value":"1px"}"#,
            ),
            "does not exist",
        );
        // a deleted node keeps its id, and is gone
        refused(
            &s,
            &steps(
                r#"{"op":"delete_node","node_id":2},
                   {"op":"set_node_css_override","node_id":2,"property":"width","value":"1px"}"#,
            ),
            "was deleted by step 3",
        );
        // a second mount replaces the document
        refused(
            &s,
            &steps(
                r#"{"op":"mount","html":["<body></body>"]},{"op":"wait_frame"},
                   {"op":"set_node_css_override","node_id":2,"property":"width","value":"1px"}"#,
            ),
            "does not exist",
        );
        // what hung below a deleted node is gone with it (html0 body1 #list2 #row3 "x"4)
        let nested = r##"["<body><div id=\"list\"><p id=\"row\">x</p></div></body>"]"##;
        let below = format!(
            r#"{{"name":"x","steps":[{{"op":"mount","html":{nested}}},{{"op":"wait_frame"}},
                {{"op":"snapshot_frame","as":"b"}},{{"op":"delete_node","node_id":2}},
                {{"op":"set_node_css_override","node_id":3,"property":"width","value":"1px"}},
                {{"op":"assert_changed","vs":"b"}}]}}"#
        );
        refused(&s, &below, "hung below node 2, which step 3 deleted");
    }

    /// A scenario of exactly `steps`.
    fn scenario_of(steps: &[&str]) -> String {
        format!(r#"{{"name":"x","steps":[{}]}}"#, steps.join(","))
    }

    /// G5. `mount` takes effect at the next frame: `set_node_image` right after
    /// it failed with "Node 1 not found" (the document did not exist yet).
    #[test]
    fn the_gate_rejects_a_node_op_before_the_mount_has_landed() {
        let s = parse_schema(&root()).unwrap();
        let mount = r##"{"op":"mount","html":["<body><div id=\"a\"></div></body>"]}"##;
        let css =
            r##"{"op":"set_node_css_override","selector":"#a","property":"width","value":"1px"}"##;
        let exists = r##"{"op":"assert_exists","selector":"#a"}"##;
        let wait_frame = r#"{"op":"wait_frame"}"#;

        refused(
            &s,
            &scenario_of(&[mount, css, exists]),
            "has not been applied yet",
        );
        refused(&s, &scenario_of(&[mount, css, exists]), "`wait_frame`");
        refused(
            &s,
            &scenario_of(&[mount, r#"{"op":"snapshot_frame","as":"b"}"#, css, exists]),
            "step 0",
        );
        for lands in [
            wait_frame,
            r#"{"op":"wait","ms":10}"#,
            r#"{"op":"tick_ms","ms":16}"#,
        ] {
            validate(&s, &scenario_of(&[mount, lands, css, exists]))
                .unwrap_or_else(|e| panic!("{lands}: {e}"));
        }

        // no mount at all
        refused(&s, &scenario_of(&[css, exists]), "nothing is mounted");
        // a second mount that has not landed hides the first
        refused(
            &s,
            &scenario_of(&[mount, wait_frame, mount, css, exists]),
            "step 2",
        );
        // an unmount leaves no document
        refused(
            &s,
            &scenario_of(&[
                mount,
                wait_frame,
                r#"{"op":"unmount"}"#,
                wait_frame,
                css,
                exists,
            ]),
            "nothing is mounted",
        );
        // an ASSERTION is a question, not an address: it may come first
        validate(
            &s,
            &scenario_of(&[mount, r##"{"op":"assert_not_exists","selector":"#zzz"}"##]),
        )
        .unwrap();
    }

    // -----------------------------------------------------------------------
    // Op classification
    // -----------------------------------------------------------------------

    /// A step list wrapped in the minimum a test needs to reach the op check.
    fn with_op(step: &str) -> String {
        format!(
            r#"{{"name":"x","steps":[{{"op":"snapshot_frame","as":"before"}},{step},
                {{"op":"assert_changed","vs":"before"}}]}}"#
        )
    }

    #[test]
    fn the_gate_rejects_a_test_that_forces_the_effect_under_test() {
        let s = parse_schema(&root()).unwrap();

        // THE regression this whole classification exists for: `set_node_text`
        // -> `redraw` -> `assert_changed` PASSES even when the invalidation
        // path is broken, because `redraw` manufactures the damage itself.
        let masked = r##"{"name":"stale_text","steps":[
            {"op":"mount","html":["<div id=\"a\">hi</div>"]},
            {"op":"wait_frame"},
            {"op":"snapshot_frame","as":"before"},
            {"op":"set_node_text","selector":"#a","text":"bye"},
            {"op":"redraw"},
            {"op":"assert_changed","vs":"before","min_damage_rects":1}]}"##;
        let e = validate(&s, masked).unwrap_err().to_string();
        assert!(e.contains("`redraw`") && e.contains("DENIED"), "{e}");

        // ...and the same test WITHOUT the forced redraw is exactly what we want.
        let honest = r##"{"name":"stale_text","steps":[
            {"op":"mount","html":["<div id=\"a\">hi</div>"]},
            {"op":"wait_frame"},
            {"op":"snapshot_frame","as":"before"},
            {"op":"set_node_text","selector":"#a","text":"bye"},
            {"op":"wait_frame"},
            {"op":"assert_changed","vs":"before","min_damage_rects":1}]}"##;
        validate(&s, honest).unwrap();

        // `relayout` is denied for the same reason.
        assert!(validate(&s, &with_op(r#"{"op":"relayout"}"#)).is_err());
        assert!(matches!(classify("redraw"), OpClass::Denied(_)));
        assert!(matches!(classify("relayout"), OpClass::Denied(_)));
    }

    #[test]
    fn the_gate_rejects_the_ide_and_geometry_families() {
        let s = parse_schema(&root()).unwrap();
        for op in [
            "create_component",
            "delete_component",
            "update_component",
            "update_component_render_fn",
            "create_library",
            "delete_library",
            "export_code",
            "export_code_zip",
            "get_component_registry",
            "resolve_function_pointers",
            "run_e2e_tests",
            "get_logs",
            "open_file",
            "close",
            // geometry — reftest's job, and the side door for a smuggled
            // geometry assertion
            "get_node_layout",
            "get_all_nodes_layout",
            "get_layout_tree",
            "get_display_list",
            "get_virtual_view_layout",
            "assert_layout",
            "assert_screenshot",
        ] {
            assert!(s.is_known(op), "`{op}` is not a real op — fix the table");
            assert!(
                matches!(classify(op), OpClass::Denied(_)),
                "`{op}` must be denied"
            );
            let json = with_op(&format!("{{\"op\":\"{op}\"}}"));
            assert!(validate(&s, &json).is_err(), "gate let `{op}` through");
        }
    }

    #[test]
    fn the_drive_and_observe_surfaces_are_allowed() {
        for op in [
            "click",
            "click_node",
            "double_click",
            "mouse_down",
            "mouse_move",
            "mouse_up",
            "key_down",
            "key_up",
            "text_input",
            "scroll",
            "touch_start",
            "touch_move",
            "touch_end",
            "pen_down",
            "pen_move",
            "pen_up",
            "pinch",
            "rotate",
            "swipe",
            "long_press",
            "move",
            "resize",
            "dpi_changed",
            "hit_test",
            "focus",
            "blur",
            "set_node_text",
            "set_node_css_override",
            "set_node_classes",
            "insert_node",
            "delete_node",
            "set_app_state",
            "scroll_node_to",
            "scroll_node_by",
            "scroll_into_view",
            "commit_undo_snapshot",
            "undo_app_state",
            "redo_app_state",
            "mount",
            "unmount",
            "tick_ms",
            "wait",
            "wait_frame",
            "reset_frame_counters",
            "snapshot_frame",
            "snapshot_resources",
            "snapshot_managers",
            "get_frame_report",
            "capture_damage_png",
            "get_state",
            "get_app_state",
            "get_dom",
            "get_focus_state",
            "get_scroll_states",
            "get_selection_state",
            "get_cursor_state",
            "assert_changed",
            "assert_idle_stable",
        ] {
            assert_eq!(classify(op), OpClass::Allowed, "`{op}` must be allowed");
        }

        // `touch_cancel` is DELIBERATELY not in that list. `TouchState` has no
        // cancel channel — a cancelled point and a lifted point are the same
        // state delta — so no `EventType::TouchCancel` can be determined and the
        // op refuses by name rather than silently lifting the points, which
        // would let a cancellation test go green on END semantics.
        assert!(
            matches!(classify("touch_cancel"), OpClass::Denied(_)),
            "`touch_cancel` must stay denied while the engine has no cancel signal"
        );
    }

    /// A NEW `DebugEvent` variant must be surfaced, not silently allowed/denied.
    #[test]
    fn every_real_op_is_classified() {
        let s = parse_schema(&root()).unwrap();
        assert_eq!(
            s.unclassified(),
            Vec::<&str>::new(),
            "these ops exist in DebugEvent but no OP_POLICY row covers them — classify them \
             (allow, or deny with a one-line reason). Until then gen-e2e reports them loudly and \
             the gate rejects any test using them."
        );
        assert_eq!(
            s.stale_policy_entries(),
            Vec::<&'static str>::new(),
            "OP_POLICY classifies ops that no longer exist in full.rs"
        );
        assert_eq!(classify("brand_new_op"), OpClass::Unclassified);
    }

    /// An eval fn whose `reject_unknown_params(.., &[])` guard allows NO
    /// params takes none: the empty scan is right, and the blind-spot alarm
    /// must not call it a scanner gap (`assert_no_unmocked_requests` did, on
    /// every run). A fn that reads a param the scanners cannot see still
    /// raises it.
    #[test]
    fn an_assertion_whose_guard_allows_no_params_is_not_a_scanner_gap() {
        let src = fs::read_to_string(root().join(FULL_RS)).unwrap();
        let s = parse_schema(&root()).unwrap();
        let none = s
            .asserts
            .iter()
            .find(|a| a.name == "assert_no_unmocked_requests")
            .expect("assert_no_unmocked_requests is in the assertion dispatch");
        assert!(none.params.is_empty(), "{:?}", none.params);
        assert!(!scanner_missed_params(&src, none));

        // A key read through an unscanned path is still a gap.
        let hidden = "\nfn eval_assert_hidden(params: &serde_json::Value) -> AssertionResult {\n    \
                      if let Some(bad) = reject_unknown_params(\"assert_hidden\", params, &[]) {\n        \
                      return bad;\n    }\n    let k = key_of();\n    read(params, k)\n}\n";
        let def = OpDef {
            name: "assert_hidden".to_string(),
            params: Vec::new(),
            doc: None,
        };
        assert!(scanner_missed_params(hidden, &def));
    }

    // -----------------------------------------------------------------------
    // Zombie ops: declared in DebugEvent, no match arm -> silently `ok`
    // -----------------------------------------------------------------------

    /// Ops that are declared in `DebugEvent` but fall through to the
    /// `_ => { log("Unhandled"); send_ok() }` catch-all. This list is a PIN, not
    /// a source of truth: the detection is derived from `full.rs`. When one of
    /// them is given a real match arm it stops being a zombie automatically —
    /// and this test tells you to strike it off the pin.
    ///
    /// EMPTY. `focus`, `blur`, `move`, `dpi_changed` and `get_dom` — the five
    /// that used to live here — now have real match arms (window focus/blur and
    /// move route through `CallbackChange::ModifyWindowState` → the shared
    /// `apply_user_change()` state-diff pass, the same one the platform focus /
    /// configure / DPI handlers drive; `get_dom` returns the nested DOM).
    /// `no_zombie_is_reachable` keeps this honest: a NEW declared-but-unhandled
    /// variant is still caught by `Schema::zombies()`, pin or no pin.
    const KNOWN_ZOMBIES: &[&str] = &[];

    #[test]
    fn declared_but_unhandled_ops_are_detected_and_rejected() {
        let s = parse_schema(&root()).unwrap();
        for op in KNOWN_ZOMBIES {
            assert!(s.is_known(op), "`{op}` is not even declared — fix the pin");
            assert!(
                s.is_zombie(op),
                "`{op}` is no longer a zombie (someone implemented it) — remove it from \
                 KNOWN_ZOMBIES; nothing else needs to change, it is usable again automatically"
            );
            // The GATE rejects it: a test using it would pass while doing nothing.
            let json = with_op(&format!("{{\"op\":\"{op}\"}}"));
            let e = validate(&s, &json).unwrap_err().to_string();
            assert!(
                e.contains("no match arm") && e.contains(op),
                "gate let the zombie `{op}` through: {e}"
            );
        }
        // ...and an op that IS handled is not a zombie.
        for op in ["click", "mount", "set_node_text", "scroll", "key_down"] {
            assert!(
                !s.is_zombie(op),
                "`{op}` has a match arm — must not be a zombie"
            );
        }
    }

    /// A zombie must never reach the generator: not in the prompt, not in the
    /// allowed count.
    #[test]
    fn no_zombie_is_reachable() {
        let s = parse_schema(&root()).unwrap();
        let doc = schema_doc(&s);
        for z in s.zombies() {
            assert!(
                !doc.contains(&format!("- {z} :")),
                "the prompt offers the zombie op `{z}` — it would generate a vacuously-green test"
            );
            assert!(validate(&s, &with_op(&format!("{{\"op\":\"{z}\"}}"))).is_err());
        }
        // The pin and the code-derived scan must agree (the scan is in enum
        // order, so compare as sets).
        let found: BTreeSet<&str> = s.zombies().into_iter().collect();
        let pinned: BTreeSet<&str> = KNOWN_ZOMBIES.iter().copied().collect();
        assert_eq!(
            found, pinned,
            "the set of declared-but-unhandled ops changed — update KNOWN_ZOMBIES (an op that \
             gained a match arm is usable again automatically)"
        );
    }

    /// THE SELF-HEALING PROPERTY, proven on a synthetic `full.rs`: the zombie
    /// scan is derived from the code, so the moment somebody gives `Focus` a
    /// real match arm it becomes usable again — no edit to `OP_POLICY`, no edit
    /// to `KNOWN_ZOMBIES`, no edit anywhere in this file.
    fn synthetic_root(dir: &Path, focus_arm: &str) -> PathBuf {
        let root = dir.to_path_buf();
        let f = root.join(FULL_RS);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(
            &f,
            format!(
                r#"
pub enum DebugEvent {{
    Focus,
    WaitFrame,
    SnapshotFrame {{
        #[serde(rename = "as")]
        name: String,
    }},
}}

fn dispatch(request: Request) {{
    match request.event {{
{focus_arm}        DebugEvent::WaitFrame => {{ }}
        DebugEvent::SnapshotFrame {{ name }} => {{ }}
        _ => {{
            log(LogLevel::Warn, format!("Unhandled: {{:?}}", request.event), None);
            send_ok(request, None, None);
        }}
    }}
}}

fn evaluate_assertion(op: &str) {{
    match op {{
        "assert_changed" => eval_assert_changed(params),
        _ => {{}}
    }}
}}

fn eval_assert_changed(params: &Value) -> AssertionResult {{
    let vs = params.get("vs");
    AssertionResult::pass("ok")
}}
"#
            ),
        )
        .unwrap();
        root
    }

    #[test]
    fn implementing_a_zombie_re_enables_it_automatically() {
        let dir = std::env::temp_dir().join(format!("gene2e-zombie-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        // 1. `Focus` is declared but has NO arm -> zombie: hidden from the prompt, rejected by the
        //    gate.
        let root = synthetic_root(&dir, "");
        let s = parse_schema(&root).unwrap();
        assert!(s.is_zombie("focus"));
        assert!(!schema_doc(&s).contains("- focus :"));
        let t = r#"{"name":"x","steps":[{"op":"snapshot_frame","as":"b"},{"op":"focus"},
            {"op":"assert_changed","vs":"b"}]}"#;
        assert!(validate(&s, t)
            .unwrap_err()
            .to_string()
            .contains("no match arm"));

        // 2. Somebody implements it in full.rs. NOTHING in gene2e.rs changes.
        let root = synthetic_root(&dir, "        DebugEvent::Focus => { do_focus(); }\n");
        let s = parse_schema(&root).unwrap();
        assert!(
            !s.is_zombie("focus"),
            "an implemented op must stop being a zombie"
        );
        assert!(schema_doc(&s).contains("- focus :"), "and be offered again");
        validate(&s, t).expect("and be accepted by the gate again");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_prompt_shows_allowed_ops_only() {
        let s = parse_schema(&root()).unwrap();
        let doc = schema_doc(&s);
        for good in [
            "- click :",
            "- set_node_text :",
            "- assert_changed :",
            "- undo_app_state :",
        ] {
            assert!(doc.contains(good), "prompt is missing `{good}`");
        }
        for bad in [
            "- redraw",
            "- relayout",
            "- create_component",
            "- export_code",
            "- get_node_layout",
            "- get_display_list",
            "- assert_layout",
            "- assert_screenshot",
            "- close",
            "- open_file",
            // NOTE: `focus`, `blur`, `move`, `dpi_changed` and `get_dom` used to
            // be listed here as ZOMBIES. They are not any more — they have real
            // match arms (see the `KNOWN_ZOMBIES = &[]` comment below), so the
            // prompt SHOULD offer them. `no_zombie_is_reachable` is the test
            // that keeps zombies out of the prompt, and it derives the set from
            // the code; hardcoding names here only rots.
        ] {
            assert!(!doc.contains(bad), "prompt must not offer `{bad}`");
        }
    }

    #[test]
    fn the_prompt_lists_every_param_an_assertion_actually_reads() {
        let s = parse_schema(&root()).unwrap();
        let doc = schema_doc(&s);

        // Extract the rendered schema line for one op ("- <name> : <params>").
        let line = |op: &str| -> String {
            doc.lines()
                .find(|l| l.starts_with(&format!("- {op} :")))
                .unwrap_or_else(|| panic!("prompt does not offer `{op}` at all"))
                .to_string()
        };

        // THE REPORTED BUG. `eval_assert_manager_invariants` reads its two
        // params through a `list(key, default)` closure, not through a literal
        // `params.get("…")`, so the scanner saw nothing and the prompt said
        // "(no params)" — the model could not narrow the assertion to a
        // manager or an invariant and emitted the broadest form every time.
        let mi = line("assert_manager_invariants");
        assert!(
            mi.contains("managers"),
            "assert_manager_invariants must advertise `managers`, got: {mi}"
        );
        assert!(
            mi.contains("cross"),
            "assert_manager_invariants must advertise `cross`, got: {mi}"
        );
        assert!(
            !mi.contains("(no params)"),
            "assert_manager_invariants must not be advertised as param-less: {mi}"
        );

        // SAME CLASS, different mechanism: `assert_resource_counts` takes one
        // free-form key per counter, looked up in the map `collect_resource_counts`
        // builds. Only `vs` was ever advertised.
        let rc = line("assert_resource_counts");
        for counter in ["vs", "fonts", "images", "parsed_fonts"] {
            assert!(
                rc.contains(counter),
                "assert_resource_counts must advertise `{counter}`, got: {rc}"
            );
        }

        // SAME CLASS, third mechanism: `assert_response` has no eval fn at all
        // (the step loop handles it inline), so it was rendered "(no params)"
        // — while being the mandatory partner of every `get_*` query op.
        let ar = line("assert_response");
        for p in ["type", "contains"] {
            assert!(
                ar.contains(p),
                "assert_response must advertise `{p}`, got: {ar}"
            );
        }

        // `assert_idle_frames` is handled in the step loop, as `assert_response` is: the
        // gate must know it and advertise every param the step reads (E2E-M2).
        let idle = line("assert_idle_frames");
        for p in ["frames", "step_ms", "vs", "threshold"] {
            assert!(
                idle.contains(p),
                "assert_idle_frames must advertise `{p}`, got: {idle}"
            );
        }
        assert!(
            !idle.contains("(no params)"),
            "assert_idle_frames takes four: {idle}"
        );

        // The two TIMER ops. They are the only drive surface the suite has for
        // `CallbackChange::AddTimer` / `RemoveTimer`, and every one of their
        // params is load-bearing: an `add_timer` rendered without `node_id` or
        // `text` cannot be written at all, and one rendered without
        // `interval_ms` produces a timer the op rejects by name. This is the
        // scanner blind spot that already bit `assert_manager_invariants`,
        // checked against the REAL full.rs rather than a synthetic fixture.
        let at = line("add_timer");
        for p in ["timer_id", "interval_ms", "node_id", "text"] {
            assert!(at.contains(p), "add_timer must advertise `{p}`, got: {at}");
        }
        assert!(!at.contains("(no params)"), "add_timer takes four: {at}");
        let rt = line("remove_timer");
        assert!(
            rt.contains("timer_id"),
            "remove_timer must advertise `timer_id`, got: {rt}"
        );
    }

    // -----------------------------------------------------------------------
    // Incremental semantics
    // -----------------------------------------------------------------------

    const CORPUS: &str = "\
[a/one] first test
[a/two] second test
[b/three] third test
";

    fn art(hash: &str, valid: bool) -> Artifact {
        Artifact {
            // Must match the REAL naming convention (`<NNNNN>-<slug>.json`), not
            // `<hash>.json`: only files named that way are eligible for pruning,
            // so a helper using an unrealistic name would silently exempt every
            // artifact in these tests from the orphan logic they exercise.
            path: PathBuf::from(format!("/out/00001-{hash}.json")),
            hash: Some(hash.to_string()),
            valid,
        }
    }

    fn hashes(w: &[Work]) -> BTreeSet<String> {
        w.iter().map(|x| x.hash.clone()).collect()
    }

    #[test]
    fn hash_is_content_addressed_and_line_number_independent() {
        assert_eq!(
            line_hash("[a/one] first test"),
            line_hash("  [a/one] first test  ")
        );
        assert_ne!(
            line_hash("[a/one] first test"),
            line_hash("[a/one] second test")
        );

        // The SAME line, moved down by an insertion, keeps its hash — only the
        // cosmetic index/filename move.
        let before = parse_corpus(CORPUS, Path::new("/out"));
        let after = parse_corpus(
            &format!("[z/new] inserted at the top\n{CORPUS}"),
            Path::new("/out"),
        );
        assert_eq!(before[0].hash, after[1].hash);
        assert_eq!(before[0].index, 1);
        assert_eq!(after[1].index, 2);
        assert_ne!(before[0].out, after[1].out); // <NNNNN>- prefix follows the line
    }

    #[test]
    fn a_valid_artifact_is_done_even_with_no_done_list() {
        let w = parse_corpus(CORPUS, Path::new("/out"));
        let arts = [art(&w[0].hash, true)];
        let p = plan(w.clone(), &hashes(&w), &arts, &BTreeSet::new(), false, None);
        assert_eq!((p.total, p.already_done, p.todo.len()), (3, 1, 2));
        assert!(p.orphans.is_empty());
        // re-running is a no-op once everything landed
        let all: Vec<Artifact> = w.iter().map(|x| art(&x.hash, true)).collect();
        let p = plan(w.clone(), &hashes(&w), &all, &BTreeSet::new(), false, None);
        assert_eq!((p.already_done, p.todo.len()), (3, 0));
    }

    #[test]
    fn an_invalid_artifact_is_not_done_and_a_done_list_cannot_override_that() {
        let w = parse_corpus(CORPUS, Path::new("/out"));
        let arts = [art(&w[0].hash, false)];
        // the done-list claims line 0 is done; the artifact says otherwise.
        let done: BTreeSet<String> = [w[0].hash.clone()].into_iter().collect();
        let p = plan(w.clone(), &hashes(&w), &arts, &done, false, None);
        assert_eq!((p.already_done, p.invalid, p.todo.len()), (0, 1, 3));
        assert_eq!(p.todo[0].hash, w[0].hash);

        // ...and a done-list entry with NO artifact on disk is likewise not done.
        let p = plan(w.clone(), &hashes(&w), &[], &done, false, None);
        assert_eq!((p.already_done, p.todo.len()), (0, 3));
    }

    #[test]
    fn limit_means_generate_n_more() {
        let w = parse_corpus(CORPUS, Path::new("/out"));
        // nothing done: first 2
        let p = plan(
            w.clone(),
            &hashes(&w),
            &[],
            &BTreeSet::new(),
            false,
            Some(2),
        );
        assert_eq!(p.todo.len(), 2);
        assert_eq!(p.todo_total, 3);
        assert_eq!(p.todo[0].index, 1);
        // now those 2 landed: --limit 2 again picks up the REMAINING one
        let arts: Vec<Artifact> = p.todo.iter().map(|x| art(&x.hash, true)).collect();
        let p = plan(
            w.clone(),
            &hashes(&w),
            &arts,
            &BTreeSet::new(),
            false,
            Some(2),
        );
        assert_eq!(p.already_done, 2);
        assert_eq!(p.todo.len(), 1);
        assert_eq!(p.todo[0].index, 3);
    }

    #[test]
    fn limit_composes_with_filter_and_filter_does_not_create_orphans() {
        let all = parse_corpus(CORPUS, Path::new("/out"));
        let corpus_hashes = hashes(&all);
        let filtered: Vec<Work> = all
            .iter()
            .filter(|w| w.tag.contains("a/"))
            .cloned()
            .collect();
        assert_eq!(filtered.len(), 2);
        // the [b/three] artifact exists but is filtered out of the work list —
        // it must NOT be reported as an orphan.
        let arts = [art(&all[2].hash, true)];
        let p = plan(
            filtered,
            &corpus_hashes,
            &arts,
            &BTreeSet::new(),
            false,
            Some(1),
        );
        assert_eq!(p.total, 2);
        assert_eq!(p.todo.len(), 1);
        assert_eq!(p.todo_total, 2);
        assert!(p.orphans.is_empty());
    }

    #[test]
    fn redo_regenerates_everything() {
        let w = parse_corpus(CORPUS, Path::new("/out"));
        let arts: Vec<Artifact> = w.iter().map(|x| art(&x.hash, true)).collect();
        let p = plan(w.clone(), &hashes(&w), &arts, &BTreeSet::new(), true, None);
        assert_eq!((p.already_done, p.todo.len()), (0, 3));
    }

    #[test]
    fn corpus_drift_orphans_the_artifacts_of_deleted_lines() {
        let w = parse_corpus(CORPUS, Path::new("/out"));
        let arts: Vec<Artifact> = w.iter().map(|x| art(&x.hash, true)).collect();

        // the corpus is regenerated: a line is INSERTED at the top and the third
        // line is DROPPED. The two survivors must stay done (hash, not index),
        // the dropped one's artifact becomes a stale orphan, the new line is work.
        let drifted = parse_corpus(
            "[z/new] brand new line\n[a/one] first test\n[a/two] second test\n",
            Path::new("/out"),
        );
        let p = plan(
            drifted.clone(),
            &hashes(&drifted),
            &arts,
            &BTreeSet::new(),
            false,
            None,
        );
        assert_eq!(p.total, 3);
        assert_eq!(
            p.already_done, 2,
            "the two moved-but-unchanged lines stay done"
        );
        assert_eq!(p.todo.len(), 1);
        assert_eq!(p.todo[0].tag, "z/new");
        // Path must match `art()`'s naming, which mirrors the real
        // `<NNNNN>-<slug>.json` convention — only such files are prunable.
        assert_eq!(
            p.orphans,
            vec![PathBuf::from(format!("/out/00001-{}.json", w[2].hash))]
        );
    }

    #[test]
    fn an_unidentified_file_is_an_orphan() {
        let w = parse_corpus(CORPUS, Path::new("/out"));

        // A file WE named but carrying no `_source_hash` (written before
        // content-addressing, or truncated mid-write) is genuinely ours and
        // genuinely unclaimed — an orphan, and `--prune` may remove it.
        let ours = Artifact {
            path: PathBuf::from("/out/00007-some-generated-slug.json"),
            hash: None,
            valid: true,
        };
        // A file we did NOT name is not ours to reclassify. The out-dir is now
        // the shared `e2e/` corpus, which also holds the hand-written scenarios;
        // orphaning those would let a single `--prune` delete the entire
        // hand-written suite, regression tests included.
        let foreign = Artifact {
            path: PathBuf::from("/out/handwritten.json"),
            hash: None,
            valid: true,
        };
        let p = plan(
            w.clone(),
            &hashes(&w),
            &[ours, foreign],
            &BTreeSet::new(),
            false,
            None,
        );
        assert_eq!(p.todo.len(), 3);
        assert_eq!(
            p.orphans,
            vec![PathBuf::from("/out/00007-some-generated-slug.json")],
            "only a file this generator NAMED may be pruned",
        );
    }

    #[test]
    fn the_stamp_round_trips_and_still_passes_the_gate() {
        let s = parse_schema(&root()).unwrap();
        let ex = fs::read_to_string(root().join(EXAMPLE_JSON)).unwrap();
        let w = parse_corpus(CORPUS, Path::new("/out")).remove(0);
        let stamped = stamp(&ex, &w);
        validate(&s, &stamped).expect("a stamped artifact must still validate");

        let dir = std::env::temp_dir().join(format!("gene2e-stamp-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("00001-x.json");
        fs::write(&p, &stamped).unwrap();
        let a = read_artifact(&s, &p);
        assert_eq!(a.hash.as_deref(), Some(w.hash.as_str()));
        assert!(a.valid);
        assert_eq!(scan_artifacts(&s, &dir).len(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    // -----------------------------------------------------------------------
    // The visual artifact (`capture_damage_png`)
    // -----------------------------------------------------------------------

    #[test]
    fn a_png_is_not_required_but_a_stray_png_path_is_refused() {
        let s = parse_schema(&root()).unwrap();

        // NOT required: a capture step in every scenario is an artifact tax on
        // 13k tests for an image nobody opens (and a blank one for every idle
        // case). Images are the reviewer's triage material instead.
        validate(
            &s,
            r#"{"name":"x","steps":[
                {"op":"snapshot_frame","as":"before"},
                {"op":"assert_changed","vs":"before"}]}"#,
        )
        .expect("a scenario without a capture step is perfectly valid");

        // ...and one that DOES capture is equally fine.
        validate(
            &s,
            r#"{"name":"x","steps":[
                {"op":"snapshot_frame","as":"before"},
                {"op":"capture_damage_png","path":"target/e2e/x.png"},
                {"op":"assert_changed","vs":"before"}]}"#,
        )
        .unwrap();

        // The `path` is the one model-authored string this process hands to
        // `fs::write`, so it is confined to `target/e2e/`.
        for bad in [
            r#""/etc/passwd.png""#,
            r#""target/e2e/../../src/main.rs.png""#,
            r#""e2e/x.png""#,
            r#""target/e2e/x.txt""#,
            r#""target/e2e/.png""#,
            "42",
        ] {
            let json = format!(
                r#"{{"name":"x","steps":[
                    {{"op":"snapshot_frame","as":"before"}},
                    {{"op":"capture_damage_png","path":{bad}}},
                    {{"op":"assert_changed","vs":"before"}}]}}"#
            );
            assert!(validate(&s, &json).is_err(), "gate accepted PNG path {bad}");
        }
    }

    /// The GENERATOR prompt must not ask for an image: that decision moved to
    /// the reviewer, which captures one only for a failure it is investigating.
    #[test]
    fn the_generation_prompt_does_not_tax_every_test_with_an_image() {
        let s = parse_schema(&root()).unwrap();
        let p = build_prompt(&schema_doc(&s), "{}", "[a/b] something happens");
        // The op is still OFFERED by the schema listing (a line may legitimately
        // call for a capture) — but no instruction demands one.
        for demand in ["MANDATORY", "must write one", "step is REJECTED"] {
            assert!(
                !p.contains(demand),
                "the prompt still taxes every test: {demand}"
            );
        }
        assert!(
            !p.contains("<your test name>.png"),
            "the per-test PNG recipe is gone; images are triage material now"
        );
    }

    /// The prompt, with every run of whitespace folded to one space, so a
    /// sentence can be looked for whatever way it wraps.
    fn flat_prompt() -> String {
        let s = parse_schema(&root()).unwrap();
        let ex = fs::read_to_string(root().join(EXAMPLE_JSON)).unwrap();
        build_prompt(&schema_doc(&s), &ex, "[a/b] something happens")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// D1. The old paragraph told the model the root of a mounted document is
    /// node 0 and its children are 1, 2, 3. They are not (`<html>` 0, `<body>`
    /// 1, a whitespace run for every newline between two tags), and 44 of 44
    /// id-addressed scenarios of the first batch landed on the page or on
    /// whitespace. The prompt now says: name every node by a selector.
    #[test]
    fn the_prompt_names_every_node_by_a_selector_and_no_longer_teaches_the_false_numbering() {
        let p = flat_prompt();
        for want in [
            "NAME EVERY NODE BY A SELECTOR",
            "never a numeric `node_id`",
            "WHY NOT A NUMBER",
            "`<html>` (node 0) and `<body>` (node 1)",
            "whitespace text node that takes an id of its own",
            "unless the `mount` says `\"collapse_whitespace\": true`",
            "the checker REJECTS such an id",
            "`parent_selector`",
            "AFTER that `wait_frame`",
        ] {
            assert!(p.contains(want), "the prompt lost `{want}`");
        }
        for false_claim in [
            "The root of a mounted document is node 0 and its children follow",
            "use low ids (1, 2, 3)",
            "(or its `node_id` form)",
            // the schema's own one-line hints must not undo the rule either
            "use Click with node_id",
        ] {
            assert!(
                !p.contains(false_claim),
                "the prompt still says `{false_claim}`"
            );
        }
    }

    /// D2 - D4, D8: the claims the first batch's review proved false are gone,
    /// and what it measured is there.
    #[test]
    fn the_prompt_states_what_the_runner_does_today() {
        let p = flat_prompt();

        // D2: a CSS override costs 0 passes for a paint-only property and 1 for a
        // geometry one (R3: the runner used to lay out twice, 2); a no-op is exactly 0
        // (R2: it used to cost what a real change costs); the control is `min` and
        // nothing else.
        assert!(p.contains("0 paint-only / 1 geometry"), "the counter table");
        assert!(p.contains("runs layout: one pass per override"));
        assert!(p.contains("is a no-op: the write is recorded, nothing is laid out"));
        for stale in [
            "0 paint-only / 2 geometry",
            "this runner reports 2 passes",
            "is not recognised as a no-op yet",
            "does not meet today",
            "never on whether the value changed",
        ] {
            assert!(
                !p.contains(stale),
                "the prompt still states what the runner no longer does: `{stale}`"
            );
        }
        assert!(
            p.contains("never put `min_layout_passes` on a recolour"),
            "a paint-only change spends no layout pass"
        );
        assert!(p.contains("is `exact_layout_passes: 0`: write it exactly like that"));
        assert!(p.contains("is `min_layout_passes: 1` and nothing more"));
        assert!(
            !p.contains("route through a full relayout"),
            "the old claim that every CSS override costs a relayout"
        );
        assert!(!p.contains("is `max_layout_passes: 0`, and nothing else expresses it"));

        // D3: damage reads an accumulator, and the closed sets are said.
        assert!(p.contains("DAMAGE ASSERTIONS READ AN ACCUMULATOR"));
        assert!(p.contains("\"frame\": \"last\""));
        assert!(p.contains("\"last\" is a value of `frame`, never of `which`"));
        assert!(
            p.contains("`assert_damage_sound` (the change must then lie inside what was presented")
        );
        assert!(
            !p.contains("which has no `which`"),
            "`assert_damage_sound` takes a `which` now (R11)"
        );
        assert!(
            !p.contains("silently treated as \"paint\""),
            "an unknown `which` is refused now, not read as paint"
        );

        // D4 (E2E-R8): the renderer-side counters count what the lists draw, and only the
        // two legacy tables are inert.
        for want in [
            "All but two counters move in this",
            "`fonts`, `font_hash_map`, `images` and `image_key_map`, which count",
            "A picture drawn by ten boxes is ONE image",
            "`font_id_map` and `font_families_map` are legacy tables no",
            "so never assert them (an `eq` on",
            "A LEAK TEST HAS TWO SAMPLES AND BOTH READ THE SAME COUNTERS",
            "A swap (`set_node_image` again) replaces the picture",
            "\"parsed_fonts\": \"gt\", \"fonts\": \"gt\"",
        ] {
            assert!(p.contains(want), "the prompt lost `{want}`");
        }
        for stale in [
            "only `parsed_fonts`, `font_chain_cache` and `font_hash_to_families` ever move",
            "`\"fonts\": \"eq\"` or `\"images\": \"eq\"` is `0 == 0`",
            "stay 0 before and after anything a scenario can do",
            "let the `images` control go red until the runner counts images",
            "ONE FAMILY OF COUNTERS PER CALL",
        ] {
            assert!(
                !p.contains(stale),
                "the prompt still says `{stale}`: the renderer-side counters move now (R8)"
            );
        }
        assert!(
            !p.contains("\"images\": \"eq\", \"fonts\": \"le\"}"),
            "the old leak recipe on counters that never move"
        );

        // E2E-M2: N idle frames are ONE step, not a quadruple per frame.
        for want in [
            "{\"op\":\"assert_idle_frames\",\"frames\":40,\"step_ms\":16}",
            "ONE step, NOT a snapshot / tick / wait_frame / assert quadruple per frame",
            "Every frame is held to the SAME picture",
            "the frames at 1, 10, 20 and 40 of a line are `\"frames\": 40` - nothing is sampled",
        ] {
            assert!(p.contains(want), "the prompt lost `{want}`");
        }
        for stale in [
            "RE-SNAPSHOT EACH FRAME",
            "one such triple per frame the line asks for",
            "Use distinct snapshot names (f0, f1, f2",
        ] {
            assert!(
                !p.contains(stale),
                "the prompt still teaches the unrolled frames: `{stale}`"
            );
        }

        // D8: a window of identical frames must not overlap the scrollbar fade.
        assert!(p.contains("must not overlap time-driven state"));
        assert!(
            p.contains("`wait_settled` is the other way to that rest: it moves the clock itself")
        );
        assert!(
            !p.contains("`wait_settled` does not help in this runner"),
            "a scripted wait_settled advances the clock now (E2E-M3)"
        );
    }

    /// Batch 3, Defect 1. An op's doc is its first PARAGRAPH, on one line - not
    /// its first line: `wheel`'s sign convention sat on the sixth line of its doc
    /// and the model never saw it.
    #[test]
    fn b3_d1_a_doc_is_its_first_paragraph_on_one_line() {
        let mut doc = DocParagraph::default();
        for line in [
            "", // blank lines before the text are nothing
            " A hardware wheel notch at (x, y):",
            "a NEGATIVE `delta_y` scrolls DOWN.",
            "",
            "The second paragraph is not shown.",
        ] {
            doc.push(line);
        }
        assert_eq!(
            doc.take().as_deref(),
            Some("A hardware wheel notch at (x, y): a NEGATIVE `delta_y` scrolls DOWN.")
        );
        // The slate is clean for the next variant: no text, no closed paragraph.
        assert_eq!(doc.take(), None);
        doc.push("Next.");
        assert_eq!(doc.take().as_deref(), Some("Next."));
    }

    /// Batch 3, Defect 1: a doc is at most `OP_DOC_MAX_CHARS` characters and is
    /// cut after a whole sentence or at a whole word, never inside one.
    #[test]
    fn b3_d1_a_long_doc_is_cut_at_a_sentence_or_a_word_never_inside_one() {
        // It fits: untouched.
        assert_eq!(cap_doc("Short. Sentences.", 300), "Short. Sentences.");
        // The last full sentence that fits, when it keeps half the room.
        assert_eq!(
            cap_doc("aaaa bbbb cccc dddd. eeee ffff gggg hhhh iiii jjjj", 30),
            "aaaa bbbb cccc dddd."
        );
        // A sentence end in the first half is not worth the room: cut at a word.
        assert_eq!(
            cap_doc("Hi. aaaa bbbb cccc dddd eeee ffff gggg hhhh", 30),
            "Hi. aaaa bbbb cccc dddd eeee…"
        );
        assert_eq!(
            cap_doc("aaaa bbbb cccc dddd eeee ffff gggg hhhh", 30),
            "aaaa bbbb cccc dddd eeee…"
        );
        // Characters, not bytes: a long run of 2-byte letters neither panics nor
        // overruns.
        let wide = "é".repeat(400);
        assert_eq!(cap_doc(&wide, OP_DOC_MAX_CHARS).chars().count(), OP_DOC_MAX_CHARS);
    }

    /// Batch 3, Defect 1, on the real `full.rs`: the schema lines carry the
    /// sentences that used to be cut. `wheel` says which way down is (a raw
    /// notch: NEGATIVE), `scroll` says it too (a programmatic delta: POSITIVE)
    /// and had no doc at all, `tick_ms` keeps its "WITHOUT sleeping", and no line
    /// is longer than the cap.
    #[test]
    fn b3_d1_the_schema_lines_say_which_way_is_down() {
        let s = parse_schema(&root()).unwrap();
        let doc = schema_doc(&s);
        let line = |op: &str| -> String {
            doc.lines()
                .find(|l| l.starts_with(&format!("- {op} :")))
                .unwrap_or_else(|| panic!("the prompt does not offer `{op}`"))
                .to_string()
        };

        let wheel = line("wheel");
        assert!(
            wheel.contains("NEGATIVE `delta_y` scrolls the content DOWN"),
            "{wheel}"
        );
        let scroll = line("scroll");
        assert!(
            scroll.contains("POSITIVE `delta_y` scrolls the content DOWN"),
            "{scroll}"
        );
        let tick = line("tick_ms");
        assert!(tick.contains("WITHOUT sleeping."), "{tick}");
        assert!(tick.contains("Everything time-driven"), "{tick}");

        for l in doc.lines().filter(|l| l.starts_with("- ")) {
            if let Some((_, d)) = l.split_once("   // ") {
                assert!(d.chars().count() <= OP_DOC_MAX_CHARS, "over the cap: {l}");
            }
        }
    }

    /// Batch 3, Defect 1: the prompt itself teaches the wheel's sign and tells the
    /// model to PROVE a scroll. `delta_y: +120` scrolled nothing, nothing proved
    /// the scroll, and the line stayed green on a list that never moved.
    #[test]
    fn b3_d1_the_prompt_teaches_the_wheel_sign_and_to_prove_the_scroll() {
        let p = flat_prompt();
        for want in [
            "SCROLLING. `wheel` is a RAW hardware notch: a NEGATIVE `delta_y` scrolls the content \
             DOWN",
            "leaves the offset at 120 once it has glided",
            "a positive `delta_y` scrolls UP, which at the top of a list moves nothing",
            "`scroll` and `scroll_node_by` are programmatic and take a POSITIVE delta for \"down\"",
            "`scroll_node_to` takes the absolute offset",
            "A wheel glides on a timer: follow it with `tick_ms` + `wait_frame` before you read \
             the offset",
            "must PROVE it with `assert_scroll {\"selector\": \".list\", \"y\": 120}` after the \
             glide",
        ] {
            assert!(p.contains(want), "the prompt lost `{want}`");
        }
    }

    /// Batch 3, Defects 2-4: a wheel/drag/hover STARTS a fade and the idle chain
    /// gets its four ticks first; the per-node fade properties are the box's own
    /// fade (E2E-R9; they were inert); the overflow recipe is a shape; `pending` is the
    /// last resort. The old wording is gone.
    #[test]
    fn b3_d2_d3_d4_the_fade_preamble_the_overflow_shape_and_the_pending_hatch() {
        let p = flat_prompt();
        for want in [
            "A wheel, a drag or a hover over a scroller makes its overlay scrollbar visible, and \
             that STARTS A FADE: 500 ms visible, then 200 ms fading",
            "fades by them instead (each alone: a box that sets only the delay keeps the 200 ms",
            "a delay of 0 is a bar that never fades, a duration of 0 a bar that is gone the moment \
             the delay is up",
            "ALWAYS put four `tick_ms` 250 ms, each followed by `wait_frame`",
            "That preamble is not one of the steps the line counts",
            "`#kid { width: 100px; height: 200px; background: blue; flex-shrink: 0; }`",
            "put `overflow` on the row and let a taller sibling stick out of it",
        ] {
            let flat = p.split_whitespace().collect::<Vec<_>>().join(" ");
            let want = want.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(flat.contains(&want), "the prompt lost `{want}`");
        }
        for stale in [
            "are parsed but do not",
            "are parsed but inert",
            "These are window-level constants",
            "four or more times",
            "that no fixture can make visible",
            "(wrap it in a window-sized frame)",
        ] {
            let flat = p.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(!flat.contains(stale), "the prompt still says `{stale}`");
        }
    }

    /// D5 - D7, D9, D10.
    #[test]
    fn the_prompt_tells_how_to_mount_what_a_control_needs_and_what_a_quantifier_is() {
        let p = flat_prompt();
        for want in [
            // D5
            "an \"empty body\" is [\"<body></body>\"], never []",
            "`html, body { margin:0; width:100%; height:100% }`",
            "A VIRTUALVIEW (a list too long to build) is a key of the `mount`, never markup",
            "\"virtual_views\":[{\"selector\":\".list\",\"rows\":5000,\"row_height\":24,",
            "The rows are the VIEW's document: the page's `css` does not reach them",
            "Look at the view with `get_virtual_view_states` and `assert_response` `contains`",
            // D6 (batch 2 replaced the paragraph: the per-property recipe)
            "A CONTROL THAT CANNOT MOVE A PIXEL PROVES NOTHING",
            "never a `flex-grow:1` item (it ignores an overridden `width`)",
            // D7
            "copy the line's own quantifier",
            "\"at least one\" is `min_*: 1` and nothing else",
            // D9
            "Do not append coverage, soundness or work-counter assertions the line does not name",
            // D10
            "a suffix that makes the name unique is appended for you",
        ] {
            assert!(p.contains(want), "the prompt lost `{want}`");
        }
        // E2E-M1: a VirtualView CAN be mounted now; the line that sent the generator to
        // the nearest plain scroller and a `pending` is gone.
        for stale in [
            "A line that needs a VirtualView cannot be expressed",
            "\"pending\": \"needs a mountable VirtualView\"",
            "do NOT create a VirtualView",
        ] {
            assert!(!p.contains(stale), "the prompt still says `{stale}`");
        }
    }

    /// E2E-M1: the paragraph that teaches `virtual_views` sends the generator only to ops the
    /// gate lets through. The first draft told it to look inside the view with
    /// `get_virtual_view_layout`, which is geometry and DENIED - every scenario that followed
    /// the prompt would have been refused by the gate.
    #[test]
    fn e2e_m1_the_virtual_view_paragraph_names_only_ops_the_gate_allows() {
        let s = parse_schema(&root()).unwrap();
        let p = flat_prompt();
        let start = p
            .find("A VIRTUALVIEW (a list too long to build)")
            .expect("the prompt has the paragraph about virtual views");
        let end = start
            + p[start..]
                .find("FONTS:")
                .expect("the paragraph is followed by FONTS");
        let mut named = Vec::new();
        // The odd pieces of a split on the backtick are the words inside backticks.
        for word in p[start..end].split('`').skip(1).step_by(2) {
            if s.is_known(word) {
                named.push(word);
            }
        }
        for op in ["scroll_node_by", "wheel", "get_virtual_view_states"] {
            assert!(
                named.contains(&op),
                "the paragraph no longer names `{op}`: {named:?}"
            );
        }
        for op in &named {
            assert_eq!(
                classify(op),
                OpClass::Allowed,
                "the paragraph names `{op}`, which the gate refuses"
            );
        }
    }

    /// E2E-M1: a `mount` whose virtual view cannot land is refused by the gate at its step, in
    /// the words of the `mount` op ([`azul_layout::e2e::check_mount_views`]) - not as a list
    /// that never appears at run time - and a view that lands passes, the ops after it
    /// addressing the host by the class the page gave it.
    #[test]
    fn e2e_m1_the_gate_refuses_a_mount_whose_virtual_view_cannot_land() {
        let s = parse_schema(&root()).unwrap();
        let scenario = |views: &str| {
            format!(
                r##"{{"name":"x","steps":[
                    {{"op":"mount","collapse_whitespace":true,"html":["<div class=\"list\"></div>"],
                      "css":[".list {{ width:300px; height:200px; overflow:auto; }}"],
                      "virtual_views":{views}}},
                    {{"op":"wait_frame"}},
                    {{"op":"snapshot_frame","as":"before"}},
                    {{"op":"scroll_node_by","selector":".list","delta_x":0,"delta_y":240}},
                    {{"op":"wait_frame"}},
                    {{"op":"assert_changed","vs":"before"}}]}}"##
            )
        };
        let view = |selector: &str, rows: &str, row_html: &str| {
            format!(
                r##"[{{"selector":"{selector}","rows":{rows},"row_height":24,"row_html":{row_html}}}]"##
            )
        };
        let row = r##"["<div class=\"label\">Row {n}</div>"]"##;

        validate(&s, &scenario(&view(".list", "5000", row)))
            .unwrap_or_else(|e| panic!("a view that lands is refused: {e}"));

        refused(
            &s,
            &scenario(&view(".nothing", "5000", row)),
            "virtual_views[0] ('.nothing'): the selector must match exactly one element",
        );
        refused(&s, &scenario(&view(".list", "0", row)), "rows is 0");
        refused(
            &s,
            &scenario(&view(".list", "5000", r##"["<div>"]"##)),
            "`row_html` is not valid markup",
        );
        // A field the view does not have is the engine's refusal, by name.
        refused(
            &s,
            &scenario(
                r##"[{"selector":".list","rows":5,"row_height":24,"row_html":["x"],"row_count":5}]"##,
            ),
            "row_count",
        );
        // The refusal names the step, as the other mount refusals do.
        refused(&s, &scenario(&view(".nothing", "5000", row)), "step 0");
    }

    /// Batch 2 of the review (defects 1, 2, 3, 5, 6). The paragraph that said "a
    /// control must be able to change a pixel" and left the model to work out how
    /// produced five dead controls in 26 no-op scenarios; the `<img>` placeholder,
    /// the growth template and the name example each cost scenarios of their own.
    #[test]
    fn the_prompt_measures_a_control_on_its_own_and_makes_it_visible() {
        let p = flat_prompt();
        for want in [
            // Defect 1: measured on its own, and visible per property.
            "MEASURE IT ON ITS OWN: right before the control's mutation call \
             `reset_frame_counters` and take a fresh `snapshot_frame` (`ctl_before`), and \
             assert the control against THAT snapshot",
            "its `assert_changed` fails as a DEAD CONTROL",
            "If you cannot, change the FIXTURE, not the property",
            "background-color: the new colour differs from the old one",
            "width / height: a FIXED-size painted box",
            "border-width: `border-style: solid`",
            "padding / margin: a painted child",
            "display:none / visibility:hidden: the node must PAINT something itself",
            // Batch 3, Defect 3: the exact shape, not "wrap it in a frame".
            "overflow: build exactly this shape. A window-sized frame",
            "holds the node under test at a SMALLER fixed size",
            "so `overflow: visible` shows the child hanging out of the box",
            "position (static -> absolute): a painted sibling FOLLOWS the node",
            "justify-content / align-items / flex-direction / flex-grow: at least TWO painted \
             flex items",
            "\"pending\": \"control cannot be made visible: <why>\"",
            // Batch 3, Defect 4: move the override before giving up.
            "NO node of ANY fixture you can build shows",
            "Before you give up, move the override to the node that CAN show the property",
            // Defect 2: an `<img>` paints nothing.
            "its placeholder raster is transparent: an `<img>` paints NOTHING until it has a \
             `background-color` or you call `set_node_image` on it",
            "A page whose only content is an `<img>` renders blank white",
            // Defect 3: a control ADDS, and the families are called apart.
            "the control must ADD to what the baseline holds",
            "Mount the baseline's nodes PLUS one new node whose text uses a font stack the \
             baseline never used",
            "A LEAK TEST HAS TWO SAMPLES AND BOTH READ THE SAME COUNTERS",
            "A fixture with no text and no image has nothing to leak",
            // Defect 5: a control that only MOVES boxes reads present damage.
            "and on `assert_changed` (a control that only MOVES boxes is `assert_changed \
             {\"vs\": \"ctl_before\", \"which\": \"present\"}`)",
            // Defect 6: the name has a form, and one example that does not lean on a family.
            "of the form <behaviour>_<property>_<fixture>",
            "mouse_move_converges_image_flex_row",
        ] {
            assert!(p.contains(want), "the prompt lost `{want}`");
        }
        for gone in [
            // the paragraph and the claims the batch proved wrong
            "A control only counts if it can change a pixel",
            "(with a placeholder raster)",
            "a second `mount` (or `set_node_text`) whose text uses a font stack the first one did \
             not",
            "still write the `eq` assertion AND the `gt` control",
            // the renderer-side counters move now (R8): no dead family to call apart
            "(red there means the engine, not the harness)",
            "write the `eq` assertion and the `gt` control as separate calls",
            "noop_width_image_flex_row",
            // assert_resource_counts judges every counter now (note B)
            "alphabetically",
            "stops at the first failure",
        ] {
            assert!(!p.contains(gone), "the prompt still says `{gone}`");
        }
    }

    /// The braces of every JSON example in the prompt came through `format!`.
    #[test]
    fn the_prompt_has_no_unescaped_braces_left() {
        let p = flat_prompt();
        assert!(
            !p.contains("{{") && !p.contains("}}"),
            "a doubled brace leaked"
        );
        for snippet in [
            "{\"op\":\"assert_damage\",\"kind\":\"none\",\"frame\":\"last\"}",
            "{\"min_layout_passes\": 1}",
        ] {
            assert!(p.contains(snippet), "missing the example {snippet}");
        }
    }

    /// The recipes the prompt teaches are accepted by the gate that will check
    /// the files written from them - a prompt whose own examples were refused
    /// would be a lie.
    #[test]
    fn the_recipes_in_the_prompt_pass_the_gate() {
        let s = parse_schema(&root()).unwrap();
        let steps = [
            // the SCROLLING paragraph's two examples (batch 3, Defect 1)
            r#"{"op":"wheel","x":100,"y":100,"delta_x":0,"delta_y":-120}"#,
            r#"{"op":"assert_scroll","selector":".list","y":120}"#,
            r#"{"op":"assert_damage","kind":"none","frame":"last"}"#,
            r#"{"op":"assert_damage","kind":"rects","max_area_ratio":0.5}"#,
            r#"{"op":"assert_work_bounded","exact_layout_passes":0}"#,
            r#"{"op":"assert_work_bounded","min_layout_passes":1}"#,
            r#"{"op":"assert_damage_covers_changes","vs":"before","which":"present"}"#,
            r#"{"op":"assert_resource_counts","vs":"baseline","parsed_fonts":"eq","font_chain_cache":"eq","font_hash_to_families":"eq"}"#,
            r#"{"op":"assert_resource_counts","vs":"baseline","parsed_fonts":"gt"}"#,
            r#"{"op":"assert_resource_counts","vs":"baseline","parsed_fonts":"gt","fonts":"gt"}"#,
            r#"{"op":"add_image_to_cache","css_id":"pic","width":16,"height":16,"color":"red"}"#,
            r#"{"op":"assert_resource_counts","vs":"baseline","images":"gt","image_key_map":"gt"}"#,
            r#"{"op":"assert_idle_frames","frames":40,"step_ms":16}"#,
            r#"{"op":"assert_idle_frames","frames":10,"vs":"before","threshold":2}"#,
            r#"{"op":"tick_ms","ms":250}"#,
        ];
        let scenario = format!(
            r##"{{"name":"x","pending":"a note the checker ignores","steps":[
                {{"op":"mount","html":["<body><div id=\"box\" class=\"list\"></div></body>"]}},
                {{"op":"wait_frame"}},{{"op":"wait","ms":100}},
                {{"op":"reset_frame_counters"}},
                {{"op":"snapshot_frame","as":"before"}},{{"op":"snapshot_resources","as":"baseline"}},
                {{"op":"set_node_css_override","selector":"#box","property":"width","value":"9px"}},
                {{"op":"focus_node","selector":"#box"}},{{"op":"mouse_move","selector":"#box"}},
                {}]}}"##,
            steps.join(",")
        );
        validate(&s, &scenario).unwrap_or_else(|e| panic!("{e}\n{scenario}"));
    }

    /// The worked example is part of the prompt, and the prompt says two resource
    /// counters (the legacy font tables) never move: the example must not teach
    /// the `0 == 0` leak test the first batch copied from it.
    #[test]
    fn the_worked_example_does_not_assert_the_counters_the_prompt_calls_inert() {
        let ex: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(root().join(EXAMPLE_JSON)).unwrap()).unwrap();
        let inert = ["font_id_map", "font_families_map"];
        let mut checked = 0;
        for step in ex["steps"].as_array().unwrap() {
            if step["op"] != "assert_resource_counts" {
                continue;
            }
            checked += 1;
            for key in step.as_object().unwrap().keys() {
                assert!(
                    !inert.contains(&key.as_str()),
                    "the example asserts the inert counter `{key}`: {step}"
                );
            }
        }
        assert!(
            checked > 0,
            "the example no longer shows assert_resource_counts"
        );
    }

    /// `take_native_screenshot` can only ever fail here: it goes through an e2e
    /// host hook that NOTHING in the workspace installs. Offering it would
    /// generate tests that are red on arrival for a reason that says nothing
    /// about the engine.
    #[test]
    fn an_op_that_always_fails_headlessly_is_not_offered() {
        let s = parse_schema(&root()).unwrap();
        assert!(s.is_known("take_native_screenshot"));
        assert!(matches!(
            classify("take_native_screenshot"),
            OpClass::Denied(_)
        ));
        assert!(!schema_doc(&s).contains("- take_native_screenshot"));
        assert!(validate(&s, &with_op(r#"{"op":"take_native_screenshot"}"#)).is_err());
        // ...while the CPU-render one, which needs no hook, stays available.
        assert_eq!(classify("take_screenshot"), OpClass::Allowed);
    }

    /// The triage instruction is rendered from the PARSED schema, so it cannot
    /// drift from the op it tells the reviewer to use.
    #[test]
    fn the_triage_instruction_is_derived_from_the_schema() {
        let s = parse_schema(&root()).unwrap();
        let t = triage_doc(&s);
        for want in [
            PNG_OP,
            TRIAGE_DIR,
            "path",
            "which?",
            "crop?",
            "byte-identical",
        ] {
            assert!(t.contains(want), "the triage doc is missing `{want}`:\n{t}");
        }
        assert!(
            t.contains("never `git add`"),
            "images must never be committed:\n{t}"
        );
        assert!(t.contains("Do NOT use `take_native_screenshot`"), "{t}");

        // An engine without the capture op gets an honest "you cannot look".
        let dir = std::env::temp_dir().join(format!("gene2e-triage-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let r = synthetic_root(&dir, "        DebugEvent::Focus => { do_focus(); }\n");
        let s2 = parse_schema(&r).unwrap();
        assert!(!s2.is_known(PNG_OP));
        assert!(triage_doc(&s2).contains("no way to capture an image"));
        fs::remove_dir_all(&dir).unwrap();
    }

    // -----------------------------------------------------------------------
    // `--review-batch`: argument parsing
    // -----------------------------------------------------------------------

    fn parse(args: &[&str]) -> Result<GenE2eOptions> {
        GenE2eOptions::parse(args)
    }

    #[test]
    fn review_batch_is_the_batch_size_and_implies_the_limit() {
        let o = parse(&["corpus.txt", "out", "--review-batch", "10"]).unwrap();
        assert_eq!(o.review_batch, Some(10));
        assert_eq!(
            o.limit,
            Some(10),
            "--review-batch N must generate N, not the whole corpus"
        );
        // Token cost is not the constraint here — quality is. A weak generator
        // writes a scenario that quietly tests the wrong thing, and a weak
        // reviewer rubber-stamps it; both defaults must be the good model.
        assert_eq!((o.model.as_str(), o.effort.as_str()), (MODEL, EFFORT));
        assert_eq!(
            (o.review_model.as_str(), o.review_effort.as_str()),
            (MODEL, EFFORT)
        );
        assert_eq!(MODEL, "opus");
        assert!(
            matches!(EFFORT, "medium" | "high" | "xhigh" | "max"),
            "the CLI scale is low|medium|high|xhigh|max and `low` is not good enough"
        );

        // Overridable.
        let o = parse(&[
            "corpus.txt",
            "out",
            "--review-batch",
            "3",
            "--review-model",
            "sonnet",
            "--review-effort",
            "medium",
        ])
        .unwrap();
        assert_eq!(
            (o.review_model.as_str(), o.review_effort.as_str()),
            ("sonnet", "medium")
        );

        // Flag order must not matter.
        let o = parse(&["--review-batch", "4", "corpus.txt", "out"]).unwrap();
        assert_eq!((o.review_batch, o.limit), (Some(4), Some(4)));
    }

    #[test]
    fn review_batch_rejects_a_second_opinion_about_the_batch_size() {
        // Two knobs fighting over one number; silently taking the smaller would
        // make the operator mis-read the batch they just reviewed.
        for args in [
            ["corpus.txt", "out", "--review-batch", "10", "--limit", "5"],
            ["corpus.txt", "out", "--limit", "5", "--review-batch", "10"],
        ] {
            let e = parse(&args).unwrap_err().to_string();
            assert!(e.contains("--review-batch"), "{e}");
        }
        assert!(parse(&["corpus.txt", "out", "--review-batch", "0"]).is_err());
        assert!(parse(&["corpus.txt", "out", "--review-batch"]).is_err());
        assert!(parse(&["corpus.txt", "out", "--review-batchh", "2"]).is_err());
        // ...and without the flag nothing changes.
        let o = parse(&["corpus.txt", "out", "--limit", "5"]).unwrap();
        assert_eq!((o.review_batch, o.limit), (None, Some(5)));
    }

    #[test]
    fn the_usage_string_documents_the_review_flag() {
        for flag in [
            "--review-batch",
            "--review-model",
            "--review-effort",
            "--limit",
            "--prune",
        ] {
            assert!(USAGE.contains(flag), "usage does not mention {flag}");
        }
        let e = parse(&["only-one-positional"]).unwrap_err().to_string();
        assert!(
            e.contains("--review-batch"),
            "the parse error must show the usage: {e}"
        );
    }

    // -----------------------------------------------------------------------
    // `--review-batch`: report assembly (pure — no `claude`, no engine)
    // -----------------------------------------------------------------------

    fn step(index: usize, op: &str, error: Option<&str>) -> azul_layout::e2e::E2eStepResult {
        azul_layout::e2e::E2eStepResult {
            step_index: index,
            op: op.to_string(),
            status: if error.is_some() { "fail" } else { "pass" }.to_string(),
            duration_ms: 1,
            logs: Vec::new(),
            screenshot: None,
            error: error.map(str::to_string),
            response: None,
        }
    }

    fn result(name: &str, steps: Vec<azul_layout::e2e::E2eStepResult>) -> E2eTestResult {
        let failed = steps.iter().filter(|s| s.status == "fail").count();
        E2eTestResult {
            name: name.to_string(),
            status: if failed == 0 { "pass" } else { "fail" }.to_string(),
            skip_reason: None,
            duration_ms: 7,
            step_count: steps.len(),
            steps_passed: steps.len() - failed,
            steps_failed: failed,
            steps,
            final_screenshot: None,
        }
    }

    /// A batch with one of everything the report has to be able to say.
    fn sample_batch() -> ReviewBatch {
        ReviewBatch {
            id: "00001-n4-deadbeef".to_string(),
            corpus: "scripts/E2E_TESTS.txt".to_string(),
            out_dir: PathBuf::from("/out"),
            entries: vec![
                ReviewEntry {
                    line: "[damage/basic] clicking a button repaints only the button".to_string(),
                    path: PathBuf::from("/out/00001-good.json"),
                    json: Some(
                        r##"{"name":"good","steps":[{"op":"click","selector":"#b"},
                        {"op":"capture_damage_png","path":"target/e2e/good.png"},
                        {"op":"assert_changed","vs":"before"},
                        {"op":"assert_damage_incremental","max_area_ratio":0.5}]}"##
                            .to_string(),
                    ),
                    gen_error: None,
                    run: Some(result("good", vec![step(0, "click", None)])),
                },
                ReviewEntry {
                    line: "[clipboard/copy] ctrl+c puts the selection on the clipboard".to_string(),
                    path: PathBuf::from("/out/00002-harness.json"),
                    json: Some(
                        r#"{"name":"harness","steps":[{"op":"key_down","key":"c"},
                        {"op":"capture_damage_png","path":"target/e2e/harness.png"},
                        {"op":"assert_changed","vs":"before"}]}"#
                            .to_string(),
                    ),
                    gen_error: None,
                    run: Some(result(
                        "harness",
                        vec![step(
                            3,
                            "key_down",
                            Some(&format!(
                                "e2e runner: CallbackChange::SetCopyContent {HARNESS_MARKER} (no \
                                 OS clipboard)"
                            )),
                        )],
                    )),
                },
                ReviewEntry {
                    line: "[damage/scroll] scrolling a list repaints the viewport".to_string(),
                    path: PathBuf::from("/out/00003-engine.json"),
                    json: Some(
                        r#"{"name":"engine","steps":[{"op":"scroll","dy":40},
                        {"op":"assert_changed","vs":"before"}]}"#
                            .to_string(),
                    ),
                    gen_error: None,
                    run: Some(result(
                        "engine",
                        vec![step(1, "assert_changed", Some("no damage after scroll"))],
                    )),
                },
                ReviewEntry {
                    line: "[layout/geometry] the box is 60px wide".to_string(),
                    path: PathBuf::from("/out/00004-rejected.json"),
                    json: None,
                    gen_error: Some("step 2: op `assert_layout` is DENIED".to_string()),
                    run: None,
                },
            ],
            verdict_block: "\ntest good ... \u{1b}[32mPASS\u{1b}[0m (7 ms)\ntest result: \
                            \u{1b}[31mFAILED\u{1b}[0m. 1 passed; 2 failed\n"
                .to_string(),
        }
    }

    #[test]
    fn the_report_states_the_facts_including_the_ugly_ones() {
        let f = render_facts(&sample_batch());

        // Header tally.
        assert!(f.contains("**4** line(s) — 3 generated, 1 rejected"), "{f}");
        assert!(f.contains("**1 passed, 2 failed**"), "{f}");
        assert!(f.contains("capture an image: 2/3 (NOT required"), "{f}");

        // Every corpus line is shown next to its artifact — a reviewer judging
        // the JSON against itself is the failure mode this exists to prevent.
        for line in [
            "clicking a button repaints only the button",
            "ctrl+c puts the selection on the clipboard",
            "the box is 60px wide",
        ] {
            assert!(
                f.contains(line),
                "the report drops the corpus line {line:?}"
            );
        }

        // ATTRIBUTION: mechanical where the runner confessed, honestly open
        // where it did not. Calling an unattributed failure an engine bug is
        // exactly the false confidence the corpus exists to avoid.
        assert!(f.contains("**HARNESS**"), "{f}");
        assert!(f.contains("SetCopyContent"), "{f}");
        assert!(f.contains("**UNATTRIBUTED**"), "{f}");
        let harness_at = f.find("**HARNESS**").unwrap();
        let engine_at = f.find("**UNATTRIBUTED**").unwrap();
        assert_ne!(harness_at, engine_at);

        // Images: a MISSING capture is not a defect any more, but the report
        // must still say where the reviewer's images went and that they are not
        // in git — otherwise the operator looks for them in the wrong place.
        assert!(
            !f.contains("NO `capture_damage_png`"),
            "a missing image is not a defect: {f}"
        );
        assert!(f.contains(TRIAGE_DIR), "{f}");
        assert!(f.contains("`.gitignore` already excludes"), "{f}");

        // Gate rejections survive into the report — they are prompt evidence.
        assert!(f.contains("Rejected before it ever ran"), "{f}");
        assert!(f.contains("assert_layout"), "{f}");

        // The verdict block is the runner's own words, minus the ANSI.
        assert!(f.contains("test result: FAILED. 1 passed; 2 failed"), "{f}");
        assert!(
            !f.contains('\u{1b}'),
            "ANSI escapes leaked into the markdown"
        );
    }

    /// The pair test of the two markers `assert_changed` writes and the report
    /// reads (one owner: `azul_layout::e2e`): the REAL producer - `assert_changed`
    /// in a real headless run - and the REAL consumer - the facts and the review
    /// prompt. A step the runner attributed itself is tagged with whose fault it
    /// is; any other red step stays open.
    #[test]
    fn the_report_marks_what_the_runner_attributed_itself() {
        let run = |tail: serde_json::Value| -> E2eTestResult {
            let mut steps = vec![
                serde_json::json!({ "op": "mount",
                    "html": ["<body><div id=\"box\"></div></body>"],
                    "css": ["html, body { margin: 0; }",
                            "#box { width: 100px; height: 80px; background: #0000ff; }"] }),
                serde_json::json!({ "op": "wait_frame" }),
                serde_json::json!({ "op": "wait", "ms": 100 }),
                serde_json::json!({ "op": "snapshot_frame", "as": "before" }),
            ];
            steps.extend(tail.as_array().unwrap().iter().cloned());
            let test: E2eTest = serde_json::from_value(serde_json::json!({
                "name": "attributed",
                "config": { "continue_on_failure": true },
                "setup": { "window_width": 400, "window_height": 300, "dpi": 96 },
                "steps": steps,
            }))
            .unwrap();
            azul_layout::e2e::run_e2e_test(&test)
        };
        // Nothing happens, nothing is repainted: the control is dead.
        let dead = run(serde_json::json!([
            { "op": "reset_frame_counters" },
            { "op": "wait_frame" },
            { "op": "assert_changed", "vs": "before" },
        ]));
        // The box is recoloured, then the damage is zeroed: the screen moved and
        // nothing says so.
        let stale = run(serde_json::json!([
            { "op": "set_node_css_override", "selector": "#box",
              "property": "background-color", "value": "#ff0000" },
            { "op": "wait_frame" },
            { "op": "reset_frame_counters" },
            { "op": "assert_changed", "vs": "before" },
        ]));

        for (result, marker, who) in [
            (dead, DEAD_CONTROL_MARKER, "BAD TEST"),
            (stale, STALE_SCREEN_MARKER, "ENGINE"),
        ] {
            let facts = {
                let mut b = sample_batch();
                b.entries.truncate(1);
                b.entries[0].run = Some(result);
                render_facts(&b)
            };
            assert!(
                facts.contains(marker),
                "the runner's own words ({marker}) are in the facts: {facts}"
            );
            assert!(
                facts.contains(&format!("**[{who}, attributed by the runner]**")),
                "the step is tagged {who}: {facts}"
            );
        }

        // A red step the runner said nothing about stays untagged.
        assert_eq!(runner_attribution("assert_work_bounded: got 2"), None);

        // The reviewer is told both markers, so it does not re-derive them.
        let s = parse_schema(&root()).unwrap();
        let p = build_review_prompt(
            &sample_batch(),
            "the generator prompt",
            &policy_doc(&s),
            &triage_doc(&s),
        );
        for marker in [STALE_SCREEN_MARKER, DEAD_CONTROL_MARKER] {
            assert!(p.contains(marker), "the review prompt never names {marker}");
        }
    }

    #[test]
    fn a_green_batch_is_not_reported_as_a_good_batch() {
        let mut b = sample_batch();
        b.entries.truncate(1);
        let f = render_facts(&b);
        assert!(f.contains("**1 passed, 0 failed**"), "{f}");
        // Zero failures must NOT read as approval: a test can pass by asserting
        // nothing, which is the whole reason a reviewer exists.
        assert!(f.contains("not automatically a good batch"), "{f}");
    }

    #[test]
    fn colliding_png_paths_are_reported_because_the_scenarios_race() {
        let mut b = sample_batch();
        b.entries[2].json = Some(
            r#"{"name":"engine","steps":[{"op":"capture_damage_png","path":"target/e2e/good.png"},
               {"op":"assert_changed","vs":"before"}]}"#
                .to_string(),
        );
        let f = render_facts(&b);
        assert!(f.contains("PNG PATH COLLISION"), "{f}");
        assert!(f.contains("target/e2e/good.png"), "{f}");
    }

    #[test]
    fn duplicate_scenario_names_are_reported_with_the_rows_that_share_them() {
        let mut b = sample_batch();
        // Row 3 carries the name of row 1, whether or not its run came back.
        b.entries[2].json = Some(
            r#"{"name":"good","steps":[{"op":"scroll","dy":40},
               {"op":"assert_changed","vs":"before"}]}"#
                .to_string(),
        );
        let f = render_facts(&b);
        assert!(f.contains("Duplicate scenario names"), "{f}");
        assert!(f.contains("- `good`: rows #1, #3"), "{f}");

        // ...and a batch with no clash says nothing.
        assert!(!render_facts(&sample_batch()).contains("Duplicate scenario names"));

        // The notice used to count only the entries that RECEIVED a run, so
        // when the pairing lost one it could never fire.
        b.entries[0].run = None;
        let f = render_facts(&b);
        assert!(f.contains("- `good`: rows #1, #3"), "{f}");
    }

    /// G1. Two scenarios of one name, two different outcomes: each file gets
    /// ITS result. Pairing by name gave both the first one's - 38 passed/failed
    /// in the header where the real tally was 5 and 45, and seven scenarios that
    /// "did not run".
    #[test]
    fn results_are_paired_with_their_files_not_with_their_names() {
        let entry = |line: &str, file: &str| ReviewEntry {
            line: line.to_string(),
            path: PathBuf::from(file),
            json: Some(r#"{"name":"same","steps":[{"op":"assert_changed"}]}"#.to_string()),
            gen_error: None,
            run: None,
        };
        let mut b = sample_batch();
        b.entries = vec![
            entry("[a/one] passes", "/out/00001-a.json"),
            entry("[a/two] fails", "/out/00002-b.json"),
            entry("[a/three] passes too", "/out/00003-c.json"),
        ];
        // The tests came from the files in this order, results in the same order.
        let owner = [0usize, 1, 2];
        let results = vec![
            result("same", vec![step(0, "assert_changed", None)]),
            result(
                "same",
                vec![step(0, "assert_changed", Some("nothing repainted"))],
            ),
            result("same", vec![step(0, "assert_changed", None)]),
        ];
        pair_results(&mut b.entries, &owner, &results);

        let status: Vec<&str> = b.entries.iter().map(|e| e.status().1).collect();
        assert_eq!(status, ["pass", "FAIL", "pass"], "one result per FILE");
        let f = render_facts(&b);
        assert!(f.contains("**2 passed, 1 failed**"), "{f}");
        assert!(!f.contains("NOT RUN"), "{f}");
        assert!(f.contains("[a/two] fails"), "{f}");
        // exactly one failure is attributed, to the file that failed
        let failures = f.split("### Failures, attributed").nth(1).unwrap();
        assert!(failures.contains("[a/two] fails"), "{f}");
        assert!(!failures.contains("[a/one] passes"), "{f}");

        // A file of several tests gives its entry the FIRST one's result, and a
        // result without an owner is dropped, never misfiled.
        let mut b2 = sample_batch();
        b2.entries = vec![entry("[a/one] two tests", "/out/00001-a.json")];
        pair_results(
            &mut b2.entries,
            &[0, 0, 7],
            &[
                result("same", vec![step(0, "assert_changed", None)]),
                result("same", vec![step(0, "assert_changed", Some("late"))]),
                result("same", vec![]),
            ],
        );
        assert_eq!(b2.entries[0].status().1, "pass");
    }

    /// The prompt tells the model to park a line the ops cannot express with a
    /// top-level "pending": a parked scenario ran no step, so it is neither a
    /// pass nor a failure - and the reason it gives is the evidence of a missing op.
    #[test]
    fn a_pending_scenario_is_parked_not_failed() {
        let mut b = sample_batch();
        let mut parked = result("good", Vec::new());
        parked.status = "pending".to_string();
        parked.skip_reason = Some("needs a mountable VirtualView".to_string());
        b.entries[0].run = Some(parked);
        let f = render_facts(&b);

        assert!(f.contains("**0 passed, 2 failed**"), "{f}");
        assert!(
            f.contains("1 parked (pending / skipped: no step ran)"),
            "{f}"
        );
        assert!(f.contains("| PENDING |"), "{f}");
        let failures = f
            .split("### Failures, attributed")
            .nth(1)
            .unwrap()
            .split("### Parked")
            .next()
            .unwrap();
        assert!(
            !failures.contains("clicking a button repaints only the button"),
            "a parked scenario is not listed among the failures:\n{failures}"
        );
        let section = f
            .split("### Parked (ran no step)")
            .nth(1)
            .expect("a parked section");
        assert!(
            section.contains("clicking a button repaints only the button"),
            "{f}"
        );
        assert!(section.contains("needs a mountable VirtualView"), "{f}");
    }

    /// A scenario that loaded and got no result back is SAID in the header,
    /// never folded into "failed" or hidden.
    #[test]
    fn a_scenario_with_no_result_is_reported_as_not_run() {
        let mut b = sample_batch();
        b.entries[0].run = None;
        let f = render_facts(&b);
        assert!(f.contains("**1 NOT RUN**"), "{f}");
        assert!(f.contains("| not run |"), "{f}");
    }

    /// G6. The first red step is the cause; the later ones ran on what it left.
    #[test]
    fn every_red_step_of_a_scenario_is_listed_and_only_the_first_is_the_cause() {
        let mut b = sample_batch();
        b.entries[2].run = Some(result(
            "engine",
            vec![
                step(1, "assert_changed", Some("no damage after scroll")),
                step(2, "assert_damage", Some("wrong paint damage kind")),
                step(5, "assert_idle_stable", Some("2400 px differ")),
            ],
        ));
        let f = render_facts(&b);
        assert!(
            f.contains("step 1 `assert_changed`: no damage after scroll"),
            "{f}"
        );
        assert!(
            f.contains("step 2 `assert_damage` (also red; may follow from the first): wrong"),
            "{f}"
        );
        assert!(f.contains("step 5 `assert_idle_stable` (also red"), "{f}");
        assert!(
            f.contains("every red step of a scenario is reported"),
            "{f}"
        );
    }

    // -----------------------------------------------------------------------
    // D10: a name of its own
    // -----------------------------------------------------------------------

    #[test]
    fn a_scenario_name_is_made_unique_by_the_source_hash() {
        let json = r#"{
  "name": "noop_css_override_no_layout",
  "description": "d",
  "steps": [
    {"op":"mount","html":["<body></body>"],"name":"inner"},
    {"op":"assert_exists","selector":"body"}
  ],
  "tail": {"name": "also inner"}
}"#;
        let a = with_unique_name(json, "fd18fff35c2bf740");
        assert!(
            a.contains(r#""name": "noop_css_override_no_layout_fd18fff3""#),
            "{a}"
        );
        // only the top-level name changed, and the layout survived byte for byte
        assert_eq!(
            a.replace("_fd18fff3", ""),
            json,
            "nothing but the suffix was spliced in"
        );
        assert!(a.contains(r#""name":"inner""#) && a.contains(r#""name": "also inner""#));

        // two lines, one model name -> two names
        let b = with_unique_name(json, "0123456789abcdef");
        assert!(b.contains("noop_css_override_no_layout_01234567"), "{b}");
        assert_ne!(a, b);

        // idempotent, so re-stamping an artifact does not grow its name
        assert_eq!(with_unique_name(&a, "fd18fff35c2bf740"), a);
        // a short hash is used whole
        assert!(with_unique_name(json, "abc").contains("no_layout_abc\""));
        // no top-level string name -> left alone (the gate rejects it)
        let nameless = r#"{"steps":[{"op":"x","name":"inner"}]}"#;
        assert_eq!(with_unique_name(nameless, "fd18fff35c2bf740"), nameless);
        let numeric = r#"{"name": 5, "steps": []}"#;
        assert_eq!(with_unique_name(numeric, "fd18fff35c2bf740"), numeric);
        // an escaped quote in the name does not end it early
        let quoted = r#"{"name": "a\"b", "steps": []}"#;
        assert_eq!(
            with_unique_name(quoted, "fd18fff35c2bf740"),
            r#"{"name": "a\"b_fd18fff3", "steps": []}"#
        );
    }

    #[test]
    fn the_artifact_that_is_written_has_a_unique_name_and_still_passes_the_gate() {
        let s = parse_schema(&root()).unwrap();
        let reply = r##"{"name":"noop_width","description":"d",
            "steps":[{"op":"mount","html":["<body><div id=\"a\"></div></body>"]},
                     {"op":"wait_frame"},{"op":"snapshot_frame","as":"b"},
                     {"op":"set_node_css_override","selector":"#a","property":"width","value":"9px"},
                     {"op":"wait_frame"},{"op":"assert_changed","vs":"b"}]}"##;
        let w = parse_corpus("[a/one] first\n[a/two] second\n", Path::new("/out"));
        let (first, second) = (artifact_text(reply, &w[0]), artifact_text(reply, &w[1]));
        validate(&s, &first).expect("the artifact passes the gate");
        validate(&s, &second).expect("the artifact passes the gate");

        let name_of = |t: &str| {
            serde_json::from_str::<serde_json::Value>(t).unwrap()["name"]
                .as_str()
                .unwrap()
                .to_string()
        };
        assert_ne!(
            name_of(&first),
            name_of(&second),
            "one model name, two lines"
        );
        assert_eq!(name_of(&first), format!("noop_width_{}", &w[0].hash[..8]));
        // the hash stamp is still the first key, and the artifact is still recognised
        assert!(first.starts_with("{\n  \"_source_hash\""), "{first}");
        let dir = std::env::temp_dir().join(format!("gene2e-unique-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("00001-x.json");
        fs::write(&path, &first).unwrap();
        let a = read_artifact(&s, &path);
        assert_eq!(a.hash.as_deref(), Some(w[0].hash.as_str()));
        assert!(a.valid);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_review_prompt_carries_the_batch_the_prompt_and_the_policy() {
        let s = parse_schema(&root()).unwrap();
        let b = sample_batch();
        let current = build_prompt(&schema_doc(&s), "{}", "[damage/basic] a click repaints");
        let p = build_review_prompt(&b, &current, &policy_doc(&s), &triage_doc(&s));

        // The generator's own prompt, verbatim — a reviewer cannot propose a fix
        // to a paragraph it was only told about.
        assert!(p.contains("GENERATOR_PROMPT"), "{p}");
        assert!(
            p.contains("A FAILING TEST IS A SUCCESS"),
            "the current prompt is missing"
        );
        // The batch: JSON, corpus line, and the run outcome including the error.
        assert!(p.contains("assert_damage_incremental"));
        assert!(p.contains("ctrl+c puts the selection on the clipboard"));
        assert!(p.contains("FAILING STEP 1 `assert_changed`: no damage after scroll"));
        assert!(p.contains("NOT GENERATED"));
        // The DENIED half of the policy, which the generator never sees.
        assert!(p.contains("assert_layout: geometry"), "{p}");
        assert!(p.contains("redraw: debugger-only"), "{p}");
        // The required output shape, including the right to say no.
        for section in [
            "VERDICT: REJECT",
            "Per-test verdict",
            "Missing ops",
            "Harness vs engine",
            "Prompt defects",
            "Visual triage",
        ] {
            assert!(
                p.contains(section),
                "the review prompt never asks for `{section}`"
            );
        }
        assert!(
            p.contains("Nothing you write is applied automatically"),
            "the reviewer must know a human applies the diff"
        );
        // ...and it must be told to LOOK at a failure rather than theorise, with
        // the copy-first rule that keeps the committed scenario untouched.
        assert!(p.contains("LOOK, DO NOT GUESS"), "{p}");
        assert!(p.contains(TRIAGE_DIR), "{p}");
        assert!(
            p.contains("do NOT propose adding capture steps to the"),
            "the reviewer must not re-introduce the per-test image tax"
        );
    }

    /// A REAL review is prose about weak tests, and says things like
    /// "insufficient assertions" or "try again with a tighter bound". Scanning
    /// the whole document for the rate-limit vocabulary would throw away exactly
    /// the reports worth reading — the scan is head-only, and skipped once the
    /// reply opens with the verdict it was asked for.
    #[test]
    fn a_critical_review_is_not_mistaken_for_a_rate_limit_message() {
        let harsh = "VERDICT: REJECT\n\nBoth tests have insufficient assertions; the bound is so \
                     wide nothing could violate it. Fix the prompt and try again. The generator \
                     appears to have been overloaded with recipes and picked the safest one. \
                     Quota of one assertion per test is not enough.";
        assert!(
            is_usable_review(harsh),
            "a harsh review was discarded as a limit message"
        );
        assert_eq!(verdict_of(harsh), Some(true));

        // ...while the actual limit message is still caught.
        for junk in [
            "You've reached your usage limit for this 5-hour window. Please try again later, or \
             upgrade your plan to continue using Claude Code today. This message is padded to \
             clear the minimum-length check so the head scan is what rejects it.",
            "short",
            "",
        ] {
            assert!(
                !is_usable_review(junk),
                "a non-review was accepted: {junk:?}"
            );
        }

        // A review that ignores the format is still a review — it just cannot
        // vote, and the operator is told to read it.
        let formatless = "The batch is fine overall, though the second scenario drifts from its \
                          corpus line and asserts a proxy property instead of the one named. \
                          Neither test forces the effect it measures, which is the important part.";
        assert!(is_usable_review(formatless));
        assert_eq!(verdict_of(formatless), None);

        // A verdict buried on page three must not flip the gate.
        let buried = format!("VERDICT: ACCEPT\n\n{}\nVERDICT: REJECT", "x".repeat(500));
        assert_eq!(verdict_of(&buried), Some(false));
    }

    #[test]
    fn a_missing_review_is_loud_not_silent() {
        let facts = "# facts\n\nsome facts\n";
        let with = assemble_report(facts, Some("VERDICT: ACCEPT\n\nlooks fine"));
        assert!(with.starts_with(facts));
        assert!(with.contains("VERDICT: ACCEPT"));

        // A rate-limited reviewer must never leave a report that reads as clean.
        let without = assemble_report(facts, None);
        assert!(
            without.starts_with(facts),
            "the facts survive a failed review"
        );
        assert!(without.contains("PRODUCED NOTHING USABLE"), "{without}");
    }

    #[test]
    fn batch_id_is_content_addressed() {
        let w = parse_corpus(CORPUS, Path::new("/out"));
        let id = batch_id(&w);
        assert_eq!(id, batch_id(&w), "the same batch must name the same report");
        assert!(id.starts_with("00001-n3-"), "{id}");
        assert_ne!(
            batch_id(&w[..2]),
            id,
            "a different batch is a different report"
        );
        assert_eq!(batch_id(&[]), format!("00000-n0-{}", &line_hash("")[..8]));
    }

    #[test]
    fn ansi_is_stripped_but_the_text_is_not() {
        assert_eq!(strip_ansi("\u{1b}[32mPASS\u{1b}[0m (7 ms)"), "PASS (7 ms)");
        assert_eq!(strip_ansi("plain"), "plain");
    }

    #[test]
    fn a_markdown_cell_cannot_break_the_table() {
        assert_eq!(cell("a|b", 10), "a/b");
        assert_eq!(cell("a\nb", 10), "a b");
        assert_eq!(cell("abcdef", 4), "abc…");
    }
}
