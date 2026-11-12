//! What a sync takes and what it never takes.
//!
//! A rule is a glob in the spirit of `.gitignore`: a pattern without a slash
//! matches a file or folder name at any depth (`*.tmp`, `.DS_Store`); a
//! pattern with a slash at its start or in its middle matches the path from
//! the folder's root (`/.azlin/`, `meet/*/cache/`), `**` standing for any
//! number of folders; a trailing slash matches folders only (and so
//! everything in them). `*` matches within a name, `?` one character. A
//! pattern starting with `!` takes back what the rules before it excluded
//! (`node_modules/` then `!keep/node_modules/`): the last rule that matches an
//! entry decides, as in `.gitignore`. A folder excluded keeps everything in it
//! excluded, so a `!` re-includes a folder, not one file inside an excluded
//! one. No `!` takes back the folder's own bookkeeping (`/.azlin/`, a state
//! folder). `\!` starts a pattern with a literal `!`.
//!
//! Every sync has [`BASE_RULES`]: temporary and lock files, the operating
//! system's bookkeeping, database journals (a database syncs closed, as its
//! file), version-control internals (a repository syncs through git, never
//! file by file) and `.azlin/` at the root (azul-storage's
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
pub const VCS: &str = "version-control internals (a repository syncs through git)";
pub const BOOKKEEPING: &str = "the folder's own bookkeeping (.azlin at its root)";
pub const CACHE: &str = "a cache (every computer builds its own)";
pub const LOG: &str = "a log of this computer";
pub const SECRET: &str = "shaped like a secret (secrets live in the OS keyring)";
pub const STATE: &str = "a device's azcloud state (never synced)";
pub const INSTALL: &str = "the installed programs and their update staging (never synced)";
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
    ("*.swx", TEMP),
    // The name "Free up space" takes a file to before it removes it (`take_away`): a kill between the
    // two leaves it there, and it must never go up as a file of the user's (X-DELTA-SYNC4-13).
    (".azul-freeing-*", TEMP),
    // Word's, Excel's and PowerPoint's owner file while a document is open.
    ("~$*", LOCK),
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
    ("System Volume Information/", OS),
    // A Linux volume's trash (one per user id) and fsck's lost files.
    (".Trash-*/", OS),
    ("lost+found/", OS),
    ("*-wal", DB_SIDE),
    ("*-shm", DB_SIDE),
    ("*-journal", DB_SIDE),
    // No trailing slash: a FILE or a link of that name is a repository's too (a `.git` file names
    // the folder git reads, `gitdir: ../x`), so it is left out like the folder
    // (X-DELTA-AZDRIVE-FILES-9). Other spellings of the names are refused by `names::key_problem`.
    (".git", VCS),
    (".hg", VCS),
    (".svn", VCS),
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

/// What the `.azlin` folder adds: a state folder someone put there, and the install tree - the
/// programs the updater installed (`apps/<app>/current/..`, their `staged.json` and
/// `request.json`) and the links to them (`bin/`). What a program runs at its next start is
/// installed by the updater's signature chain and nothing else: a synced file there would be run
/// with no chain involved (AZ-Y5).
pub const HOME_RULES: &[(&str, &str)] =
    &[("azcloud/", STATE), ("/apps/", INSTALL), ("/bin/", INSTALL)];

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
    /// A `!` rule: it takes back what the rules before it excluded.
    negated: bool,
    /// One of the engine's own folders (the bookkeeping, the state, the install tree): matched
    /// without regard to case ([`Rule::matches`]).
    lowercase: bool,
}

