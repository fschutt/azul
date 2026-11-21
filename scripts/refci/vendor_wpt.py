#!/usr/bin/env python3
"""Vendor the curated web-platform-tests subset into tests/wpt/.

    python3 scripts/refci/vendor_wpt.py --wpt <a WPT checkout>          # re-vendor
    python3 scripts/refci/vendor_wpt.py --fetch [--commit <sha>]        # sparse-clone first
    python3 scripts/refci/vendor_wpt.py --wpt <dir> --list-eligible css/css-lists

Inputs (hand-written, reviewed like code):
    tests/wpt/selection.txt   which reftests, grouped under the engine gap they probe
    tests/wpt/local/**        azul's own reftests in the same format (UA defaults
                              WPT only covers with scripts), MIT like the rest of azul

Outputs (generated, committed; never edit by hand):
    tests/wpt/LICENSE.md              WPT's BSD-3-Clause licence, verbatim
    tests/wpt/upstream/<wpt path>     every vendored test, reference and support
                                      stylesheet, byte for byte
    tests/wpt/normalized/<path>       the same pages as strict XHTML
                                      (scripts/refci/htmlnorm.py) - what azul loads
    doc/working/wpt-<flat name>.xht   every TEST page again (not the references),
                                      for `azul-doc reftest`, which renders each
                                      .xht of doc/working in Chrome and in azul and
                                      compares the pixels: "does it match Chrome?"
                                      (doc/reftest_baseline.txt lists what must
                                      keep passing there)
    tests/wpt/reftests.tsv            test, match|mismatch, reference, fuzzy, gap
    tests/wpt/skipped.tsv             every reftest of the scanned areas that is NOT
                                      run, with the reason
    tests/wpt/editing/<command>.json  WPT editing/data cases for the commands mail
                                      compose needs, filtered, with stable ids

The runner is layout/tests/wpt/ (`cargo test --release -p azul-layout
--features wpt_tests --test wpt`); its known-failure lists
(tests/wpt/*_expectations.txt) are NOT written here.
"""

import argparse
import fnmatch
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, HERE)
import htmlnorm  # noqa: E402

OUT = os.path.join(REPO, "tests", "wpt")
WORKING = os.path.join(REPO, "doc", "working")
WORKING_PREFIX = "wpt-"
WPT_URL = "https://github.com/web-platform-tests/wpt"
PAGE_EXT = (".html", ".htm", ".xht", ".xhtml")
MAX_BYTES = 32 * 1024

# CSS2.1 test-suite flags (<meta name="flags">) that mean "cannot run here".
SKIP_FLAGS = {
    "ahem": "needs the Ahem font",
    "font": "needs a special font",
    "image": "needs an image",
    "interact": "needs interaction",
    "animated": "animated",
    "paged": "paged media",
    "may": "optional behaviour (may)",
    "should": "recommended behaviour (should)",
    "dom": "needs the DOM / script",
    "http": "needs an HTTP server",
    "userstyle": "needs a user stylesheet",
    "invalid": "tests invalid CSS error handling",
    "scroll": "needs scrolling",
    "svg": "needs SVG",
    "nonhtml": "not HTML",
    "speech": "speech",
}

# The WPT editing/data commands mail compose needs.
EDITING_COMMANDS = [
    "bold", "italic", "underline", "inserttext", "insertparagraph", "insertlinebreak",
    "delete", "forwarddelete", "createlink", "insertunorderedlist", "insertorderedlist",
    "indent", "outdent",
]
# Setup commands a case may run before its main command.
EDITING_SETUP = {"stylewithcss", "defaultparagraphseparator"}
# Markup a mail compose editor holds; a case whose markup has anything else is left out.
EDITING_TAGS = {
    "p", "div", "span", "b", "strong", "i", "em", "u", "s", "a", "br", "ul", "ol", "li",
    "blockquote", "h1", "h2", "h3",
}
EDITING_CAP = 60


def read(path):
    with open(path, "rb") as f:
        raw = f.read()
    return raw, raw.decode("utf-8", errors="replace")


