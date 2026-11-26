//! The commit AzReview is built from, as `AZREVIEW_BUILD` (the short hash, `+` after it when files were
//! changed since; `unknown` without git): the About box says which build is running, so a report names its
//! binary.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn main() {
    let hash = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    let changed = git(&["status", "--porcelain", "--untracked-files=no"]).is_some();
    println!(
        "cargo:rustc-env=AZREVIEW_BUILD={hash}{}",
        if changed && hash != "unknown" { "+" } else { "" }
    );
    // A new commit moves the branch's ref, not HEAD: the build id is read again when either changes.
    for path in ["HEAD".to_string()]
        .into_iter()
        .chain(git(&["symbolic-ref", "-q", "HEAD"]))
    {
        if let Some(p) = git(&["rev-parse", "--git-path", &path]) {
            println!("cargo:rerun-if-changed={p}");
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
}
