//! Lint: examples may only invoke symbols the bindings actually export.
//!
//! Renaming a function in api.json renames the generated symbol in all ~38
//! bindings at once, but the hand-written examples and guide snippets that
//! call it are ordinary source files that nothing regenerates. Compiling them
//! is the only other check, and it runs late in the language board and is
//! skipped for every language whose toolchain fails to install — so a rename
//! can sit broken in several bindings with no gate noticing.
//!
//! Renaming `Dom::create_text` took three rounds of chasing precisely this,
//! because each binding spells the same symbol its own way and a grep tuned
//! to one convention misses the others:
//!
//!   C           AzDom_createText
//!   Crystal     azDom_createText
//!   Haskell     c_AzDom_createText_via
//!   Fortran     az_dom_create_text
//!   C#          Dom.CreateText
//!   Pascal      TDom.CreateText
//!   PowerShell  [Azul.Dom]::CreateText
//!   Go          NewDomCreateText
//!
//! So the lint matches the conventions rather than one spelling.
//!
//! The oracle is `target/codegen`, NOT api.json. api.json is the generator's
//! input, not the surface it produces: `AzString_fromConstStr`, the `Vec`
//! constructors and every Option/Result accessor are real exported symbols
//! with no api.json entry. Checking against api.json reported ~70 of those as
//! missing and needed a growing allowlist to stay quiet. Checking against the
//! generated bindings needs no allowlist at all — a symbol exists exactly when
//! the generator emitted it.
//!
//! It only speaks when sure. A prefixed symbol (`Az…`, `az…`, `c_…_via`) is
//! unambiguously ours, so it must appear verbatim in the generated output. A
//! bare `Class.member` is only checked when `Class` is a name api.json
//! defines, or `Console.WriteLine` and every other host call is a finding.

use std::{
    collections::{BTreeSet, HashSet},
    path::Path,
    sync::LazyLock,
};

use regex::Regex;

use crate::api::ApiData;

/// One example call site naming a symbol the bindings do not export.
#[derive(Debug, Clone)]
pub struct Finding {
    pub file: String,
    pub line: usize,
    pub token: String,
    pub function: String,
    /// Closest exported spelling, when a rename merely extended a name.
    pub suggestion: Option<String>,
}

/// Convention-insensitive form: casing and separators are what differ between
/// bindings, so `create_text`, `createText` and `CreateText` all collapse to
/// `createtext`.
fn norm(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

// Haskell wraps each C symbol; the inner symbol is what must exist.
static RE_HASKELL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bc_(Az[A-Za-z0-9]+_[A-Za-z0-9_]+)_via\b").unwrap());
// C, C++, and every binding that re-exports the raw symbol.
static RE_C: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bAz[A-Z][A-Za-z0-9]*_[A-Za-z0-9_]+\b").unwrap());
// Crystal: azDom_createText.
static RE_CRYSTAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\baz[A-Z][A-Za-z0-9]*_[A-Za-z0-9_]+\b").unwrap());
// PowerShell: [Azul.Dom]::CreateText.
static RE_PWSH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[Azul\.([A-Za-z0-9]+)\]::([A-Za-z0-9_]+)").unwrap());
// C#, Pascal (TDom.), and any other object wrapper. Only trusted when the
// class is one api.json defines.
static RE_DOT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(T?[A-Z][A-Za-z0-9]*)(?:\.|::)([A-Za-z_][A-Za-z0-9_]*)\s*\(").unwrap()
});
// Go: NewDomCreateText — no separator, so the class prefix is matched greedily.
static RE_GO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bNew([A-Z][A-Za-z0-9]*)\b").unwrap());

/// Stand-in names the guides use when explaining the naming convention
/// itself (`AzXxx_yyy`, `AzMyWidgetOnFooCallback_...`). They are illustrations,
/// not calls.
const PLACEHOLDER_CLASSES: &[&str] =
    &["Xxx", "Yyy", "Foo", "Bar", "Baz", "MyType", "MyWidget", "MyStruct", "Example"];

