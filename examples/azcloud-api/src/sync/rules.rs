//! What a sync takes and what it never takes.
//!
//! A rule is a glob in the spirit of `.gitignore`: a pattern without a slash
//! matches a file or folder name at any depth (`*.tmp`, `.DS_Store`); a
//! pattern with a slash at its start or in its middle matches the path from
//! the folder's root (`/.azlin/`, `meet/*/cache/`), `**` standing for any
//! number of folders; a trailing slash matches folders only (and so
//! everything in them). `*` matches within a name, `?` one character.
//!
//! Every sync has [`BASE_RULES`]: temporary and lock files, the operating
//! system's bookkeeping, database journals (a database syncs closed, as its
//! file), version-control internals (PLAN §13.10: a repository syncs through
//! git, never file by file) and `.azlin/` at the root (azul-storage's
//! bookkeeping, the data tree's manifest). The Azlin tree adds
//! [`AZLIN_RULES`]: caches, logs and anything shaped like a secret (secrets
//! live in the OS keyring; this is the second line of defence). A folder's
//! `.azcloudignore` adds its own lines (and syncs, so every device shares
//! them). A rule never deletes anything: an excluded key is neither uploaded,
//! downloaded nor deleted on either side.

/// Why a key is skipped, for the report.
pub const TEMP: &str = "a temporary file";
pub const LOCK: &str = "a lock file";
pub const OS: &str = "the operating system's bookkeeping";
pub const DB_SIDE: &str = "a database's journal (a database syncs as its file, closed)";
pub const VCS: &str = "version-control internals (a repository syncs through git, PLAN §13.10)";
pub const BOOKKEEPING: &str = "the folder's own bookkeeping (.azlin at its root)";
pub const CACHE: &str = "a cache (every computer builds its own)";
pub const LOG: &str = "a log of this computer";
pub const SECRET: &str = "shaped like a secret (secrets live in the OS keyring, PLAN §13.5)";
pub const STATE: &str = "a device's azcloud state (never synced)";
pub const IGNORED: &str = "the folder's .azcloudignore";

/// The file a folder lists its own exclusions in.
pub const IGNORE_FILE: &str = ".azcloudignore";

/// The rules of every sync.
pub const BASE_RULES: &[(&str, &str)] = &[
    ("/.azlin/", BOOKKEEPING),
    ("*.tmp", TEMP),
    ("*.temp", TEMP),
    ("*.partial", TEMP),
    ("*.part", TEMP),
    ("*.crdownload", TEMP),
    ("*.download", TEMP),
    ("*~", TEMP),
    (".#*", TEMP),
    ("*.swp", TEMP),
    ("*.swo", TEMP),
    (".~lock.*#", LOCK),
    ("*.lck", LOCK),
    ("LOCK", LOCK),
    ("lockfile", LOCK),
    ("*.pid", LOCK),
    (".DS_Store", OS),
    ("._*", OS),
    ("Thumbs.db", OS),
    ("thumbs.db", OS),
    ("ehthumbs.db", OS),
    ("desktop.ini", OS),
    (".directory", OS),
    (".Spotlight-V100/", OS),
    (".Trashes/", OS),
    (".fseventsd/", OS),
    (".TemporaryItems/", OS),
    (".DocumentRevisions-V100/", OS),
    ("$RECYCLE.BIN/", OS),
    ("*-wal", DB_SIDE),
    ("*-shm", DB_SIDE),
    ("*-journal", DB_SIDE),
    (".git/", VCS),
    (".hg/", VCS),
    (".svn/", VCS),
];

/// What the Azlin tree (the data root and the `.azlin` folder) adds: no app
/// keeps a `Cargo.lock` there, so every `*.lock` is a lock.
pub const AZLIN_RULES: &[(&str, &str)] = &[
    ("cache/", CACHE),
    ("caches/", CACHE),
    ("Cache/", CACHE),
    ("Caches/", CACHE),
    (".cache/", CACHE),
    (".thumbnails/", CACHE),
    ("tmp/", TEMP),
    ("temp/", TEMP),
    ("*.log", LOG),
    ("logs/", LOG),
    ("*.lock", LOCK),
    ("*.pem", SECRET),
    ("*.p12", SECRET),
    ("*.pfx", SECRET),
    ("*.keychain", SECRET),
    ("*.keychain-db", SECRET),
    ("id_rsa*", SECRET),
    ("id_ecdsa*", SECRET),
    ("id_ed25519*", SECRET),
    (".env", SECRET),
    ("*.secret", SECRET),
    ("secrets.json", SECRET),
    ("credentials.json", SECRET),
];

/// What the `.azlin` folder adds: a state folder someone put there.
pub const HOME_RULES: &[(&str, &str)] = &[("azcloud/", STATE)];

/// One rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// As written.
    pub pattern: String,
    /// Why it excludes.
    pub reason: String,
    /// The pattern's names, split at `/`.
    segments: Vec<String>,
    /// Folders only.
    dir_only: bool,
    /// Matched against the whole path from the root (a slash at its start or
    /// in its middle), else against the last name.
    path: bool,
}

