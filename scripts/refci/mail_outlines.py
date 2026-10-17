#!/usr/bin/env python3
"""The trees Chrome builds from the mail corpus, as outlines: the expected trees of
layout/tests/real_mail_html_parses_like_a_browser.rs.

    python3 scripts/refci/mail_outlines.py

For every tests/mail_corpus/<dir>/<mail>.html, Chrome (scripts/refci/cdp.py, scripting off)
parses the RAW mail with `DOMParser` (text/html) and this writes
tests/mail_corpus/outlines/<dir>/<mail>.txt: two lines, `head: <outline>` and
`body: <outline>`, the children of <head> and <body> in the outline format of
`azul_core::xml::html::outline` (with attributes): an element is its name, then
`[name=value ...]`, then `{children}`; a text is a JSON string with its white space runs
collapsed to one space; white-space-only text is left out.
"""

import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, HERE)
from cdp import Chrome  # noqa: E402

CORPUS = os.path.join(REPO, "tests", "mail_corpus")
OUT = os.path.join(CORPUS, "outlines")

OUTLINE_JS = r"""
(function(src){
  const doc = new DOMParser().parseFromString(src, 'text/html');
  function ws(t){ return t.replace(/[ \t\n\r\f]+/g, ' '); }
  function out(n){
    if (n.nodeType === 3) { if (/^[ \t\n\r\f]*$/.test(n.data)) return null; return JSON.stringify(ws(n.data)); }
    if (n.nodeType !== 1) return null;
    let s = n.localName;
    if (n.prefix) s = n.prefix + ':' + s;
    if (n.attributes.length) {
      const a = [];
      for (const at of n.attributes) a.push(at.name + '=' + at.value);
      s += '[' + a.join(' ') + ']';
    }
    const kids = inner(n);
    if (kids) s += '{' + kids + '}';
    return s;
  }
  function inner(n){
    const kids = [];
    const cn = n.localName === 'template' ? n.content.childNodes : n.childNodes;
    for (const c of cn) { const o = out(c); if (o !== null) kids.push(o); }
    return kids.join(' ');
  }
  return JSON.stringify({head: inner(doc.head), body: inner(doc.body)});
})
"""


def main():
    mails = []
    for d in sorted(os.listdir(CORPUS)):
        full = os.path.join(CORPUS, d)
        if not os.path.isdir(full) or d == "outlines":
            continue
        for f in sorted(os.listdir(full)):
            if f.endswith(".html"):
                mails.append((d, f))
    with Chrome() as chrome:
        page = chrome.open("about:blank", width=400, height=300)
        for d, f in mails:
            with open(os.path.join(CORPUS, d, f), encoding="utf-8") as fh:
                src = fh.read()
            r = json.loads(page.evaluate("(" + OUTLINE_JS + ")(" + json.dumps(src) + ")"))
            os.makedirs(os.path.join(OUT, d), exist_ok=True)
            target = os.path.join(OUT, d, f[: -len(".html")] + ".txt")
            with open(target, "w", encoding="utf-8") as fh:
                fh.write("head: " + r["head"] + "\nbody: " + r["body"] + "\n")
            print("wrote", os.path.relpath(target, REPO))


if __name__ == "__main__":
    main()