def rel_url(base_file, href, wpt_root):
    """Resolve a link href from `base_file` to a path relative to the WPT root."""
    href = href.strip().split("#", 1)[0].split("?", 1)[0]
    if not href or re.match(r"^[a-z]+:", href, re.I):
        return None
    if href.startswith("/"):
        p = os.path.normpath(href.lstrip("/"))
    else:
        base_dir = os.path.dirname(os.path.relpath(base_file, wpt_root))
        p = os.path.normpath(os.path.join(base_dir, href))
    if p.startswith(".."):
        return None
    return p.replace(os.sep, "/")


class Page:
    """One parsed page (test or reference) and why it cannot run, if it cannot."""

    def __init__(self, root, rel):
        self.root = root
        self.rel = rel
        self.path = os.path.join(root, rel)
        self.raw, self.text = read(self.path)
        self.xhtml = rel.endswith((".xht", ".xhtml"))
        self.doc = htmlnorm.parse(self.text, xhtml=self.xhtml)
        self.support = []  # stylesheets this page pulls in (paths rel. to root)
        self.reasons = []
        self._check()

    def resolve_css(self, href):
        p = rel_url(self.path, href, self.root)
        if p is None or not os.path.isfile(os.path.join(self.root, p)):
            return None
        if p not in self.support:
            self.support.append(p)
        _, css = read(os.path.join(self.root, p))
        return css

    def normalized(self):
        return self.doc.to_xhtml(self.resolve_css)

    def _check(self):
        low = self.text.lower()
        r = self.reasons
        if len(self.raw) > MAX_BYTES:
            r.append("larger than 32 KB")
        if self.doc.has_script or re.search(r"\bon[a-z]+\s*=", low):
            r.append("script")
        if "reftest-wait" in self.doc.root_classes():
            r.append("reftest-wait")
        css = self.doc.stylesheet_text(self.resolve_css).lower()
        everything = low + "\n" + css
        if "ahem" in everything:
            r.append("needs the Ahem font")
        if "@font-face" in everything or "/fonts/" in everything:
            r.append("web font")
        if "@import" in css:
            r.append("@import")
        if "url(" in everything:
            r.append("CSS url() resource")
        if re.search(r"writing-mode|text-orientation|direction\s*:|\bdir\s*=|unicode-bidi", everything):
            r.append("bidi or vertical text")
        tags = set()

        def walk(n):
            for c in n.children:
                if isinstance(c, htmlnorm.Node):
                    tags.add(c.tag)
                    walk(c)

        walk(self.doc.body)
        media = tags & {"img", "svg", "canvas", "video", "audio", "iframe", "object", "embed",
                        "picture", "math", "frameset", "frame"}
        if media:
            r.append("embedded content <%s>" % sorted(media)[0])
        forms = tags & {"input", "select", "textarea", "button", "fieldset", "legend", "form"}
        if forms:
            r.append("form control <%s> (azul makes these widgets)" % sorted(forms)[0])
        if any(":" in t for t in tags):
            r.append("foreign (namespaced) content")
        for name, content in self.doc.metas:
            if name == "flags":
                for flag in content.lower().split():
                    if flag in SKIP_FLAGS:
                        r.append("flags: " + SKIP_FLAGS[flag])
        for issue in self.doc.issues:
            if issue.startswith("unresolved stylesheet"):
                r.append(issue)
        # de-duplicate, keep order
        seen = set()
        self.reasons = [x for x in r if not (x in seen or seen.add(x))]

    def references(self):
        """[(rel, path)] of the page's match / mismatch links, resolved."""
        out = []
        for rel, href in self.doc.links:
            if rel in ("match", "mismatch"):
                out.append((rel, rel_url(self.path, href, self.root), href.strip()))
        return out

    def fuzzy(self):
        """{ref path or None: 'a-b;c-d'} from <meta name=fuzzy>."""
        out = {}
        for name, content in self.doc.metas:
            if name != "fuzzy":
                continue
            key = None
            value = content
            if ":" in content:
                k, value = content.rsplit(":", 1)
                key = rel_url(self.path, k, self.root)
            ranges = {}
            positional = []
            for part in value.split(";"):
                part = part.strip()
                nm = None
                if "=" in part:
                    nm, part = (x.strip() for x in part.split("=", 1))
                lo, _, hi = part.partition("-")
                rng = "%d-%d" % (int(lo), int(hi or lo))
                if nm:
                    ranges[nm] = rng
                else:
                    positional.append(rng)
            md = ranges.get("maxDifference") or (positional.pop(0) if positional else "0-0")
            tp = ranges.get("totalPixels") or (positional.pop(0) if positional else "0-0")
            out[key] = md + ";" + tp
        return out


