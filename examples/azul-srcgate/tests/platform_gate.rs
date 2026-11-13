//! Gate: which system a program runs on is a value, and what a probe cannot answer is never a
//! number (AZ-W3, AZ-W6, X-AR5a-12, ARCH28 RF49 / AR5a-9, Z-16).
//!
//! Three rules over the shipped code of the four storage crates:
//!
//! - **`cfg-windows`**: `cfg!(windows)` (and `cfg!(target_os = "windows")`) as a value is written in
//!   `System::here()` (`azul-storage/src/key.rs`) and nowhere else of the logic. A policy that
//!   differs by system asks `System::here()` or takes a `System` as an argument, so it is tested for
//!   Windows on every host (the name policy and the open-target check are); the code that talks to
//!   Windows is `cfg(windows)` ATTRIBUTES (`win.rs`, the Windows branches of `beneath.rs`), which
//!   this gate does not read. A `cfg!(windows)` in the logic is a branch no test on a Mac or a Linux
//!   machine takes.
//! - **`cfg-unix`**: the same for the other half: `cfg!(unix)` as a value is a branch no test on
//!   Windows takes, and a policy that depends on it cannot be asked "and on Windows?" by a test on a
//!   Mac. The six that stand are counted below, each with what converts it.
//! - **`max-default`**: `.unwrap_or(<int>::MAX)` on something that is not `try_from(..)` - and its
//!   other spellings, `.map_or(<int>::MAX, ..)`, `.unwrap_or_else(|| <int>::MAX)` and
//!   `None => <int>::MAX`. A probe that cannot answer returned `None` and the caller read it as
//!   "endless" - the disk-reserve guard was off on Windows for exactly that reason
//!   (`free_space(..).unwrap_or(u64::MAX)`); a probe returns `Cap` now, which has no such method.
//!   `u64::try_from(x).unwrap_or(u64::MAX)` is a saturating conversion, not a probe, and is not read.
//!
//! What stays is a counted allowance with its reason: the one `System::here()`, the AzDrive files
//! that choose a shell or a program by system (their lanes convert them), the test harness, and a
//! transport limit. The counts are exact and only go down. Comments and test items are not read.

use azul_srcgate::{Allow, Budget, Gate, Located, Orphans, Root, Rule, Tok};

/// The rules' names.
const CFG_WINDOWS: &str = "cfg-windows";
const CFG_UNIX: &str = "cfg-unix";
const MAX_DEFAULT: &str = "max-default";

const ROOTS: &[Root] = &[
    Root {
        dir: "examples/azul-storage/src",
        min_files: 80,
    },
    Root {
        dir: "examples/azcloud-kit/src",
        min_files: 72,
    },
    Root {
        dir: "examples/azul-drive/src",
        min_files: 88,
    },
    Root {
        dir: "examples/azul-appkit/src",
        min_files: 26,
    },
];

/// Every `cfg!(..)` whose arguments name `windows` (as an identifier or as `target_os = "windows"`).
fn cfg_windows(t: &[Located]) -> Vec<usize> {
    cfg_naming(t, "windows")
}

/// Every `cfg!(..)` whose arguments name `unix` (as an identifier or as `target_family = "unix"`).
fn cfg_unix(t: &[Located]) -> Vec<usize> {
    cfg_naming(t, "unix")
}

/// Every `cfg!(..)` whose arguments name `word`, as an identifier or as a string.
fn cfg_naming(t: &[Located], word: &str) -> Vec<usize> {
    let mut lines = Vec::new();
    for (i, x) in t.iter().enumerate() {
        let is_macro = x.tok.is_ident("cfg")
            && t.get(i + 1).is_some_and(|n| n.tok.is_punct('!'))
            && t.get(i + 2).is_some_and(|n| n.tok.is_punct('('));
        if !is_macro {
            continue;
        }
        let mut depth = 0usize;
        for y in &t[i + 2..] {
            if y.tok.is_punct('(') {
                depth += 1;
            } else if y.tok.is_punct(')') {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            } else if y.tok.is_ident(word) || matches!(&y.tok, Tok::Str(s) if s == word) {
                lines.push(x.line);
                break;
            }
        }
    }
    lines
}

/// Whether `tok` names an integer type.
fn is_int_type(tok: &Tok) -> bool {
    tok.ident().is_some_and(|name| {
        matches!(
            name,
            "u8" | "u16"
                | "u32"
                | "u64"
                | "u128"
                | "usize"
                | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "i128"
                | "isize"
        )
    })
}

/// The index after an integer maximum (`u64::MAX`) that starts at `at`, if one does.
fn int_max_at(t: &[Located], at: usize) -> Option<usize> {
    match (t.get(at), t.get(at + 1), t.get(at + 2), t.get(at + 3)) {
        (Some(ty), Some(c1), Some(c2), Some(m))
            if is_int_type(&ty.tok)
                && c1.tok.is_punct(':')
                && c2.tok.is_punct(':')
                && m.tok.is_ident("MAX") =>
        {
            Some(at + 4)
        }
        _ => None,
    }
}