fn placeholder(symbol: &str) -> bool {
    let Some(rest) = symbol.strip_prefix("Az") else { return false };
    let class = rest.split('_').next().unwrap_or("");
    PLACEHOLDER_CLASSES.iter().any(|p| class.starts_with(p))
}

/// Host-language types whose names collide with an api.json class. `String` is
/// the problem case: `String.valueOf` in Java is not azul's `String`.
const HOST_COLLISIONS: &[&str] = &["String", "Object", "Error", "Result", "Option"];

/// What the bindings actually export, harvested from `target/codegen`.
pub struct Surface {
    /// Every identifier appearing in a generated binding, verbatim.
    exact: HashSet<String>,
    /// The same, case- and separator-insensitive, for member lookups.
    loose: HashSet<String>,
    /// api.json class names — used only to decide whether a `Class.member`
    /// expression is ours at all.
    classes: HashSet<String>,
}

impl Surface {
    pub fn build(project_root: &Path, api: &ApiData) -> Option<Self> {
        let codegen = project_root.join("target").join("codegen");
        if !codegen.is_dir() {
            return None; // nothing to check against; stay silent
        }

        let mut exact = HashSet::new();
        for entry in walkdir::WalkDir::new(&codegen).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                continue; // binary artifact
            };
            for tok in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
                if tok.len() > 2 {
                    exact.insert(tok.to_string());
                }
            }
        }
        if exact.is_empty() {
            return None;
        }
        let loose = exact.iter().map(|s| norm(s)).collect();

        let mut classes = HashSet::new();
        if let Some(version) = api.get_latest_version_str().map(str::to_string) {
            if let Some(vd) = api.get_version(&version) {
                for module in vd.api.values() {
                    for class_name in module.classes.keys() {
                        classes.insert(class_name.clone());
                    }
                }
            }
        }

        Some(Self { exact, loose, classes })
    }

    fn exports(&self, symbol: &str) -> bool {
        self.exact.contains(symbol)
    }

    fn exports_member(&self, member: &str) -> bool {
        self.loose.contains(&norm(member))
    }

    /// A rename usually extends or trims a name, so an exported spelling that
    /// shares a prefix is the useful hint.
    fn suggest(&self, name: &str) -> Option<String> {
        let want = norm(name);
        if want.len() < 4 {
            return None;
        }
        // Only propose a symbol of the SAME shape, or the hint is noise:
        // `AzDom_setInlineStyle` must be answered with another `AzDom_*`, not
        // with whatever fragment happens to share three letters.
        let same_class_prefix = name
            .find('_')
            .filter(|_| name.starts_with("Az"))
            .map(|i| name[..=i].to_string());
        let mut best: Option<&String> = None;
        for cand in &self.exact {
            match &same_class_prefix {
                Some(p) if !cand.starts_with(p.as_str()) => continue,
                None if cand.contains('_') => continue,
                _ => {}
            }
            let n = norm(cand);
            // A rename normally EXTENDS a name, so prefer that direction; the
            // reverse is only a useful hint when the shared stem is long
            // enough to mean something (`fro` for `from_const_slice` is not).
            let plausible = n.starts_with(&want) || (want.starts_with(&n) && n.len() >= 6);
            if n != want && plausible && best.is_none_or(|b| cand.len() < b.len()) {
                best = Some(cand);
            }
        }
        best.cloned()
    }
}

