//! Gate: the generated conformance programs gate the shipped tier, and the shipped bindings that
//! have no program yet are a ledger that can only shrink (the CI step "Conformance programs
//! (generated, gates the shipped tier)", found red on 2026-11-25).
//!
//! `azul-doc codegen all` renders ONE conformance plan, but into C only ("C first", 20eadd85b5):
//! `c` is the one language with a program. `scripts/conformance.sh` fails a SHIPPED binding
//! (`SHIPPED_LANGS` of `scripts/e2e_language_matrix.sh`) that has none, and the step sits behind
//! the language gate, so no full run reached it between 2026-10-17 and 2026-11-25; the first one
//! that did failed it in every family. `scripts/conformance_pending.txt` is the ledger of the
//! shipped bindings still to get a program: the script reports each as PENDING and does not fail
//! it, and it fails when
//!
//! - a shipped binding has neither a program nor a ledger line,
//! - a ledger line names a binding that has a program now (the line goes with the commit that
//!   adds the program) or one that is not shipped,
//! - a program fails (the ledger never excuses a program).
//!
//! Two halves, both here. The script is run against a small tree made for each case (it finds its
//! root from its own place), so every rule is shown to fire; and the checked-in files are read,
//! so the ledger is exactly the shipped bindings that have no runner in the script, and a program
//! is a renderer in the generator AND a runner in the script, never one of the two.

use std::fs;

use azul_srcgate::workspace;

const SCRIPT: &str = "scripts/conformance.sh";
const MATRIX: &str = "scripts/e2e_language_matrix.sh";
const LEDGER: &str = "scripts/conformance_pending.txt";
const GENERATOR: &str = "doc/src/codegen/v2/generator.rs";

