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
//! Hard-won lessons inherited from `scripts/autotest_fleet.sh`:
//!   1. A RATE-LIMITED `claude -p` exits 0 and answers with the limit message
//!      as PLAIN TEXT. That must never be written out as a test.
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
//!   * artifact whose hash is no longer in the corpus -> STALE ORPHAN, reported,
//!     deleted only with `--prune`
//! `--limit N` means "generate N MORE", i.e. it truncates the not-yet-done list
//! (after `--filter`), never the corpus.
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
use azul_layout::e2e::{load_e2e_tests, render_report, E2eTest, E2eTestResult};

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
                         [--effort E] [--limit N] [--filter <tag>] [--dry-run] [--redo] \
                         [--prune] [--review-batch N [--review-model M] [--review-effort E]]";

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
    /// One-line doc, if the enum variant carried one.
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
    // -- ALLOW: MOCK INPUT — the primary drive surface ----------------------
    ("mouse_move",                None),
    ("mouse_down",                None),
    ("mouse_up",                  None),
    ("click",                     None),
    ("click_node",                None),
    ("double_click",              None),
    ("scroll",                    None),
    // Content-chokepoint image ops (overlay refactor O1): mutate through
    // LayoutWindow::apply_content_change exactly like a callback would.
    ("set_node_image",            None),
    ("add_image_to_cache",        None),
    ("remove_image_from_cache",   None),
    ("key_down",                  None),
    ("key_up",                    None),
    ("text_input",                None),
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

    // -- ALLOW: HARNESS CONTROL --------------------------------------------
    ("mount",                     None),
    ("unmount",                   None),
    ("tick_ms",                   None),
    ("wait",                      None),
    ("wait_frame",                None),
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
    ("get_node_hierarchy",        None),
    ("get_html_string",           None),
    ("get_node_css_properties",   None),
    ("get_node_dataset",          None),
    ("get_focus_state",           None),
    ("get_cursor_state",          None),
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
    ("update_component_compile_fn",Some("visual-editor/IDE surface, not engine behaviour")),
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
    ("export_code",                Some("codegen surface, not engine behaviour")),
    ("export_code_zip",            Some("codegen surface, not engine behaviour")),
    ("resolve_function_pointers",  Some("editor/codegen plumbing, not engine behaviour")),
    ("run_e2e_tests",              Some("the test runner itself — a test may not recurse into it")),
    ("get_logs",                   Some("debug-server tooling, asserts nothing about the engine")),
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
    let mut pending_doc: Option<String> = None;
    let mut pending_default = false;
    let mut pending_rename: Option<String> = None;

    for line in body.lines().skip(1) {
        let t = line.trim();

        if depth == 0 {
            // Between variants: collect the doc comment + serde attrs.
            if let Some(d) = t.strip_prefix("///") {
                let d = d.trim();
                if !d.is_empty() && pending_doc.is_none() {
                    pending_doc = Some(d.to_string());
                }
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
        if !a.params.is_empty() {
            continue;
        }
        let Some(body) = top_level_fn_body(&src, &format!("eval_{}", a.name)) else {
            continue;
        };
        if body.contains("params") {
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
        "\n`?` = optional. Params NOT listed here do not exist — do not invent any.\n\
         The op list above is EXHAUSTIVE: an op you do not see above is REJECTED by the \
         validator, and your test is thrown away. In particular there is NO op that forces a \
         repaint or a relayout — the engine must decide to do that BY ITSELF in response to the \
         input/mutation you perform; that decision is exactly what these tests measure.\n\
         `vs` always names a snapshot created EARLIER in the same timeline by \
         `snapshot_frame {\"as\": …}` (pixels), `snapshot_resources {\"as\": …}` \
         (resource counters) or `snapshot_managers {\"as\": …}` (every manager's \
         state, for `assert_only_managers_changed`).\n",
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
- "name": a short snake_case identifier.
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
gate checks JSON shape and op names only. So you will never be told whether it
passed, and you must not try to guess.

Assert what the engine SHOULD do. If the engine is buggy, your test fails — that is
the POINT of this corpus and a genuinely useful result. A test that passes because
you softened it until it could not fail is WORSE THAN NO TEST: it hides the bug and
lends false confidence forever after.

Concretely:
- Pick the bound the one-liner IMPLIES, not a safe-looking one. "does not re-run
  layout" is `max_layout_passes: 0` — never 1 "just in case". "returns to idle" is
  `assert_idle_stable` with damage `none` — never "some small damage is probably OK".
- AN UPPER BOUND ALONE IS SATISFIED BY ZERO. `max_relayouts: 2` is true of an engine
  that dropped the input on the floor and did nothing at all. Whenever the line says
  the step DID something, pin the lower end too (`min_*`, or `exact_*` when the count
  is determined) — see step 6.
- WHEN THE LINE ASSERTS AN ABSENCE, ADD THE CONTROL IT ASKS FOR. "no damage", "no
  layout pass", "the counter did not move" is also exactly what a dead engine
  reports. Most such one-liners spell the control out ("... then change the node's
  background-color as a POSITIVE CONTROL and assert THAT does produce damage"). Put
  it in the SAME timeline, after the absence assertion — never in a second file.
- The numbers in the recipes below are ILLUSTRATIVE placeholders. Derive each bound
  from the sentence you were given; do not copy them by default.
- Never widen a bound, drop an assertion, or downgrade `eq` to `le` to make the test
  feel safer. If the description implies an exact count, assert equality.
- Never add an assertion the line does not ask for merely to make the file look
  substantial. Each assertion must trace to the one-liner.
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
   FONTS: for any text, use only the built-in mock fonts — `Azul Mock Mono`
   (0.5em advance) or `Azul Mock Wide` (1.0em advance). They are registered
   automatically, always resolve, and need no @font-face. If a case needs N
   DISTINCT families, invent N distinct names but ALWAYS end the stack with a mock
   font, e.g. `font-family: MyFakeFamilyA, "Azul Mock Mono";`. NEVER name a real
   system font (Arial, Helvetica, Times, Courier, Verdana): on the CI box they match
   nothing and collapse onto one shared FontId, which makes font-identity and
   leak/font assertions vacuously green.
2. `wait_frame`, then `wait {{"ms": 100}}` to let the first frame settle.
3. `reset_frame_counters`, then `snapshot_frame {{"as": "before"}}` and, if you will
   assert resource counts, `snapshot_resources {{"as": "baseline"}}`.
4. Perform the interaction / mutation / CSS change the line describes (click,
   scroll, key_down, text_input, resize, set_node_css_override, insert_node,
   delete_node, set_node_classes, a second `mount` with changed markup, …).
   KEYBOARD EDITING NEEDS DOM FOCUS FIRST. `text_input` hard-errors with "No focused
   node - text input requires focus on contenteditable" unless some node holds DOM
   focus, and the `focus` / `blur` ops are WINDOW focus — they set `window_focused`
   and nothing else. Use `focus_node {{"selector": ".editor"}}` (or its `node_id`
   form) immediately before typing. Do NOT substitute a click: a click needs a
   coordinate, and you may not guess coordinates.
5. `wait_frame` + `wait` (and `tick_ms` for anything time-driven: momentum,
   fade, blink, animation — `tick_ms` advances the engine clock WITHOUT sleeping).
6. Assert what the line asks for:
   - "the pixels change" / liveness      -> assert_changed  {{"vs": "before", "min_damage_rects": 1}}
   - "damage covers the change" / sound  -> assert_damage_covers_changes {{"vs": "before"}}
   - "a patch, not a full redraw"        -> assert_damage_incremental {{"max_area_ratio": 0.5}}
   - "returns to idle / zero damage"     -> tick_ms, wait, then assert_idle_stable {{"vs": "<a snapshot_frame taken after the change>"}}
   - "EVERY frame identical to the PREVIOUS one" / "stays idle for N frames"
     -> this is NOT the single-snapshot form above. `assert_idle_stable` compares
        against ONE named snapshot, so `snapshot f0; tick; tick; tick; assert vs f0`
        only proves frame_N == frame_0 — an engine that oscillates A -> B -> A -> B -> A
        PASSES it. When the line says every frame equals the previous one, or asks
        the window to stay idle across N frames, RE-SNAPSHOT EACH FRAME and assert
        against the frame immediately before it:
            {{"op":"snapshot_frame","as":"f0"}},
            {{"op":"tick_ms","ms":16}}, {{"op":"wait_frame"}},
            {{"op":"assert_idle_stable","vs":"f0"}}, {{"op":"snapshot_frame","as":"f1"}},
            {{"op":"tick_ms","ms":16}}, {{"op":"wait_frame"}},
            {{"op":"assert_idle_stable","vs":"f1"}}, {{"op":"snapshot_frame","as":"f2"}},
            … one such triple per frame the line asks for.
        Use distinct snapshot names (f0, f1, f2 …); reusing one name compares
        against the wrong frame and silently weakens the test.
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
     animation, `scroll_dirty` cleared, `scrollbar_fade_active` cleared, no latched
     `display_list_dirty`, no orphan caret blink, no unresolved focus request.
     Strictly stronger than `assert_idle_stable` for "did every manager notice the
     interaction ended?".

   ### THE THREE WORK COUNTERS — pick the RIGHT one, they measure different things
   `assert_work_bounded` reads three independent counters. Choosing the wrong one is
   the single most common way to write a test that cannot fail:

   | counter          | what it counts                          | idle | inert pointer event | set_node_css_override | resize |
   |------------------|-----------------------------------------|------|---------------------|-----------------------|--------|
   | `relayouts`      | EVENT PASSES (`process_window_events`)  | 0    | 1                   | 0                     | 1      |
   | `dom_regens`     | runs of the layout callback             | 0    | 0                   | 0                     | 1      |
   | `layout_passes`  | times LAYOUT ACTUALLY RAN               | 0    | 0                   | 1                     | 1      |

   - `relayouts` is NOT a layout counter, despite the name. It is the recursion depth
     of the event pass. `0` means "no state delta was processed" — what an IDLE frame
     looks like. Any input that changes window state runs exactly ONE pass, so
     `max_relayouts: 0` after a click is a test that ALWAYS FAILS against a correct
     engine. `> 1` is the engine's own invalidation-loop signal.
   - `layout_passes` is the one that sees a CALLBACK-API mutation. `set_node_css_override`,
     `set_node_text` and `set_node_classes` never enter the event pass at all, so
     `relayouts` stays `0` for them no matter how much work the engine does — but they
     route through a full relayout, which `layout_passes` counts. "a no-op CSS write
     must not re-run layout" is `max_layout_passes: 0`, and nothing else expresses it.
   - Rows: "does not re-run layout"      -> assert_work_bounded {{"max_layout_passes": 0}}
           "costs exactly one layout pass"-> assert_work_bounded {{"exact_layout_passes": 1}}
           "one event pass, no re-entry" -> assert_work_bounded {{"exact_relayouts": 1, "max_dom_regens": 0}}
           "bounded, and it DID run"     -> assert_work_bounded {{"min_relayouts": 1, "max_relayouts": 2}}
           "never trips the depth cap"   -> any assert_work_bounded (it fails on `hit_depth_cap` unless `allow_depth_cap`)

   - "no leak / counters return"         -> assert_resource_counts {{"vs": "baseline", "images": "eq", "fonts": "le"}}
   - "the counter PROVABLY moved" (a control)
                                         -> assert_resource_counts {{"vs": "baseline", "fonts": "gt"}}
     `gt` / `lt` are what make a leak test mean something: `"fonts": "eq"` is `0 == 0`
     on a window that never loaded a font, and holds on an engine that was deleted.
   - "damage kind / no full redraw"      -> assert_damage {{"kind": "rects", "max_area_ratio": 0.5}}  (kind is "none" | "rects" | "full")
   - "nothing panics"                    -> the steps running at all IS the assertion; still end with a liveness or idle assertion.
   - structure survived a mutation       -> assert_exists / assert_not_exists / assert_node_count / assert_text
7. If the line mentions a NodeId-renumbering mutation (insert/delete/reorder a
   sibling), do the mutation and then assert the DOM still holds
   (assert_node_count / assert_exists) and the window settles (assert_idle_stable).

Node ids: DOM-mutation ops take a numeric `node_id`. The root of a mounted document
is node 0 and its children follow in document order, so mount a small tree and use
low ids (1, 2, 3). Prefer selector-based ops (`click`, `assert_exists`) wherever an
op offers `selector`.

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
                "step {i}: op `{op}` is declared in DebugEvent but has no match arm — it \
                 silently returns ok and does nothing; implement it or remove it before it may \
                 appear in a test"
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
        // `mount` html must be the pretty ARRAY-OF-LINES form.
        if op == "mount" {
            let html = s.get("html").context("mount: missing `html`")?;
            if !html.is_array() && !html.is_string() {
                bail!("step {i}: mount `html` must be an array of lines");
            }
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
        if a.path != *want && !want.exists() && !opts.dry_run {
            if fs::rename(&a.path, want).is_ok() {
                a.path = want.to_path_buf();
                renamed += 1;
            }
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
            "[dry-run] total={} already-done={} to-generate={} stale-orphans={}. Nothing \
             launched.",
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
    let stamped = stamp(json, w);
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

    /// `generated` / `gate-rejected`, and `pass` / `fail` / `not run`.
    fn status(&self) -> (&'static str, &'static str) {
        let gen = if self.json.is_some() {
            "ok"
        } else {
            "REJECTED"
        };
        let run = match self.run.as_ref().map(|r| r.status.as_str()) {
            Some("pass") => "pass",
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
    let failed = b
        .entries
        .iter()
        .filter(|e| e.run.as_ref().is_some_and(|r| r.status != "pass"))
        .count();
    let with_png = b
        .entries
        .iter()
        .filter(|e| !e.png_paths().is_empty())
        .count();

    let mut s = String::new();
    s.push_str(&format!("# gen-e2e batch review — `{}`\n\n", b.id));
    s.push_str(
        "Written by `azul-doc gen-e2e --review-batch N`. The **FACTS** section is machine-generated \
         from the actual run and cannot be argued with. The **REVIEW** section is one agent's \
         reading of it, and is advisory: nothing in it is applied automatically.\n\n",
    );
    s.push_str(&format!(
        "- corpus: `{}`\n- out-dir: `{}`\n- batch: **{n}** line(s) — {generated} generated, {} \
         rejected by the gate\n- run: **{passed} passed, {failed} failed**\n- scenarios that \
         capture an image: {with_png}/{generated} (NOT required — images are triage material, \
         see below)\n\n",
        b.corpus,
        b.out_dir.display(),
        n - generated,
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
    let failures: Vec<&ReviewEntry> = b
        .entries
        .iter()
        .filter(|e| e.run.as_ref().is_some_and(|r| r.status != "pass"))
        .collect();
    s.push_str("\n### Failures, attributed\n\n");
    if failures.is_empty() {
        s.push_str(
            "None. (A batch with zero failures is not automatically a good batch — see the \
                    review below: a test can pass because it asserts nothing.)\n",
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
            for (idx, op, err) in e.failed_steps() {
                s.push_str(&format!("  - step {idx} `{op}`: {}\n", cell(err, 400)));
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
    let mut names: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &b.entries {
        if let Some(r) = e.run.as_ref() {
            *names.entry(r.name.as_str()).or_default() += 1;
        }
    }
    let clashes: Vec<&&str> = names
        .iter()
        .filter(|(_, c)| **c > 1)
        .map(|(n, _)| n)
        .collect();
    if !clashes.is_empty() {
        s.push_str(
            "\n### Duplicate scenario names\n\nThe verdict report pairs results with fixtures BY \
             NAME, so duplicates make the run un-attributable:\n",
        );
        for n in clashes {
            s.push_str(&format!("- `{n}`\n"));
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
         dispatch answers `ok` without doing anything — hidden from the generator and rejected \
         by the gate)\n",
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
            "(this engine has no `{PNG_OP}` op, so there is no way to capture an image — attribute \
             failures from the error text alone.)\n"
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
         about to call an ENGINE bug — LOOK AT WHAT RENDERED instead of guessing:\n\
         \n\
         1. COPY the scenario JSON to `{TRIAGE_DIR}<name>.json`. NEVER edit the artifact in the \
         out-dir: it is the committed test, it must stay byte-identical, and triage steps that \
         leak into it would be committed noise.\n\
         2. In the COPY, insert `{PNG_OP}` steps ({params}) at the interesting points — right \
         after the mount settles, and again after the interaction — writing to \
         `{TRIAGE_DIR}<name>-<step>.png`. It writes the frame MASKED TO THE DAMAGE REGION \
         (transparent elsewhere), which is exactly the picture that settles \"did the engine \
         repaint the right area?\". `which` selects `paint` (default) or `present` damage; `crop` \
         trims to the damage bounding box. A 1x1 transparent PNG means THERE WAS NO DAMAGE — that \
         is a finding, not a broken capture.\n\
         3. Run the copy: `./target/release/azul-doc e2e {TRIAGE_DIR}<name>.json`, then open the \
         PNGs you wrote and say what you see.\n\
         4. Do NOT use `take_native_screenshot` (no host hook is installed anywhere — it fails in \
         every runner we have) and do not bother with `take_screenshot` (it answers with base64 in \
         the step response, it writes no file you can open).\n\
         5. Images are YOUR WORKING MATERIAL, not an artifact. `{TRIAGE_DIR}` is under `target/`, \
         which `.gitignore` excludes — leave them there, never copy one into the repo, never \
         `git add` anything.\n"
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
`dll/src/desktop/shell2/common/event.rs::apply_user_change`. For everything else, USE THE TRIAGE
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
        println!("[review] running {} generated scenario(s) …", tests.len());
        let results = crate::e2erun::run_tests(&tests, Some(opts.jobs))?;
        // `run_tests` sorts by name; pair back to the corpus line by name, and
        // report the collision rather than guessing when names repeat.
        for r in &results {
            if let Some(i) = tests
                .iter()
                .position(|t| t.name == r.name)
                .and_then(|ti| owner.get(ti).copied())
            {
                if entries[i].run.is_none() {
                    entries[i].run = Some(r.clone());
                }
            }
        }
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
        let masked = r#"{"name":"stale_text","steps":[
            {"op":"mount","html":["<div id=\"a\">hi</div>"]},
            {"op":"snapshot_frame","as":"before"},
            {"op":"set_node_text","node_id":1,"text":"bye"},
            {"op":"redraw"},
            {"op":"assert_changed","vs":"before","min_damage_rects":1}]}"#;
        let e = validate(&s, masked).unwrap_err().to_string();
        assert!(e.contains("`redraw`") && e.contains("DENIED"), "{e}");

        // ...and the same test WITHOUT the forced redraw is exactly what we want.
        let honest = r#"{"name":"stale_text","steps":[
            {"op":"mount","html":["<div id=\"a\">hi</div>"]},
            {"op":"snapshot_frame","as":"before"},
            {"op":"set_node_text","node_id":1,"text":"bye"},
            {"op":"wait_frame"},
            {"op":"assert_changed","vs":"before","min_damage_rects":1}]}"#;
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
            "update_component_compile_fn",
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

        // 1. `Focus` is declared but has NO arm -> zombie: hidden from the
        //    prompt, rejected by the gate.
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
    fn duplicate_scenario_names_are_reported_because_results_pair_by_name() {
        let mut b = sample_batch();
        b.entries[2].run = Some(result("good", vec![step(0, "click", None)]));
        let f = render_facts(&b);
        assert!(f.contains("Duplicate scenario names"), "{f}");
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
