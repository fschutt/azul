//! `azul-doc autotest` — adversarial test-generation harness.
//!
//! This is the test-generation analogue of `autofix`. Instead of syncing api.json, it:
//!
//!   1. enumerates every testable function per source file (free `fn`s + impl methods),
//!   2. categorizes each (parser / serializer / round-trip / constructor / predicate /
//!      getter / numeric / other) and attaches tailored adversarial test *strategies*,
//!   3. emits a machine-readable `manifest.json` plus one human-readable task file per
//!      source file under `tasks/`,
//!
//! so a fleet of LLM coding agents can then write the actual `#[cfg(test)]` bodies
//! file-by-file, run `cargo test -p <crate>`, and keep only the tests that pass.
//!
//! It writes ONLY under the `--out` directory (default `target/autotest/`). It never
//! touches source files, api.json, or generated code.

mod categorize;
mod extract;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Serialize;

use self::{
    categorize::{categorize_file, CategorizedFn, Category},
    extract::extract_functions_from_file,
};

/// The pure-logic workspace crates scanned by default. (crate-name, src-relative-dir)
const DEFAULT_CRATES: &[(&str, &str)] = &[
    ("azul-core", "core/src"),
    ("azul-css", "css/src"),
    ("azul-layout", "layout/src"),
];

/// Parsed `autotest` invocation options.
#[derive(Debug, Default)]
pub struct AutotestOptions {
    /// Narrow to a single crate (e.g. `azul-css`).
    pub crate_filter: Option<String>,
    /// Scan exactly one file (path relative to project root, e.g. `core/src/json.rs`).
    pub single_file: Option<String>,
    /// Output directory (default `target/autotest`).
    pub out_dir: Option<PathBuf>,
}

impl AutotestOptions {
    /// Parse the argv tail after `autotest` into options.
    /// Supported flags: `--crate <name>`, `--file <relpath>`, `--out <dir>`.
    pub fn parse(args: &[&str]) -> Result<Self> {
        let mut opts = AutotestOptions::default();
        let mut i = 0;
        while i < args.len() {
            match args[i] {
                "--crate" => {
                    let v = args
                        .get(i + 1)
                        .ok_or_else(|| anyhow::anyhow!("--crate requires a value"))?;
                    opts.crate_filter = Some(v.to_string());
                    i += 2;
                }
                "--file" => {
                    let v = args
                        .get(i + 1)
                        .ok_or_else(|| anyhow::anyhow!("--file requires a value"))?;
                    opts.single_file = Some(v.to_string());
                    i += 2;
                }
                "--out" => {
                    let v = args
                        .get(i + 1)
                        .ok_or_else(|| anyhow::anyhow!("--out requires a value"))?;
                    opts.out_dir = Some(PathBuf::from(v));
                    i += 2;
                }
                other => {
                    anyhow::bail!(
                        "Unknown autotest argument: '{}'. Usage: autotest [--crate <name>] \
                         [--file <relpath>] [--out <dir>]",
                        other
                    );
                }
            }
        }
        Ok(opts)
    }
}

// // manifest schema (serialized to manifest.json)

/// One function entry in the manifest.
#[derive(Debug, Serialize)]
struct ManifestFn {
    name: String,
    signature: String,
    category: Category,
    #[serde(skip_serializing_if = "Option::is_none")]
    self_type: Option<String>,
    is_pub: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    cfg: Option<String>,
    strategies: Vec<categorize::Strategy>,
}

/// One file entry in the manifest.
#[derive(Debug, Serialize)]
struct ManifestFile {
    #[serde(rename = "crate")]
    crate_name: String,
    file: String,
    functions: Vec<ManifestFn>,
}

/// A scanned file plus its categorized functions, kept together for both manifest and
/// task-file emission.
struct ScannedFile {
    crate_name: String,
    /// Path relative to the project root, with forward slashes (e.g. `core/src/json.rs`).
    rel_path: String,
    funcs: Vec<CategorizedFn>,
}