def test_verdict(root, rel):
    """(Page or None, [(rel, ref path, fuzzy)], [reasons])."""
    try:
        page = Page(root, rel)
    except Exception as e:  # noqa: BLE001 - a vendoring tool reports and moves on
        return None, [], ["unreadable: %s" % e]
    refs = page.references()
    reasons = list(page.reasons)
    page.ref_pages = []
    if re.search(r"-crash\.(html?|xht|xhtml)$", rel) and not refs:
        # A crash test (the WPT naming convention): it passes when it renders.
        return page, ([] if reasons else [("crash", "-", "-")]), reasons
    if not refs:
        reasons.append("not a reftest")
    fuzzy = page.fuzzy()
    out = []
    ref_pages = []
    for kind, path, href in refs:
        if path is None:
            reasons.append("reference outside the tree: " + href)
            continue
        if not path.endswith(PAGE_EXT):
            reasons.append("image reference (%s)" % os.path.basename(path))
            continue
        if not os.path.isfile(os.path.join(root, path)):
            reasons.append("reference missing: " + path)
            continue
        ref = Page(root, path)
        if ref.reasons:
            reasons.append("reference: " + ref.reasons[0])
        if any(k in ("match", "mismatch") for k, _ in ref.doc.links):
            reasons.append("reference chain (the reference has references)")
        out.append((kind, path, fuzzy.get(path) or fuzzy.get(None) or "-"))
        ref_pages.append(ref)
    page.ref_pages = ref_pages
    seen = set()
    reasons = [x for x in reasons if not (x in seen or seen.add(x))]
    return page, out, reasons


def parse_selection(path):
    """[(pattern, gap)], [areas]."""
    items, areas = [], []
    gap = "-"
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.split("#", 1)[0].strip()
            if not line:
                continue
            if line.startswith("@area "):
                areas.append(line[6:].strip())
                continue
            m = re.match(r"^\[([A-Z0-9][A-Za-z0-9-]*)\]", line)
            if m:
                gap = m.group(1)
                continue
            items.append((line, gap))
    return items, areas


def list_pages(root, area):
    out = []
    for dp, dn, fn in os.walk(os.path.join(root, area)):
        dn[:] = sorted(d for d in dn if d not in ("reference", "references", "support", "resources", "crashtests"))
        for f in sorted(fn):
            if f.endswith(PAGE_EXT) and "-ref." not in f and not f.endswith(("-notref.html",)):
                out.append(os.path.relpath(os.path.join(dp, f), root).replace(os.sep, "/"))
    return out


def expand(root, pattern):
    if any(ch in pattern for ch in "*?["):
        base = pattern.split("*", 1)[0].rsplit("/", 1)[0]
        return [p for p in list_pages(root, base) if fnmatch.fnmatch(p, pattern)]
    return [pattern]


def write(path, data):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    mode = "wb" if isinstance(data, bytes) else "w"
    with open(path, mode, **({} if mode == "wb" else {"encoding": "utf-8", "newline": "\n"})) as f:
        f.write(data)


def git_head(root):
    try:
        return subprocess.check_output(["git", "-C", root, "rev-parse", "HEAD"], text=True).strip()
    except Exception:  # noqa: BLE001
        return "unknown"


# ---------------------------------------------------------------------------
# editing data
# ---------------------------------------------------------------------------

