//! Gate: the token server an Azlin drive's credentials are SENT to is chosen in one place, and
//! the address a drive entry carries (`account_url` in `drives.json`) is read only where this
//! list says (X-DELTA-AZDRIVE-KEYS-9, AZ-K20, REVIEW K20, P7, ARCH28 RF39).
//!
//! A drive token, the period tokens, a recovery key, a voucher and a lockdown go to the token
//! server a run resolves; `drives.json` and `account.json` are files any process of the user can
//! write, and a lying token server can seed the address of a drive it hands out
//! (`DriveBundle::entry_named`). AzDrive's refresh asked `anchors::token_server_for` (REVIEW K20)
//! and eight other call sites read the entry's `account_url` themselves. They all go through
//! `periods::azlin_drive` now, which asks the anchors: a shipped build answers the run's (the
//! live) server, whatever the entry says. This gate keeps a ninth site from growing: a new file
//! that names the identifier `account_url` fails, and a file that stops naming it has to leave
//! the list - the counts only go down.
//!
//! What it finds is the identifier in shipped code - a field, a pattern, a variable, a
//! parameter. Comments, strings and test items are not read. AzMail's own copy of the choice is
//! `azul-mail-core`'s `Endpoints::token_url_for`, listed under its heading.

use std::path::{Path, PathBuf};

use azul_srcgate::{idents, Allow, Budget, Gate, Orphans, Root, Rule};

const ROOTS: &[Root] = &[Root {
    dir: "examples",
    min_files: 500,
}];

/// The rule's name.
const READ: &str = "account-url";

/// Where the drive entry's token-server address is named today. The counts are exact: a new site
/// is a failure, and so is one that went away.
const ALLOWED: &[Allow] = &[
    // ---- The data: the entry's field, and where it is made ----
    Allow::exactly(
        "examples/azul-storage/src/config.rs",
        READ,
        3,
        "DriveAuth::Azlin's field and DriveEntry::azlin(), which hands it on",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/bundle.rs",
        READ,
        3,
        "DriveBundle::entry_named fills an empty address with the server the drive came from \
         (a record of where it was made; nothing reads it to send a credential there in a \
         shipped build)",
    ),
    // ---- The choices: where the address becomes a destination ----
    Allow::exactly(
        "examples/azul-drive/src/periods.rs",
        READ,
        2,
        "azlin_drive: the one place of AzDrive that asks the anchors which server a drive's \
         credentials go to (a shipped build: the run's, whatever the entry says)",
    ),
    Allow::exactly(
        "examples/azul-mail-core/src/azlin.rs",
        READ,
        4,
        "Endpoints::token_url_for: AzMail's choice (a shipped build: the live server, whatever \
         the account's saved address says)",
    ),
];

fn rules() -> Vec<Rule> {
    vec![Rule::new(
        READ,
        |t| idents(t, "account_url"),
        &[
            "let url = &entry.account_url;",
            "let DriveAuth::Azlin { account_url, .. } = auth else { return };",
            "send(token, account_url);",
        ],
        &[
            "let account_urls = 1;",
            "let url = entry.token_url;",
            "println!(\"account_url is a file\");",
        ],
    )]
}

fn gate(roots: &'static [Root], allowed: &'static [Allow]) -> Gate<'static> {
    Gate {
        name: "the drive entry's token-server address is named only where the list says",
        roots,
        rules: rules(),
        budget: Budget(allowed),
        orphans: Orphans::Ignore,
    }
}

#[test]
fn the_drive_entrys_token_server_address_is_named_only_in_the_listed_files() {
    gate(ROOTS, ALLOWED).run();
}

/// Prints every site: `cargo test -p azul-srcgate --test token_server_gate -- --ignored
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
        "azul-srcgate-tokenserver-{name}-{}",
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
    "account-url",
    1,
    "the planted site",
)];

fn check(base: &Path, allowed: &'static [Allow]) -> Result<usize, Vec<String>> {
    gate(SMALL, allowed).check(base).map(|hits| hits.len())
}

/// A ninth site that reads the entry's address - the shape the eight call sites had - is a
/// finding with its line; a listed site is counted; a site that went away leaves a stale count.
#[test]
fn a_new_reader_of_the_entrys_address_is_caught_and_a_listed_one_is_counted() {
    let planted = "pub fn token_url(entry: &DriveEntry) -> String {\n    let (_, account_url) = entry.azlin().unwrap();\n    account_url.to_string()\n}\n";
    let base = base_with(
        "planted",
        &[
            ("core/src/lib.rs", "mod url;\n"),
            ("core/src/url.rs", planted),
        ],
    );
    let wrong = check(&base, &[]).expect_err("an unlisted reader is a finding");
    assert_eq!(wrong.len(), 2, "{wrong:?}");
    assert!(
        wrong.iter().any(|w| w.starts_with("core/src/url.rs:2")),
        "{wrong:?}"
    );
    assert!(
        wrong.iter().any(|w| w.starts_with("core/src/url.rs:3")),
        "{wrong:?}"
    );
    // The list takes one by count: two sites are one too many.
    let wrong = check(&base, LISTED).expect_err("one more site than the list says");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 2 x account-url where its allowance says 1")),
        "{wrong:?}"
    );
    std::fs::write(
        base.join("core/src/url.rs"),
        "pub fn token_url(entry: &DriveEntry) -> String {\n    entry.account_url.clone()\n}\n",
    )
    .unwrap();
    assert_eq!(check(&base, LISTED), Ok(1));
    std::fs::write(base.join("core/src/url.rs"), "pub fn clean() {}\n").unwrap();
    let wrong = check(&base, LISTED).expect_err("fewer sites than the list says");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 0 x account-url where its allowance says 1")),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}

/// A comment, a string and a test are no site.
#[test]
fn the_identifier_in_a_comment_a_string_or_a_test_is_no_site() {
    let base = base_with(
        "quiet",
        &[(
            "core/src/lib.rs",
            "// account_url\n/// the `account_url` of a drive\npub fn a() {\n    println!(\"account_url\");\n}\n#[cfg(test)]\nmod tests {\n    fn t(account_url: &str) {}\n}\n",
        )],
    );
    assert_eq!(check(&base, &[]), Ok(0));
    let _ = std::fs::remove_dir_all(base);
}
