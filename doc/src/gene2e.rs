//! `azul-doc gen-e2e <txt> <out-dir>` — fan out a fleet of cheap Claude agents
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
//! (`dll/src/desktop/shell2/common/debug_server/full.rs`) at run time — the op
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

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};

use anyhow::{bail, Context, Result};

/// Relative path of the file that DEFINES the e2e schema. Single source of truth.
const FULL_RS: &str = "dll/src/desktop/shell2/common/debug_server/full.rs";
/// Relative path of the worked example handed to every agent.
const EXAMPLE_JSON: &str = "tests/e2e/mount_damage_smoke.json";

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
}

impl GenE2eOptions {
    pub fn parse(args: &[&str]) -> Result<Self> {
        let mut positional: Vec<&str> = Vec::new();
        let mut opts = Self {
            txt: PathBuf::new(),
            out_dir: PathBuf::new(),
            jobs: 6,
            model: "haiku".to_string(),
            effort: "low".to_string(),
            dry_run: false,
            redo: false,
            limit: None,
            filter: None,
            prune: false,
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
            _ => bail!(
                "usage: azul-doc gen-e2e <txt-file> <out-dir> [--jobs N] [--model M] [--effort \
                 E] [--limit N] [--filter <tag>] [--dry-run] [--redo] [--prune]"
            ),
        }
        if opts.jobs == 0 {
            bail!("gen-e2e: --jobs must be >= 1");
        }
        Ok(opts)
    }
}

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
    /// Ops handled directly by the E2E step loop (not `DebugEvent` variants).
    extra: Vec<String>,
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
        self.known_op(op).is_some() || self.extra.iter().any(|e| e == op)
    }
    /// Every op the engine has, in one list (timeline ops, step-loop ops, asserts).
    fn all_op_names(&self) -> impl Iterator<Item = &str> {
        self.ops
            .iter()
            .chain(self.asserts.iter())
            .map(|o| o.name.as_str())
            .chain(self.extra.iter().map(String::as_str))
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
// and handing all of it to a cheap generator produces self-defeating tests.
//
// What these tests ARE: HEADLESS BEHAVIOUR tests over the cpurender path —
//     MOCK INPUT EVENT -> engine -> CORRECT DAMAGE PATCH / correct behaviour.
// Real OS input is out of scope (manual testing owns it). Layout/geometry
// correctness is out of scope (`azul-doc reftest` owns it). Everything below
// the OS boundary — every Callback API path — is in scope.
//
// The table below classifies EVERY op. It is the law: the prompt is rendered
// from the ALLOWED half (a cheap model cannot use what it is not shown), and
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
    // -- ALLOW: MOCK INPUT — the primary drive surface ----------------------
    ("mouse_move",                None),
    ("mouse_down",                None),
    ("mouse_up",                  None),
    ("click",                     None),
    ("click_node",                None),
    ("double_click",              None),
    ("scroll",                    None),
    ("key_down",                  None),
    ("key_up",                    None),
    ("text_input",                None),
    ("touch_start",               None),
    ("touch_move",                None),
    ("touch_end",                 None),
    ("touch_cancel",              None),
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

    // -- ALLOW: HARNESS CONTROL --------------------------------------------
    ("mount",                     None),
    ("unmount",                   None),
    ("tick_ms",                   None),
    ("wait",                      None),
    ("wait_frame",                None),
    ("reset_frame_counters",      None),
    ("snapshot_frame",            None),
    ("snapshot_resources",        None),
    ("get_frame_report",          None),
    ("capture_damage_png",        None),
    ("take_screenshot",           None),
    ("take_native_screenshot",    None),

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
            if let Some(r) = t.split("rename = \"").nth(1).and_then(|s| s.split('"').next()) {
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
            let name = pending_rename.take().unwrap_or_else(|| name.trim().to_string());
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
        bail!("gen-e2e: no `DebugEvent::` match arms found in full.rs — the dispatch shape changed");
    }

    // ---- 3. ops the E2E step loop handles itself (not DebugEvent variants) --
    let extra: Vec<String> = ["commit_undo_snapshot", "undo_app_state", "redo_app_state"]
        .into_iter()
        .filter(|o| src.contains(&format!("\"{o}\"")))
        .map(str::to_string)
        .collect();

    Ok(Schema {
        ops,
        asserts,
        extra,
        handled,
    })
}

