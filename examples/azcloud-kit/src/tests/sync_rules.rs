//! What a sync takes and what it never takes: name and path globs, the base rules, the Azlin
//! tree's, a folder's `.azcloudignore`.

use crate::sync::rules::{match_name, Rules, BOOKKEEPING, CACHE, IGNORED, LOG, OS, STATE, TEMP};

#[test]
fn a_name_glob_takes_stars_and_question_marks() {
    assert!(match_name("*.tmp", "a.tmp"));
    assert!(match_name("*.tmp", ".tmp"));
    assert!(!match_name("*.tmp", "a.tmpx"));
    assert!(match_name("id_rsa*", "id_rsa.pub"));
    assert!(match_name(".~lock.*#", ".~lock.Book1.xlsx#"));
    assert!(match_name("a?c", "abc"));
    assert!(!match_name("a?c", "ac"));
    assert!(match_name("*", ""));
    assert!(match_name("*-wal", "notes.db-wal"));
    assert!(match_name("**", "anything"));
}

#[test]
fn a_name_rule_takes_any_depth_and_a_path_rule_starts_at_the_root() {
    let mut rules = Rules::none();
    rules.add("*.tmp", TEMP);
    rules.add("/.azlin/", BOOKKEEPING);
    rules.add("meet/*/cache/", CACHE);
    rules.add("logs/", LOG);
    rules.add("docs/**/draft.md", "drafts");
    assert_eq!(rules.excluded("a/b/c.tmp"), Some(TEMP));
    assert_eq!(rules.excluded(".azlin/cache"), Some(BOOKKEEPING));
    assert_eq!(
        rules.excluded("notes/.azlin/cache"),
        None,
        "only at the root"
    );
    assert_eq!(rules.excluded("meet/ab1/cache/x.bin"), Some(CACHE));
    assert_eq!(rules.excluded("meet/cache/x.bin"), None);
    assert_eq!(rules.excluded("music/logs/a.txt"), Some(LOG));
    assert_eq!(
        rules.excluded("music/logs"),
        None,
        "a folder rule never takes a file of that name"
    );
    assert_eq!(rules.excluded("docs/draft.md"), Some("drafts"));
    assert_eq!(rules.excluded("docs/a/b/draft.md"), Some("drafts"));
    assert_eq!(rules.excluded_dir("music/logs"), Some(LOG));
    assert_eq!(rules.excluded_dir("music"), None);
}

#[test]
fn the_base_rules_keep_temporary_lock_os_journal_and_repository_files_out() {
    let rules = Rules::base();
    for key in [
        ".azlin/cache",
        "notes/.a.md.azul-storage-12-0.tmp",
        "notes/.config.json.azcloud-9-1.tmp",
        "sheets/.~lock.Book1.xlsx#",
        ".DS_Store",
        "photos/._IMG_1.jpg",
        "db/app.sqlite-wal",
        "code/proj/.git/HEAD",
        "dl/movie.mkv.part",
    ] {
        assert!(rules.excluded(key).is_some(), "{key}");
    }
    for key in [
        "notes/Notes/a.md",
        "code/proj/Cargo.lock",
        "calculator/history.jsonl",
        "notes/.history/4b07.json",
        "keys/vaults/abc.azkv",
        ".azcloudignore",
    ] {
        assert_eq!(rules.excluded(key), None, "{key}");
    }
}

#[test]
fn the_azlin_tree_also_keeps_caches_logs_locks_and_secret_shaped_files_out() {
    let rules = Rules::azlin_data();
    for key in [
        "music/cache/cover.jpg",
        "reader/books/x/.cache/page1.png",
        "term/session.log",
        "mail/acct/logs/sync.txt",
        "app/state.lock",
        "mail/dkim.pem",
        "code/.env",
        "x/id_ed25519",
        "drive/credentials.json",
    ] {
        assert!(rules.excluded(key).is_some(), "{key}");
    }
    assert_eq!(rules.excluded("music/library.json"), None);
    assert_eq!(rules.excluded("contacts/ab.vcf"), None);
    let home = Rules::azlin_home();
    assert_eq!(home.excluded("azcloud/secrets.json"), Some(STATE));
    assert_eq!(home.excluded("config.json"), None);
}

