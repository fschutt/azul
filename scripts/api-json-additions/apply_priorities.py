#!/usr/bin/env python3
"""Apply the curated docs reading order in `priorities.json` to api.json.

`priority` (float, 0-100) decides what the API REFERENCE shows first. Entries
that declare one are pulled to the front of their level, highest first;
everything unmarked keeps api.json's own order behind them. Unset is zero, so
curating a level means naming the handful that belong at the top rather than
ranking all two hundred. Nothing about codegen changes - the generators walk
api.json in file order, where the order is ABI-relevant.

The ranking lives in `priorities.json` rather than inline in api.json so it
can be reviewed as one list, and so re-running this after an api.json rewrite
restores it. Every key is checked against the real file first: a name that
does not resolve is reported, never silently ignored.

Running it twice is a no-op.

Usage:
    python3 scripts/api-json-additions/apply_priorities.py
    python3 scripts/api-json-additions/apply_priorities.py --check
"""

from __future__ import annotations

import collections
import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
API_JSON = REPO_ROOT / "api.json"
PRIORITIES = Path(__file__).resolve().parent / "priorities.json"


def latest_version_block(api_data: dict) -> dict:
    """api.json is keyed by version string; the last key is the current one."""
    if not api_data:
        raise SystemExit("api.json is empty")
    return api_data[list(api_data)[-1]]


def set_priority(obj: dict, value: float) -> bool:
    """Write `priority` in the slot the Rust struct declares it in - right
    after `doc` - so a later `azul-doc normalize` is a no-op. Returns whether
    anything changed."""
    if obj.get("priority") == value:
        return False
    items = [(k, v) for k, v in obj.items() if k != "priority"]
    obj.clear()
    placed = False
    for key, val in items:
        obj[key] = val
        if key == "doc" and not placed:
            obj["priority"] = value
            placed = True
    if not placed:
        rest = list(obj.items())
        obj.clear()
        obj["priority"] = value
        obj.update(rest)
    return True


def resolve(block: dict, section: str, key: str):
    """The object a key names, or None. Keys are `<module>`,
    `<module>.<Class>` and `<module>.<Class>.<member>`."""
    modules = block["api"]
    parts = key.split(".")
    if section == "modules":
        return modules.get(key) if len(parts) == 1 else None
    if len(parts) < 2:
        return None
    module = modules.get(parts[0])
    if module is None:
        return None
    klass = module["classes"].get(parts[1])
    if section == "classes":
        return klass if len(parts) == 2 else None
    if klass is None or len(parts) != 3:
        return None
    return (klass.get(section) or {}).get(parts[2])


def main() -> int:
    check_only = "--check" in sys.argv[1:]

    api_data = json.loads(API_JSON.read_text(), object_pairs_hook=collections.OrderedDict)
    block = latest_version_block(api_data)
    ranking = json.loads(PRIORITIES.read_text())

    unresolved: list[str] = []
    out_of_range: list[str] = []
    changed = collections.Counter()
    seen = collections.Counter()

    for section in ("modules", "classes", "constructors", "functions"):
        for key, raw in (ranking.get(section) or {}).items():
            value = float(raw)
            if not 0.0 < value <= 100.0:
                out_of_range.append(f"{section} {key} = {value}")
                continue
            target = resolve(block, section, key)
            if target is None:
                unresolved.append(f"{section} {key}")
                continue
            seen[section] += 1
            if set_priority(target, value):
                changed[section] += 1

    for label, problems in (("unresolved", unresolved), ("out of range", out_of_range)):
        if problems:
            print(f"{len(problems)} {label}:")
            for p in problems[:40]:
                print(f"    {p}")
            if len(problems) > 40:
                print(f"    ... and {len(problems) - 40} more")

    if unresolved or out_of_range:
        return 1

    total = sum(changed.values())
    summary = ", ".join(f"{k}={seen[k]}" for k in ("modules", "classes", "constructors", "functions"))
    if check_only:
        print(f"{summary} ({total} would change)")
        return 1 if total else 0

    if total:
        API_JSON.write_text(json.dumps(api_data, indent=4, ensure_ascii=False) + "\n")
        print(f"{summary} - {total} written to api.json")
        print("run `cd doc && cargo run --release -- normalize` to canonicalise")
    else:
        print(f"{summary} - api.json already matches priorities.json")
    return 0


if __name__ == "__main__":
    sys.exit(main())
