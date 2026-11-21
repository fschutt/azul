//! Repository facts handed to each rewrite-pass agent, so it does not spend
//! turns searching for them: what the types and functions its file uses look
//! like, which items its file defines are mentioned nowhere else, and, given
//! coverage data, which of its functions and lines no test runs.
//!
//! Type facts (fields, variants, methods with signatures and docs) come from
//! the autofix type index of core, css, layout and dll, the same index
//! `azul-doc autofix` builds. Free functions and the "who mentions this name"
//! counts come from one pass of the style guard's lexer over every tracked
//! Rust file, plus the words of api.json, so a name the bindings export counts
//! as used. Coverage comes from an lcov file (`cargo llvm-cov --lcov`).

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs,
    path::Path,
    process::Command,
};

use rayon::prelude::*;

use super::style_guard::{self, Tok, TokKind};
use crate::autofix::type_index::{TypeDefKind, TypeDefinition, TypeIndex};

/// Upper bound for one file's context section; past it, lists are cut short.
const MAX_CONTEXT_BYTES: usize = 12_000;
/// A free function name with more definitions than this is too ambiguous to list.
const MAX_FN_DEFINITIONS: usize = 3;

/// Type names every file uses, whose index entries would only add noise.
const COMMON_TYPES: &[&str] = &[
    "Self", "String", "Vec", "Option", "Result", "Box", "Some", "None", "Ok", "Err", "Rc", "Arc",
    "Cow", "Default", "Clone", "Debug",
];

/// Method names that trait impls and std give to everything; a count of who
/// mentions them says nothing about one item.
const COMMON_FNS: &[&str] = &[
    "new", "default", "clone", "fmt", "eq", "ne", "hash", "cmp", "partial_cmp", "drop", "from",
    "into", "deref", "deref_mut", "as_ref", "as_mut", "next", "main", "len", "is_empty", "get",
];

/// Where a free function is defined.
struct FnDef {
    /// Index into `RepoContext::files`.
    file: u32,
    line: usize,
    /// `fn name(args) -> Ret`, whitespace collapsed, cut at 160 characters.
    signature: String,
}

/// The facts for the whole repository, built once per run.
pub struct RepoContext {
    index: TypeIndex,
    /// Repo-relative paths; position = file id. The last entry is `api.json`.
    files: Vec<String>,
    /// Identifier -> ids of the files whose code mentions it.
    mentions: HashMap<String, Vec<u32>>,
    /// Function name -> its definitions.
    fns: HashMap<String, Vec<FnDef>>,
    /// Coverage per source path as the lcov file names it, if one was given.
    coverage: Option<(String, BTreeMap<String, FileCoverage>)>,
}

/// What the tests leave unrun in one file, from an lcov record.
#[derive(Default)]
struct FileCoverage {
    /// Start lines of functions no test calls.
    unrun_fns: BTreeSet<usize>,
    /// Lines with code no test runs.
    unrun_lines: BTreeSet<usize>,
    /// Lines with a branch no test takes.
    untaken_branches: BTreeSet<usize>,
}

/// Reads an lcov file: `SF:` starts a file, `FN:`/`FNDA:` name functions and
/// their call counts, `DA:` counts a line, `BRDA:` counts a branch (`-` or
/// `0` means never taken).
fn read_lcov(text: &str) -> BTreeMap<String, FileCoverage> {
    let mut files = BTreeMap::new();
    let mut current: Option<(String, FileCoverage, HashMap<String, usize>)> = None;
    for line in text.lines() {
        let Some((tag, value)) = line.split_once(':') else {
            if line == "end_of_record" {
                if let Some((path, cov, _)) = current.take() {
                    files.insert(path, cov);
                }
            }
            continue;
        };
        if tag == "SF" {
            current = Some((value.to_string(), FileCoverage::default(), HashMap::new()));
            continue;
        }
        let Some((_, cov, fn_lines)) = current.as_mut() else {
            continue;
        };
        let fields: Vec<&str> = value.split(',').collect();
        match (tag, fields.as_slice()) {
            ("FN", [line, name, ..]) => {
                if let Ok(line) = line.parse() {
                    fn_lines.insert(name.to_string(), line);
                }
            }
            ("FNDA", ["0", name, ..]) => {
                if let Some(&line) = fn_lines.get(*name) {
                    cov.unrun_fns.insert(line);
                }
            }
            ("DA", [line, "0", ..]) => {
                if let Ok(line) = line.parse() {
                    cov.unrun_lines.insert(line);
                }
            }
            ("BRDA", [line, _, _, taken]) if *taken == "-" || *taken == "0" => {
                if let Ok(line) = line.parse() {
                    cov.untaken_branches.insert(line);
                }
            }
            _ => {}
        }
    }
    files
}

