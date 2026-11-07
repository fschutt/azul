//! The config bag's gate (ARCH28 RF54, Z-20): what the shared config file lets a same-user
//! process do to the apps, and what leaves the machine from a form.
//!
//! - ONE WRITER. `~/.azlin/config.json` holds the pinned roots, the OAuth client secrets and the
//!   update choices; a writer that did not read what the file says NOW undid the manager's and the
//!   apps' changes in turn, and a writer that took a typo for "no config" erased what the user
//!   typed (AZ-CN1, AZ-CN16). `AzlinConfig::edit` is the only writer: it locks, reads, refuses a
//!   file that is no config, and puts the result in place durably. There is no `store`, `save` or
//!   `update`.
//! - A TLS DROP-DOWN OPENS ON THE WORD THAT VERIFIES. A database form left untouched must check the
//!   server's certificate and name; the opportunistic words are chosen on purpose (AZ-CN2).
//! - A TEXT THAT LEAVES THE PROCESS HOLDS NO SECRET. The golden cases of `scrub_url`: pCloud's
//!   password in the query, WebHDFS's delegation token, `user:password@` typed into an address
//!   (AZ-CN3).

use azul_storage::{
    catalog::{services, FieldKind},
    scrub::{scrub_url, HIDDEN},
};

fn source(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The part of a file the shipped build compiles: its tests come last.
fn shipped(text: &str) -> &str {
    text.split("\n#[cfg(test)]").next().unwrap_or(text)
}

/// The text from `start` up to `end` (both found in `text`).
fn between<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
    let from = text
        .find(start)
        .unwrap_or_else(|| panic!("`{start}` is not in the file"));
    let rest = &text[from..];
    let to = rest
        .find(end)
        .unwrap_or_else(|| panic!("`{end}` does not follow `{start}`"));
    &rest[..to]
}

#[test]
fn the_config_file_is_written_by_edit_and_by_nothing_else() {
    let file = source("azlin_config.rs");
    let code = shipped(&file);

    // One place puts the file in place, and it is private.
    assert_eq!(
        code.matches("durable::replace(").count(),
        1,
        "a second place writes the config"
    );
    assert!(
        code.contains("    fn write(&self, path: &Path) -> std::io::Result<()> {"),
        "the write that puts the config in place is no longer private to the type"
    );
    // ... and `edit` is the only caller.
    let edit = between(code, "    pub fn edit(", "    pub fn set_look(");
    assert_eq!(
        edit.matches("config.write(path)").count(),
        1,
        "edit does not write the config it made"
    );
    assert_eq!(
        code.matches("config.write(path)").count(),
        1,
        "the config is written from somewhere besides edit"
    );
    // edit locks first and refuses a file that is no config.
    assert!(
        edit.contains("AzlinConfig::lock(path)?"),
        "edit does not take the file's lock"
    );
    assert!(
        edit.contains("ConfigState::Broken"),
        "edit would overwrite a file that is no config"
    );
    // No other way in: there is no store, save or update.
    for name in [
        "pub fn store(",
        "pub fn save(",
        "pub fn update(",
        "pub fn write(",
        "pub fn write_to(",
        "pub fn save_to(",
    ] {
        assert!(!code.contains(name), "`{name}` is a second writer");
    }
}

#[test]
fn only_the_look_leaves_the_computer_with_the_synced_folder() {
    let file = source("azlin_config.rs");
    let code = shipped(&file);
    // The keys that sync are a short allow-list, not a deny-list that grows a hole with every
    // section added (AZ-CN13): theme and mode.
    let list = between(code, "pub const SYNCED_KEYS", ";");
    assert!(
        list.contains("THEME_KEY") && list.contains("MODE_KEY"),
        "{list}"
    );
    assert_eq!(
        list.matches("KEY").count(),
        3,
        "a key joined the list that leaves this computer: {list}"
    );
    for secret_section in ["oauth", "endpoints", "updates", "trust"] {
        assert!(
            !azul_appkit::azlin_config::is_synced(secret_section),
            "{secret_section} syncs"
        );
    }
}

/// AZ-CN2: every TLS drop-down of the source catalog opens on a word that verifies the server, and
/// it is the first of its list; a word that encrypts without verifying is chosen on purpose.
#[test]
fn every_tls_drop_down_of_the_source_catalog_opens_on_the_word_that_verifies() {
    let mut seen = 0;
    for service in services() {
        for field in service.fields {
            let FieldKind::Choice(words) = field.kind else {
                continue;
            };
            if !(field.key.contains("ssl") || field.key.contains("tls")) {
                continue;
            }
            seen += 1;
            assert!(
                field.default.starts_with("verify"),
                "{}.{} opens on {:?}, which does not verify the server",
                service.id,
                field.key,
                field.default
            );
            assert_eq!(
                words.first().copied(),
                Some(field.default),
                "{}.{}: the default is first in the list",
                service.id,
                field.key
            );
            let last = words.last().copied().unwrap_or_default();
            assert!(
                matches!(last, "prefer" | "preferred"),
                "{}.{}: the opportunistic word goes last, {last:?}",
                service.id,
                field.key
            );
        }
    }
    assert!(
        seen >= 2,
        "PostgreSQL and MySQL have a TLS drop-down each: {seen}"
    );
}

/// AZ-CN3: the golden cases.
#[test]
fn scrub_url_hides_the_password_the_token_and_the_user_info_of_a_url() {
    let cases = [
        // pCloud puts the password in the query of every request.
        (
            "Connection failed: https://api.pcloud.com/listfolder?path=/&username=ann&password=hunter2",
            "Connection failed: https://api.pcloud.com/listfolder?<hidden>",
            "hunter2",
        ),
        // WebHDFS its delegation token.
        (
            "https://nn.example:9871/webhdfs/v1/data?op=LISTSTATUS&delegation=T0K3N123",
            "https://nn.example:9871/webhdfs/v1/data?<hidden>",
            "T0K3N123",
        ),
        // A user typed `user:password@` into an address.
        (
            "tls handshake with https://ann:pw-of-ann@cloud.example/dav failed",
            "tls handshake with https://cloud.example/dav failed",
            "pw-of-ann",
        ),
        // A pair outside a URL.
        ("password=hunter2 given", "password=<hidden> given", "hunter2"),
    ];
    for (text, expected, secret) in cases {
        let said = scrub_url(text);
        assert_eq!(said, expected, "{text}");
        assert!(!said.contains(secret), "{said}");
        assert_eq!(scrub_url(&said), said, "scrubbing again changes nothing");
    }
    assert!(HIDDEN.starts_with('<'));
    // Text without a URL or a pair is as it was.
    assert_eq!(
        scrub_url("the name did not resolve"),
        "the name did not resolve"
    );
}