fn read(rel: &str) -> String {
    let path = workspace().join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The words of the `SHIPPED_LANGS=( .. )` array of the e2e matrix, comments dropped: the shipped
/// tier as the script reads it.
fn shipped_langs(matrix: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut inside = false;
    for line in matrix.lines() {
        if line.starts_with("SHIPPED_LANGS=(") {
            inside = true;
        }
        if !inside {
            continue;
        }
        let code = line.split('#').next().unwrap_or("");
        let code = code.replace("SHIPPED_LANGS=(", " ").replace(')', " ");
        words.extend(code.split_whitespace().map(str::to_string));
        if line.starts_with(')') {
            break;
        }
    }
    words
}

/// The languages the script can run: its `conf_<lang>() {` functions.
fn runner_langs(script: &str) -> Vec<String> {
    script
        .lines()
        .filter_map(|line| {
            let name = line.strip_prefix("conf_")?.strip_suffix("() {")?;
            Some(name.to_string())
        })
        .collect()
}

/// The languages the generator renders a program for: `"conformance/<lang>/...` paths.
fn rendered_langs(generator: &str) -> Vec<String> {
    const MARK: &str = "\"conformance/";
    let mut langs: Vec<String> = Vec::new();
    for (at, _) in generator.match_indices(MARK) {
        let rest = &generator[at + MARK.len()..];
        let lang: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !lang.is_empty() && rest[lang.len()..].starts_with('/') && !langs.contains(&lang) {
            langs.push(lang);
        }
    }
    langs
}

/// The ledger's languages: one per line, `#` starts a comment.
fn ledger_langs(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.split('#').next().unwrap_or("").trim())
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn sorted(mut list: Vec<String>) -> Vec<String> {
    list.sort();
    list
}

#[test]
fn a_program_is_a_renderer_in_the_generator_and_a_runner_in_the_script() {
    let rendered = rendered_langs(&read(GENERATOR));
    let runners = runner_langs(&read(SCRIPT));
    assert!(
        !rendered.is_empty(),
        "{GENERATOR} renders no `conformance/<lang>/` path: this gate reads its shape"
    );
    assert_eq!(
        sorted(rendered),
        sorted(runners),
        "the generator renders a program for one set of languages, {SCRIPT} runs another: a \
         language gets its renderer (doc/src/codegen/v2/conformance/) and its `conf_<lang>` \
         together, in one commit, and loses its line of {LEDGER} in that commit"
    );
}

#[test]
fn the_ledger_is_exactly_the_shipped_bindings_that_have_no_program() {
    let shipped = shipped_langs(&read(MATRIX));
    let runners = runner_langs(&read(SCRIPT));
    let ledger = ledger_langs(&read(LEDGER));
    assert!(
        shipped.iter().any(|l| l == "c") && shipped.len() >= 10,
        "the shipped tier was not read from {MATRIX}: {shipped:?}"
    );
    let ledger = sorted(ledger);
    let mut once = ledger.clone();
    once.dedup();
    assert_eq!(once, ledger, "a language is listed twice in {LEDGER}");
    let stale: Vec<&String> = ledger
        .iter()
        .filter(|l| !shipped.contains(l) || runners.contains(l))
        .collect();
    assert!(
        stale.is_empty(),
        "{LEDGER} lists {stale:?}: a binding that has a program now, or one that is not shipped. \
         The ledger can only shrink: delete the line"
    );
    let missing: Vec<&String> = shipped
        .iter()
        .filter(|l| !runners.contains(l) && !ledger.contains(l))
        .collect();
    assert!(
        missing.is_empty(),
        "shipped without a program and without a line in {LEDGER}: {missing:?}"
    );
}

/// What the cases below run: bash, and for the cases that run a program a C compiler.
#[cfg(unix)]
mod script {
    use std::{
        fs,
        path::{Path, PathBuf},
        process::{Command, Output},
    };

    use super::{workspace, SCRIPT};

    /// A C compiler of this machine, when it has one.
    fn c_compiler() -> Option<&'static str> {
        ["clang", "gcc", "cc"].into_iter().find(|cc| {
            Command::new(*cc)
                .arg("--version")
                .output()
                .is_ok_and(|o| o.status.success())
        })
    }

    /// A tree the script finds as its root: a copy of the script, a shipped tier, a ledger, and
    /// the generated program folders the case chooses (an empty folder is a program the script
    /// finds and does not run unless it is asked for `c`).
    pub(super) struct Tree {
        root: PathBuf,
    }

    impl Tree {
        pub(super) fn new(case: &str, shipped: &[&str], ledger: &[&str], programs: &[&str]) -> Tree {
            let root = std::env::temp_dir()
                .join(format!("azul-conformance-gate-{}-{case}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            let scripts = root.join("scripts");
            fs::create_dir_all(&scripts).unwrap();
            fs::create_dir_all(root.join("target/codegen/conformance")).unwrap();
            fs::copy(workspace().join(SCRIPT), scripts.join("conformance.sh")).unwrap();
            // The script reads the tier from the matrix's own array: the tree's has just that.
            fs::write(
                scripts.join("e2e_language_matrix.sh"),
                format!("SHIPPED_LANGS=(\n  {}\n)\n", shipped.join(" ")),
            )
            .unwrap();
            fs::write(
                scripts.join("conformance_pending.txt"),
                format!("# the ledger of this case\n{}\n", ledger.join("\n")),
            )
            .unwrap();
            for lang in programs {
                fs::create_dir_all(root.join("target/codegen/conformance").join(lang)).unwrap();
            }
            Tree { root }
        }

        /// The `c` program is `source`, linked against an empty libazul made by the C compiler.
        /// `false` when the machine has no C compiler (the case is then not run).
        pub(super) fn with_c_program(&self, source: &str) -> bool {
            let Some(cc) = c_compiler() else {
                return false;
            };
            let dir = self.root.join("target/codegen/conformance/c");
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("conformance.c"), source).unwrap();
            let release = self.root.join("target/release");
            fs::create_dir_all(&release).unwrap();
            let empty = release.join("empty.c");
            fs::write(&empty, "int az_conformance_gate_empty;\n").unwrap();
            let (library, flag) = if cfg!(target_os = "macos") {
                ("libazul.dylib", "-dynamiclib")
            } else {
                ("libazul.so", "-shared")
            };
            let made = Command::new(cc)
                .arg(flag)
                .args(["-fPIC", "-o"])
                .arg(release.join(library))
                .arg(&empty)
                .status()
                .unwrap();
            assert!(made.success(), "the empty libazul");
            true
        }

        /// `bash scripts/conformance.sh <args>` in the tree.
        pub(super) fn run(&self, args: &[&str]) -> Output {
            Command::new("bash")
                .arg("scripts/conformance.sh")
                .args(args)
                .current_dir(&self.root)
                .output()
                .expect("bash runs")
        }

        pub(super) fn root(&self) -> &Path {
            &self.root
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    pub(super) fn said(out: &Output) -> (String, String) {
        (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }
}

#[cfg(unix)]
mod cases {
    use super::script::{said, Tree};

    const PENDING: &str = "PENDING (no generated conformance program yet)";

    #[test]
    fn a_pending_binding_is_reported_pending_and_does_not_fail() {
        let tree = Tree::new("pending", &["c", "cpp", "rust"], &["cpp", "rust"], &["c"]);
        let out = tree.run(&["cpp", "rust"]);
        let (stdout, stderr) = said(&out);
        assert!(out.status.success(), "stdout:\n{stdout}\nstderr:\n{stderr}");
        assert!(stdout.contains(&format!("[cpp] {PENDING}")), "{stdout}");
        assert!(stdout.contains(&format!("[rust] {PENDING}")), "{stdout}");
    }

    #[test]
    fn a_pending_binding_is_pending_under_the_memtest_too() {
        let tree = Tree::new("pending-memtest", &["c", "cpp"], &["cpp"], &["c"]);
        let out = tree.run(&["--memtest", "cpp"]);
        let (stdout, stderr) = said(&out);
        assert!(out.status.success(), "stdout:\n{stdout}\nstderr:\n{stderr}");
        assert!(stdout.contains(&format!("[cpp] {PENDING}")), "{stdout}");
    }

    #[test]
    fn a_shipped_binding_with_neither_a_program_nor_a_ledger_line_fails() {
        let tree = Tree::new("missing", &["c", "cpp", "rust"], &["cpp"], &["c"]);
        let out = tree.run(&["rust"]);
        let (stdout, stderr) = said(&out);
        assert!(!out.status.success(), "stdout:\n{stdout}\nstderr:\n{stderr}");
        assert!(
            stderr.contains("[rust] FAIL: shipped binding without a generated conformance program"),
            "{stderr}"
        );
        // The ledger is exact for the whole tier, whichever languages a run asks for.
        let other = tree.run(&["cpp"]);
        let (stdout, stderr) = said(&other);
        assert!(!other.status.success(), "stdout:\n{stdout}\nstderr:\n{stderr}");
        assert!(stderr.contains("[rust] FAIL"), "{stderr}");
        assert!(stdout.contains(&format!("[cpp] {PENDING}")), "{stdout}");
    }

    #[test]
    fn a_ledger_line_of_a_binding_that_has_a_program_now_must_go() {
        let tree = Tree::new("stale", &["c", "cpp"], &["c", "cpp"], &["c"]);
        let out = tree.run(&["cpp"]);
        let (stdout, stderr) = said(&out);
        assert!(!out.status.success(), "stdout:\n{stdout}\nstderr:\n{stderr}");
        assert!(
            stderr.contains("[c] FAIL") && stderr.contains("delete the line"),
            "{stderr}"
        );
        // Asked for the language itself: the line is reported and the program is not run.
        let asked = tree.run(&["c"]);
        let (stdout, stderr) = said(&asked);
        assert!(!asked.status.success(), "stdout:\n{stdout}\nstderr:\n{stderr}");
        assert!(stderr.contains("[c] FAIL") && stderr.contains("delete the line"), "{stderr}");
        assert!(!stdout.contains("[c] OK"), "{stdout}");
    }

    #[test]
    fn a_ledger_line_of_a_language_that_is_not_shipped_must_go() {
        let tree = Tree::new("unshipped", &["c", "cpp"], &["cpp", "nim"], &["c"]);
        let out = tree.run(&["cpp"]);
        let (stdout, stderr) = said(&out);
        assert!(!out.status.success(), "stdout:\n{stdout}\nstderr:\n{stderr}");
        assert!(
            stderr.contains("[nim] FAIL") && stderr.contains("not a shipped binding"),
            "{stderr}"
        );
    }

    #[test]
    fn half_a_program_fails_whatever_the_ledger_says() {
        // The generator rendered a folder for cpp and the script has no conf_cpp.
        let rendered_only = Tree::new("half", &["c", "cpp"], &["cpp"], &["c", "cpp"]);
        let out = rendered_only.run(&["cpp"]);
        let (stdout, stderr) = said(&out);
        assert!(!out.status.success(), "stdout:\n{stdout}\nstderr:\n{stderr}");
        assert!(
            stderr.contains("[cpp] FAIL: half a conformance program"),
            "{stderr}"
        );
    }

    #[test]
    fn a_language_that_is_not_shipped_is_skipped() {
        let tree = Tree::new("skip", &["c"], &[], &["c"]);
        let out = tree.run(&["nim"]);
        let (stdout, stderr) = said(&out);
        assert!(out.status.success(), "stdout:\n{stdout}\nstderr:\n{stderr}");
        assert!(
            stdout.contains("[nim] SKIP: not shipped, no conformance program"),
            "{stdout}"
        );
    }

    #[test]
    fn a_failing_program_still_fails_the_script() {
        let tree = Tree::new("failing", &["c", "cpp"], &["cpp"], &["c"]);
        let source = "#include <stdio.h>\nint main(void) { fprintf(stderr, \"FAIL planted\\n\"); return 1; }\n";
        if !tree.with_c_program(source) {
            eprintln!("no C compiler: the case is not run");
            return;
        }
        let out = tree.run(&["c", "cpp"]);
        let (stdout, stderr) = said(&out);
        assert!(!out.status.success(), "stdout:\n{stdout}\nstderr:\n{stderr}");
        assert!(stderr.contains("FAIL planted") && stderr.contains("[c] FAIL"), "{stderr}");
        // The ledger language beside it is still just pending.
        assert!(stdout.contains(&format!("[cpp] {PENDING}")), "{stdout}");
    }

    #[test]
    fn a_passing_program_passes_next_to_the_pending_bindings() {
        let tree = Tree::new("passing", &["c", "cpp"], &["cpp"], &["c"]);
        if !tree.with_c_program("int main(void) { return 0; }\n") {
            eprintln!("no C compiler: the case is not run");
            return;
        }
        let out = tree.run(&["c", "cpp"]);
        let (stdout, stderr) = said(&out);
        assert!(out.status.success(), "{} stdout:\n{stdout}\nstderr:\n{stderr}", tree.root().display());
        assert!(stdout.contains("[c] OK"), "{stdout}");
        assert!(stdout.contains(&format!("[cpp] {PENDING}")), "{stdout}");
    }
}
