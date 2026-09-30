#!/usr/bin/env python3
"""Mail HTML in azul vs Chrome: compare the layout BOXES, not the pixels.

    python3 scripts/refci/mail_boxes.py                       # the whole corpus
    python3 scripts/refci/mail_boxes.py --only cerberus       # a subset
    python3 scripts/refci/mail_boxes.py --sanitizer normalize # without AzMail's sanitizer

Every mail of the corpus (tests/mail_corpus/, see SOURCES.tsv) is
1. sanitized the way AzMail shows it: `azmail-sanitize` (AzMail's
   examples/azul-mail/src/html.rs as a filter; build it with
   `AZ_LINK_PATH=$PWD/target/azul-lib cargo build --release -p AzMail --bin azmail-sanitize`),
   or `--sanitizer normalize` (scripts/refci/htmlnorm.py: well-formed, styles
   kept, nothing removed) or `none` (the raw bytes);
2. given a class `azr-<n>` on every element, so the two engines' boxes pair up;
3. rendered by headless Chrome (scripts/refci/cdp.py; every host name fails to
   resolve, so nothing is fetched) and by azul headless (a prebuilt app with the
   debug server, scripts/refci/azul_debug.py, the `mount` op), both at the same
   viewport, with the same base font pinned (--base-css) so that text metrics
   are as close as two text stacks get;
4. compared element by element: Chrome's getBoundingClientRect() against
   azul's get_all_nodes_layout rect. Inline boxes are skipped by default
   (azul reports no rect for them; --include-inline keeps them).

Output (default target/refci/mail/): index.html (per mail: the first element
that diverges in document order - usually the root cause - the worst
mismatches, and the two full-page screenshots side by side with those boxes
outlined), summary.tsv, and <mail>/boxes.json. The console gets one line per
mail. The exit code is 0 unless a mail could not be processed at all
(--fail-on-mismatch makes mismatches fail too).

Needs: python3, Google Chrome or Chromium (CHROME=<binary>), a prebuilt azul
app and its library (AZUL_APP, AZUL_LIB_DIR; see azul_debug.py).
"""

import argparse
import html
import json
import os
import re
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, HERE)
import htmlnorm  # noqa: E402
from azul_debug import AzulHeadless  # noqa: E402
from cdp import Chrome  # noqa: E402

BASE_CSS = "html { font-family: Arial, Helvetica, sans-serif; font-size: 16px; }"
BOXES_JS = r"""(() => {
  const out = [];
  for (const el of document.querySelectorAll('[class]')) {
    const m = /(?:^|\s)azr-(\d+)(?:\s|$)/.exec(el.getAttribute('class') || '');
    if (!m) continue;
    const r = el.getBoundingClientRect();
    const cs = getComputedStyle(el);
    out.push({n: +m[1], tag: el.tagName.toLowerCase(), x: r.x, y: r.y, w: r.width, h: r.height,
              display: cs.display, hidden: cs.visibility === 'hidden'});
  }
  const d = document.documentElement;
  return JSON.stringify({boxes: out, width: d.scrollWidth, height: d.scrollHeight});
})()"""


def sanitize(raw, mode, sanitizer_bin, lib_dir):
    """The mail as azul's XML loader will see it, and which sanitizer made it."""
    if mode in ("azmail", "auto") and os.access(sanitizer_bin, os.X_OK):
        env = dict(os.environ)
        for var in ("DYLD_LIBRARY_PATH", "LD_LIBRARY_PATH"):
            env[var] = lib_dir + (os.pathsep + env[var] if env.get(var) else "")
        p = subprocess.run([sanitizer_bin], input=raw.encode("utf-8"), capture_output=True, env=env, timeout=60)
        if p.returncode != 0:
            raise RuntimeError("azmail-sanitize failed: " + p.stderr.decode("utf-8", "replace")[-300:])
        return p.stdout.decode("utf-8"), "azmail"
    if mode == "azmail":
        raise RuntimeError(
            "no %s; build it: AZ_LINK_PATH=$PWD/target/azul-lib cargo build --release -p AzMail "
            "--bin azmail-sanitize (or pass --sanitizer normalize)" % sanitizer_bin)
    if mode == "none":
        return raw, "none"
    return htmlnorm.parse(raw).to_xhtml(), "normalize"


