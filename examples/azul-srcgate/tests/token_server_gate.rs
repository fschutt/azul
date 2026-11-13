//! Gate: the token server an Azlin drive's credentials are SENT to is chosen in one place, and
//! the address a drive entry carries (`account_url` in `drives.json`) is read only where this
//! list says (X-DELTA-AZDRIVE-KEYS-9, AZ-K20, REVIEW K20, P7, ARCH28 RF39, ARCH29 C6d,
//! X-DELTA-STRUCT3-11). Two rules:
//!
//! - **`account-url`**: the identifier `account_url`, the address itself. `DriveEntry::azlin()` hands out the
//!   drive's id and no address any more (it gave it to eight places), so the field and the resolver are all.
//! - **`token-server-choice`**: every use of the function that turns an entry's address and the run's into one
//!   server, `anchors::token_server_choice`, of its twin `token_server_for` (deleted: it was a second way
//!   in, asked by the account's own constructor) and of the kit's `azlin_target_choice` - the resolver with the
//!   build's choice as a `bool`, crate-private since X-DELTA-STRUCT4-5 (it was `pub`: any crate could send a drive
//!   token to the address `drives.json` carries with one `true`), asked once by `azlin_target`. The kit's
//!   `azlin_target_choice` asks the anchors once - THE resolver, which `open_azlin` calls and whose answer the
//!   account takes - and AzMail's account (no entry) asks them for its own rule. A third place that chooses, or
//!   that asks the kit's resolver with a `bool`, is a finding, whatever spelling it uses.
//!
//! A drive token, the period tokens, a recovery key, a voucher and a lockdown go to the token
//! server a run resolves; `drives.json` and `account.json` are files any process of the user can
//! write, and a lying token server can seed the address of a drive it hands out
//! (`DriveBundle::entry_named`). AzDrive's refresh asked `anchors::token_server_for` (REVIEW K20)
//! and eight other call sites read the entry's `account_url` themselves; then they went through
//! `periods::azlin_drive`, AzDrive's own resolver, beside AzMail's copy. They all go through
//! `azcloud_kit::azlin_target` now (`azcloud-kit/src/drive.rs`), which `open_azlin` asks and which asks
//! the anchors: a shipped build answers the run's (the live) server, whatever the entry says. This gate
//! keeps another site from growing: a new file that names the identifier `account_url` fails, and a
//! file that stops naming it has to leave the list - the counts only go down.
//!
//! What it finds is the identifier in shipped code - a field, a pattern, a variable, a
//! parameter. Comments, strings and test items are not read. AzMail's account has no entry: its
//! `Endpoints::token_url_for` takes the account's address as `named` and asks the same anchors function
//! for a test build (a shipped one answers the live server as a constant).

use std::path::{Path, PathBuf};

use azul_srcgate::{idents, Allow, Budget, Gate, Located, Orphans, Root, Rule};

const ROOTS: &[Root] = &[Root {
    dir: "examples",
    min_files: 500,
}];

/// The rule's name.
const READ: &str = "account-url";
/// The rule that counts the choices of the token server.
const CHOICE: &str = "token-server-choice";

/// Every use of `token_server_choice`, `token_server_for` and `azlin_target_choice` that is no definition (`fn ..`): a
/// call under any path, an import under another name, a function value.
fn choices(t: &[Located]) -> Vec<usize> {
    t.iter()
        .enumerate()
        .filter(|(at, x)| {
            (x.tok.is_ident("token_server_choice")
                || x.tok.is_ident("token_server_for")
                || x.tok.is_ident("azlin_target_choice"))
                && !(*at > 0 && t[*at - 1].tok.is_ident("fn"))
        })
        .map(|(_, x)| x.line)
        .collect()
}