fn check_line(line: &str, s: &Surface, file: &str, lineno: usize, out: &mut Vec<Finding>) {
    let mut flag = |symbol: &str, token: &str, out: &mut Vec<Finding>| {
        if placeholder(symbol) {
            return;
        }
        out.push(Finding {
            file: file.to_string(),
            line: lineno,
            token: token.to_string(),
            function: symbol.to_string(),
            suggestion: s.suggest(symbol),
        });
    };

    // Haskell's c_<symbol>_via.
    let mut covered: Vec<(usize, usize)> = Vec::new();
    for c in RE_HASKELL.captures_iter(line) {
        let m = c.get(0).unwrap();
        covered.push((m.start(), m.end()));
        if !s.exports(&c[1]) {
            flag(&c[1], m.as_str(), out);
        }
    }

    // C / C++ / Crystal: the whole symbol must be exported verbatim.
    for re in [&*RE_C, &*RE_CRYSTAL] {
        for m in re.find_iter(line) {
            if covered.iter().any(|(a, b)| m.start() >= *a && m.start() < *b) {
                continue;
            }
            // Prose sometimes writes the Haskell wrapper without its `c_`.
            let sym = m.as_str().strip_suffix("_via").unwrap_or(m.as_str());
            if !s.exports(sym) {
                flag(sym, m.as_str(), out);
            }
        }
    }

    for c in RE_PWSH.captures_iter(line) {
        if s.classes.contains(&c[1]) && !s.exports_member(&c[2]) {
            flag(&c[2], c.get(0).unwrap().as_str(), out);
        }
    }

    for c in RE_DOT.captures_iter(line) {
        let raw = &c[1];
        let class = raw.strip_prefix('T').unwrap_or(raw);
        if HOST_COLLISIONS.contains(&class) {
            continue;
        }
        if (s.classes.contains(raw) || s.classes.contains(class)) && !s.exports_member(&c[2]) {
            flag(&c[2], c.get(0).unwrap().as_str().trim_end_matches('('), out);
        }
    }

    for c in RE_GO.captures_iter(line) {
        let whole = c.get(0).unwrap().as_str();
        if s.exports(whole) || s.exports_member(whole) {
            continue;
        }
        // Only a finding when the prefix really names one of our classes.
        if s.classes.iter().any(|k| c[1].starts_with(k.as_str())) {
            flag(whole, whole, out);
        }
    }
}

fn scannable(path: &Path) -> bool {
    const SKIP: &[&str] = &[
        "png", "jpg", "jpeg", "gif", "ico", "pdf", "zip", "gz", "obj", "exe",
        // Rust examples are type-checked by cargo and call the Rust crates
        // directly, whose inherent methods are not a binding surface.
        "rs",
    ];
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => !SKIP.contains(&ext.to_ascii_lowercase().as_str()),
        // Extensionless files in examples/ are compiled binaries.
        None => false,
    }
}

/// Files git tracks, relative to the repo root.
///
/// The build drops generated bindings into the example trees
/// (examples/c/azul.h, examples/lua/azul.lua). Those are outputs, not written
/// examples. Git already knows which files a human wrote, so ask it.
fn tracked_files(project_root: &Path) -> Option<HashSet<String>> {
    let out = std::process::Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(project_root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&out.stdout)
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

/// Run the lint over the example, e2e and guide trees.
///
/// `doc/guide/en/internals/**` is excluded: it documents Rust-internal APIs
/// and deliberate placeholder names (`AzMyType_do_thing`), neither of which is
/// an exported binding symbol.
pub fn run(project_root: &Path, api: &ApiData) -> Vec<Finding> {
    let (Some(surface), Some(tracked)) =
        (Surface::build(project_root, api), tracked_files(project_root))
    else {
        return Vec::new();
    };

    let mut findings = Vec::new();
    for root in ["examples", "tests/e2e", "doc/guide"] {
        let dir = project_root.join(root);
        if !dir.is_dir() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&dir).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if !path.is_file() || !scannable(path) {
                continue;
            }
            let rel = path
                .strip_prefix(project_root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            if !tracked.contains(&rel) || rel.starts_with("doc/guide/en/internals/") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            for (i, line) in text.lines().enumerate() {
                check_line(line, &surface, &rel, i + 1, &mut findings);
            }
        }
    }
    // One report per (file, token): a name called ten times is one mistake.
    let mut seen = BTreeSet::new();
    findings.retain(|f| seen.insert((f.file.clone(), f.token.clone())));
    findings
}