/// `1, 2, 3, 7` -> `1-3, 7`, at most `max` ranges.
fn line_ranges(lines: &BTreeSet<usize>, max: usize) -> String {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for &l in lines {
        match ranges.last_mut() {
            Some((_, end)) if *end + 1 == l => *end = l,
            _ => ranges.push((l, l)),
        }
    }
    let shown: Vec<String> = ranges
        .iter()
        .take(max)
        .map(|&(a, b)| if a == b { a.to_string() } else { format!("{a}-{b}") })
        .collect();
    let more = ranges.len().saturating_sub(max);
    if more > 0 {
        format!("{} (+{more} more)", shown.join(", "))
    } else {
        shown.join(", ")
    }
}

impl RepoContext {
    /// Builds the type index and lexes every tracked `.rs` file, and reads the
    /// coverage file if given. Takes a few seconds; the run does it once
    /// before writing the prompts.
    pub fn build(project_root: &Path, coverage: Option<&Path>) -> Result<RepoContext, String> {
        let coverage = match coverage {
            Some(path) => {
                let text = fs::read_to_string(path)
                    .map_err(|e| format!("cannot read the coverage file {}: {e}", path.display()))?;
                Some((path.display().to_string(), read_lcov(&text)))
            }
            None => None,
        };
        let index = TypeIndex::build(project_root, false).map_err(|e| format!("type index: {e}"))?;
        let out = Command::new("git")
            .args(["ls-files", "-z", "--", "*.rs"])
            .current_dir(project_root)
            .output()
            .map_err(|e| format!("git ls-files: {e}"))?;
        let mut files: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect();
        files.sort();

        let per_file: Vec<(Vec<String>, Vec<(String, usize, String)>)> = files
            .par_iter()
            .map(|rel| scan_file(&fs::read_to_string(project_root.join(rel)).unwrap_or_default()))
            .collect();

        let mut mentions: HashMap<String, Vec<u32>> = HashMap::new();
        let mut fns: HashMap<String, Vec<FnDef>> = HashMap::new();
        for (id, (idents, defs)) in per_file.into_iter().enumerate() {
            for ident in idents {
                mentions.entry(ident).or_default().push(id as u32);
            }
            for (name, line, signature) in defs {
                fns.entry(name).or_default().push(FnDef {
                    file: id as u32,
                    line,
                    signature,
                });
            }
        }
        let api_id = files.len() as u32;
        files.push("api.json".to_string());
        let api = fs::read_to_string(project_root.join("api.json")).unwrap_or_default();
        let api_words: BTreeSet<&str> = api
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|w| !w.is_empty())
            .collect();
        for word in api_words {
            mentions.entry(word.to_string()).or_default().push(api_id);
        }
        Ok(RepoContext {
            index,
            files,
            mentions,
            fns,
            coverage,
        })
    }

    /// The coverage section for one file, if coverage data names it. The lcov
    /// paths are absolute paths of the checkout that ran the tests, so a file
    /// is found by its repo-relative path at the end.
    fn coverage_section(&self, rel: &str, facts: &FileFacts) -> String {
        let Some((source, files)) = &self.coverage else {
            return String::new();
        };
        let suffix = format!("/{rel}");
        let Some(cov) = files
            .iter()
            .find(|(path, _)| path.as_str() == rel || path.ends_with(&suffix))
            .map(|(_, cov)| cov)
        else {
            return String::new();
        };
        let unrun_fns: Vec<String> = facts
            .defined
            .iter()
            .filter(|(_, line)| cov.unrun_fns.contains(line))
            .map(|(name, _)| format!("`{name}`"))
            .collect();
        let mut out = format!(
            "\n### Test coverage (from `{source}`; line numbers are those of the file when the \
             tests ran)\n\n"
        );
        if !unrun_fns.is_empty() {
            out.push_str(&format!("- Functions no test calls: {}.\n", unrun_fns.join(", ")));
        }
        if !cov.unrun_lines.is_empty() {
            out.push_str(&format!("- Lines no test runs: {}.\n", line_ranges(&cov.unrun_lines, 25)));
        }
        if !cov.untaken_branches.is_empty() {
            out.push_str(&format!(
                "- Branches no test takes, at lines: {}.\n",
                line_ranges(&cov.untaken_branches, 25)
            ));
        }
        if unrun_fns.is_empty() && cov.unrun_lines.is_empty() && cov.untaken_branches.is_empty() {
            out.push_str("- The tests run every line and branch of this file.\n");
        }
        out
    }

    /// The context section for one file's prompt (markdown, capped in size).
    pub fn for_file(&self, project_root: &Path, rel: &str, src: &str) -> String {
        let Ok(toks) = style_guard::lex(src) else {
            return String::new();
        };
        let code: Vec<&Tok> = toks
            .iter()
            .filter(|t| !matches!(t.kind, TokKind::Comment(_)))
            .collect();
        let facts = FileFacts::collect(&code);
        let self_id = self.files.iter().position(|f| f == rel).map(|p| p as u32);
        let api_id = (self.files.len() - 1) as u32;

        let mut out = String::from(
            "## Context from the repository index\n\n\
             Facts collected by the tool, so you need not search for them. Types come from the \
             autofix type index of core, css, layout and dll; \"mentioned\" means the name appears \
             in the code of another tracked file or in api.json.\n",
        );

        let types = self.used_types(project_root, rel, &facts);
        if !types.is_empty() {
            out.push_str("\n### Types this file uses\n\n");
            push_capped(&mut out, &types);
        }

        let calls = self.called_fns(self_id, &facts);
        if !calls.is_empty() {
            out.push_str("\n### Functions this file calls\n\n");
            push_capped(&mut out, &calls);
        }

        // Items defined here: which are mentioned nowhere else, and which api.json names.
        let mut unused = Vec::new();
        let mut exported = Vec::new();
        // Names only: rustfmt runs before the agent reads the file, so line
        // numbers taken here could be off.
        for (name, _) in &facts.defined {
            let ids = self.mentions.get(*name).map(Vec::as_slice).unwrap_or(&[]);
            if ids.contains(&api_id) {
                exported.push(format!("`{name}`"));
                continue;
            }
            let elsewhere = ids.iter().any(|&id| Some(id) != self_id);
            let here = facts.counts.get(name).copied().unwrap_or(0);
            if !elsewhere && here <= 1 {
                unused.push(format!("`{name}`"));
            }
        }
        if !unused.is_empty() || !exported.is_empty() {
            out.push_str("\n### Items defined here\n\n");
            if !unused.is_empty() {
                out.push_str(&format!(
                    "- Mentioned nowhere else, not even again in this file: {}. A dead-code \
                     candidate, not proof: a macro, a trait or a string may still reach it.\n",
                    unused.join(", ")
                ));
            }
            if !exported.is_empty() {
                out.push_str(&format!(
                    "- Named in api.json, so the bindings use them: {}.\n",
                    exported.join(", ")
                ));
            }
        }
        out.push_str(&self.coverage_section(rel, &facts));
        out
    }

    /// One entry per type the file names that is defined in another file.
    fn used_types(&self, project_root: &Path, rel: &str, facts: &FileFacts) -> Vec<String> {
        let mut entries = Vec::new();
        for name in &facts.idents {
            if !name.starts_with(|c: char| c.is_ascii_uppercase())
                || name.len() < 3
                || COMMON_TYPES.contains(name)
            {
                continue;
            }
            let Some(defs) = self.index.get_all_by_name(name) else {
                continue;
            };
            let here = |d: &TypeDefinition| {
                d.file_path.strip_prefix(project_root).ok().is_some_and(|p| p == Path::new(rel))
            };
            if defs.iter().any(|d| here(d.as_ref())) {
                continue;
            }
            for def in defs.iter().take(2) {
                entries.push(describe_type(def, project_root, &facts.methods));
            }
        }
        entries
    }

    /// One line per free function the file calls that is defined elsewhere
    /// and not too ambiguous to name.
    fn called_fns(&self, self_id: Option<u32>, facts: &FileFacts) -> Vec<String> {
        let mut entries = Vec::new();
        for name in &facts.calls {
            let Some(defs) = self.fns.get(*name) else {
                continue;
            };
            if defs.len() > MAX_FN_DEFINITIONS || defs.iter().any(|d| Some(d.file) == self_id) {
                continue;
            }
            for d in defs {
                entries.push(format!(
                    "- `{}` ({}:{})",
                    d.signature, self.files[d.file as usize], d.line
                ));
            }
        }
        entries
    }
}