/// Where the drive entry's token-server address is named today. The counts are exact: a new site
/// is a failure, and so is one that went away.
const ALLOWED: &[Allow] = &[
    // ---- The data: the entry's field, and where it is made ----
    Allow::exactly(
        "examples/azul-storage/src/config.rs",
        READ,
        1,
        "DriveAuth::Azlin's field: `DriveEntry::azlin()` hands out the drive's id and no address any more \
         (X-DELTA-STRUCT3-11; it was 3: the field, and the pattern and the value of that accessor)",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/bundle.rs",
        READ,
        3,
        "DriveBundle::entry_named fills an empty address with the server the drive came from \
         (a record of where it was made; nothing reads it to send a credential there in a \
         shipped build)",
    ),
    // ---- The choice: where the address becomes a destination ----
    Allow::exactly(
        "examples/azcloud-kit/src/drive.rs",
        READ,
        2,
        "azlin_target_choice: THE resolver - the one place of the apps that reads the entry's address and \
         asks the anchors which server a drive's credentials go to (a shipped build: the run's, whatever \
         the entry says); `open_azlin` and AzDrive's every look go through it (ARCH29 C6d)",
    ),
    // ---- The choice: who asks the anchors which server an address becomes ----
    Allow::exactly(
        "examples/azcloud-kit/src/drive.rs",
        CHOICE,
        2,
        "azlin_target_choice's one call of the anchors, and `azlin_target`'s one call of azlin_target_choice (the \
         bool is the build's `layers_honoured()`); `open_azlin` takes the account's server from its answer, and \
         `AzlinAccount::new` no longer chooses again (X-DELTA-STRUCT3-11, X-DELTA-STRUCT4-5)",
    ),
    Allow::exactly(
        "examples/azul-mail-core/src/azlin.rs",
        CHOICE,
        1,
        "Endpoints::token_url_for_with: AzMail's account has no drive entry; a test build asks the same function \
         a drive's entry is asked, a shipped one answers its own constant (ARCH29 C6d)",
    ),
];

