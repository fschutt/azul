//! Gate: a FOREIGN transport is made only where the list says (RF39 2c, ENT27-M16, ARCH28 AR5b-2
//! acceptance gate (d)).
//!
//! `AzulTransport::azlin` carries the Azlin service's traffic - the token server and the blocks
//! and nodes of its drives, with a drive token or a signature for one - on the pool that trusts the
//! pinned authorities, and sends to Azlin hosts only. `AzulTransport::foreign` carries everybody
//! else's on the app's roots and never to an Azlin host. There is no third constructor: a call
//! site cannot leave the class unsaid. What this gate keeps is the other direction: a call site
//! that takes `foreign` because it is easier. A token server, a drive's endpoint or a node is
//! Azlin's; the sites below are the ones that are not - a data source the user typed, an OAuth
//! provider, a feed, a link that is signed and never sent. A new one is a decision, and a
//! decision gets a line here with its reason; the counts only go down.
//!
//! What it finds is a call of `AzulTransport::foreign` in shipped code. Comments and test items
//! are not read.

use std::path::{Path, PathBuf};

use azul_srcgate::{calls, Allow, Budget, Gate, Orphans, Root, Rule};

const ROOTS: &[Root] = &[Root {
    dir: "examples",
    min_files: 500,
}];

/// The rule's name.
const FOREIGN: &str = "foreign-transport";

/// Where a foreign transport is made today, and why it is not Azlin's.
const ALLOWED: &[Allow] = &[
    Allow::exactly(
        "examples/azul-drive/src/jobs.rs",
        FOREIGN,
        2,
        "the test connection of a data source the user typed, and the OAuth code exchange of a \
         provider's sign-in",
    ),
    Allow::exactly(
        "examples/azul-drive/src/actions.rs",
        FOREIGN,
        1,
        "the public link of an S3 drive: signed on this computer, never sent",
    ),
    Allow::exactly(
        "examples/azul-drive/src/lib.rs",
        FOREIGN,
        1,
        "the drive of an entry that is not an Azlin drive (a data source, a bucket of the user's)",
    ),
    Allow::exactly(
        "examples/azul-drive/src/encryption.rs",
        FOREIGN,
        1,
        "the end of a provider's sign-in when its drive is removed",
    ),
    Allow::exactly(
        "examples/azul-mail/src/lib.rs",
        FOREIGN,
        1,
        "the OAuth refresh of an IMAP account at its provider",
    ),
    Allow::exactly(
        "examples/azul-news/src/jobs.rs",
        FOREIGN,
        4,
        "a feed, an article and its pictures are anybody's",
    ),
];

fn rules() -> Vec<Rule> {
    vec![Rule::new(
        FOREIGN,
        |t| calls(t, "AzulTransport::foreign"),
        &[
            "let transport = AzulTransport::foreign(USER_AGENT);",
            "let t = azul_storage::azul_transport::AzulTransport::foreign(ua);",
        ],
        &[
            "let transport = AzulTransport::azlin(USER_AGENT);",
            "let foreign = 1;",
            "println!(\"AzulTransport::foreign(ua)\");",
        ],
    )]
}

fn gate(roots: &'static [Root], allowed: &'static [Allow]) -> Gate<'static> {
    Gate {
        name: "a foreign transport is made only where the list says",
        roots,
        rules: rules(),
        budget: Budget(allowed),
        orphans: Orphans::Ignore,
    }
}

#[test]
fn a_foreign_transport_is_made_only_in_the_listed_files() {
    gate(ROOTS, ALLOWED).run();
}

/// Prints every site: `cargo test -p azul-srcgate --test transport_class_gate -- --ignored
/// --nocapture print_reads`, to write the list above.
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
    let base = std::env::temp_dir().join(format!(
        "azul-srcgate-transportclass-{name}-{}",
        std::process::id()
    ));
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
    "foreign-transport",
    1,
    "the planted site",
)];

fn check(base: &Path, allowed: &'static [Allow]) -> Result<usize, Vec<String>> {
    gate(SMALL, allowed).check(base).map(|hits| hits.len())
}

/// A token-server call site that took the easy transport is a finding with its line; a listed one
/// is counted; one that went away leaves a stale count.
#[test]
fn a_new_foreign_site_is_caught_and_a_listed_one_is_counted() {
    let planted = "pub fn open(token_url: &str) {\n    let transport = AzulTransport::foreign(USER_AGENT);\n    let server = TokenServer::new(token_url, &transport);\n}\n";
    let base = base_with(
        "planted",
        &[
            ("core/src/lib.rs", "mod url;\n"),
            ("core/src/url.rs", planted),
        ],
    );
    let wrong = check(&base, &[]).expect_err("an unlisted foreign transport is a finding");
    assert_eq!(wrong.len(), 1, "{wrong:?}");
    assert!(wrong[0].starts_with("core/src/url.rs:2"), "{wrong:?}");
    assert_eq!(check(&base, LISTED), Ok(1));
    std::fs::write(
        base.join("core/src/url.rs"),
        "pub fn open() {\n    let transport = AzulTransport::azlin(USER_AGENT);\n}\n",
    )
    .unwrap();
    let wrong = check(&base, LISTED).expect_err("fewer sites than the list says");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 0 x foreign-transport where its allowance says 1")),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}

/// An Azlin transport, a comment and a test are no foreign site.
#[test]
fn an_azlin_transport_a_comment_and_a_test_are_no_foreign_site() {
    let base = base_with(
        "quiet",
        &[(
            "core/src/lib.rs",
            "// AzulTransport::foreign(ua)\npub fn a() {\n    let t = AzulTransport::azlin(ua);\n}\n#[cfg(test)]\nmod tests {\n    fn t() { let _ = AzulTransport::foreign(ua); }\n}\n",
        )],
    );
    assert_eq!(check(&base, &[]), Ok(0));
    let _ = std::fs::remove_dir_all(base);
}
