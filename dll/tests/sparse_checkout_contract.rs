//! Every script the `deploy_pages` job runs by path must be in its
//! sparse-checkout list.
//!
//! `deploy_pages` checks out with `sparse-checkout-cone-mode: false`, so a file
//! not named in the list is simply ABSENT from the workspace. Referencing one
//! by path then fails at run time, in a job that runs only on a real deploy.
//!
//! This has now happened twice:
//!
//! 1. `scripts/build_registry_mirrors.sh` was missing from 2026-05-30 (when the
//!    script was added) to 2026-07-27, dying with exit 127 every single time. A
//!    `|| true` swallowed it, so the deploy went green while /ui/azul, /ui/npm,
//!    /ui/nuget and /ui/gems were never built.
//! 2. `scripts/prune_dead_release_links.py` was missing from the day it was
//!    added. That step has no `|| true`, so it FAILED the deploy outright — the
//!    website did not publish even though every artifact had built correctly.
//!
//! The first failed silently, the second loudly; both came from the same gap
//! between "the workflow runs this file" and "the workflow checked this file
//! out". Nothing else checks it, because the sparse list is data, not code.

const WORKFLOW: &str = include_str!("../../.github/workflows/rust.yml");

/// The `sparse-checkout:` block belonging to deploy_pages' checkout step.
fn sparse_paths() -> Vec<String> {
    let at = WORKFLOW
        .find("sparse-checkout: |")
        .expect("deploy_pages must declare a sparse-checkout block");
    let rest = &WORKFLOW[at..];
    let end = rest
        .find("sparse-checkout-cone-mode:")
        .expect("the block must be terminated by the cone-mode key");
    rest[..end]
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// Scripts invoked by path anywhere in the workflow's deploy_pages region.
///
/// Deliberately simple: find `scripts/<name>` tokens that appear as the target
/// of a `python3`/`bash`/`sh`/`./` invocation. A false positive here is a
/// script someone must add to the list or stop calling; both are fine outcomes.
fn scripts_invoked_after_deploy_pages() -> Vec<String> {
    let start = WORKFLOW
        .find("\n  deploy_pages:")
        .expect("deploy_pages job must exist");
    // Bound the region to THIS job. Scanning to end-of-file swept up
    // check_dep_justifications.py and docs_to_pdf.sh, which belong to later
    // jobs that check the repo out in full — a false positive that would have
    // made this test lie about a real problem.
    let body = &WORKFLOW[start + 1..];
    let end = body
        .match_indices("\n  ")
        .filter(|(i, _)| {
            let after = &body[i + 3..];
            let key: String = after.chars().take_while(|c| *c != '\n').collect();
            // a sibling job key: two-space indent, ends in ':', no leading '-'
            !key.starts_with('-')
                && !key.starts_with('#')
                && key.ends_with(':')
                && !key.contains(' ')
                && *i > 0
        })
        .map(|(i, _)| i)
        .next()
        .unwrap_or(body.len());
    let region = &body[..end];
    let mut out = Vec::new();
    for line in region.lines() {
        let t = line.trim();
        if t.starts_with('#') {
            continue;
        }
        for pat in ["python3 scripts/", "bash scripts/", "sh scripts/", "./scripts/"] {
            if let Some(i) = t.find(pat) {
                let tail = &t[i + pat.len()..];
                let name: String = tail
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
                    .collect();
                if !name.is_empty() {
                    let full = format!("scripts/{name}");
                    if !out.contains(&full) {
                        out.push(full);
                    }
                }
            }
        }
    }
    out
}

#[test]
fn deploy_pages_checks_out_every_script_it_runs() {
    let listed = sparse_paths();
    let invoked = scripts_invoked_after_deploy_pages();
    assert!(
        !invoked.is_empty(),
        "found no scripts invoked in the deploy_pages region — the matcher is \
         probably broken, which would make this test silently vacuous"
    );

    let missing: Vec<&String> = invoked.iter().filter(|s| !listed.contains(s)).collect();
    assert!(
        missing.is_empty(),
        "deploy_pages runs {missing:?} by path but does not check them out.\n\n\
         That job uses `sparse-checkout-cone-mode: false`, so a file not named \
         in the sparse-checkout list is ABSENT from the workspace. The step \
         will fail with \"No such file or directory\" on a real deploy — or, if \
         it is guarded by `|| true`, pass while doing nothing at all, which is \
         how the registry mirrors were dead for two months behind a green \
         deploy.\n\n\
         Currently checked out: {listed:?}"
    );
}