impl Rule {
    /// The rule of `pattern`; `None` for an empty line or a comment (`#`).
    #[must_use]
    pub fn new(pattern: &str, reason: &str) -> Option<Rule> {
        let p = pattern.trim();
        if p.is_empty() || p.starts_with('#') {
            return None;
        }
        let anchored = p.starts_with('/');
        let dir_only = p.ends_with('/');
        let core = p.trim_start_matches('/').trim_end_matches('/');
        if core.is_empty() {
            return None;
        }
        let segments: Vec<String> = core.split('/').map(String::from).collect();
        let path = anchored || segments.len() > 1;
        Some(Rule {
            pattern: p.to_string(),
            reason: reason.to_string(),
            segments,
            dir_only,
            path,
        })
    }

    /// Whether the rule takes the entry whose path from the root is `parts`
    /// (`is_dir`: a folder).
    fn matches(&self, parts: &[&str], is_dir: bool) -> bool {
        if self.dir_only && !is_dir {
            return false;
        }
        if self.path {
            let pattern: Vec<&str> = self.segments.iter().map(String::as_str).collect();
            match_path(&pattern, parts)
        } else {
            parts
                .last()
                .is_some_and(|name| match_name(&self.segments[0], name))
        }
    }
}

/// `*` (any run of characters) and `?` (one character) within one name.
#[must_use]
pub fn match_name(glob: &str, name: &str) -> bool {
    let g: Vec<char> = glob.chars().collect();
    let n: Vec<char> = name.chars().collect();
    let (mut gi, mut ni) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut mark = 0usize;
    while ni < n.len() {
        if gi < g.len() && (g[gi] == '?' || g[gi] == n[ni]) {
            gi += 1;
            ni += 1;
        } else if gi < g.len() && g[gi] == '*' {
            star = Some(gi);
            mark = ni;
            gi += 1;
        } else if let Some(s) = star {
            gi = s + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    while gi < g.len() && g[gi] == '*' {
        gi += 1;
    }
    gi == g.len()
}

/// A path pattern against a path, name by name; `**` stands for any number
/// of names.
fn match_path(pattern: &[&str], parts: &[&str]) -> bool {
    match pattern.split_first() {
        None => parts.is_empty(),
        Some((first, rest)) if *first == "**" => {
            (0..=parts.len()).any(|i| match_path(rest, &parts[i..]))
        }
        Some((first, rest)) => match parts.split_first() {
            Some((part, more)) => match_name(first, part) && match_path(rest, more),
            None => false,
        },
    }
}

/// The rules of one sync.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rules {
    rules: Vec<Rule>,
}

impl Rules {
    /// No rules at all (tests).
    #[must_use]
    pub fn none() -> Rules {
        Rules::default()
    }

    /// The rules of every sync ([`BASE_RULES`]).
    #[must_use]
    pub fn base() -> Rules {
        let mut rules = Rules::default();
        rules.add_table(BASE_RULES);
        rules
    }

    /// The data root's: [`BASE_RULES`] and [`AZLIN_RULES`].
    #[must_use]
    pub fn azlin_data() -> Rules {
        let mut rules = Rules::base();
        rules.add_table(AZLIN_RULES);
        rules
    }

    /// The `.azlin` folder's: the data root's and [`HOME_RULES`].
    #[must_use]
    pub fn azlin_home() -> Rules {
        let mut rules = Rules::azlin_data();
        rules.add_table(HOME_RULES);
        rules
    }

    fn add_table(&mut self, table: &[(&str, &str)]) {
        for (pattern, reason) in table {
            self.add(pattern, reason);
        }
    }

    /// Adds one rule; whether `pattern` was one (not empty, not a comment).
    pub fn add(&mut self, pattern: &str, reason: &str) -> bool {
        match Rule::new(pattern, reason) {
            Some(rule) => {
                self.rules.push(rule);
                true
            }
            None => false,
        }
    }

    /// Adds the lines of a folder's `.azcloudignore`.
    pub fn add_ignore_file(&mut self, text: &str) {
        for line in text.lines() {
            self.add(line, IGNORED);
        }
    }

    /// Why the file `key` is not synced (a rule took it or one of its
    /// folders); `None` = it is.
    #[must_use]
    pub fn excluded(&self, key: &str) -> Option<&str> {
        let parts: Vec<&str> = key.split('/').filter(|s| !s.is_empty()).collect();
        for i in 0..parts.len() {
            let is_dir = i + 1 < parts.len();
            if let Some(rule) = self.rules.iter().find(|r| r.matches(&parts[..=i], is_dir)) {
                return Some(&rule.reason);
            }
        }
        None
    }

    /// Why the folder `dir` (a path from the root, no trailing slash) is
    /// skipped as a whole; `None` = it is walked.
    #[must_use]
    pub fn excluded_dir(&self, dir: &str) -> Option<&str> {
        let parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
        for i in 0..parts.len() {
            if let Some(rule) = self.rules.iter().find(|r| r.matches(&parts[..=i], true)) {
                return Some(&rule.reason);
            }
        }
        None
    }

    /// Every rule as `(pattern, reason)`, for `azcloud sync --explain`.
    #[must_use]
    pub fn list(&self) -> Vec<(String, String)> {
        self.rules
            .iter()
            .map(|r| (r.pattern.clone(), r.reason.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