def tag_elements(xhtml):
    """(body markup with azr-<n> classes, css text, {n: tag})."""
    doc = htmlnorm.parse(xhtml, xhtml=True)
    tags = {}
    counter = [0]

    def walk(node):
        for c in node.children:
            if isinstance(c, htmlnorm.Node):
                counter[0] += 1
                n = counter[0]
                tags[n] = c.tag
                attrs = [(k, v) for k, v in c.attrs if k != "class"]
                old = dict(c.attrs).get("class") or ""
                attrs.append(("class", ("%s azr-%d" % (old, n)).strip()))
                c.attrs = attrs
                walk(c)

    walk(doc.body)
    out = ["<body" + htmlnorm.attrs_xml(doc.body.attrs) + ">"]
    for c in doc.body.children:
        htmlnorm.serialize(c, out)
    out.append("</body>")
    return "".join(out), doc.stylesheet_text(), tags


def chrome_boxes(chrome, path, width, height):
    page = chrome.open("file://" + os.path.abspath(path), width=width, height=height)
    try:
        data = json.loads(page.evaluate(BOXES_JS))
        png = page.screenshot(full_page=True)
    finally:
        page.close()
    return data, png


def azul_boxes(az, body, css, width, height):
    az.resize(width, height)
    az.mount(body, css)  # unmounts the previous document first
    nodes = az.all_nodes_layout()
    boxes = {}
    page_h = height
    for n in nodes:
        r = n.get("rect")
        if n.get("tag") == "html" and r:
            page_h = max(page_h, int(r["y"] + r["height"] + 0.5))
        for cls in n.get("classes") or []:
            m = re.fullmatch(r"azr-(\d+)", cls)
            if m and r:
                boxes[int(m.group(1))] = (r["x"], r["y"], r["width"], r["height"])
    if page_h > height:
        az.resize(width, min(page_h, 16000))
    png = az.screenshot()
    return boxes, png


def compare(chrome_data, azul, tags, include_inline, tol):
    rows = []
    for b in chrome_data["boxes"]:
        if b["hidden"] or b["display"] == "none":
            continue
        if b["display"] in ("inline", "contents") and not include_inline:
            continue
        n = b["n"]
        c = (b["x"], b["y"], b["w"], b["h"])
        a = azul.get(n)
        if a is None:
            if b["w"] * b["h"] > 0:
                rows.append({"n": n, "tag": b["tag"], "display": b["display"], "chrome": c, "azul": None,
                             "score": max(b["w"], b["h"]), "what": "no box in azul"})
            continue
        d = [a[i] - c[i] for i in range(4)]
        lim = [tol, tol, max(tol, 0.1 * c[2]), max(tol, 0.1 * c[3])]
        over = [abs(d[i]) - lim[i] for i in range(4)]
        score = max(over)
        if score > 0:
            what = []
            for i, name in enumerate(("x", "y", "width", "height")):
                if over[i] > 0:
                    what.append("%s %+.0f" % (name, d[i]))
            rows.append({"n": n, "tag": b["tag"], "display": b["display"], "chrome": c, "azul": a,
                         "score": round(score, 1), "what": ", ".join(what)})
    return rows


