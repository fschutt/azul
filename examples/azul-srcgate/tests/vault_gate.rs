//! Gate: a PLAIN period-token store is made only where the list says (AZ-K13).
//!
//! The period vault of a drive holds the issue keys of paid checkouts - bearers for months a
//! customer paid for - and the period tokens. `PeriodTokenStore::sealed` keeps them as ciphertext
//! under a random key the keyring holds; `PeriodTokenStore::new` keeps plain JSON, which every
//! program of the user reads. Only the build without the encryption (tests of the plain paths,
//! never shipped) and tests make one. A new plain store in shipped code is a decision, and a
//! decision gets a line here with its reason; the counts only go down.
//!
//! What it finds is a call of `PeriodTokenStore::new` in shipped code. Comments and test items are
//! not read.

use std::path::{Path, PathBuf};

use azul_srcgate::{calls, Allow, Budget, Gate, Orphans, Root, Rule};

const ROOTS: &[Root] = &[Root {
    dir: "examples",
    min_files: 500,
}];

/// The rule's name.
const PLAIN: &str = "plain-period-store";

/// Where a plain store is made today, and why.
const ALLOWED: &[Allow] = &[Allow::exactly(
    "examples/azul-drive/src/lib.rs",
    PLAIN,
    1,
    "the build without the encryption feature (`--no-default-features`: the tests of the plain \
     paths, never shipped); the build with it makes the sealed store",
)];

fn rules() -> Vec<Rule> {
    vec![Rule::new(
        PLAIN,
        |t| calls(t, "PeriodTokenStore::new"),
        &[
            "let store = PeriodTokenStore::new(dir);",
            "let store = azcloud_kit::PeriodTokenStore::new(period_tokens_dir(None));",
        ],
        &[
            "let store = PeriodTokenStore::sealed(dir, keyring);",
            "let kept = store.through(keyring);",
            "println!(\"PeriodTokenStore::new(dir)\");",
        ],
    )]
}

fn gate(roots: &'static [Root], allowed: &'static [Allow]) -> Gate<'static> {
    Gate {
        name: "a plain period-token store is made only where the list says",
        roots,
        rules: rules(),
        budget: Budget(allowed),
        orphans: Orphans::Ignore,
    }
}

#[test]
fn a_plain_period_store_is_made_only_in_the_listed_files() {
    gate(ROOTS, ALLOWED).run();
}

/// Prints every site: `cargo test -p azul-srcgate --test vault_gate -- --ignored --nocapture
/// print_reads`, to write the list above.
#[test]
#[ignore = "prints the sites; the list above is written from it"]
fn print_reads() {
    let hits = azul_srcgate::hits(
        &azul_srcgate::files(&azul_srcgate::workspace(), ROOTS, Orphans::Ignore)
            .unwrap_or_else(|wrong| panic!("{wrong:#?}")),
        &rules(),
    );
    let mut per_file: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for h in &hits {
        *per_file.entry(h.rel.as_str()).or_default() += 1;
    }
    for (file, n) in per_file {
        println!("{n:>3} {file}");
    }
}

/// A tree on disk below a fresh base: (path below the base, text).
fn base_with(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let base =
        std::env::temp_dir().join(format!("azul-srcgate-vault-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    for (path, text) in files {
        let full = base.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }
    base
}

const SMALL: &[Root] = &[Root {
    dir: "core/src",
    min_files: 1,
}];

const LISTED: &[Allow] = &[Allow::exactly(
    "core/src/url.rs",
    "plain-period-store",
    1,
    "the planted site",
)];

fn check(base: &Path, allowed: &'static [Allow]) -> Result<usize, Vec<String>> {
    gate(SMALL, allowed).check(base).map(|hits| hits.len())
}

/// A plain store in a new place is a finding with its line; a listed one is counted; a sealed one,
/// a comment and a test are none.
#[test]
fn a_new_plain_store_is_caught_and_a_sealed_one_a_comment_and_a_test_are_not() {
    let planted = "pub fn open(dir: &str) {\n    let store = PeriodTokenStore::new(dir);\n}\n";
    let base = base_with(
        "planted",
        &[
            ("core/src/lib.rs", "mod url;\n"),
            ("core/src/url.rs", planted),
        ],
    );
    let wrong = check(&base, &[]).expect_err("an unlisted plain store is a finding");
    assert_eq!(wrong.len(), 1, "{wrong:?}");
    assert!(wrong[0].starts_with("core/src/url.rs:2"), "{wrong:?}");
    assert_eq!(check(&base, LISTED), Ok(1));
    std::fs::write(
        base.join("core/src/url.rs"),
        "// PeriodTokenStore::new(dir)\npub fn open(dir: &str, k: K) {\n    let store = PeriodTokenStore::sealed(dir, k);\n}\n#[cfg(test)]\nmod tests {\n    fn t() { let _ = PeriodTokenStore::new(\"d\"); }\n}\n",
    )
    .unwrap();
    let wrong = check(&base, LISTED).expect_err("fewer sites than the list says");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 0 x plain-period-store where its allowance says 1")),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}