/// Run the `autotest` command end-to-end.
pub fn run(project_root: &Path, opts: &AutotestOptions) -> Result<()> {
    let out_dir = opts
        .out_dir
        .clone()
        .unwrap_or_else(|| project_root.join("target").join("autotest"));
    let tasks_dir = out_dir.join("tasks");

    println!("[AUTOTEST] Adversarial test-generation harness\n");

    // 1. Collect the set of files to scan.
    let files = collect_target_files(project_root, opts)?;
    if files.is_empty() {
        anyhow::bail!(
            "No source files matched the requested scope (crate={:?}, file={:?}).",
            opts.crate_filter,
            opts.single_file
        );
    }

    // 2. Parse + categorize each file (skip files that yield zero testable functions).
    let mut scanned: Vec<ScannedFile> = Vec::new();
    let mut parse_failures: Vec<String> = Vec::new();
    for (crate_name, abs_path) in &files {
        match extract_functions_from_file(abs_path) {
            Ok(funcs) => {
                if funcs.is_empty() {
                    continue;
                }
                let categorized = categorize_file(funcs);
                if categorized.is_empty() {
                    continue;
                }
                scanned.push(ScannedFile {
                    crate_name: crate_name.clone(),
                    rel_path: rel_path_string(project_root, abs_path),
                    funcs: categorized,
                });
            }
            Err(e) => parse_failures.push(e),
        }
    }

    scanned.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

    // 3. Write outputs.
    std::fs::create_dir_all(&tasks_dir)
        .with_context(|| format!("Failed to create {}", tasks_dir.display()))?;

    write_manifest(&out_dir, &scanned)?;
    let task_files_written = write_task_files(&tasks_dir, &scanned)?;

    // 4. Summary.
    print_summary(&scanned, files.len(), task_files_written, &parse_failures);
    println!("\n[OK] manifest: {}", out_dir.join("manifest.json").display());
    println!("[OK] tasks:    {}", tasks_dir.display());

    Ok(())
}

/// Collect the (crate-name, absolute-path) pairs to scan, honoring `--file` / `--crate`.
fn collect_target_files(
    project_root: &Path,
    opts: &AutotestOptions,
) -> Result<Vec<(String, PathBuf)>> {
    // Single-file mode: resolve the relative path and infer its crate.
    if let Some(rel) = &opts.single_file {
        let abs = resolve_rel(project_root, rel);
        if !abs.exists() {
            anyhow::bail!("--file path not found: {} ({})", rel, abs.display());
        }
        let crate_name = infer_crate_for_path(&abs).unwrap_or_else(|| "unknown".to_string());
        return Ok(vec![(crate_name, abs)]);
    }

    // Crate / default mode: walk the src dirs of the in-scope crates.
    let mut out = Vec::new();
    for (crate_name, src_rel) in DEFAULT_CRATES {
        if let Some(filter) = &opts.crate_filter {
            if filter != crate_name {
                continue;
            }
        }
        let src_dir = project_root.join(src_rel);
        if src_dir.exists() {
            collect_rust_files(&mut out, crate_name, &src_dir);
        }
    }

    if let Some(filter) = &opts.crate_filter {
        if out.is_empty() {
            anyhow::bail!(
                "Unknown or empty crate '{}'. Known crates: {}",
                filter,
                DEFAULT_CRATES
                    .iter()
                    .map(|(c, _)| *c)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }

    Ok(out)
}

/// Recursively collect `.rs` files under `dir`, skipping tests/examples/benches/build.rs
/// (via [`should_exclude_path`]) and obvious generated/codegen output.
fn collect_rust_files(files: &mut Vec<(String, PathBuf)>, crate_name: &str, dir: &Path) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if should_exclude_path(&path) {
            continue;
        }
        if path.is_dir() {
            collect_rust_files(files, crate_name, &path);
        } else if path.extension().map_or(false, |e| e == "rs") {
            files.push((crate_name.to_string(), path));
        }
    }
}

