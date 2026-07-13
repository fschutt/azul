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
                 E] [--limit N] [--filter <tag>] [--dry-run] [--redo]"
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

/// The schema section of the agent prompt — rendered from the parsed `full.rs`.
fn schema_doc(schema: &Schema) -> String {
    let mut s = String::new();
    s.push_str("### TIMELINE OPS (`{\"op\": \"<name>\", …}`)\n");
    for op in &schema.ops {
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
    for e in &schema.extra {
        s.push_str(&format!("- {e} : (no params)\n"));
    }
    s.push_str("\n### ASSERTIONS\n");
    for a in &schema.asserts {
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

## HOW TO TURN THE ONE-LINER INTO A TIMELINE
1. `mount` the DOM the line describes (invent plausible, minimal HTML+CSS for it).
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
        // SCOPE: layout-correctness is reftest's job, not e2e's.
        if op == "assert_layout" || op == "assert_screenshot" {
            bail!("step {i}: `{op}` is out of scope for a generated behaviour test");
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

struct Work {
    /// 1-based line number in the corpus — the stable id used by the done-list.
    index: usize,
    tag: String,
    line: String,
    out: PathBuf,
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

pub fn run(project_root: &Path, opts: &GenE2eOptions) -> Result<()> {
    let txt = resolve(project_root, &opts.txt);
    let out_dir = resolve(project_root, &opts.out_dir);

    let schema = parse_schema(project_root)?;
    let corpus = fs::read_to_string(&txt)
        .with_context(|| format!("gen-e2e: cannot read {}", txt.display()))?;

    // --- work list -------------------------------------------------------
    let mut work: Vec<Work> = Vec::new();
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
        if let Some(f) = &opts.filter {
            if !tag.contains(f.as_str()) {
                continue;
            }
        }
        let desc = line.split_once(']').map_or(line, |(_, d)| d.trim());
        let index = i + 1;
        let out = out_dir.join(format!("{:05}-{}.json", index, slug(&tag, desc)));
        work.push(Work {
            index,
            tag,
            line: line.to_string(),
            out,
        });
    }
    if work.is_empty() {
        bail!("gen-e2e: empty work list ({})", txt.display());
    }
    let total = work.len();

    // --- resume ----------------------------------------------------------
    fs::create_dir_all(&out_dir)?;
    let done_file = out_dir.join(".done-gen-e2e");
    let done: BTreeSet<usize> = if opts.redo {
        BTreeSet::new()
    } else {
        fs::read_to_string(&done_file)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.trim().parse::<usize>().ok())
            .collect()
    };
    if !opts.redo {
        // Two independent resume signals: the done-list AND the artifact itself,
        // so a lost done-list does not restart a 13k-line run from scratch.
        work.retain(|w| !done.contains(&w.index) && !w.out.exists());
    }
    let skipped = total - work.len();
    if let Some(limit) = opts.limit {
        work.truncate(limit);
    }

    println!(
        "[gen-e2e] corpus={} total={} already-done={} to-generate={} model={} effort={} jobs={}",
        txt.display(),
        total,
        skipped,
        work.len(),
        opts.model,
        opts.effort,
        opts.jobs
    );
    println!("[gen-e2e] schema: {} ops + {} assertions + {} step-loop ops (parsed from {})",
        schema.ops.len(), schema.asserts.len(), schema.extra.len(), FULL_RS);

    if opts.dry_run {
        let mut by_tag: BTreeMap<&str, usize> = BTreeMap::new();
        for w in &work {
            *by_tag.entry(w.tag.as_str()).or_default() += 1;
            if work.len() <= 50 {
                println!("[dry] {:05} [{}] -> {}", w.index, w.tag, w.out.display());
            }
        }
        if work.len() > 50 {
            for (tag, n) in &by_tag {
                println!("[dry] {n:6} x [{tag}]");
            }
            println!("[dry] first: {:05} -> {}", work[0].index, work[0].out.display());
            let last = &work[work.len() - 1];
            println!("[dry] last:  {:05} -> {}", last.index, last.out.display());
        }
        println!("[dry-run] {} test(s) would be generated. Nothing launched.", work.len());
        return Ok(());
    }
    if work.is_empty() {
        println!("[gen-e2e] nothing left to do — every line already generated.");
        return Ok(());
    }

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
                    if let Ok(mut f) = done_out.lock() {
                        let _ = writeln!(f, "{}", w.index);
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

    // Write the agent's JSON VERBATIM: serde_json's Map is a BTreeMap here, so a
    // re-emit would sort the keys ("css" before "op") and wreck the readability
    // that the array-of-lines format exists for.
    fs::write(&w.out, format!("{}\n", json.trim()))
        .with_context(|| format!("cannot write {}", w.out.display()))?;
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
}