def fmt_box(b):
    return "-" if b is None else "%.0f,%.0f %.0fx%.0f" % b


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--corpus", default=os.path.join(REPO, "tests", "mail_corpus"))
    ap.add_argument("--out", default=os.path.join(REPO, "target", "refci", "mail"))
    ap.add_argument("--sanitizer", choices=["auto", "azmail", "normalize", "none"], default="auto")
    ap.add_argument("--sanitizer-bin", default=os.environ.get("AZMAIL_SANITIZE") or os.path.join(REPO, "target", "release", "azmail-sanitize"))
    ap.add_argument("--width", type=int, default=760)
    ap.add_argument("--height", type=int, default=1100)
    ap.add_argument("--tolerance", type=float, default=4.0, help="px; sizes also get 10%%")
    ap.add_argument("--base-css", default=BASE_CSS)
    ap.add_argument("--only", default="")
    ap.add_argument("--include-inline", action="store_true")
    ap.add_argument("--top", type=int, default=10)
    ap.add_argument("--fail-on-mismatch", action="store_true")
    args = ap.parse_args()

    mails = []
    for dp, dn, fn in os.walk(args.corpus):
        dn.sort()
        for f in sorted(fn):
            if f.endswith((".html", ".htm")):
                rel = os.path.relpath(os.path.join(dp, f), args.corpus)
                if args.only in rel:
                    mails.append(rel)
    if not mails:
        print("mail_boxes: no mail selected")
        return 1
    os.makedirs(args.out, exist_ok=True)

    results = []
    errors = 0
    started = time.time()
    lib_dir = os.environ.get("AZUL_LIB_DIR") or os.path.join(REPO, "target", "azul-lib")
    # One app at a time, memory-capped, and a fresh one every few mails: the
    # 8 GB Mac that hosts this panicked under an app that grew without bound.
    az = AzulHeadless(log_path=os.path.join(args.out, "azul.log"), cap_mb=1500, seconds=600)
    since_restart = 0
    with Chrome() as chrome:
        print("mail_boxes: %s vs azul (%s), %d mails, %dx%d" % (
            chrome.version.get("product"), os.path.basename(az.app), len(mails), args.width, args.height))
        for rel in mails:
            if since_restart >= 8 or not az.alive():
                az.restart()
                since_restart = 0
            since_restart += 1
            name = re.sub(r"[^A-Za-z0-9._-]+", "_", os.path.splitext(rel)[0])
            d = os.path.join(args.out, name)
            os.makedirs(d, exist_ok=True)
            try:
                with open(os.path.join(args.corpus, rel), "rb") as f:
                    raw_bytes = f.read()
                try:
                    raw = raw_bytes.decode("utf-8")
                except UnicodeDecodeError:
                    raw = raw_bytes.decode("latin-1")
                xhtml, used = sanitize(raw, args.sanitizer, args.sanitizer_bin, lib_dir)
                body, css, tags = tag_elements(xhtml)
                css = args.base_css + "\n" + css
                page = os.path.join(d, "chrome.html")
                with open(page, "w", encoding="utf-8") as f:
                    f.write('<!DOCTYPE html>\n<html><head><meta charset="utf-8"><style>\n%s\n</style></head>\n%s\n</html>\n'
                            % (css, body))
                cdata, cpng = chrome_boxes(chrome, page, args.width, args.height)
                abox, apng = azul_boxes(az, body, css, args.width, args.height)
                with open(os.path.join(d, "chrome.png"), "wb") as f:
                    f.write(cpng)
                with open(os.path.join(d, "azul.png"), "wb") as f:
                    f.write(apng)
                rows = compare(cdata, abox, tags, args.include_inline, args.tolerance)
                compared = sum(1 for b in cdata["boxes"]
                               if not b["hidden"] and b["display"] not in ("none",)
                               and (args.include_inline or b["display"] not in ("inline", "contents")))
                first = min(rows, key=lambda r: r["n"]) if rows else None
                worst = sorted(rows, key=lambda r: -r["score"])[: args.top]
                res = {"mail": rel, "name": name, "sanitizer": used, "compared": compared,
                       "mismatches": len(rows), "missing": sum(1 for r in rows if r["azul"] is None),
                       "first": first, "worst": worst, "chrome_page": [cdata["width"], cdata["height"]]}
                with open(os.path.join(d, "boxes.json"), "w", encoding="utf-8") as f:
                    json.dump({"result": res, "all": rows, "chrome": cdata["boxes"],
                               "azul": {str(k): v for k, v in abox.items()}}, f, indent=1)
                results.append(res)
                print("  %-44s %4d boxes, %3d mismatched (%d missing); first: %s" % (
                    rel, compared, len(rows), res["missing"],
                    "-" if not first else "<%s> azr-%d %s" % (first["tag"], first["n"], first["what"])))
            except Exception as e:  # noqa: BLE001 - one bad mail must not stop the corpus
                errors += 1
                results.append({"mail": rel, "name": name, "error": str(e)[:300]})
                print("  %-44s ERROR %s" % (rel, str(e)[:200]))
                if not az.alive():  # azul died on this mail: a fresh one for the next
                    az.restart()
                    since_restart = 0
    az.close()

    write_report(args, results)
    total = sum(r.get("mismatches", 0) for r in results)
    print("mail_boxes: %d mails, %d mismatched boxes, %d errors in %.0fs; report %s" % (
        len(results), total, errors, time.time() - started, os.path.join(args.out, "index.html")))
    if errors or (args.fail_on_mismatch and total):
        return 1
    return 0