/// Paths to exclude from scanning: tests, examples, benches, build scripts, and
/// generated / codegen output. (Mirrors `autofix::module_map::should_exclude_path`
/// plus a `codegen` directory exclusion — kept local since autofix is read-only here.)
fn should_exclude_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    if s.contains("/tests/") || s.contains("/test/") {
        return true;
    }
    if s.contains("/examples/") || s.contains("/example/") {
        return true;
    }
    if s.contains("/benches/") || s.contains("/bench/") {
        return true;
    }
    // Skip generated / codegen output directories.
    if s.contains("/codegen/") || s.contains("/generated/") {
        return true;
    }
    if s.ends_with("build.rs") {
        return true;
    }
    false
}

/// Infer the crate name for an absolute path by matching against the known src dirs.
fn infer_crate_for_path(abs: &Path) -> Option<String> {
    let s = abs.to_string_lossy().replace('\\', "/");
    for (crate_name, src_rel) in DEFAULT_CRATES {
        // src_rel is like "core/src"; match "/core/src/" or a leading "core/src/".
        let needle = format!("/{}/", src_rel);
        let lead = format!("{}/", src_rel);
        if s.contains(&needle) || s.starts_with(&lead) {
            return Some(crate_name.to_string());
        }
    }
    None
}

/// Resolve a user-supplied relative path against the project root, falling back to the
/// path as-is if it is absolute or already exists.
fn resolve_rel(project_root: &Path, rel: &str) -> PathBuf {
    let p = PathBuf::from(rel);
    if p.is_absolute() {
        return p;
    }
    let joined = project_root.join(&p);
    if joined.exists() {
        joined
    } else if p.exists() {
        p
    } else {
        joined
    }
}

/// Render an absolute path as a project-root-relative, forward-slashed string.
fn rel_path_string(project_root: &Path, abs: &Path) -> String {
    let rel = abs.strip_prefix(project_root).unwrap_or(abs);
    rel.to_string_lossy().replace('\\', "/")
}

/// Write `manifest.json` (an array of file entries).
fn write_manifest(out_dir: &Path, scanned: &[ScannedFile]) -> Result<()> {
    let manifest: Vec<ManifestFile> = scanned
        .iter()
        .map(|sf| ManifestFile {
            crate_name: sf.crate_name.clone(),
            file: sf.rel_path.clone(),
            functions: sf
                .funcs
                .iter()
                .map(|cf| ManifestFn {
                    name: cf.func.name.clone(),
                    signature: cf.func.signature.clone(),
                    category: cf.category,
                    self_type: cf.func.self_type.clone(),
                    is_pub: cf.func.is_pub,
                    cfg: cf.func.cfg.clone(),
                    strategies: cf.strategies.clone(),
                })
                .collect(),
        })
        .collect();

    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("Failed to create {}", out_dir.display()))?;
    let json = serde_json::to_string_pretty(&manifest)?;
    let manifest_path = out_dir.join("manifest.json");
    std::fs::write(&manifest_path, json)
        .with_context(|| format!("Failed to write {}", manifest_path.display()))?;
    Ok(())
}

/// Write one task file per scanned file. Returns the number of task files written.
fn write_task_files(tasks_dir: &Path, scanned: &[ScannedFile]) -> Result<usize> {
    let mut count = 0;
    for sf in scanned {
        let task_name = task_file_name(&sf.crate_name, &sf.rel_path);
        let body = render_task_file(sf);
        let path = tasks_dir.join(&task_name);
        std::fs::write(&path, body)
            .with_context(|| format!("Failed to write {}", path.display()))?;
        count += 1;
    }
    Ok(count)
}