#[test]
fn an_ignore_file_adds_its_lines_and_skips_comments() {
    let mut rules = Rules::base();
    rules.add_ignore_file("# exports are rebuilt\nsheets/exports/\n\n*.bak\n");
    assert_eq!(rules.excluded("sheets/exports/Book1.pdf"), Some(IGNORED));
    assert_eq!(rules.excluded("a/b.bak"), Some(IGNORED));
    assert_eq!(rules.excluded("sheets/b.xlsx"), None);
    assert!(!rules.add("# just a comment", IGNORED));
    assert!(!rules.add("/", IGNORED));
}

/// P11 names a Linux volume's trash (`.Trash-<uid>`) and fsck's `lost+found` among the
/// systems' bookkeeping, and the base rules had neither: a pairing rooted at a volume sent
/// both up (KDRIVE-007).
#[test]
fn a_linux_volume_trash_and_lost_and_found_never_sync() {
    let rules = Rules::base();
    assert_eq!(rules.excluded(".Trash-1000/files/old.txt"), Some(OS));
    assert_eq!(rules.excluded("disk/.Trash-0/info/a.trashinfo"), Some(OS));
    assert_eq!(rules.excluded("lost+found/#123"), Some(OS));
    assert_eq!(rules.excluded_dir("lost+found"), Some(OS));
    // Only the folders: a file of such a name is a user's.
    assert_eq!(rules.excluded(".Trash-notes.txt"), None);
    assert_eq!(rules.excluded("lost+found"), None);
}

/// A `!` line took nothing back: `!keep/node_modules/` was read as a folder named
/// "!keep", so the re-included subtree never synced (FILEN-022). The last rule that
/// matches decides, as in `.gitignore`.
#[test]
fn a_negated_rule_takes_back_what_the_rules_before_it_excluded() {
    let mut rules = Rules::base();
    rules.add_ignore_file("node_modules/\n!keep/node_modules/\n");
    assert_eq!(rules.excluded("node_modules/x.js"), Some(IGNORED));
    assert_eq!(rules.excluded("app/node_modules/x.js"), Some(IGNORED));
    assert_eq!(rules.excluded("keep/node_modules/y.js"), None);
    assert_eq!(rules.excluded_dir("keep/node_modules"), None);
    // A rule after the `!` excludes again.
    rules.add_ignore_file("keep/node_modules/*.map\n");
    assert_eq!(rules.excluded("keep/node_modules/y.js.map"), Some(IGNORED));
    // A base rule is taken back too ...
    let mut logs = Rules::azlin_data();
    logs.add_ignore_file("!keep.log\n");
    assert_eq!(logs.excluded("keep.log"), None);
    assert_eq!(logs.excluded("other.log"), Some(LOG));
}

/// ... but never the folder's own bookkeeping, and a file inside an excluded folder
/// stays excluded (the folder is not walked).
#[test]
fn no_negated_rule_takes_back_the_bookkeeping_or_a_file_in_an_excluded_folder() {
    let mut rules = Rules::base();
    rules.add_ignore_file("!/.azlin/\n!.azlin/index.json\nbuild/\n!build/keep.txt\n");
    assert_eq!(rules.excluded(".azlin/index.json"), Some(BOOKKEEPING));
    assert_eq!(rules.excluded_dir(".azlin"), Some(BOOKKEEPING));
    assert_eq!(rules.excluded("build/keep.txt"), Some(IGNORED));
    let mut home = Rules::azlin_home();
    home.add_ignore_file("!azcloud/\n");
    assert_eq!(home.excluded("azcloud/secrets.json"), Some(STATE));
}

/// `\!` is a literal `!` at a pattern's start, not a negation.
#[test]
fn an_escaped_bang_is_a_name_that_starts_with_it() {
    let mut rules = Rules::none();
    rules.add_ignore_file("\\!important.txt\n");
    assert_eq!(rules.excluded("!important.txt"), Some(IGNORED));
    assert_eq!(rules.excluded("important.txt"), None);
}
