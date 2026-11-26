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

WHY IT WAS NOT PUBLISHED
------------------------
With `--demo-status demo-status.json` (the file `scripts/demo_apps.py status`
writes: {"apps": [{"app": .., "cells": {"<column>": {"status", "reason",
"first_error", "artifact"}}}]}) a dead link of a demo app whose build FAILED
says so: "not available: build failed: <first error of the build>" instead of
the bare "not available". A link belongs to a failed cell when its file name
(or the name minus ".tar.gz") is the cell's `artifact` or the file the build
stages for that app and column (`<App>-linux`, `<App>-android-arm64.apk`, ...:
the same names as `staged_name` of scripts/demo_apps.py, which a test pins).
A dead link of any other cell keeps the bare text, and a status file that
cannot be read only costs the reasons, never the prune.

Usage:
    python3 scripts/prune_dead_release_links.py \
        --version 0.2.0 \
        --release-dir website/ui/release/0.2.0 \
        --release-assets /tmp/release-asset-names.txt \
        --repo fschutt/azul \
        [--demo-status demo-status.json] \
        website/ui/release/0.2.0.html website/ui/release/0.2.0/index.html
"""
import argparse
import html
import json
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


# The file each demo build stages, per column of demo-status.json: the same names as `staged_name` of
# scripts/demo_apps.py (test_prune_dead_release_links.py holds the two to each other). The prune runs on
# the deploy runner's own python3, which has no tomllib, so it cannot import that script.
DESKTOP_SUFFIX = {"linux": "linux", "macos-arm64": "macos", "macos-intel": "macos-intel", "windows": "windows.exe"}
PHONE_SUFFIX = {
    "android-arm64": "android-arm64.apk",
    "android-x86_64": "android-x86_64.apk",
    "ios-device": "ios-device.app.zip",
    "ios-sim": "ios-sim.app.zip",
}
MAX_REASON = 200


def staged_name(app, column):
    """The file the build of `app` stages for `column`, or None for a column this script does not know."""
    if column in DESKTOP_SUFFIX:
        return f"{app}-{DESKTOP_SUFFIX[column]}"
    if column in PHONE_SUFFIX:
        return f"{app}-{PHONE_SUFFIX[column]}"
    return None


def reason_text(cell):
    """The one line a failed cell gives as its reason: the first error of the build, else the reason."""
    text = " ".join(str(cell.get("first_error") or cell.get("reason") or "no reason recorded").split())
    if len(text) > MAX_REASON:
        text = text[: MAX_REASON - 3].rstrip() + "..."
    return text


def failed_builds(status):
    """{file name: reason} of every FAILED cell of a parsed demo-status.json: the cell's `artifact` and
    the file its build stages. Anything that is not the shape `scripts/demo_apps.py status` writes is
    skipped, not an error."""
    failed = {}
    apps = status.get("apps") if isinstance(status, dict) else None
    for entry in apps if isinstance(apps, list) else []:
        if not isinstance(entry, dict):
            continue
        app = entry.get("app")
        cells = entry.get("cells")
        for column, cell in (cells.items() if isinstance(cells, dict) else []):
            if not isinstance(cell, dict) or cell.get("status") != "failed":
                continue
            why = reason_text(cell)
            for name in (cell.get("artifact"), staged_name(app, column) if isinstance(app, str) else None):
                if name and isinstance(name, str):
                    failed[name] = why
    return failed


def load_failed_builds(path):
    """failed_builds() of the file at `path`; a file that is missing or not JSON only costs the
    reasons, so it is a warning and the page is pruned all the same."""
    try:
        with open(path, encoding="utf-8") as fh:
            return failed_builds(json.load(fh))
    except (OSError, ValueError) as e:
        print(f"::warning::cannot read the demo status {path} ({e}): dead demo links say \"not available\" without a reason")
        return {}


def why_failed(key, failed):
    """The reason the build of the file behind the release link `key` failed, or None."""
    base = key.split("?", 1)[0].split("#", 1)[0].rsplit("/", 1)[-1]
    for name in (base, base[: -len(".tar.gz")] if base.endswith(".tar.gz") else None):
        if name and name in failed:
            return failed[name]
    return None


def replacement(attrs, rest, inner, why=None):
    """The inert stand-in for a dead link; `why` is the reason its build failed, if one is known."""
    classes = ""
    m = CLASS_ATTR.search(attrs) or CLASS_ATTR.search(rest)
    if m:
        classes = m.group(2)
    text = "not available"
    title = ""
    if why:
        text += ": build failed: " + html.escape(why, quote=True)
        # the whole reason is the title too: a table cell shows the first lines of it (docs-release.css)
        title = f" title='{text}'"
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
            f"{body}<p class='docs-card-file'>{text}</p></div>"
        )
    return (
        f"<span class='release-link-missing is-missing'{title}>{inner} "
        f"&mdash; {text}</span>"
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
        "--demo-status",
        help="demo-status.json of scripts/demo_apps.py status: a dead link of a build that failed says why",
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
    failed = load_failed_builds(args.demo_status) if args.demo_status else {}
    if args.demo_status:
        print(f"{len(failed)} file names of failed demo builds in {args.demo_status}")

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
            why = why_failed(key, failed)
            removed_here.append((kind, key, label_of(m.group("inner")), why))
            return replacement(m.group("attrs"), m.group("rest"), m.group("inner"), why)

        out, n = ANCHOR.subn(sub, src)
        total_links += n
        if removed_here:
            with open(path, "w", encoding="utf-8") as fh:
                fh.write(out)
            for kind, key, label, why in removed_here:
                pruned.setdefault((kind, key), (label, why))
        print(f"  {path}: {n} anchors scanned, {len(removed_here)} pruned")

    print(f"\n=== dead-link prune: {args.version} ===")
    print(f"  anchors scanned:        {total_links}")
    print(f"  artifact links kept:    {kept}")
    print(f"  artifact links pruned:  {len(pruned)}")
    for (kind, key), (label, why) in sorted(pruned.items()):
        print(f"    [{kind}] {key}  ({label})" + (f"  build failed: {why}" if why else ""))

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