/// Appends list entries until the section reaches `MAX_CONTEXT_BYTES`.
fn push_capped(out: &mut String, entries: &[String]) {
    for (k, entry) in entries.iter().enumerate() {
        if out.len() + entry.len() > MAX_CONTEXT_BYTES {
            out.push_str(&format!("- ... and {} more\n", entries.len() - k));
            return;
        }
        out.push_str(entry);
        out.push('\n');
    }
}

/// A short description of one type: kind, path, file, its fields or
/// variants, and the methods this file uses.
fn describe_type(def: &TypeDefinition, project_root: &Path, used_methods: &HashSet<&str>) -> String {
    let file = def
        .file_path
        .strip_prefix(project_root)
        .unwrap_or(&def.file_path)
        .display();
    let (kind, shape) = match def.get_expanded_kind() {
        TypeDefKind::Struct { fields, .. } => {
            let list: Vec<String> = fields
                .values()
                .map(|f| format!("{}: {}{}", f.name, f.ref_kind.as_prefix(), f.ty))
                .collect();
            ("struct", capped_list("fields", &list, 8))
        }
        TypeDefKind::Enum { variants, .. } => {
            let list: Vec<String> = variants
                .values()
                .map(|v| match &v.ty {
                    Some(ty) => format!("{}({})", v.name, ty),
                    None => v.name.clone(),
                })
                .collect();
            ("enum", capped_list("variants", &list, 10))
        }
        TypeDefKind::TypeAlias { target, .. } => ("type alias", format!("alias of `{target}`")),
        TypeDefKind::CallbackTypedef { args, returns } => {
            let list: Vec<String> = args.iter().map(|a| format!("{}{}", a.ref_kind.as_prefix(), a.ty)).collect();
            (
                "callback",
                format!("fn({}) -> {}", list.join(", "), returns.as_deref().unwrap_or("()")),
            )
        }
        TypeDefKind::MacroGenerated { source_macro, base_type, .. } => {
            ("macro-made type", format!("from `{source_macro}!` over `{base_type}`"))
        }
    };
    let mut entry = format!("- `{}`: {kind} `{}` ({file}); {shape}", def.type_name, def.full_path);
    let methods: Vec<String> = def
        .methods
        .iter()
        .filter(|m| used_methods.contains(m.name.as_str()))
        .take(8)
        .map(|m| {
            let doc = m.doc.first().map(|d| d.trim()).filter(|d| !d.is_empty());
            match doc {
                Some(d) => format!("`{}{}` ({})", m.name, m.signature(), cut(d, 90)),
                None => format!("`{}{}`", m.name, m.signature()),
            }
        })
        .collect();
    if !methods.is_empty() {
        entry.push_str(&format!("\n  - methods used here: {}", methods.join("; ")));
    }
    entry
}