class JsLiteral:
    """Reads the JS array literal of editing/data/*.js (single- and double-quoted
    strings, trailing commas, // comments) - it is not JSON."""

    NUM = re.compile(r"-?\d+(\.\d+)?")
    IDENT = re.compile(r"[A-Za-z_$][\w$]*")

    def __init__(self, s):
        self.s = s
        self.i = 0

    def ws(self):
        s = self.s
        while self.i < len(s):
            if s[self.i] in " \t\r\n":
                self.i += 1
            elif s.startswith("//", self.i):
                j = s.find("\n", self.i)
                self.i = len(s) if j < 0 else j
            elif s.startswith("/*", self.i):
                self.i = s.index("*/", self.i) + 2
            else:
                break

    def value(self):
        self.ws()
        s = self.s
        c = s[self.i]
        if c in "[{":
            close = "]" if c == "[" else "}"
            self.i += 1
            out = [] if c == "[" else {}
            while True:
                self.ws()
                if s[self.i] == close:
                    self.i += 1
                    return out
                if c == "[":
                    out.append(self.value())
                else:
                    if s[self.i] in "\"'":
                        k = self.string()
                    else:
                        m = self.IDENT.match(s, self.i)
                        k, self.i = m.group(0), m.end()
                    self.ws()
                    self.i += 1  # ':'
                    out[k] = self.value()
                self.ws()
                if s[self.i] == ",":
                    self.i += 1
        if c in "\"'":
            return self.string()
        for word, v in (("true", True), ("false", False), ("null", None)):
            if s.startswith(word, self.i):
                self.i += len(word)
                return v
        m = self.NUM.match(s, self.i)
        if m:
            self.i = m.end()
            return float(m.group(0)) if m.group(1) else int(m.group(0))
        raise ValueError("unexpected %r" % s[self.i:self.i + 40])

    def string(self):
        s = self.s
        q = s[self.i]
        self.i += 1
        out = []
        while True:
            c = s[self.i]
            if c == q:
                self.i += 1
                return "".join(out)
            if c == "\\":
                n = s[self.i + 1]
                if n == "u":
                    out.append(chr(int(s[self.i + 2:self.i + 6], 16)))
                    self.i += 6
                elif n == "x":
                    out.append(chr(int(s[self.i + 2:self.i + 4], 16)))
                    self.i += 4
                else:
                    out.append({"n": "\n", "t": "\t", "r": "\r", "b": "\b", "f": "\f", "0": "\0"}.get(n, n))
                    self.i += 2
                continue
            out.append(c)
            self.i += 1


def editing_case_ok(initial, commands):
    """None if the case fits mail compose, else why not."""
    main = commands[-1][0].lower()
    for name, value in commands[:-1]:
        n = name.lower()
        if n not in EDITING_SETUP:
            return "setup command %s" % name
        if n == "stylewithcss" and str(value).lower() != "false":
            return "styleWithCSS=true (azul formats with elements, not style attributes)"
    if main not in EDITING_COMMANDS:
        return "main command %s" % main
    if re.search(r"data-(start|end)|contenteditable|<!--|style\s*=|<table|<img|<hr|<font|&[a-z]", initial, re.I):
        return "markup outside the mail subset"
    for tag in re.findall(r"</?\s*([a-zA-Z][a-zA-Z0-9]*)", initial):
        if tag.lower() not in EDITING_TAGS:
            return "tag <%s>" % tag.lower()
    for attr in re.findall(r"<[a-zA-Z][^>]*?\s([a-zA-Z-]+)\s*=", initial):
        if attr.lower() != "href":
            return "attribute %s" % attr
    return None


def canonical(markup):
    """Tags and text only, markers kept: see htmlnorm.fragment_to_xhtml."""
    return htmlnorm.fragment_to_xhtml(markup, with_attrs=False)