/// Whether the token at `at` is the punctuation `c`.
fn punct_at(t: &[Located], at: usize, c: char) -> bool {
    t.get(at).is_some_and(|n| n.tok.is_punct(c))
}

/// Whether the method whose name is at `i` is called on a conversion (`..try_from(..)`): a
/// saturating conversion, not a probe.
fn on_conversion(t: &[Located], i: usize) -> bool {
    if i < 2 || !t[i - 2].tok.is_punct(')') {
        return false;
    }
    let mut depth = 0usize;
    for k in (0..=i - 2).rev() {
        if t[k].tok.is_punct(')') {
            depth += 1;
        } else if t[k].tok.is_punct('(') {
            depth -= 1;
            if depth == 0 {
                return k >= 1 && t[k - 1].tok.is_ident("try_from");
            }
        }
    }
    false
}

/// Every default of an integer maximum for something that did not answer, written as
/// `.unwrap_or(<int>::MAX)`, `.map_or(<int>::MAX, ..)`, `.unwrap_or_else(|| <int>::MAX)` or
/// `None => <int>::MAX` - unless the receiver is a call of `try_from`.
fn max_default(t: &[Located]) -> Vec<usize> {
    let mut lines = Vec::new();
    for (i, x) in t.iter().enumerate() {
        let method = i >= 1 && punct_at(t, i - 1, '.') && punct_at(t, i + 1, '(');
        let hit = if method && x.tok.is_ident("unwrap_or") {
            int_max_at(t, i + 2).is_some_and(|end| punct_at(t, end, ')')) && !on_conversion(t, i)
        } else if method && x.tok.is_ident("map_or") {
            int_max_at(t, i + 2).is_some_and(|end| punct_at(t, end, ',')) && !on_conversion(t, i)
        } else if method && x.tok.is_ident("unwrap_or_else") {
            punct_at(t, i + 2, '|')
                && punct_at(t, i + 3, '|')
                && int_max_at(t, i + 4).is_some_and(|end| punct_at(t, end, ')'))
                && !on_conversion(t, i)
        } else if x.tok.is_ident("None") {
            punct_at(t, i + 1, '=')
                && punct_at(t, i + 2, '>')
                && int_max_at(t, i + 3)
                    .is_some_and(|end| punct_at(t, end, ',') || punct_at(t, end, '}'))
        } else {
            false
        };
        if hit {
            lines.push(x.line);
        }
    }
    lines
}

fn rules() -> Vec<Rule> {
    vec![
        Rule::new(
            CFG_WINDOWS,
            cfg_windows,
            &[
                "let a = cfg!(windows);",
                "if cfg!(any(target_os = \"macos\", windows)) { f(); }",
                "let b = cfg!(target_os = \"windows\");",
                "let c = !cfg!(windows) && x;",
            ],
            &[
                "let a = cfg!(unix);",
                "let b = cfg!(target_os = \"macos\");",
                "let c = System::here() == System::Windows;",
                "let d = cfg!(debug_assertions);",
            ],
        ),
        Rule::new(
            CFG_UNIX,
            cfg_unix,
            &[
                "let a = cfg!(unix);",
                "let b = !cfg!(unix);",
                "if cfg!(all(unix, not(target_os = \"macos\"))) { f(); }",
                "let c = cfg!(target_family = \"unix\");",
            ],
            &[
                "let a = cfg!(windows);",
                "let b = cfg!(target_os = \"macos\");",
                "let c = System::here() == System::Linux;",
                "let d = cfg!(debug_assertions);",
            ],
        ),
        Rule::new(
            MAX_DEFAULT,
            max_default,
            &[
                "let free = free_space(dir).unwrap_or(u64::MAX);",
                "let n = probe(path).ok().unwrap_or(usize::MAX);",
                "let n = value.unwrap_or(u64::MAX);",
                "let n = s.len().checked_mul(2).unwrap_or(i64::MAX);",
                "let n = free_space(dir).map_or(u64::MAX, |bytes| bytes);",
                "let n = free_space(dir).unwrap_or_else(|| u64::MAX);",
                "let n = match free_space(dir) { Some(b) => b, None => u64::MAX };",
                "let n = match free_space(dir) {\n    Some(b) => b,\n    None => usize::MAX,\n};",
            ],
            &[
                "let n = u64::try_from(x).unwrap_or(u64::MAX);",
                "let n = i64::try_from(a.b(c)).unwrap_or(i64::MAX);",
                "let n = u64::try_from(x).map_or(u64::MAX, |v| v);",
                "let n = x.unwrap_or(0);",
                "let n = x.unwrap_or(max_len());",
                "let n = x.unwrap_or(LIMIT::MAX);",
                "let n = x.map_or(0, |v| v + 1);",
                "let n = x.unwrap_or_else(|| limit());",
                "let n = match x { Some(v) => v, None => 0 };",
                "let n = match x { Some(v) => v, None => u64::MAX / 2 };",
                "let n = match step { Step::Last => isize::MAX / 2, Step::None => 0 };",
            ],
        ),
    ]
}