def write_report(args, results):
    with open(os.path.join(args.out, "summary.tsv"), "w", encoding="utf-8") as f:
        f.write("# mail\tsanitizer\tboxes\tmismatches\tmissing\tfirst divergence\n")
        for r in results:
            if "error" in r:
                f.write("%s\t-\t-\t-\t-\tERROR %s\n" % (r["mail"], r["error"]))
                continue
            first = r["first"]
            f.write("%s\t%s\t%d\t%d\t%d\t%s\n" % (
                r["mail"], r["sanitizer"], r["compared"], r["mismatches"], r["missing"],
                "-" if not first else "<%s> azr-%d %s" % (first["tag"], first["n"], first["what"])))

    parts = ["""<!DOCTYPE html><html><head><meta charset="utf-8"><title>Mail boxes: azul vs Chrome</title>
<style>
:root { color-scheme: light dark; --fg: #1a1a1a; --bg: #ffffff; --muted: #666; --line: #ddd; }
@media (prefers-color-scheme: dark) { :root { --fg: #e6e6e6; --bg: #181818; --muted: #aaa; --line: #333; } }
body { font: 14px/1.4 system-ui, sans-serif; margin: 16px; color: var(--fg); background: var(--bg); }
table { border-collapse: collapse; margin: 8px 0; } td, th { border: 1px solid var(--line); padding: 2px 6px; text-align: left; }
.shots { display: flex; gap: 12px; align-items: flex-start; overflow-x: auto; }
.shot { position: relative; flex: none; } .shot img { display: block; border: 1px solid var(--line); }
.box { position: absolute; border: 2px solid; pointer-events: none; } .c { border-color: #0a0; } .a { border-color: #e00; }
.muted { color: var(--muted); }
</style></head><body><h1>Mail boxes: azul vs Chrome</h1>
<p class="muted">Boxes compared with a tolerance of %.0f px (sizes also 10%%). Green: Chrome's box; red: azul's box, for the worst mismatches.</p>
""" % args.tolerance]
    for r in results:
        parts.append("<h2>%s</h2>" % html.escape(r["mail"]))
        if "error" in r:
            parts.append("<p>ERROR: %s</p>" % html.escape(r["error"]))
            continue
        first = r["first"]
        parts.append("<p>sanitizer <b>%s</b>, %d boxes compared, <b>%d</b> mismatched (%d with no azul box). First divergence: %s</p>" % (
            r["sanitizer"], r["compared"], r["mismatches"], r["missing"],
            "none" if not first else html.escape("<%s> azr-%d: %s" % (first["tag"], first["n"], first["what"]))))
        if r["worst"]:
            parts.append("<table><tr><th>element</th><th>display</th><th>Chrome</th><th>azul</th><th>off by</th></tr>")
            for w in r["worst"]:
                parts.append("<tr><td>&lt;%s&gt; azr-%d</td><td>%s</td><td>%s</td><td>%s</td><td>%s</td></tr>" % (
                    html.escape(w["tag"]), w["n"], html.escape(w["display"]), fmt_box(w["chrome"]),
                    fmt_box(w["azul"]), html.escape(w["what"])))
            parts.append("</table>")
        parts.append('<div class="shots">')
        for label, img, key, cls in (("Chrome", "chrome.png", "chrome", "c"), ("azul", "azul.png", "azul", "a")):
            parts.append('<div class="shot"><div>%s</div><div style="position:relative"><img src="%s/%s" alt="%s">'
                         % (label, r["name"], img, label))
            for w in r["worst"][:5]:
                b = w[key]
                if b:
                    parts.append('<div class="box %s" style="left:%dpx;top:%dpx;width:%dpx;height:%dpx"></div>'
                                 % (cls, b[0], b[1], max(1, b[2]), max(1, b[3])))
            parts.append("</div></div>")
        parts.append("</div>")
    parts.append("</body></html>\n")
    with open(os.path.join(args.out, "index.html"), "w", encoding="utf-8") as f:
        f.write("".join(parts))


if __name__ == "__main__":
    sys.exit(main())