/// Build the task filename: `<crate>__<path-with-slashes-as-__>.md`.
fn task_file_name(crate_name: &str, rel_path: &str) -> String {
    let path_part = rel_path.replace('/', "__").replace(".rs", "");
    format!("{}__{}.md", crate_name, path_part)
}

/// Render the Markdown task file for a single scanned source file.
fn render_task_file(sf: &ScannedFile) -> String {
    let mut s = String::new();

    // Are there any private (non-pub) functions? If so, an inline test module is
    // required (it can reach private fns). If everything is pub, a `tests/` file works
    // too — we surface that choice to the agent.
    let any_private = sf.funcs.iter().any(|cf| !cf.func.is_pub);
    let all_pub = !any_private;

    s.push_str(&format!("# Autotest task: `{}`\n\n", sf.rel_path));
    s.push_str(&format!("Crate: `{}`\n\n", sf.crate_name));
    s.push_str(&format!(
        "{} testable function(s) found in this file.\n\n",
        sf.funcs.len()
    ));

    // Category histogram for this file.
    let mut hist: BTreeMap<&'static str, usize> = BTreeMap::new();
    for cf in &sf.funcs {
        *hist.entry(cf.category.as_str()).or_default() += 1;
    }
    s.push_str("Categories in this file: ");
    s.push_str(
        &hist
            .iter()
            .map(|(c, n)| format!("{}={}", c, n))
            .collect::<Vec<_>>()
            .join(", "),
    );
    s.push_str("\n\n");

    // // instructions
    s.push_str("## Your task\n\n");
    s.push_str(
        "Write adversarial unit tests for the functions listed below. For each function, \
         turn the suggested adversarial cases into concrete `#[test]` assertions. Aim to \
         provoke panics, overflow, infinite loops, and incorrect results — then assert the \
         function behaves safely (returns `Err`/`None`, saturates, or produces the documented \
         value) instead.\n\n",
    );

    if any_private {
        s.push_str(&format!(
            "Some functions here are **private**, so you MUST add an inline test module to \
             `{}` itself (an inline module can test private functions for better coverage):\n\n",
            sf.rel_path
        ));
        s.push_str("```rust\n");
        s.push_str("#[cfg(test)]\n");
        s.push_str("mod autotest_generated {\n");
        s.push_str("    use super::*;\n\n");
        s.push_str("    #[test]\n");
        s.push_str("    fn parse_empty_input_does_not_panic() {\n");
        s.push_str("        // ... your assertion ...\n");
        s.push_str("    }\n");
        s.push_str("}\n");
        s.push_str("```\n\n");
    } else {
        s.push_str(&format!(
            "All functions here are **public**, so you may EITHER add an inline \
             `#[cfg(test)] mod autotest_generated {{ use super::*; ... }}` to `{}`, OR create a \
             new file under that crate's `tests/` directory that imports the crate. Prefer the \
             inline module unless the crate convention says otherwise.\n\n",
            sf.rel_path
        ));
        s.push_str("```rust\n");
        s.push_str("#[cfg(test)]\n");
        s.push_str("mod autotest_generated {\n");
        s.push_str("    use super::*;\n");
        s.push_str("    // ... your #[test] fns ...\n");
        s.push_str("}\n");
        s.push_str("```\n\n");
    }

    s.push_str(&format!(
        "After writing the tests, run:\n\n```sh\ncargo test -p {} 2>&1 | tail -40\n```\n\n",
        sf.crate_name
    ));
    s.push_str(
        "Keep ONLY the tests that compile and pass. If an assertion reveals a genuine bug \
         (a real panic / wrong result), note it in your report rather than weakening the test \
         to make it pass. Do not modify the functions under test.\n\n",
    );

    s.push_str("---\n\n");
    s.push_str("## Functions\n\n");

    for (idx, cf) in sf.funcs.iter().enumerate() {
        let f = &cf.func;
        let heading = f.qualified_name();
        s.push_str(&format!(
            "### {}. `{}`  _(category: {})_\n\n",
            idx + 1,
            heading,
            cf.category.as_str()
        ));

        s.push_str(&format!("- Signature: `{}`\n", f.signature));
        s.push_str(&format!(
            "- Visibility: {}\n",
            if f.is_pub { "pub" } else { "private" }
        ));
        if let Some(st) = &f.self_type {
            let recv = f
                .self_kind
                .map(|sk| sk.as_str())
                .unwrap_or("static / no self");
            s.push_str(&format!("- Method of `{}` (receiver: {})\n", st, recv));
        }
        if !f.generics.is_empty() {
            s.push_str(&format!("- Generics: `{}`\n", f.generics.join(", ")));
        }
        if let Some(cfg) = &f.cfg {
            s.push_str(&format!(
                "- Gated behind: `#[cfg({})]` — test must respect this gate\n",
                cfg
            ));
        }
        if !f.args.is_empty() {
            let arglist = f
                .args
                .iter()
                .map(|a| format!("`{}: {}`", a.name, a.ty))
                .collect::<Vec<_>>()
                .join(", ");
            s.push_str(&format!("- Args: {}\n", arglist));
        }
        if let Some(ret) = &f.return_type {
            s.push_str(&format!("- Returns: `{}`\n", ret));
        }
        if !f.doc.is_empty() {
            let doc_joined = f.doc.join(" ").trim().to_string();
            if !doc_joined.is_empty() {
                // Keep it to a short blurb.
                let blurb: String = doc_joined.chars().take(400).collect();
                s.push_str(&format!("- Doc: {}\n", blurb));
            }
        }

        s.push_str("\nSuggested adversarial cases:\n\n");
        for strat in &cf.strategies {
            s.push_str(&format!("- **{}**: {}\n", strat.label, strat.case));
        }
        s.push('\n');
    }

    s
}