impl Rule {
    /// The rule of `pattern`; `None` for an empty line or a comment (`#`).
    #[must_use]
    pub fn new(pattern: &str, reason: &str) -> Option<Rule> {
        let p = pattern.trim();
        if p.is_empty() || p.starts_with('#') {
            return None;
        }
        let negated = p.starts_with('!');
        let body = if negated {
            &p[1..]
        } else {
            p.strip_prefix('\\').filter(|rest| rest.starts_with('!')).unwrap_or(p)
        };
        let anchored = body.starts_with('/');
        let dir_only = body.ends_with('/');
        let core = body.trim_start_matches('/').trim_end_matches('/');
        if core.is_empty() {
            return None;
        }
        // P2: a rule matches in NFC, as a name is compared (a pattern typed composed takes a
        // folder a Mac named decomposed).
        let segments: Vec<String> = core.split('/').map(super::names::nfc).collect();
        let path = anchored || segments.len() > 1;
        let lowercase = !negated && (reason == BOOKKEEPING || reason == STATE || reason == INSTALL);
        Some(Rule {
            pattern: p.to_string(),
            reason: reason.to_string(),
            segments,
            dir_only,
            path,
            negated,
            lowercase,
        })
    }

    /// A rule no `!` takes back: the folder's own bookkeeping.
    fn protected(&self) -> bool {
        !self.negated
            && (self.reason == BOOKKEEPING || self.reason == STATE || self.reason == INSTALL)
    }

    /// Whether the rule takes the entry whose path from the root is `parts`
    /// (`is_dir`: a folder).
    fn matches(&self, parts: &[&str], is_dir: bool) -> bool {
        if self.dir_only && !is_dir {
            return false;
        }
        // The folders the engine keeps for itself (the bookkeeping, the state and the install
        // tree) are those folders in whatever case a disk reads them: `Apps/` is `apps/` on APFS
        // and NTFS, and a synced file there is run with no signature chain
        // (X-DELTA-AZDRIVE-FILES-32; AZ-Y5).
        if self.lowercase {
            return self.matches_lowercase(parts);
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

    /// [`Rule::matches`] with both sides lowercased.
    fn matches_lowercase(&self, parts: &[&str]) -> bool {
        let pattern: Vec<String> = self.segments.iter().map(|s| s.to_lowercase()).collect();
        if self.path {
            let pattern: Vec<&str> = pattern.iter().map(String::as_str).collect();
            let parts: Vec<String> = parts.iter().map(|p| p.to_lowercase()).collect();
            let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
            match_path(&pattern, &parts)
        } else {
            parts
                .last()
                .is_some_and(|name| match_name(&pattern[0], &name.to_lowercase()))
        }
    }
}

/// `key` in NFC, borrowed when it is already (most names are).
fn in_nfc(key: &str) -> std::borrow::Cow<'_, str> {
    use unicode_normalization::{is_nfc_quick, IsNormalized};
    if is_nfc_quick(key.chars()) == IsNormalized::Yes {
        std::borrow::Cow::Borrowed(key)
    } else {
        std::borrow::Cow::Owned(super::names::nfc(key))
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

    /// The rule that excludes the entry `parts` (`is_dir`: a folder) itself,
    /// its folders left out: the first rule that takes it after the last `!`
    /// that took it back (a protected rule stays whatever comes after it).
    fn verdict(&self, parts: &[&str], is_dir: bool) -> Option<&Rule> {
        let mut hit: Option<&Rule> = None;
        for rule in self.rules.iter().filter(|r| r.matches(parts, is_dir)) {
            if rule.negated {
                if hit.is_some_and(|h| !h.protected()) {
                    hit = None;
                }
            } else if hit.is_none() {
                hit = Some(rule);
            }
        }
        hit
    }

    /// Why the file `key` is not synced (a rule took it or one of its
    /// folders); `None` = it is.
    #[must_use]
    pub fn excluded(&self, key: &str) -> Option<&str> {
        let key = in_nfc(key);
        let parts: Vec<&str> = key.split('/').filter(|s| !s.is_empty()).collect();
        for i in 0..parts.len() {
            let is_dir = i + 1 < parts.len();
            if let Some(rule) = self.verdict(&parts[..=i], is_dir) {
                return Some(&rule.reason);
            }
        }
        None
    }

    /// Why the folder `dir` (a path from the root, no trailing slash) is
    /// skipped as a whole; `None` = it is walked.
    #[must_use]
    pub fn excluded_dir(&self, dir: &str) -> Option<&str> {
        let dir = in_nfc(dir);
        let parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
        for i in 0..parts.len() {
            if let Some(rule) = self.verdict(&parts[..=i], true) {
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
