//! `doc-coverage` — API documentation-completeness harness.
//!
//! The sibling of `autotest`, but for documentation: it walks api.json for
//! public-facing items whose `doc` is empty (the class itself, its struct
//! fields, enum variants, constructors, and methods), maps each gap to its
//! Rust source file (via the class `external` path), and splits the work into
//! N balanced per-agent task files under `target/doc-coverage/`. A fleet of
//! agents then adds `///` doc comments to the SOURCE (no code changes), after
//! which `azul-doc autofix` syncs those comments into api.json.
//!
//! Usage: `azul-doc doc-coverage [--agents <N>] [--out <dir>]`  (default N=20)

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;

use crate::api::ApiData;

pub struct DocCoverageOptions {
    pub agents: usize,
    pub out: Option<String>,
}

impl DocCoverageOptions {
    pub fn parse(args: &[&str]) -> Result<Self> {
        let mut agents = 20;
        let mut out = None;
        let mut i = 0;
        while i < args.len() {
            match args[i] {
                "--agents" => {
                    i += 1;
                    agents = args
                        .get(i)
                        .and_then(|s| s.parse().ok())
                        .ok_or_else(|| anyhow::anyhow!("--agents needs a number"))?;
                }
                "--out" => {
                    i += 1;
                    out = args.get(i).map(|s| s.to_string());
                }
                other => anyhow::bail!("unknown doc-coverage arg: {other}"),
            }
            i += 1;
        }
        if agents == 0 {
            anyhow::bail!("--agents must be >= 1");
        }
        Ok(Self { agents, out })
    }
}

/// One undocumented item, e.g. `struct Dom`, `field Dom.root`,
/// `variant NodeType::Div`, `fn Dom::new`.
struct Undoc {
    kind: &'static str, // struct | enum | field | variant | constructor | method
    label: String,
}

fn is_empty_doc(doc: &Option<Vec<String>>) -> bool {
    match doc {
        None => true,
        Some(v) => v.iter().all(|s| s.trim().is_empty()),
    }
}

/// Map a class `external` path (`azul_core::dom::Dom`) to its source file
/// (`core/src/dom.rs` or `core/src/dom/mod.rs`).
fn external_to_source(external: &str, root: &Path) -> Option<String> {
    let parts: Vec<&str> = external.split("::").collect();
    if parts.len() < 2 {
        return None;
    }
    let crate_dir = match parts[0] {
        "azul_core" | "azul_impl" => "core",
        "azul_css" => "css",
        "azul_layout" => "layout",
        "azul_dll" => "dll",
        _ => return None,
    };
    let mods = &parts[1..parts.len() - 1]; // drop crate + leaf type name
    if mods.is_empty() {
        return None;
    }
    let base = format!("{}/src/{}", crate_dir, mods.join("/"));
    let flat = format!("{base}.rs");
    let modrs = format!("{base}/mod.rs");
    if root.join(&flat).exists() {
        Some(flat)
    } else if root.join(&modrs).exists() {
        Some(modrs)
    } else {
        Some(flat) // best guess; the agent resolves it
    }
}