fn capped_list(what: &str, items: &[String], max: usize) -> String {
    if items.is_empty() {
        return format!("no {what}");
    }
    let shown: Vec<&str> = items.iter().take(max).map(String::as_str).collect();
    let more = items.len().saturating_sub(max);
    if more > 0 {
        format!("{what}: {} (+{more} more)", shown.join(", "))
    } else {
        format!("{what}: {}", shown.join(", "))
    }
}

fn cut(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        format!("{}...", text.chars().take(max).collect::<String>())
    } else {
        text.to_string()
    }
}

/// What one file's code names, calls and defines.
struct FileFacts<'a> {
    /// Every identifier, once, in order of first appearance.
    idents: Vec<&'a str>,
    /// How often each identifier appears in this file's code.
    counts: HashMap<&'a str, usize>,
    /// Names used as methods or associated items (`.name`, `Type::name`).
    methods: HashSet<&'a str>,
    /// Lower-case names called as free functions (`name(`, `module::name(`).
    calls: Vec<&'a str>,
    /// Functions and types defined here, with their line; tests left out.
    defined: Vec<(&'a str, usize)>,
}

impl<'a> FileFacts<'a> {
    fn collect(code: &[&Tok<'a>]) -> FileFacts<'a> {
        let mut facts = FileFacts {
            idents: Vec::new(),
            counts: HashMap::new(),
            methods: HashSet::new(),
            calls: Vec::new(),
            defined: Vec::new(),
        };
        let text = |k: usize| code.get(k).map_or("", |t| t.text);
        for (k, t) in code.iter().enumerate() {
            if t.kind != TokKind::Ident {
                continue;
            }
            let name = t.text;
            let count = facts.counts.entry(name).or_insert(0);
            *count += 1;
            if *count == 1 {
                facts.idents.push(name);
            }
            let prev = if k > 0 { text(k - 1) } else { "" };
            let next = text(k + 1);
            let after_path = k >= 2 && prev == ":" && text(k - 2) == ":";
            match prev {
                "fn" => {
                    let is_test = (k.saturating_sub(6)..k).any(|j| text(j) == "test");
                    if !is_test && !COMMON_FNS.contains(&name) && !facts.defined.iter().any(|(n, _)| *n == name) {
                        facts.defined.push((name, t.line));
                    }
                    continue;
                }
                "struct" | "enum" | "union" | "trait" | "type" => {
                    if !facts.defined.iter().any(|(n, _)| *n == name) {
                        facts.defined.push((name, t.line));
                    }
                    continue;
                }
                "." => {
                    facts.methods.insert(name);
                    continue;
                }
                _ => {}
            }
            if after_path {
                facts.methods.insert(name);
            }
            let module_call = after_path && k >= 3 && text(k - 3).starts_with(|c: char| c.is_ascii_lowercase());
            let plain_call = !after_path;
            if next == "(" && (plain_call || module_call)
                && name.starts_with(|c: char| c.is_ascii_lowercase())
                && !facts.calls.contains(&name)
            {
                facts.calls.push(name);
            }
        }
        facts
    }
}

/// The identifiers and function definitions of one file, for the repository
/// index. Falls back to splitting on non-word characters if it does not lex.
fn scan_file(src: &str) -> (Vec<String>, Vec<(String, usize, String)>) {
    let Ok(toks) = style_guard::lex(src) else {
        let words: BTreeSet<&str> = src
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|w| !w.is_empty())
            .collect();
        return (words.into_iter().map(str::to_string).collect(), Vec::new());
    };
    let code: Vec<&Tok> = toks
        .iter()
        .filter(|t| !matches!(t.kind, TokKind::Comment(_)))
        .collect();
    let mut idents: BTreeSet<&str> = BTreeSet::new();
    let mut defs = Vec::new();
    for (k, t) in code.iter().enumerate() {
        if t.kind != TokKind::Ident {
            continue;
        }
        idents.insert(t.text);
        if t.text != "fn" {
            continue;
        }
        let Some(name) = code.get(k + 1).filter(|n| n.kind == TokKind::Ident) else {
            continue;
        };
        // The signature runs from `fn` to the body or the `;` of a declaration.
        let end = code[k + 1..]
            .iter()
            .find(|n| n.text == "{" || n.text == ";")
            .map_or(src.len(), |n| n.start);
        let signature: String = src[t.start..end].split_whitespace().collect::<Vec<_>>().join(" ");
        defs.push((name.text.to_string(), t.line, cut(&signature, 160)));
    }
    (idents.into_iter().map(str::to_string).collect(), defs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_facts_tell_calls_methods_and_definitions_apart() {
        let src = "fn helper() {}\nfn run(w: &Window) { helper(); w.resize(1); layout::solve(2); Dom::new(); }\n#[test]\nfn checks() {}\n";
        let toks = style_guard::lex(src).unwrap();
        let code: Vec<&Tok> = toks.iter().collect();
        let facts = FileFacts::collect(&code);
        assert_eq!(facts.calls, ["helper", "solve"]);
        assert!(facts.methods.contains("resize") && facts.methods.contains("new"));
        let defined: Vec<&str> = facts.defined.iter().map(|(n, _)| *n).collect();
        assert_eq!(defined, ["helper", "run"]);
    }

    #[test]
    fn function_signatures_stop_at_the_body() {
        let (_, defs) = scan_file("pub fn area(w: f32,\n    h: f32) -> f32 { w * h }\n");
        assert_eq!(defs[0].0, "area");
        assert_eq!(defs[0].2, "fn area(w: f32, h: f32) -> f32");
    }

    #[test]
    fn lcov_records_give_unrun_functions_lines_and_branches() {
        let lcov = "SF:/ci/azul/core/src/a.rs\nFN:3,_RNv1a4used\nFN:9,_RNv1a6unused\nFNDA:4,_RNv1a4used\n\
                    FNDA:0,_RNv1a6unused\nDA:3,4\nDA:10,0\nDA:11,0\nDA:13,0\nBRDA:4,0,0,1\nBRDA:4,0,1,-\n\
                    end_of_record\n";
        let files = read_lcov(lcov);
        let cov = &files["/ci/azul/core/src/a.rs"];
        assert_eq!(cov.unrun_fns.iter().copied().collect::<Vec<_>>(), [9]);
        assert_eq!(line_ranges(&cov.unrun_lines, 10), "10-11, 13");
        assert_eq!(cov.untaken_branches.iter().copied().collect::<Vec<_>>(), [4]);
    }
}