/// Where each stands today. TO SHRINK, never to grow: the reason says who converts it.
const ALLOWED: &[Allow] = &[
    Allow::exactly(
        "examples/azul-storage/src/key.rs",
        CFG_WINDOWS,
        1,
        "`System::here()`: the one place that asks which system this is",
    ),
    // ---- the other half: cfg!(unix) as a value (X-DELTA-AZDRIVE-FILES-35) ----
    Allow::exactly(
        "examples/azul-storage/src/local.rs",
        CFG_UNIX,
        1,
        "a rename across disks is refused with EXDEV on Unix: whether the error code means \
         'another disk' is asked of the build; a `System` argument would let a test play the other \
         half (B30-KIT)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/merge.rs",
        CFG_UNIX,
        1,
        "`TRACKS_EXEC`: whether the execute bit is part of a file's content - a const of the build; \
         the volume's probe (`Volume::no_exec_bits`) is what should answer (B30-KIT)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/root.rs",
        CFG_UNIX,
        2,
        "`no_exec_bits: !cfg!(unix)`: the volume of a folder that keeps no execute bit, taken from \
         the build instead of from the disk (B30-KIT)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/sync/mod.rs",
        CFG_UNIX,
        2,
        "links are made on Unix only: the refusal and its sentence ask the build (S8; B30-KIT)",
    ),
    Allow::exactly(
        "examples/azul-drive/src/command_line.rs",
        CFG_WINDOWS,
        1,
        "`Platform::this()`, the command line's own two-valued platform, a `const fn` (and \
         `System::here()` is not): the shell a line runs in (Z30-C2)",
    ),
    Allow::exactly(
        "examples/azul-drive/src/ui_command_bar.rs",
        CFG_WINDOWS,
        1,
        "COMSPEC or SHELL: the variable that names the shell the command bar runs (B30-GUI)",
    ),
    Allow::exactly(
        "examples/azul-drive/src/actions.rs",
        CFG_WINDOWS,
        3,
        "the file name of a sibling app (`.exe`), the terminal opened in a folder, and Print: which \
         program to spawn on which system (Z30-A1B, B30-GUI)",
    ),
    Allow::exactly(
        "examples/azul-drive/src/e2e/",
        CFG_WINDOWS,
        1,
        "the E2E commands play a user on the machine they run on; what they do per system is the \
         harness' business; counted, so a new one is seen",
    ),
    Allow::exactly(
        "examples/azul-storage/src/transport.rs",
        MAX_DEFAULT,
        1,
        "a range with no end asks for the rest of the object: the most the answer may have is then \
         what the object has, and the transport reads it under its time budget (`Meter`), not by \
         this number",
    ),
];

fn gate(roots: &'static [Root], allowed: &'static [Allow]) -> Gate<'static> {
    Gate {
        name: "the system is a value and an unknown probe is never a number",
        roots,
        rules: rules(),
        budget: Budget(allowed),
        orphans: Orphans::Ignore,
    }
}

#[test]
fn the_system_is_asked_in_one_place_and_a_probe_that_cannot_answer_is_never_endless() {
    gate(ROOTS, ALLOWED).run();
}

/// The rules' own cases: what they must find and must not.
#[test]
fn the_rules_find_what_they_plant_and_leave_the_clean_code_alone() {
    azul_srcgate::self_test(&rules());
}

/// Prints every hit: `cargo test -p azul-srcgate --test platform_gate -- --ignored --nocapture
/// print_hits`, to write the list above.
#[test]
#[ignore = "X-EDGE-C-1: prints the hits; the list above is written from it"]
fn print_hits() {
    let hits = azul_srcgate::hits(
        &azul_srcgate::files(&azul_srcgate::workspace(), ROOTS, Orphans::Ignore)
            .unwrap_or_else(|wrong| panic!("{wrong:#?}")),
        &rules(),
    );
    let mut per_file: std::collections::BTreeMap<(&str, &str), usize> =
        std::collections::BTreeMap::new();
    for h in &hits {
        *per_file
            .entry((h.rel.as_str(), h.rule.as_str()))
            .or_default() += 1;
    }
    for ((file, rule), n) in per_file {
        println!("{n:>3} {rule} {file}");
    }
}

/// Every allowance names a file that exists: a stale entry is a failure on its own.
#[test]
fn every_allowance_names_a_file_that_exists() {
    let base = azul_srcgate::workspace();
    for a in ALLOWED {
        assert!(
            base.join(a.file.trim_end_matches('/')).exists(),
            "{} is allowed ({}) and is not there",
            a.file,
            a.why
        );
    }
}
