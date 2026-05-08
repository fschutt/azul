#!/usr/bin/env python3
"""Merge per-language sidecar patches into api.json.

Each sidecar describes:
  - tabOrder_append:        a slug to append to installation.tabOrder (idempotent)
  - languages_entry:        {slug: {displayName, platforms}} merged under installation.languages
  - examples_code_additions: {example_name: relative/path.ext} added to every
                             matching examples[].code map

The script preserves the JSON ordering of the original file by parsing with
collections.OrderedDict (default in Python 3.7+ dict semantics), and only
touches the sections it owns. Running it twice is a no-op.

Usage:
    python3 scripts/api-json-additions/merge_into_api_json.py
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
API_JSON = REPO_ROOT / "api.json"
PATCH_DIR = Path(__file__).resolve().parent


def find_latest_version_block(api_data: dict) -> dict:
    """The api.json top-level is keyed by version string. Pick the latest."""
    if not api_data:
        raise SystemExit("api.json is empty")
    # Versions sort lexicographically as the file uses; preserve insertion order
    # but assume the *first* key is the active one (matches code in api.rs).
    first_key = next(iter(api_data))
    return api_data[first_key]


def merge_one(api_block: dict, patch: dict, slug_label: str) -> list[str]:
    """Apply one patch to the version block. Returns list of human-readable
    operations performed (empty if everything was already present)."""
    actions: list[str] = []

    install = api_block.setdefault("installation", {})

    # 1. tabOrder
    tab_slug = patch.get("tabOrder_append")
    if tab_slug:
        tab_order = install.setdefault("tabOrder", [])
        if tab_slug not in tab_order:
            tab_order.append(tab_slug)
            actions.append(f"appended {tab_slug!r} to installation.tabOrder")

    # 2. languages.<slug>
    languages_entry = patch.get("languages_entry") or {}
    languages = install.setdefault("languages", {})
    for slug, entry in languages_entry.items():
        if slug not in languages:
            languages[slug] = entry
            actions.append(f"added installation.languages.{slug}")
        else:
            actions.append(f"installation.languages.{slug} already present (skipped)")

    # 3. examples[].code additions
    code_adds = patch.get("examples_code_additions") or {}
    examples = api_block.get("examples", [])
    for ex_name, code_path in code_adds.items():
        # First two slugs from the patch are the same — derive the language slug
        # from the path (everything before the first '/').
        slug = code_path.split("/", 1)[0]
        matched = False
        for ex in examples:
            if ex.get("name") == ex_name:
                code_map = ex.setdefault("code", {})
                if slug not in code_map:
                    code_map[slug] = code_path
                    actions.append(
                        f"added examples[{ex_name!r}].code.{slug} = {code_path!r}"
                    )
                else:
                    actions.append(
                        f"examples[{ex_name!r}].code.{slug} already present (skipped)"
                    )
                matched = True
                break
        if not matched:
            actions.append(
                f"warning: example {ex_name!r} not found in api.json (skipped {code_path!r})"
            )

    return actions


def main() -> int:
    if not API_JSON.exists():
        print(f"error: {API_JSON} does not exist", file=sys.stderr)
        return 1
    if not PATCH_DIR.is_dir():
        print(f"error: {PATCH_DIR} does not exist", file=sys.stderr)
        return 1

    with API_JSON.open() as fh:
        api_data = json.load(fh)
    api_block = find_latest_version_block(api_data)

    patches = sorted(PATCH_DIR.glob("*.json"))
    if not patches:
        print("no sidecar patches found")
        return 0

    total_actions: list[str] = []
    for patch_path in patches:
        with patch_path.open() as fh:
            patch = json.load(fh)
        slug_label = patch_path.stem
        actions = merge_one(api_block, patch, slug_label)
        for action in actions:
            print(f"[{slug_label}] {action}")
        total_actions.extend(actions)

    if not total_actions:
        print("nothing to do (all patches already applied)")
        return 0

    # Rewrite api.json with 4-space indent matching the original style.
    with API_JSON.open("w") as fh:
        json.dump(api_data, fh, indent=4)
        fh.write("\n")
    print(f"\nwrote {API_JSON}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
