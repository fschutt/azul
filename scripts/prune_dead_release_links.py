#!/usr/bin/env python3
"""Turn every advertised download that was not actually published into a
visibly-unavailable entry instead of a 404.

WHY THIS EXISTS
---------------
`doc/src/dllgen/deploy.rs` renders the release page from a STATIC list of
what a release is *supposed* to contain, and it renders it during the
website-skeleton build — long before any real build artifact exists. So a
producer job that ships nothing does not remove a row from the page; it
leaves a link pointing at a file nobody ever uploaded.

Measured on the live 0.2.0 page: 71 GitHub-Release download links, 18 of
them 404 — fifteen iOS demo bundles that CI has never once produced, and
all three `azul-writer` desktop tarballs (the crate's binary is named
`azwriter`, so the staging loop copied a path that does not exist and
called it a cache hit).

A missing artifact must be an OMITTED row, never a dead link. This script
runs in `deploy_pages` after the real artifacts have been merged in and
the LARGE ones uploaded to the GitHub Release, so for the first time in
the pipeline both facts are available at once: what the page claims, and
what exists.

WHAT IT CHECKS
--------------
Two link shapes, matching `deploy.rs::asset_url`:

  https://github.com/<owner>/<repo>/releases/download/<version>/<name>
      present  <=>  <name> is in the release's asset list (--release-assets)

  https://azul.rs/ui/release/<version>/<path>
      present  <=>  <release-dir>/<path> exists on disk

Anything else is left alone.

An `<a>` whose target is absent becomes a non-clickable element carrying
the same label plus "not available", styled with the `is-missing` class
the page already uses for absent per-OS binaries.

Usage:
    python3 scripts/prune_dead_release_links.py \
        --version 0.2.0 \
        --release-dir website/ui/release/0.2.0 \
        --release-assets /tmp/release-asset-names.txt \
        --repo fschutt/azul \
        website/ui/release/0.2.0.html website/ui/release/0.2.0/index.html
"""
import argparse
import html
import os
import re
import sys

ANCHOR = re.compile(
    r"<a\b(?P<attrs>[^>]*?)\bhref=(?P<q>['\"])(?P<url>[^'\"]+)(?P=q)(?P<rest>[^>]*)>"
    r"(?P<inner>.*?)</a>",
    re.DOTALL | re.IGNORECASE,
)
CLASS_ATTR = re.compile(r"class=(['\"])(.*?)\1", re.IGNORECASE)
TAGS = re.compile(r"<[^>]+>")


def classify(url, version, repo, release_assets, release_dir, pages_prefix):
    """-> (kind, key) or None when the URL is not a release artifact link."""
    rel_prefix = f"https://github.com/{repo}/releases/download/{version}/"
    if url.startswith(rel_prefix):
        return ("release", url[len(rel_prefix):])
    for prefix in pages_prefix:
        if url.startswith(prefix):
            return ("pages", url[len(prefix):])
    return None


def is_present(kind, key, release_assets, release_dir):
    if kind == "release":
        return key in release_assets
    # Pages asset: strip any query/fragment, then look for the file.
    path = key.split("?", 1)[0].split("#", 1)[0]
    if not path:
        return True  # a link to the release dir itself
    candidate = os.path.join(release_dir, path)
    if os.path.exists(candidate):
        return True
    # An extensionless "clean URL" resolves to <path>/index.html.
    return os.path.exists(os.path.join(candidate, "index.html"))


def label_of(inner):
    """A readable label for a link whose body may be plain text or a card."""
    text = TAGS.sub(" ", inner)
    text = html.unescape(text)
    return " ".join(text.split())


def replacement(attrs, rest, inner):
    classes = ""
    m = CLASS_ATTR.search(attrs) or CLASS_ATTR.search(rest)
    if m:
        classes = m.group(2)
    if "docs-card" in classes:
        # Mirror deploy.rs::generate_asset_card's missing-asset tile exactly, so
        # a pruned card looks identical to one the generator knew was absent.
        body = re.sub(
            r"<p class='docs-card-file'>.*?</p>",
            "",
            inner,
            flags=re.DOTALL,
        )
        return (
            "<div class='docs-card is-missing'>"
            f"{body}<p class='docs-card-file'>not available</p></div>"
        )
    return (
        f"<span class='release-link-missing is-missing'>{inner} "
        "&mdash; not available</span>"
    )


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--version", required=True)
    ap.add_argument("--repo", required=True, help="owner/repo")
    ap.add_argument("--release-dir", required=True)
    ap.add_argument(
        "--release-assets",
        required=True,
        help="file with one published GitHub-Release asset name per line",
    )
    ap.add_argument(
        "--fail-on-missing",
        action="store_true",
        help="exit 1 after pruning (for a run that must not ship holes at all)",
    )
    ap.add_argument("pages", nargs="+", help="HTML files to rewrite")
    args = ap.parse_args()

    with open(args.release_assets, encoding="utf-8") as fh:
        release_assets = {ln.strip() for ln in fh if ln.strip()}
    print(f"{len(release_assets)} published release assets for {args.version}")

    pages_prefix = (
        f"https://azul.rs/ui/release/{args.version}/",
        f"/ui/release/{args.version}/",
    )

    total_links = 0
    pruned = {}
    kept = 0

    for path in args.pages:
        if not os.path.exists(path):
            print(f"  (skip, absent) {path}")
            continue
        with open(path, encoding="utf-8") as fh:
            src = fh.read()

        removed_here = []

        def sub(m):
            nonlocal kept
            url = m.group("url")
            what = classify(
                url, args.version, args.repo, release_assets,
                args.release_dir, pages_prefix,
            )
            if what is None:
                return m.group(0)
            kind, key = what
            if is_present(kind, key, release_assets, args.release_dir):
                kept += 1
                return m.group(0)
            removed_here.append((kind, key, label_of(m.group("inner"))))
            return replacement(m.group("attrs"), m.group("rest"), m.group("inner"))

        out, n = ANCHOR.subn(sub, src)
        total_links += n
        if removed_here:
            with open(path, "w", encoding="utf-8") as fh:
                fh.write(out)
            for kind, key, label in removed_here:
                pruned.setdefault((kind, key), label)
        print(f"  {path}: {n} anchors scanned, {len(removed_here)} pruned")

    print(f"\n=== dead-link prune: {args.version} ===")
    print(f"  anchors scanned:        {total_links}")
    print(f"  artifact links kept:    {kept}")
    print(f"  artifact links pruned:  {len(pruned)}")
    for (kind, key), label in sorted(pruned.items()):
        print(f"    [{kind}] {key}  ({label})")

    if pruned:
        # A warning, not an error: the page is now HONEST, which is the point.
        # The producer job that shipped nothing is where the red belongs, and
        # the post-release watchdog fetches whatever links survive.
        names = ", ".join(k for _, k in sorted(pruned))
        print(
            f"::warning::{len(pruned)} advertised artifact(s) were never "
            f"published and have been marked unavailable on the release page "
            f"instead of shipping as dead links: {names}"
        )
        if args.fail_on_missing:
            return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