/// Every `params.get("…")` key read inside `fn <name>(…)`.
fn eval_fn_params(src: &str, fn_name: &str) -> Vec<(String, bool)> {
    let Some(start) = src.find(&format!("\nfn {fn_name}(")) else {
        return Vec::new();
    };
    let body = &src[start + 1..];
    // end of fn = first line that is exactly "}" at column 0
    let end = body.find("\n}\n").map_or(body.len(), |e| e + 2);
    let body = &body[..end];

    let mut out: Vec<(String, bool)> = Vec::new();
    let mut rest = body;
    while let Some(p) = rest.find(".get(\"") {
        rest = &rest[p + 6..];
        if let Some(k) = rest.split('"').next() {
            let k = k.to_string();
            // every assertion param is read with `if let Some(..)` = optional,
            // except `vs`/`selector`/`expected`, which the eval fns hard-require.
            let required = matches!(k.as_str(), "selector" | "expected" | "reference");
            if !out.iter().any(|(n, _)| *n == k) {
                out.push((k, required));
            }
        }
    }
    out
}

// ===========================================================================
// The prompt
// ===========================================================================

/// The schema section of the agent prompt — rendered from the parsed `full.rs`,
/// FILTERED THROUGH `OP_POLICY` (only ALLOWED ops are ever shown) and through the
/// ZOMBIE scan (an op with no match arm does nothing, so it is not offered
/// either). A cheap model cannot use what it is not shown; the gate then enforces
/// both rules for real (see `validate`).
fn schema_doc(schema: &Schema) -> String {
    let mut s = String::new();
    s.push_str("### TIMELINE OPS (`{\"op\": \"<name>\", …}`)\n");
    for op in schema
        .ops
        .iter()
        .filter(|o| classify(&o.name) == OpClass::Allowed && !schema.is_zombie(&o.name))
    {
        let params = if op.params.is_empty() {
            "(no params)".to_string()
        } else {
            op.params
                .iter()
                .map(|(n, req)| if *req { n.clone() } else { format!("{n}?") })
                .collect::<Vec<_>>()
                .join(", ")
        };
        match &op.doc {
            Some(d) => s.push_str(&format!("- {} : {}   // {}\n", op.name, params, d)),
            None => s.push_str(&format!("- {} : {}\n", op.name, params)),
        }
    }
    for e in schema
        .extra
        .iter()
        .filter(|e| classify(e) == OpClass::Allowed)
    {
        s.push_str(&format!("- {e} : (no params)\n"));
    }
    s.push_str("\n### ASSERTIONS\n");
    for a in schema
        .asserts
        .iter()
        .filter(|a| classify(&a.name) == OpClass::Allowed)
    {
        let params = if a.params.is_empty() {
            "(no params)".to_string()
        } else {
            a.params
                .iter()
                .map(|(n, req)| if *req { n.clone() } else { format!("{n}?") })
                .collect::<Vec<_>>()
                .join(", ")
        };
        s.push_str(&format!("- {} : {}\n", a.name, params));
    }
    s.push_str(
        "\n`?` = optional. Params NOT listed here do not exist — do not invent any.\n\
         The op list above is EXHAUSTIVE: an op you do not see above is REJECTED by the \
         validator, and your test is thrown away. In particular there is NO op that forces a \
         repaint or a relayout — the engine must decide to do that BY ITSELF in response to the \
         input/mutation you perform; that decision is exactly what these tests measure.\n\
         `vs` always names a snapshot created EARLIER in the same timeline by \
         `snapshot_frame {\"as\": …}` (pixels) or `snapshot_resources {\"as\": …}` \
         (resource counters).\n",
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

## A REAL, PASSING TEST (the ground truth for the format)
```json
{example}
```

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
5. `wait_frame` + `wait` (and `tick_ms` for anything time-driven: momentum,
   fade, blink, animation — `tick_ms` advances the engine clock WITHOUT sleeping).
6. Assert what the line asks for:
   - "the pixels change" / liveness      -> assert_changed  {{"vs": "before", "min_damage_rects": 1}}
   - "damage covers the change" / sound  -> assert_damage_covers_changes {{"vs": "before"}}
   - "a patch, not a full redraw"        -> assert_damage_incremental {{"max_area_ratio": 0.5}}
   - "returns to idle / zero damage"     -> tick_ms, wait, then assert_idle_stable {{"vs": "<a snapshot_frame taken after the change>"}}
   - "bounded work / no relayout storm"  -> assert_work_bounded {{"max_relayouts": 4, "max_dom_regens": 3}}
   - "does not trigger a relayout"       -> assert_work_bounded {{"max_relayouts": 0}}
   - "no leak / counters return"         -> assert_resource_counts {{"vs": "baseline", "images": "eq", "fonts": "le"}}
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
    let v: serde_json::Value =
        serde_json::from_str(json).context("output is not valid JSON")?;
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
        if op.starts_with("assert_") {
            asserted = true;
        }
        match op {
            "snapshot_frame" | "snapshot_resources" => {
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
                             snapshot_frame/snapshot_resources created"
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
    }

    if !asserted {
        bail!("the test contains no assert_* step");
    }
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
    for a in artifacts {
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
    let expected: BTreeMap<&str, &Path> =
        all.iter().map(|w| (w.hash.as_str(), w.out.as_path())).collect();
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
    println!("[gen-e2e] schema: {} ops + {} assertions + {} step-loop ops (parsed from {})",
        schema.ops.len(), schema.asserts.len(), schema.extra.len(), FULL_RS);
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
            println!("[stale] {} — no corpus line claims this (use --prune)", o.display());
        }
    }

    if opts.dry_run {
        let mut by_tag: BTreeMap<&str, usize> = BTreeMap::new();
        for w in &p.todo {
            *by_tag.entry(w.tag.as_str()).or_default() += 1;
            if p.todo.len() <= 50 {
                println!("[dry] {:05} {} [{}] -> {}", w.index, w.hash, w.tag, w.out.display());
            }
        }
        if p.todo.len() > 50 {
            for (tag, n) in &by_tag {
                println!("[dry] {n:6} x [{tag}]");
            }
            println!("[dry] first: {:05} -> {}", p.todo[0].index, p.todo[0].out.display());
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
        return Ok(());
    }
    if p.todo.is_empty() {
        println!("[gen-e2e] nothing left to do — every line already generated and valid.");
        return Ok(());
    }
    let work = p.todo;

    which_claude()?;

    let example = fs::read_to_string(project_root.join(EXAMPLE_JSON))
        .with_context(|| format!("gen-e2e: cannot read {EXAMPLE_JSON}"))?;
    let schema_txt = schema_doc(&schema);

    let done_out = Mutex::new(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&done_file)?,
    );
    let ok = AtomicUsize::new(0);
    let fail = AtomicUsize::new(0);

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(opts.jobs)
        .build()?;
    pool.install(|| {
        use rayon::prelude::*;
        work.par_iter().for_each(|w| {
            match generate_one(&schema, &schema_txt, &example, opts, w) {
                Ok(()) => {
                    ok.fetch_add(1, Ordering::Relaxed);
                    // ONLY now is the line done: the artifact landed and validated.
                    // The key is the CONTENT HASH, not the line number, so the
                    // list survives the corpus being regenerated.
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
                }
                Err(e) => {
                    fail.fetch_add(1, Ordering::Relaxed);
                    let _ = fs::remove_file(&w.out); // never leave an invalid artifact
                    println!("[fail] {:05} [{}] — {e:#}  (not marked done)", w.index, w.tag);
                }
            }
        });
    });

    let (ok, fail) = (ok.load(Ordering::Relaxed), fail.load(Ordering::Relaxed));
    println!("\n[gen-e2e] {ok} generated, {fail} failed -> {}", out_dir.display());
    if fail > 0 {
        println!("[gen-e2e] re-run the same command to retry the failures (resume is automatic).");
    }
    Ok(())
}

fn generate_one(
    schema: &Schema,
    schema_txt: &str,
    example: &str,
    opts: &GenE2eOptions,
    w: &Work,
) -> Result<()> {
    let prompt = build_prompt(schema_txt, example, &w.line);

    let out = Command::new("claude")
        .arg("-p")
        .arg(&prompt)
        .arg("--model")
        .arg(&opts.model)
        .arg("--effort")
        .arg(&opts.effort)
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
    let raw = String::from_utf8_lossy(&out.stdout);

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

    validate(schema, json)?;

    // Write the agent's JSON VERBATIM (only `_source_hash`/`_source` spliced in
    // front): serde_json's Map is a BTreeMap here, so a re-emit would sort the
    // keys ("css" before "op") and wreck the readability that the
    // array-of-lines format exists for. The stamp is what makes the out-dir a
    // self-contained resume record — the done-list is only a cache.
    let stamped = stamp(json, w);
    debug_assert!(validate(schema, &stamped).is_ok());
    fs::write(&w.out, stamped).with_context(|| format!("cannot write {}", w.out.display()))?;
    Ok(())
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
        assert_eq!(mount.params, vec![("html".into(), true), ("css".into(), false)]);
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
            "click", "click_node", "double_click", "mouse_down", "mouse_move", "mouse_up",
            "key_down", "key_up", "text_input", "scroll", "touch_start", "touch_move", "touch_end",
            "touch_cancel", "pen_down", "pen_move", "pen_up", "pinch", "rotate", "swipe",
            "long_press", "move", "resize", "dpi_changed", "hit_test", "focus", "blur",
            "set_node_text", "set_node_css_override", "set_node_classes", "insert_node",
            "delete_node", "set_app_state", "scroll_node_to", "scroll_node_by", "scroll_into_view",
            "commit_undo_snapshot", "undo_app_state", "redo_app_state", "mount", "unmount",
            "tick_ms", "wait", "wait_frame", "reset_frame_counters", "snapshot_frame",
            "snapshot_resources", "get_frame_report", "capture_damage_png", "get_state",
            "get_app_state", "get_dom", "get_focus_state", "get_scroll_states",
            "get_selection_state", "get_cursor_state", "assert_changed", "assert_idle_stable",
        ] {
            assert_eq!(classify(op), OpClass::Allowed, "`{op}` must be allowed");
        }
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
            assert!(!s.is_zombie(op), "`{op}` has a match arm — must not be a zombie");
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
    SnapshotFrame {{ name: String }},
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
        assert!(validate(&s, t).unwrap_err().to_string().contains("no match arm"));

        // 2. Somebody implements it in full.rs. NOTHING in gene2e.rs changes.
        let root = synthetic_root(&dir, "        DebugEvent::Focus => { do_focus(); }\n");
        let s = parse_schema(&root).unwrap();
        assert!(!s.is_zombie("focus"), "an implemented op must stop being a zombie");
        assert!(schema_doc(&s).contains("- focus :"), "and be offered again");
        validate(&s, t).expect("and be accepted by the gate again");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_prompt_shows_allowed_ops_only() {
        let s = parse_schema(&root()).unwrap();
        let doc = schema_doc(&s);
        for good in ["- click :", "- set_node_text :", "- assert_changed :", "- undo_app_state :"] {
            assert!(doc.contains(good), "prompt is missing `{good}`");
        }
        for bad in [
            "- redraw", "- relayout", "- create_component", "- export_code", "- get_node_layout",
            "- get_display_list", "- assert_layout", "- assert_screenshot", "- close", "- open_file",
            // zombies (allowed by policy, but they do nothing — see
            // `no_zombie_is_reachable`)
            "- focus :", "- blur :", "- move :", "- dpi_changed :", "- get_dom :",
        ] {
            assert!(!doc.contains(bad), "prompt must not offer `{bad}`");
        }
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
            path: PathBuf::from(format!("/out/{hash}.json")),
            hash: Some(hash.to_string()),
            valid,
        }
    }

    fn hashes(w: &[Work]) -> BTreeSet<String> {
        w.iter().map(|x| x.hash.clone()).collect()
    }

    #[test]
    fn hash_is_content_addressed_and_line_number_independent() {
        assert_eq!(line_hash("[a/one] first test"), line_hash("  [a/one] first test  "));
        assert_ne!(line_hash("[a/one] first test"), line_hash("[a/one] second test"));

        // The SAME line, moved down by an insertion, keeps its hash — only the
        // cosmetic index/filename move.
        let before = parse_corpus(CORPUS, Path::new("/out"));
        let after = parse_corpus(&format!("[z/new] inserted at the top\n{CORPUS}"), Path::new("/out"));
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
        let p = plan(w.clone(), &hashes(&w), &[], &BTreeSet::new(), false, Some(2));
        assert_eq!(p.todo.len(), 2);
        assert_eq!(p.todo_total, 3);
        assert_eq!(p.todo[0].index, 1);
        // now those 2 landed: --limit 2 again picks up the REMAINING one
        let arts: Vec<Artifact> = p.todo.iter().map(|x| art(&x.hash, true)).collect();
        let p = plan(w.clone(), &hashes(&w), &arts, &BTreeSet::new(), false, Some(2));
        assert_eq!(p.already_done, 2);
        assert_eq!(p.todo.len(), 1);
        assert_eq!(p.todo[0].index, 3);
    }

    #[test]
    fn limit_composes_with_filter_and_filter_does_not_create_orphans() {
        let all = parse_corpus(CORPUS, Path::new("/out"));
        let corpus_hashes = hashes(&all);
        let filtered: Vec<Work> = all.iter().filter(|w| w.tag.contains("a/")).cloned().collect();
        assert_eq!(filtered.len(), 2);
        // the [b/three] artifact exists but is filtered out of the work list —
        // it must NOT be reported as an orphan.
        let arts = [art(&all[2].hash, true)];
        let p = plan(filtered, &corpus_hashes, &arts, &BTreeSet::new(), false, Some(1));
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
        let p = plan(drifted.clone(), &hashes(&drifted), &arts, &BTreeSet::new(), false, None);
        assert_eq!(p.total, 3);
        assert_eq!(p.already_done, 2, "the two moved-but-unchanged lines stay done");
        assert_eq!(p.todo.len(), 1);
        assert_eq!(p.todo[0].tag, "z/new");
        assert_eq!(p.orphans, vec![PathBuf::from(format!("/out/{}.json", w[2].hash))]);
    }

    #[test]
    fn an_unidentified_file_is_an_orphan() {
        let w = parse_corpus(CORPUS, Path::new("/out"));
        let stray = Artifact {
            path: PathBuf::from("/out/handwritten.json"),
            hash: None,
            valid: true,
        };
        let p = plan(w.clone(), &hashes(&w), &[stray], &BTreeSet::new(), false, None);
        assert_eq!(p.todo.len(), 3);
        assert_eq!(p.orphans, vec![PathBuf::from("/out/handwritten.json")]);
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
}