def vendor_editing(root, commit):
    for cmd in EDITING_COMMANDS:
        src_rel = "editing/data/%s.js" % cmd
        _, src = read(os.path.join(root, src_rel))
        cases = JsLiteral(src[src.index("[", src.index("browserTests")):]).value()
        kept = []
        left_out = {}
        for case in cases:
            initial, commands, expected = case[0], case[1], case[2]
            why = editing_case_ok(initial, commands)
            if why:
                left_out[why] = left_out.get(why, 0) + 1
                continue
            if commands[-1][0].lower() != cmd:
                left_out["main command differs from the file"] = left_out.get("main command differs from the file", 0) + 1
                continue
            variants = expected if isinstance(expected, list) else [expected]
            key = json.dumps([initial, commands, variants], ensure_ascii=False)
            cid = "%s-%s" % (cmd, hashlib.sha1(key.encode("utf-8")).hexdigest()[:8])
            forms = []
            for v in variants:
                c = canonical(v)
                if c not in forms:
                    forms.append(c)
            kept.append({
                "id": cid,
                "initial": initial,
                "initial_xhtml": htmlnorm.fragment_to_xhtml(initial),
                "commands": [[c[0].lower(), c[1]] for c in commands],
                "expected": variants,
                "expected_canonical": forms,
            })
        total_ok = len(kept)
        if len(kept) > EDITING_CAP:
            left_out["over the cap of %d" % EDITING_CAP] = len(kept) - EDITING_CAP
            kept = kept[:EDITING_CAP]
        data = {
            "source": src_rel,
            "upstream": WPT_URL,
            "upstream_commit": commit,
            "license": "BSD-3-Clause, see tests/wpt/LICENSE.md",
            "generated_by": "scripts/refci/vendor_wpt.py",
            "upstream_cases": len(cases),
            "eligible_cases": total_ok,
            "left_out": dict(sorted(left_out.items())),
            "cases": kept,
        }
        write(os.path.join(OUT, "editing", cmd + ".json"),
              json.dumps(data, indent=1, ensure_ascii=False) + "\n")
        print("editing %-20s %4d of %4d cases" % (cmd, len(kept), len(cases)))


# ---------------------------------------------------------------------------
# reftests
# ---------------------------------------------------------------------------

def working_name(rel):
    """`css/CSS2/tables/x.html` -> `wpt-CSS2-tables-x.xht`: one flat name per
    test page for doc/working (azul-doc's reftest reads that directory flat)."""
    stem = os.path.splitext(rel)[0]
    if stem.startswith("css/"):
        stem = stem[4:]
    stem = stem.replace("non-replaced-elements/", "")
    return WORKING_PREFIX + stem.replace("/", "-") + ".xht"


def vendor_reftests(root, commit, from_vendored=False):
    """`from_vendored`: `root` IS tests/wpt/upstream - keep it and skipped.tsv,
    regenerate only normalized/ and doc/working/."""
    keep_skipped = from_vendored
    items, areas = parse_selection(os.path.join(OUT, "selection.txt"))
    for sub in (("normalized",) if from_vendored else ("upstream", "normalized")):
        shutil.rmtree(os.path.join(OUT, sub), ignore_errors=True)
    os.makedirs(WORKING, exist_ok=True)
    for name in os.listdir(WORKING):
        if name.startswith(WORKING_PREFIX):
            os.remove(os.path.join(WORKING, name))
    rows = []
    skipped = {}
    selected = set()
    copied = set()
    working_names = {}

    def copy_upstream(rel):
        if rel in copied or from_vendored:
            return
        copied.add(rel)
        raw, _ = read(os.path.join(root, rel))
        write(os.path.join(OUT, "upstream", rel), raw)

    for pattern, gap in items:
        local = pattern.startswith("local/")
        base = OUT if local else root
        for rel in expand(base, pattern):
            if rel in selected:
                continue
            selected.add(rel)
            page, refs, reasons = test_verdict(base, rel)
            if reasons:
                skipped[rel] = "selected, but: " + "; ".join(reasons)
                continue
            for p in [page] + page.ref_pages:
                normalized = p.normalized()
                write(os.path.join(OUT, "normalized", p.rel), normalized)
                if p is page:
                    name = working_name(rel)
                    if name in working_names:
                        raise SystemExit("doc/working name clash: %s and %s -> %s" % (working_names[name], rel, name))
                    working_names[name] = rel
                    write(os.path.join(WORKING, name), normalized)
                if not local:
                    copy_upstream(p.rel)
                    for s in p.support:
                        copy_upstream(s)
            for kind, ref, fuzzy in refs:
                rows.append((rel, kind, ref, fuzzy, gap))

    # Everything else in the scanned areas: why it does not run.
    if keep_skipped:
        skipped = None
    else:
        for area in areas:
            for rel in list_pages(root, area):
                if rel in selected or rel in skipped:
                    continue
                _, refs, reasons = test_verdict(root, rel)
                if "not a reftest" in reasons:
                    continue
                skipped[rel] = "; ".join(reasons) if reasons else "eligible, not selected"

    head = [
        "# Generated by scripts/refci/vendor_wpt.py - do not edit.",
        "# upstream: %s @ %s (BSD-3-Clause, tests/wpt/LICENSE.md)" % (WPT_URL, commit),
        "# local/*: azul's own reftests (MIT), same format.",
        "# Pages are read from tests/wpt/normalized/. Fuzzy is maxDifference;totalPixels (WPT).",
    ]
    write(os.path.join(OUT, "reftests.tsv"),
          "\n".join(head + ["# test\trel\treference\tfuzzy\tgap"]
                    + ["\t".join(r) for r in rows]) + "\n")
    if skipped is not None:
        write(os.path.join(OUT, "skipped.tsv"),
              "\n".join(head[:2] + ["# test\treason"]
                        + ["%s\t%s" % kv for kv in sorted(skipped.items())]) + "\n")
    if os.path.isfile(os.path.join(root, "LICENSE.md")):
        shutil.copyfile(os.path.join(root, "LICENSE.md"), os.path.join(OUT, "LICENSE.md"))
    tests = len({r[0] for r in rows})
    print("reftests: %d tests (%d links) vendored, %d pages in doc/working, %s skipped with a reason"
          % (tests, len(rows), len(working_names), "(kept)" if skipped is None else len(skipped)))