pub fn run(root: &Path, api_data: &ApiData, opts: &DocCoverageOptions) -> Result<()> {
    // source file -> list of undocumented items in it
    let mut per_file: BTreeMap<String, Vec<Undoc>> = BTreeMap::new();
    let mut skipped_no_source = 0usize;

    for (_ver, vd) in &api_data.0 {
        for (_mod_name, module) in &vd.api {
            for (class_name, class) in &module.classes {
                let Some(ext) = class.external.as_deref() else { continue };
                let Some(file) = external_to_source(ext, root) else {
                    skipped_no_source += 1;
                    continue;
                };
                let bucket = per_file.entry(file).or_default();
                if is_empty_doc(&class.doc) {
                    bucket.push(Undoc { kind: "type", label: class_name.clone() });
                }
                for fmap in class.struct_fields.iter().flatten() {
                    for (fname, fd) in fmap {
                        if is_empty_doc(&fd.doc) {
                            bucket.push(Undoc {
                                kind: "field",
                                label: format!("{class_name}.{fname}"),
                            });
                        }
                    }
                }
                for emap in class.enum_fields.iter().flatten() {
                    for (vname, vdata) in emap {
                        if is_empty_doc(&vdata.doc) {
                            bucket.push(Undoc {
                                kind: "variant",
                                label: format!("{class_name}::{vname}"),
                            });
                        }
                    }
                }
                for (cname, cd) in class.constructors.iter().flatten() {
                    if is_empty_doc(&cd.doc) {
                        bucket.push(Undoc {
                            kind: "constructor",
                            label: format!("{class_name}::{cname}"),
                        });
                    }
                }
                for (fname, fd) in class.functions.iter().flatten() {
                    if is_empty_doc(&fd.doc) {
                        bucket.push(Undoc {
                            kind: "method",
                            label: format!("{class_name}::{fname}"),
                        });
                    }
                }
            }
        }
    }
    per_file.retain(|_, v| !v.is_empty());

    let total: usize = per_file.values().map(|v| v.len()).sum();
    let nfiles = per_file.len();

    // Greedy balance: assign files (heaviest first) to the least-loaded agent.
    let mut files: Vec<(String, Vec<Undoc>)> = per_file.into_iter().collect();
    files.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    let n = opts.agents.min(files.len().max(1));
    let mut agents: Vec<Vec<(String, Vec<Undoc>)>> = (0..n).map(|_| Vec::new()).collect();
    let mut loads = vec![0usize; n];
    for (file, items) in files {
        let (mi, _) = loads.iter().enumerate().min_by_key(|(_, l)| **l).unwrap();
        loads[mi] += items.len();
        agents[mi].push((file, items));
    }

    let out_dir = match &opts.out {
        Some(o) => root.join(o),
        None => root.join("target").join("doc-coverage"),
    };
    let tasks_dir = out_dir.join("tasks");
    std::fs::create_dir_all(&tasks_dir)?;

    let mut manifest = Vec::new();
    for (ai, agent) in agents.iter().enumerate() {
        if agent.is_empty() {
            continue;
        }
        let count: usize = agent.iter().map(|(_, v)| v.len()).sum();
        let mut md = String::new();
        md.push_str(&format!(
            "# Documentation task — agent {:02} of {}\n\n",
            ai + 1,
            n
        ));
        md.push_str(&task_instructions());
        md.push_str(&format!(
            "\n## Items to document ({count} items across {} files)\n",
            agent.len()
        ));
        let mut files_json = Vec::new();
        for (file, items) in agent {
            md.push_str(&format!("\n### `{file}`\n"));
            let mut items_json = Vec::new();
            for u in items {
                md.push_str(&format!("- {} `{}`\n", u.kind, u.label));
                items_json.push(serde_json::json!({ "kind": u.kind, "label": u.label }));
            }
            files_json.push(serde_json::json!({ "file": file, "items": items_json }));
        }
        let task_path = tasks_dir.join(format!("agent-{:02}.md", ai + 1));
        std::fs::write(&task_path, md)?;
        manifest.push(serde_json::json!({
            "agent": ai + 1,
            "task_file": task_path.strip_prefix(root).unwrap_or(&task_path).to_string_lossy(),
            "item_count": count,
            "files": files_json,
        }));
    }

    std::fs::write(
        out_dir.join("manifest.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "total_undocumented": total,
            "files": nfiles,
            "agents": manifest.len(),
            "tasks": manifest,
        }))?,
    )?;

    println!(
        "doc-coverage: {total} undocumented items across {nfiles} source files \
         -> {} agent task files in {}",
        manifest.len(),
        tasks_dir.display()
    );
    if skipped_no_source > 0 {
        println!("  ({skipped_no_source} classes skipped — no resolvable source path)");
    }
    Ok(())
}

fn task_instructions() -> String {
    "\
azul-doc's `autofix` syncs Rust `///` doc comments from SOURCE into api.json.
Your job: add accurate `///` doc comments to the SOURCE definitions listed
below. **DOCS ONLY** — do not change code, signatures, types, derives, or
visibility (no recompile should be needed).

## Rules
- For each item add a concise, correct `///` doc comment on its source
  definition (the `struct`, struct field, `enum`, enum variant, or method/fn).
  The first line must be a one-sentence summary.
- Write ACCURATE docs: read the type and grep the workspace for usages
  (`rg <Name>`) to infer its real purpose — do not guess vaguely.
- If an item is a STUB / placeholder / unimplemented (empty body, `todo!()`,
  `unimplemented!()`, always returns default/None/empty, or a \"not
  implemented\" comment), PREFIX its doc with `WIP:` and say so, e.g.
  `/// WIP: stub — not yet implemented on this platform.`
- These are public, `#[repr(C)]` FFI types consumed by 30+ language bindings;
  keep docs precise and binding-relevant.
- Do NOT edit api.json (autofix pulls your source docs into it afterward).
- Some labels are `Az`-prefixed in api.json; the source type usually has no
  `Az` prefix — match by the un-prefixed name in the listed file.
"
    .to_string()
}
