#!/usr/bin/env python3
"""Fetch the Haiku icons this crate ships and convert them to HVIF.

    python3 tools/fetch.py [NAME ...] [--verify-against-rdefs]

Downloads each icon from the Haiku project's repository at the pinned commit
(`data/artwork/icons/<NAME>`, an Icon-O-Matic document), converts it with
`iom2hvif.py` (Icon-O-Matic's own HVIF export, ported) into
`icons/<NAME>.hvif` and writes `icons/UPSTREAM.txt`: the commit, each
file's git blob id (check it with `git ls-tree <commit> data/artwork/icons/`)
and the sha256 of the HVIF written. Without names it takes every icon the
table in `src/lib.rs` includes.

`--verify-against-rdefs` also reads every `.rdef` resource file of that
commit and notes, per icon, where Haiku itself ships the very same HVIF bytes
(an Icon-O-Matic export someone pasted into the sources) - the conversion is
then proven byte-exact for that icon.
"""

import hashlib
import json
import os
import re
import sys
import urllib.parse
import urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import iom2hvif  # noqa: E402

COMMIT = "60e29589f83123ca4cb8e9fdcc83951d217af1e5"
REPOSITORY = "https://github.com/haiku/haiku"
RAW = "https://raw.githubusercontent.com/haiku/haiku/" + COMMIT + "/"
TREE = "https://api.github.com/repos/haiku/haiku/git/trees/" + COMMIT + "?recursive=1"
CRATE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ICONS = os.path.join(CRATE, "icons")


def get(url):
    with urllib.request.urlopen(url, timeout=60) as response:
        return response.read()


def blob_id(data):
    """The git blob id of `data` (what `git ls-tree` prints)."""
    return hashlib.sha1(b"blob %d\0" % len(data) + data).hexdigest()


def names_in_table():
    table = open(os.path.join(CRATE, "src", "lib.rs"), encoding="utf-8").read()
    return sorted(set(re.findall(r'include_bytes!\("\.\./icons/([A-Za-z0-9_\-]+)\.hvif"\)', table)))


def rdef_blobs():
    """sha256 of every HVIF in the commit's .rdef files -> where it is."""
    tree = json.loads(get(TREE))
    found = {}
    block = re.compile(r"resource\s*(\([^)]*\))?[^{]*\{(.*?)\}\s*;", re.S)
    hexes = re.compile(r'\$"([0-9A-Fa-f\s]*)"')
    for item in tree["tree"]:
        path = item["path"]
        if not path.endswith(".rdef"):
            continue
        text = get(RAW + urllib.parse.quote(path)).decode("utf-8", "replace")
        for match in block.finditer(text):
            data = bytes.fromhex(re.sub(r"\s", "", "".join(hexes.findall(match.group(2)))))
            if data.startswith(b"ncif"):
                label = (match.group(1) or "").strip()
                found.setdefault(hashlib.sha256(data).hexdigest(), []).append(
                    "%s %s" % (path, label) if label else path
                )
    return found


def main(argv):
    verify = "--verify-against-rdefs" in argv
    names = [a for a in argv if not a.startswith("--")] or names_in_table()
    shipped = rdef_blobs() if verify else {}
    os.makedirs(ICONS, exist_ok=True)
    lines = [
        "The icons in this folder, converted from the Haiku project's icon set.",
        "",
        "Repository: " + REPOSITORY,
        "Commit:     " + COMMIT,
        "Folder:     data/artwork/icons (Icon-O-Matic documents, MIT - see ../LICENSE)",
        "Converted:  tools/iom2hvif.py (Icon-O-Matic's HVIF export, ported), by tools/fetch.py",
        "",
        "<file>.hvif  <- upstream git blob of data/artwork/icons/<file>, sha256 of the .hvif",
    ]
    if verify:
        lines.append("  = the same HVIF bytes Haiku ships in that .rdef resource")
    lines.append("")
    for name in names:
        source = get(RAW + "data/artwork/icons/" + urllib.parse.quote(name))
        hvif = iom2hvif.convert(source)
        with open(os.path.join(ICONS, name + ".hvif"), "wb") as out:
            out.write(hvif)
        digest = hashlib.sha256(hvif).hexdigest()
        lines.append("%s.hvif  <- blob %s, sha256 %s (%d bytes)" % (name, blob_id(source), digest, len(hvif)))
        for where in shipped.get(digest, [])[:2]:
            lines.append("  = " + where)
        print(name, len(source), "->", len(hvif))
    with open(os.path.join(ICONS, "UPSTREAM.txt"), "w", encoding="utf-8") as out:
        out.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main(sys.argv[1:])