def list_eligible(root, area):
    for rel in list_pages(root, area):
        page, refs, reasons = test_verdict(root, rel)
        if reasons:
            continue
        title = ""
        m = re.search(r"<title>(.*?)</title>", page.text, re.S | re.I)
        if m:
            title = " ".join(m.group(1).split())
        print("%s\t%s" % (rel, title[:100]))


def fetch(commit, dest):
    dirs = ["css/CSS2", "css/css-tables", "css/css-lists", "css/css-backgrounds", "css/css-images",
            "css/css-color-adjust", "css/css-text-decor", "css/support", "html/rendering", "editing",
            "fonts"]
    if not os.path.isdir(os.path.join(dest, ".git")):
        subprocess.check_call(["git", "clone", "--filter=blob:none", "--no-checkout", WPT_URL, dest])
    subprocess.check_call(["git", "-C", dest, "sparse-checkout", "set"] + dirs)
    subprocess.check_call(["git", "-C", dest, "checkout", commit])
    return dest


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--wpt", help="an existing WPT checkout")
    ap.add_argument("--fetch", action="store_true", help="sparse-clone WPT into --dest first")
    ap.add_argument("--commit", default="master")
    ap.add_argument("--dest", default=os.path.join(REPO, "target", "wpt-upstream"))
    ap.add_argument("--list-eligible", metavar="AREA")
    ap.add_argument("--only", choices=["reftests", "editing"])
    ap.add_argument("--from-vendored", action="store_true",
                    help="re-generate normalized/ and doc/working/ from tests/wpt/upstream (no WPT "
                         "checkout needed; skipped.tsv and the editing data are kept)")
    args = ap.parse_args()
    root = args.wpt
    if args.fetch:
        root = fetch(args.commit, args.dest)
    if args.from_vendored:
        root = os.path.join(OUT, "upstream")
        args.only = "reftests"
    if not root:
        ap.error("give --wpt <checkout>, --fetch or --from-vendored")
    if args.list_eligible:
        list_eligible(root, args.list_eligible)
        return
    commit = git_head(root)
    if args.from_vendored:
        # The recorded commit is the one the upstream copies came from.
        m = re.search(r"@ ([0-9a-f]{40})", open(os.path.join(OUT, "reftests.tsv"), encoding="utf-8").read())
        commit = m.group(1) if m else commit
    if args.only in (None, "reftests"):
        vendor_reftests(root, commit, from_vendored=args.from_vendored)
    if args.only in (None, "editing"):
        vendor_editing(root, commit)


if __name__ == "__main__":
    main()