fn rules() -> Vec<Rule> {
    vec![
        Rule::new(
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
        ),
        Rule::new(
            CHOICE,
            choices,
            &[
                "let url = azul_appkit::anchors::token_server_choice(true, named, run);",
                "let url = anchors::token_server_for(named, run);",
                "use azul_appkit::anchors::token_server_choice as pick;",
                "let pick = anchors::token_server_choice;",
                "let target = azcloud_kit::azlin_target_choice(true, &entry, run);",
                "let target = azcloud_kit::drive::azlin_target_choice(true, &entry, None);",
                "use azcloud_kit::drive::azlin_target_choice as aim;",
                "let aim = azlin_target_choice;",
            ],
            &[
                "pub fn token_server_choice<'a>(honour: bool, named: &'a str, run: &'a str) -> &'a str {}",
                "pub(crate) fn azlin_target_choice(honour: bool, entry: &DriveEntry, run: Option<&str>) {}",
                "let token_server_choices = 1;",
                "let azlin_target_choices = 1;",
                "let url = azlin_target(entry, run);",
            ],
        ),
    ]
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
#[ignore = "X-EDGE-C-1: prints the sites; the list above is written from it"]
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

/// X-DELTA-STRUCT3-11. The account chose the token server again in its own constructor
/// (`AzlinAccount::new` asked `token_server_for`), so a later change of the one choice did not reach the account's
/// refresh. Every use of the function that chooses is a counted line now, whatever it is spelled: a third place is
/// a finding with its line, the listed ones are counted, and a site that went away leaves a stale count. RED on the old
/// gate: it read the identifier `account_url` only, and the account's constructor read none.
#[test]
fn a_third_place_that_chooses_the_token_server_is_caught_whatever_its_spelling() {
    const CHOSEN: &[Allow] = &[Allow::exactly(
        "core/src/choice.rs",
        "token-server-choice",
        1,
        "the planted resolver",
    )];
    let planted = [
        "pub fn resolver(entry: &DriveEntry, run: &str) -> String {",
        "    azul_appkit::anchors::token_server_choice(true, \"\", run).to_string()",
        "}",
        "",
    ]
    .join("\n");
    let account = [
        "pub fn new(entry: &DriveEntry, run: &str) -> String {",
        "    let url = azul_appkit::anchors::token_server_for(\"\", run);",
        "    url.to_string()",
        "}",
        "use azul_appkit::anchors::token_server_choice as pick;",
        "pub fn value() { let f = anchors::token_server_choice; }",
        "",
    ]
    .join("\n");
    let base = base_with(
        "choice",
        &[
            ("core/src/lib.rs", "mod choice;\nmod account;\n"),
            ("core/src/choice.rs", planted.as_str()),
            ("core/src/account.rs", account.as_str()),
        ],
    );
    let wrong = check(&base, CHOSEN).expect_err("three more places choose");
    let at = |line: usize| {
        wrong.iter().any(|w| {
            w.starts_with(&format!("core/src/account.rs:{line}:"))
                && w.contains("token-server-choice")
        })
    };
    assert!(at(2), "the call of the twin: {wrong:?}");
    assert!(at(5), "an import under another name: {wrong:?}");
    assert!(at(6), "a function value: {wrong:?}");
    // The resolver itself is the listed line; without the others the gate holds.
    std::fs::write(base.join("core/src/account.rs"), "pub fn new() {}\n").unwrap();
    assert_eq!(check(&base, CHOSEN), Ok(1));
    let _ = std::fs::remove_dir_all(base);
}

/// X-DELTA-STRUCT4-5. The kit's resolver with the build's choice as a `bool`, `azlin_target_choice(true, &entry, ..)`, was
/// a `pub` function: any crate could send a drive token to the address `drives.json` carries with one literal, and no
/// gate saw the call (the gate counted `token_server_choice` and `account_url`). It is crate-private now, and every use of
/// its name is a counted line whatever the spelling - a path call, a path through the module, an import under another name,
/// a function value: a place in any other crate is a finding with its line, the kit's two lines (the one call of the anchors
/// and the one call of the resolver) are the listed ones, and a third in the kit's own file is a new site. RED on the old
/// gate: it found none of them.
#[test]
fn x_delta_struct4_5_a_use_of_the_resolver_with_a_bool_is_a_counted_line_whatever_its_spelling() {
    const RESOLVER: &[Allow] = &[Allow::exactly(
        "core/src/drive.rs",
        "token-server-choice",
        2,
        "the planted resolver: its call of the anchors and `azlin_target`'s call of it",
    )];
    let resolver = [
        "pub fn azlin_target(entry: &DriveEntry, run: Option<&str>) -> Option<AzlinTarget> {",
        "    azlin_target_choice(honoured(), entry, run)",
        "}",
        "pub(crate) fn azlin_target_choice(honour: bool, entry: &DriveEntry, run: Option<&str>) -> Option<AzlinTarget> {",
        "    azul_appkit::anchors::token_server_choice(honour, \"\", \"\");",
        "    None",
        "}",
        "",
    ]
    .join("\n");
    let elsewhere = [
        "pub fn aimed(entry: &DriveEntry) {",
        "    let _ = azcloud_kit::azlin_target_choice(true, entry, None);",
        "}",
        "pub fn through_the_module(entry: &DriveEntry) {",
        "    let _ = azcloud_kit::drive::azlin_target_choice(true, entry, None);",
        "}",
        "use azcloud_kit::drive::azlin_target_choice as aim;",
        "pub fn value() { let _f = azlin_target_choice; }",
        "",
    ]
    .join("\n");
    let base = base_with(
        "bool-resolver",
        &[
            ("core/src/lib.rs", "mod drive;\nmod elsewhere;\n"),
            ("core/src/drive.rs", resolver.as_str()),
            ("core/src/elsewhere.rs", elsewhere.as_str()),
        ],
    );
    let wrong = check(&base, RESOLVER).expect_err("four more places name the resolver");
    let at = |line: usize| {
        wrong.iter().any(|w| {
            w.starts_with(&format!("core/src/elsewhere.rs:{line}:"))
                && w.contains("token-server-choice")
        })
    };
    assert!(at(2), "the call through the crate root: {wrong:?}");
    assert!(at(5), "a path through the module: {wrong:?}");
    assert!(at(7), "an import under another name: {wrong:?}");
    assert!(at(8), "a function value: {wrong:?}");
    // The resolver's own two lines are the listed ones; without the others the gate holds.
    std::fs::write(base.join("core/src/elsewhere.rs"), "pub fn clean() {}\n").unwrap();
    assert_eq!(check(&base, RESOLVER), Ok(2));
    // A third line in the resolver's file is a new site.
    std::fs::write(
        base.join("core/src/drive.rs"),
        format!("{resolver}pub fn third(entry: &DriveEntry) {{\n    let _ = azlin_target_choice(true, entry, None);\n}}\n"),
    )
    .unwrap();
    let wrong = check(&base, RESOLVER).expect_err("a third place in the kit's file");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 3 x token-server-choice where its allowance says 2")),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}
