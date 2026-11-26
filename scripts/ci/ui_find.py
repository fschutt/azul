#!/usr/bin/env python3
"""ui_find.py: is a text on the screen? Reads the accessibility dump of the screen and says, for each text asked, whether a node
shows it.

    ui_find.py <dump> <text> [<text> ...]       prints {"<text>": true|false, ...} (JSON)
    ui_find.py --after <dump> <package>         prints {"after_found": {...}, "after_present": {...}} for the texts the manifest names
                                                for <package> in AZ_SMOKE_AFTER (JSON {package: {"found": [...], "gone": [...]}},
                                                scripts/demo_apps.py plan): `after_found` is whether each text that must be on the
                                                screen after the button was pressed is there, `after_present` whether each text that
                                                must be gone still is

The dump is uiautomator's XML (Android: `adb shell uiautomator dump`: a node's `text` and `content-desc`) or idb's JSON (iOS:
`idb ui describe-all --json`: the `AXLabel`, `label`, `AXValue`, `value` and `title` of every element). A text matches when it is
part of one of them, whatever its case. A dump that cannot be read finds nothing: a text that is not seen is not there.
Standard library only.
"""
import json
import os
import sys
import xml.etree.ElementTree as ET

IDB_KEYS = ("AXLabel", "label", "AXValue", "value", "title")


def haystack(path):
    """The strings of the dump's nodes that a person sees or hears, lower-cased."""
    try:
        raw = open(path, encoding="utf-8", errors="replace").read()
    except OSError:
        return []
    raw = raw.strip()
    if not raw:
        return []
    if raw[:1] in "[{":
        try:
            items = json.loads(raw)
        except ValueError:
            return []
        if isinstance(items, dict):
            items = [items]
        return [str(it[k]).lower() for it in items if isinstance(it, dict) for k in IDB_KEYS if it.get(k)]
    try:
        root = ET.fromstring(raw)
    except ET.ParseError:
        return []
    return [n.get(k, "").lower() for n in root.iter("node") for k in ("text", "content-desc") if n.get(k)]


def find(path, texts):
    """{text: whether some node of the dump at `path` shows it}."""
    seen = haystack(path)
    return {t: any(t.lower() in s for s in seen) for t in texts}


def after(path, package, env=None):
    """The after_found / after_present answers of one app (see the module doc)."""
    wanted = json.loads((env if env is not None else os.environ).get("AZ_SMOKE_AFTER") or "{}").get(package) or {}
    return {"after_found": find(path, wanted.get("found") or []), "after_present": find(path, wanted.get("gone") or [])}


def main(argv):
    if len(argv) >= 3 and argv[0] == "--after":
        print(json.dumps(after(argv[1], argv[2])))
        return 0
    if len(argv) < 2:
        print(__doc__, file=sys.stderr)
        return 2
    print(json.dumps(find(argv[0], argv[1:])))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
