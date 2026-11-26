//! What the repository says about the code, read from the files of `.git` (never by running git): the hash of HEAD,
//! the branch, the remote's address and its web form, the name that is configured; anything odd is nothing. No window,
//! no azul; folders made for the test.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use super::*;

const HASH: &str = "0123456789abcdef0123456789abcdef01234567";
const OTHER: &str = "fedcba9876543210fedcba9876543210fedcba98";

/// A folder of its own for a test.
fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("azreview-git-{tag}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("a folder");
    dir
}

fn write(path: &std::path::Path, text: &str) {
    fs::create_dir_all(path.parent().expect("a folder")).expect("made");
    fs::write(path, text).expect("written");
}

/// A working tree with a `.git` folder: HEAD on branch `main` at `HASH`, a remote and a name.
fn repo(tag: &str) -> PathBuf {
    let root = scratch(tag);
    write(&root.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(&root.join(".git/refs/heads/main"), &format!("{HASH}\n"));
    write(
        &root.join(".git/config"),
        "[core]\n\trepositoryformatversion = 0\n[remote \"origin\"]\n\turl = https://github.com/fschutt/azul.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n[user]\n\tname = Ada Lovelace\n\temail = ada@example.org\n",
    );
    root
}

fn done(root: &std::path::Path) {
    let _gone = fs::remove_dir_all(root);
}

#[test]
fn a_branch_at_a_hash_is_read_with_its_remote_in_both_forms_and_the_name_but_never_the_email() {
    let root = repo("branch");
    let facts = read(&root, None).expect("a repository");
    assert_eq!(facts.rev.as_deref(), Some(HASH));
    assert_eq!(facts.branch.as_deref(), Some("main"));
    assert_eq!(facts.remote_url.as_deref(), Some("https://github.com/fschutt/azul.git"));
    assert_eq!(facts.web_url.as_deref(), Some("https://github.com/fschutt/azul"));
    assert_eq!(facts.host, Host::GitHub);
    assert_eq!(facts.author.as_deref(), Some("Ada Lovelace"));
    let shown = format!("{facts:?}");
    assert!(!shown.contains("ada@example.org"), "the email is not read: {shown}");
    done(&root);
}

#[test]
fn a_folder_below_the_working_tree_finds_it_and_a_folder_with_no_repository_is_nothing() {
    let root = repo("below");
    let deep = root.join("src/a/b");
    fs::create_dir_all(&deep).expect("made");
    assert_eq!(read(&deep, None).and_then(|f| f.rev), Some(HASH.to_string()));
    let lone = scratch("lone");
    assert_eq!(read(&lone, None), None);
    done(&root);
    done(&lone);
}

#[test]
fn a_detached_head_is_a_hash_and_no_branch_and_a_hash_that_is_none_is_no_hash() {
    let root = repo("detached");
    write(&root.join(".git/HEAD"), &format!("{OTHER}\n"));
    let facts = read(&root, None).expect("a repository");
    assert_eq!((facts.rev.as_deref(), facts.branch.as_deref()), (Some(OTHER), None));
    // Odd contents: no hash, and no panic.
    for odd in ["", "ref: ", "ref: refs/heads/nowhere\n", "ref: refs/heads/main extra words\n", "zzzz\n", "0123\n", "ref: ../../etc/passwd\n"] {
        write(&root.join(".git/HEAD"), odd);
        let facts = read(&root, None).expect("still a repository");
        assert_eq!(facts.rev, None, "{odd:?}");
    }
    // A hash that is not hex is no hash.
    write(&root.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(&root.join(".git/refs/heads/main"), "not a hash at all\n");
    assert_eq!(read(&root, None).and_then(|f| f.rev), None);
    done(&root);
}

#[test]
fn a_ref_that_is_only_in_packed_refs_is_found() {
    let root = repo("packed");
    fs::remove_file(root.join(".git/refs/heads/main")).expect("removed");
    write(
        &root.join(".git/packed-refs"),
        &format!("# pack-refs with: peeled fully-peeled sorted\n{OTHER} refs/heads/other\n{HASH} refs/heads/main\n^{OTHER}\n"),
    );
    assert_eq!(read(&root, None).and_then(|f| f.rev), Some(HASH.to_string()));
    done(&root);
}

#[test]
fn a_git_file_that_points_to_a_gitdir_is_followed_for_worktrees_and_submodules() {
    let root = scratch("worktree");
    let real = scratch("realgit");
    write(&real.join("HEAD"), "ref: refs/heads/topic\n");
    write(&real.join("refs/heads/topic"), &format!("{OTHER}\n"));
    write(&real.join("config"), "[remote \"origin\"]\n\turl = git@github.com:fschutt/azul.git\n");
    write(&root.join(".git"), &format!("gitdir: {}\n", real.display()));
    let facts = read(&root, None).expect("a repository");
    assert_eq!((facts.rev.as_deref(), facts.branch.as_deref()), (Some(OTHER), Some("topic")));
    assert_eq!(facts.web_url.as_deref(), Some("https://github.com/fschutt/azul"), "the scp form becomes the web address");
    // A relative gitdir is relative to the working tree.
    write(&root.join(".git"), "gitdir: ../relative-git\n");
    let beside = root.parent().expect("a parent").join("relative-git");
    write(&beside.join("HEAD"), &format!("{HASH}\n"));
    assert_eq!(read(&root, None).and_then(|f| f.rev), Some(HASH.to_string()));
    // One that points nowhere is no repository's HEAD.
    write(&root.join(".git"), "gitdir: /nowhere/at/all\n");
    assert_eq!(read(&root, None).and_then(|f| f.rev), None);
    done(&root);
    done(&real);
    done(&beside);
}

#[test]
fn a_linked_worktree_reads_its_own_head_and_the_shared_refs_and_config() {
    let root = scratch("linked");
    let common = scratch("common");
    let own = common.join("worktrees/wt1");
    write(&own.join("HEAD"), "ref: refs/heads/feature\n");
    write(&own.join("commondir"), "../..\n");
    write(&common.join("refs/heads/feature"), &format!("{HASH}\n"));
    write(&common.join("config"), "[remote \"origin\"]\n\turl = https://gitlab.com/group/sub/project.git\n[user]\n\tname = Grace\n");
    write(&root.join(".git"), &format!("gitdir: {}\n", own.display()));
    let facts = read(&root, None).expect("a repository");
    assert_eq!((facts.rev.as_deref(), facts.branch.as_deref()), (Some(HASH), Some("feature")));
    assert_eq!(facts.web_url.as_deref(), Some("https://gitlab.com/group/sub/project"));
    assert_eq!((facts.host, facts.author.as_deref()), (Host::GitLab, Some("Grace")));
    done(&root);
    done(&common);
}

#[test]
fn the_name_comes_from_the_repository_first_then_the_global_config_and_the_email_never() {
    let root = repo("names");
    let home = scratch("home");
    write(&home.join(".gitconfig"), "[user]\n\tname = Global Name\n\temail = g@example.org\n");
    assert_eq!(read(&root, Some(&home)).and_then(|f| f.author), Some("Ada Lovelace".to_string()), "the repository's own wins");
    write(&root.join(".git/config"), "[remote \"origin\"]\n\turl = https://github.com/a/b\n");
    assert_eq!(read(&root, Some(&home)).and_then(|f| f.author), Some("Global Name".to_string()));
    assert_eq!(read(&root, None).and_then(|f| f.author), None, "no home, no global name");
    // Quoted values and other spellings.
    write(&root.join(".git/config"), "[USER]\n  NAME = \"Quoted Name\"  \n# comment\n; another\n");
    assert_eq!(read(&root, None).and_then(|f| f.author), Some("Quoted Name".to_string()));
    done(&root);
    done(&home);
}

#[test]
fn the_remote_is_written_as_the_web_address_without_credentials_whatever_form_it_has() {
    let cases = [
        ("https://github.com/fschutt/azul.git", "https://github.com/fschutt/azul", Host::GitHub),
        ("https://github.com/fschutt/azul", "https://github.com/fschutt/azul", Host::GitHub),
        ("https://github.com/fschutt/azul/", "https://github.com/fschutt/azul", Host::GitHub),
        ("https://user:secret@github.com/fschutt/azul.git", "https://github.com/fschutt/azul", Host::GitHub),
        ("https://x-access-token:abc123@github.com/fschutt/azul.git", "https://github.com/fschutt/azul", Host::GitHub),
        ("git@github.com:fschutt/azul.git", "https://github.com/fschutt/azul", Host::GitHub),
        ("ssh://git@github.com/fschutt/azul.git", "https://github.com/fschutt/azul", Host::GitHub),
        ("ssh://git@gitlab.example.com:2222/team/app.git", "https://gitlab.example.com/team/app", Host::GitLab),
        ("git://github.com/fschutt/azul.git", "https://github.com/fschutt/azul", Host::GitHub),
        ("git@bitbucket.org:team/app.git", "https://bitbucket.org/team/app", Host::Bitbucket),
        ("https://git.example.org/some/where.git", "https://git.example.org/some/where", Host::Other),
        ("https://codeberg.org/o/r.git", "https://codeberg.org/o/r", Host::Gitea),
        ("git@forgejo.example.net:o/r.git", "https://forgejo.example.net/o/r", Host::Gitea),
    ];
    for (given, web, host) in cases {
        assert_eq!(web_address(given), Some((web.to_string(), host)), "{given}");
    }
    for no in ["", "/home/me/code/azul", "file:///home/me/azul", "../azul", "C:\\\\code\\\\azul", "https://", "https://github.com", "git@github.com", "not a url at all"] {
        assert_eq!(web_address(no), None, "{no:?}");
    }
}

#[test]
fn a_permalink_names_the_revision_the_file_and_the_lines_in_the_form_of_each_host() {
    let gh = "https://github.com/fschutt/azul";
    assert_eq!(
        permalink(gh, Host::GitHub, HASH, "core/src/dom.rs", 456, 470, None),
        Some(format!("https://github.com/fschutt/azul/blob/{HASH}/core/src/dom.rs#L456-L470"))
    );
    assert_eq!(permalink(gh, Host::GitHub, HASH, "a.rs", 7, 7, None), Some(format!("{gh}/blob/{HASH}/a.rs#L7")), "one line is one line");
    assert_eq!(
        permalink("https://gitlab.com/g/p", Host::GitLab, HASH, "src/x.rs", 3, 9, None),
        Some(format!("https://gitlab.com/g/p/-/blob/{HASH}/src/x.rs#L3-9"))
    );
    assert_eq!(
        permalink("https://bitbucket.org/t/a", Host::Bitbucket, HASH, "src/x.rs", 3, 9, None),
        Some(format!("https://bitbucket.org/t/a/src/{HASH}/src/x.rs#lines-3:9"))
    );
    assert_eq!(
        permalink("https://codeberg.org/o/r", Host::Gitea, HASH, "src/x.rs", 3, 9, None),
        Some(format!("https://codeberg.org/o/r/src/commit/{HASH}/src/x.rs#L3-L9"))
    );
    // Another host: the template of the settings (the words in it are the web address, the revision, the path, the
    // lines), and without one the GitHub form.
    let other = "https://git.example.org/some/where";
    assert_eq!(permalink(other, Host::Other, HASH, "a.rs", 1, 2, None), Some(format!("{other}/blob/{HASH}/a.rs#L1-L2")));
    assert_eq!(
        permalink(other, Host::Other, HASH, "a.rs", 1, 2, Some("{web}/raw/{rev}/{path}?from={start}&to={end}")),
        Some(format!("{other}/raw/{HASH}/a.rs?from=1&to=2"))
    );
    // Paths are written so that a link works: spaces and the like are escaped, the slashes stay.
    assert_eq!(
        permalink(gh, Host::GitHub, HASH, "docs/a b/c#d?.md", 1, 1, None),
        Some(format!("{gh}/blob/{HASH}/docs/a%20b/c%23d%3F.md#L1"))
    );
    // Lines the wrong way round, and no revision, are no link.
    assert_eq!(permalink(gh, Host::GitHub, HASH, "a.rs", 9, 3, None), Some(format!("{gh}/blob/{HASH}/a.rs#L3-L9")), "put in order");
    assert_eq!(permalink(gh, Host::GitHub, "", "a.rs", 1, 2, None), None);
    assert_eq!(permalink(gh, Host::GitHub, "not-a-hash", "a.rs", 1, 2, None), None);
    // A path that leaves the repository, or is empty, is none.
    assert_eq!(permalink(gh, Host::GitHub, HASH, "../secret", 1, 2, None), None);
    assert_eq!(permalink(gh, Host::GitHub, HASH, "", 1, 2, None), None);
}

#[test]
fn what_is_read_is_bounded_a_huge_head_or_config_is_not_read_to_the_end() {
    let root = repo("huge");
    let big = "x".repeat(10 * 1024 * 1024);
    write(&root.join(".git/config"), &format!("[user]\n\tname = {big}\n"));
    let facts = read(&root, None).expect("a repository");
    assert_eq!(facts.rev.as_deref(), Some(HASH), "the hash does not depend on the config");
    assert_eq!(facts.author, None, "a name that is far too long is none");
    write(&root.join(".git/HEAD"), &big);
    assert_eq!(read(&root, None).and_then(|f| f.rev), None);
    done(&root);
}

#[test]
fn a_remote_with_a_password_or_a_token_in_it_is_kept_without_it() {
    let root = repo("secret");
    write(&root.join(".git/config"), "[remote \"origin\"]\n\turl = https://user:s3cret@github.com/fschutt/azul.git\n");
    let facts = read(&root, None).expect("a repository");
    assert_eq!(facts.remote_url.as_deref(), Some("https://github.com/fschutt/azul.git"));
    assert!(!format!("{facts:?}").contains("s3cret"));
    assert_eq!(public_remote("https://tok@host.example/a/b"), "https://host.example/a/b");
    assert_eq!(public_remote("git@github.com:a/b.git"), "git@github.com:a/b.git", "an ssh user is no secret");
    assert_eq!(public_remote("ssh://git@github.com/a/b.git"), "ssh://git@github.com/a/b.git");
    done(&root);
}

#[test]
fn the_first_remote_is_the_one_when_none_is_called_origin() {
    let root = repo("remotes");
    write(&root.join(".git/config"), "[remote \"upstream\"]\n\turl = https://github.com/up/stream\n[remote \"fork\"]\n\turl = https://github.com/me/fork\n");
    assert_eq!(read(&root, None).and_then(|f| f.web_url).as_deref(), Some("https://github.com/up/stream"));
    write(&root.join(".git/config"), "[remote \"fork\"]\n\turl = https://github.com/me/fork\n[remote \"origin\"]\n\turl = https://github.com/the/origin\n");
    assert_eq!(read(&root, None).and_then(|f| f.web_url).as_deref(), Some("https://github.com/the/origin"), "origin wins");
    write(&root.join(".git/config"), "[core]\n\tbare = false\n");
    let facts = read(&root, None).expect("a repository");
    assert_eq!((facts.remote_url, facts.web_url, facts.host), (None, None, Host::Other), "no remote: only the hash and the path can be said");
    done(&root);
}

#[test]
fn the_output_of_git_remote_v_gives_each_remote_once_with_its_fetch_address_and_no_secret() {
    let out = "origin\thttps://user:tok3n@github.com/fschutt/azul.git (fetch)\n\
               origin\thttps://user:tok3n@github.com/fschutt/azul.git (push)\n\
               fork\tgit@github.com:me/azul.git (push)\n\
               fork\tgit@github.com:me/azul.git (fetch)\n\
               odd\n\
               \n\
               pushonly\thttps://example.org/a/b (push)\n";
    let remotes = parse_remote_v(out);
    assert_eq!(
        remotes,
        vec![
            ("origin".to_string(), "https://github.com/fschutt/azul.git".to_string()),
            ("fork".to_string(), "git@github.com:me/azul.git".to_string()),
            ("pushonly".to_string(), "https://example.org/a/b".to_string()),
        ]
    );
    assert!(!format!("{remotes:?}").contains("tok3n"));
    assert_eq!(parse_remote_v(""), Vec::new());
    assert_eq!(parse_remote_v("garbage without a tab"), Vec::new());
    // origin is the one; else the first; the person's choice first.
    assert_eq!(choose_remote(&remotes, None).map(|r| r.0.as_str()), Some("origin"));
    assert_eq!(choose_remote(&remotes, Some("fork")).map(|r| r.0.as_str()), Some("fork"));
    assert_eq!(choose_remote(&remotes, Some("nope")).map(|r| r.0.as_str()), Some("origin"));
    let no_origin = vec![("up".to_string(), "https://a.b/c/d".to_string()), ("fork".to_string(), "https://a.b/e/f".to_string())];
    assert_eq!(choose_remote(&no_origin, None).map(|r| r.0.as_str()), Some("up"));
    assert_eq!(choose_remote(&[], None), None);
}

/// A program of its own in a folder of its own: a script (Unix) that prints what the test wants git to print.
#[cfg(unix)]
fn fake_git(tag: &str, script: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch(tag);
    let path = dir.join("git");
    fs::write(&path, format!("#!/bin/sh\n{script}\n")).expect("written");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("executable");
    path
}

#[cfg(unix)]
#[test]
fn git_is_found_by_looking_at_files_and_the_stub_of_a_mac_without_the_tools_is_not_taken() {
    let with = fake_git("found", "exit 0");
    let dir = with.parent().expect("a folder").to_path_buf();
    let empty = scratch("empty");
    assert_eq!(find_git(&[empty.clone(), dir.clone()], true), Some(with.clone()), "the first folder that has it");
    assert_eq!(find_git(&[empty.clone()], true), None);
    // A file that is not a program is not git.
    let plain = scratch("plain");
    fs::write(plain.join("git"), "not a program").expect("written");
    assert_eq!(find_git(&[plain.clone()], true), None);
    // /usr/bin/git without the tools is skipped for the next one; with them it counts (when it is there).
    let stub = [PathBuf::from("/usr/bin"), dir];
    if is_program(Path::new("/usr/bin/git")) {
        assert_eq!(find_git(&stub, true), Some(PathBuf::from("/usr/bin/git")));
        assert_eq!(find_git(&stub, false), Some(with.clone()));
    }
    done(with.parent().expect("a folder"));
    done(&empty);
    done(&plain);
}

#[cfg(unix)]
#[test]
fn git_remote_v_is_run_once_with_no_prompt_and_its_words_are_taken_and_a_slow_or_failing_git_is_none() {
    let root = repo("run");
    let out = "printf 'origin\\thttps://github.com/fschutt/azul.git (fetch)\\norigin\\thttps://github.com/fschutt/azul.git (push)\\n'";
    let git = fake_git("run", out);
    let said = run_remote_v(&git, &root, Duration::from_secs(5)).expect("it answered");
    assert_eq!(parse_remote_v(&said), vec![("origin".to_string(), "https://github.com/fschutt/azul.git".to_string())]);
    // It is told not to ask, not to lock and not to page; it runs in the working tree.
    let env = fake_git("env", "echo \"$GIT_TERMINAL_PROMPT $GIT_OPTIONAL_LOCKS $PWD $*\"");
    let said = run_remote_v(&env, &root, Duration::from_secs(5)).expect("it answered");
    assert!(said.starts_with("0 0 "), "{said}");
    assert!(said.contains("--no-pager remote -v"), "{said}");
    // One that does not end in time is killed and is none (and the test does not wait for it).
    let slow = fake_git("slow", "sleep 30");
    let began = Instant::now();
    assert_eq!(run_remote_v(&slow, &root, Duration::from_millis(200)), None);
    assert!(began.elapsed() < Duration::from_secs(5), "{:?}", began.elapsed());
    // One that fails, and one that is not there.
    let failing = fake_git("failing", "echo fatal >&2\nexit 128");
    assert_eq!(run_remote_v(&failing, &root, Duration::from_secs(5)), None);
    assert_eq!(run_remote_v(Path::new("/nowhere/at/all/git"), &root, Duration::from_secs(5)), None);
    // Output far larger than a few lines is cut, not held.
    let flood = fake_git("flood", "yes 'origin\thttps://github.com/a/b (fetch)' | head -c 2000000");
    let began = Instant::now();
    let said = run_remote_v(&flood, &root, Duration::from_secs(10)).expect("it ended: what it printed is read as it comes");
    assert!(said.len() as u64 <= MAX_OUTPUT, "bounded: {}", said.len());
    assert!(began.elapsed() < Duration::from_secs(5), "and not waited for in vain: {:?}", began.elapsed());
    for p in [&git, &env, &slow, &failing, &flood] {
        done(p.parent().expect("a folder"));
    }
    done(&root);
}

#[test]
fn a_folder_inside_the_repository_knows_where_it_lies_and_the_repository_s_own_folder_is_empty() {
    let root = repo("prefix");
    assert_eq!(read(&root, None).expect("a repository").prefix, "");
    let deep = root.join("examples/azul-review");
    fs::create_dir_all(&deep).expect("made");
    assert_eq!(read(&deep, None).expect("a repository").prefix, "examples/azul-review");
    // A file of the folder: the folder it is in.
    write(&deep.join("src/lib.rs"), "fn main() {}\n");
    assert_eq!(read(&deep.join("src/lib.rs"), None).expect("a repository").prefix, "examples/azul-review/src");
    assert_eq!(prefix_of(Path::new("/elsewhere/else"), &root), "", "outside the root: nothing is made up");
    done(&root);
}

#[test]
fn a_status_with_a_line_is_a_change_and_nothing_is_not() {
    assert!(parse_dirty(" M src/lib.rs\n"));
    assert!(parse_dirty("M  a\nA  b\n"));
    assert!(!parse_dirty(""));
    assert!(!parse_dirty("\n  \n"));
}

#[cfg(unix)]
#[test]
fn any_git_command_is_run_the_same_way_with_its_words_after_the_no_pager_one() {
    let root = repo("rungit");
    let echo = fake_git("rungit", "echo \"$*\"");
    assert_eq!(run_git(&echo, &root, &["status", "--porcelain=v1"], Duration::from_secs(5)).as_deref(), Some("--no-pager status --porcelain=v1\n"));
    let dirty_git = fake_git("dirtygit", "echo ' M src/lib.rs'");
    let clean_git = fake_git("cleangit", "exit 0");
    let said = |g: &Path| run_git(g, &root, &["status", "--porcelain=v1", "--untracked-files=no"], Duration::from_secs(5)).map(|t| parse_dirty(&t));
    assert_eq!(said(&dirty_git), Some(true));
    assert_eq!(said(&clean_git), Some(false), "nothing printed and it succeeded: clean");
    let failing = fake_git("failgit", "exit 1");
    assert_eq!(said(&failing), None, "a failure says nothing");
    for p in [&echo, &dirty_git, &clean_git, &failing] {
        done(p.parent().expect("a folder"));
    }
    done(&root);
}