/// Print the run summary: files scanned, functions found (by category), task files written.
fn print_summary(
    scanned: &[ScannedFile],
    files_considered: usize,
    task_files_written: usize,
    parse_failures: &[String],
) {
    let mut total_fns = 0usize;
    let mut by_cat: BTreeMap<&'static str, usize> = BTreeMap::new();
    for sf in scanned {
        for cf in &sf.funcs {
            total_fns += 1;
            *by_cat.entry(cf.category.as_str()).or_default() += 1;
        }
    }

    println!("\n=== autotest summary ===");
    println!(
        "Files considered: {} | files with testable fns: {}",
        files_considered,
        scanned.len()
    );
    println!("Functions found: {}", total_fns);
    println!("  by category:");
    // Stable, readable category order.
    for cat in [
        "parser",
        "serializer",
        "round_trip",
        "constructor",
        "predicate",
        "getter",
        "numeric",
        "other",
    ] {
        if let Some(n) = by_cat.get(cat) {
            println!("    {:<12} {}", cat, n);
        }
    }
    println!("Task files written: {}", task_files_written);

    if !parse_failures.is_empty() {
        println!(
            "\n[WARN] {} file(s) failed to parse and were skipped:",
            parse_failures.len()
        );
        for (i, e) in parse_failures.iter().enumerate() {
            if i >= 10 {
                println!("    ... and {} more", parse_failures.len() - 10);
                break;
            }
            println!("    - {}", e);
        }
    }

    // A single, copy-pasteable summary line (handy for piping / logs).
    let cat_line = [
        "parser",
        "serializer",
        "round_trip",
        "constructor",
        "predicate",
        "getter",
        "numeric",
        "other",
    ]
    .iter()
    .filter_map(|c| by_cat.get(*c).map(|n| format!("{}={}", c, n)))
    .collect::<Vec<_>>()
    .join(" ");
    println!(
        "\nSUMMARY: {} files scanned, {} functions found ({}), {} task files written",
        scanned.len(),
        total_fns,
        cat_line,
        task_files_written
    );
}
