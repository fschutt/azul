#!/usr/bin/env python3
"""APPS23 proof "Tab+Enter": rows, chips and swatches that answered only the mouse are tab stops, and Enter
on them does what a click does. Headless. tab_enter.py BINDIR OUT"""
import os, sys, tempfile, time, json
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import azlin_e2e as e
from azlin_e2e import Failure

bindir, out = sys.argv[1], sys.argv[2]
ONLY = set(sys.argv[3:])  # the apps to prove (all when none is named)
os.makedirs(out, exist_ok=True)
sys.path.insert(0, os.path.dirname(e.__file__))
from azpdf_resize_e2e import fixture

def focused(app):
    v = app.value("get_focus_state")
    node = (v or {}).get("focused_node") if isinstance(v, dict) else None
    return node or {}

def texts_under(app, dom, index):
    nodes = [d for d in e.dicts(app.op("get_node_hierarchy", dom_id=dom)) if "index" in d]
    by = {n["index"]: n for n in nodes}
    out, stack = [], [index]
    while stack:
        n = by.get(stack.pop())
        if not n:
            continue
        if n.get("text"):
            out.append(n["text"])
        stack.extend(n.get("children") or [])
    return out

def tab_to(app, want, limit=80, exact=False, selector=None):
    """Tab until the focused node holds a text containing `want` (or equal to it); the stops."""
    seen = []
    for i in range(limit):
        app.key("tab", frames=1)
        f = focused(app)
        if not f:
            continue
        words = texts_under(app, f.get("dom_id", 0), f.get("node_id"))
        seen.append(" / ".join(words)[:40])
        if selector and selector in (f.get("selector") or ""):
            print("  stop %d %s" % (i + 1, f.get("selector")), flush=True)
            return i + 1, f
        # A row is a small stop: a pane holding every row is a stop too (the shell's).
        if not selector and len(words) <= 5 and any((w == want) if exact else (want in w) for w in words):
            print("  stop %d %s: %s" % (i + 1, f.get("selector"), words[:6]), flush=True)
            return i + 1, f
    raise Failure("Tab never reached %r in %d stops: %s" % (want, limit, seen[-25:]))

results = []
def proof(name, binary, args, prepare, want, marker, pattern, env=None, exact=False, selector=None,
          check=None):
    if ONLY and name not in ONLY:
        return
    tmp = tempfile.mkdtemp(prefix="tabenter-")
    app = e.App(name, os.path.join(bindir, binary), ["--data-dir", tmp, "--language", "en"] + args,
                8836, tmp, 120, extra_env=env)
    try:
        app.until("the debug server", lambda: app.op("wait_frame") is not None)
        app.frame(3)
        prepare(app)
        stops, f = tab_to(app, want, exact=exact, selector=selector)
        if check:
            before = check(app)
            app.key("return")
            app.until("the change after Enter", lambda: check(app) != before)
            said = "%r -> %r" % (before, check(app))
        else:
            before = app.count(marker, pattern)
            app.key("return")
            app.until("%s after Enter" % marker, lambda: app.count(marker, pattern) > before)
            said = "%s %s" % (marker, app.printed(marker, pattern)[-1])
        app.screenshot(os.path.join(out, "%s.png" % name))
        results.append("PASS %s: %d Tab stops to %r (%s), Enter -> %s"
                       % (name, stops, selector or want, f.get("selector"), said))
    except Failure as err:
        results.append("FAIL %s: %s" % (name, err))
    finally:
        app.stop()
    print(results[-1], flush=True)

def none(app):
    pass

def contacts_ready(app):
    app.until("the contacts", lambda: app.printed("AZCONTACTS_LOADED"))
    app.frame(3)

proof("azcontacts", "AzContacts", ["--sample", "--size", "1100x720"], contacts_ready, "Ben",
      "AZCONTACTS_SELECTED", r".*Ben.*")

def keys_ready(app):
    app.until("the sample vault", lambda: app.shows("All items ("))
    app.frame(3)

proof("azkeys", "AzKeys", ["--sample", "--size", "1200x760"], keys_ready, "CodeHost",
      "AZKEYS_SELECTED", r"CodeHost.*")

pdf = os.path.join(tempfile.mkdtemp(prefix="tabenter-pdf-"), "four.pdf")
fixture(pdf)
def pdf_ready(app):
    app.until("page 1", lambda: app.printed("AZPDF_RENDERED", r"1 dom"))
    app.frame(3)

proof("azpdf", "AzPdf", [pdf, "--size", "1280x860"], pdf_ready, "3", "AZPDF_PAGE", r"3", exact=True)
def clock_ready(app):
    app.until("the world clock", lambda: app.shows("Tokyo"))
    app.frame(3)

proof("azclock", "AzClock", ["--sample", "--size", "720x560", "--screen", "world"], clock_ready, "Tokyo",
      None, None, check=lambda app: app.shows("Show here"))

def fg(app):
    v = app.value("get_node_css_properties", selector="#__azphoto_fg-chip")
    props = (v or {}).get("properties") or [] if isinstance(v, dict) else []
    return [p for p in props if p.startswith("background")]

proof("azphoto", "AzPhoto", ["--sample", "--theme", "flat", "--mode", "light"], lambda app: app.frame(3), "",
      None, None, selector="__azphoto_swatch-3", check=fg)
def bin_selected(app):
    f = focused(app)
    v = app.value("get_node_css_properties", node_id=f.get("node_id")) if f else None
    props = (v or {}).get("properties") or [] if isinstance(v, dict) else []
    return [p for p in props if p.startswith("background")]

def videocut_ready(app):
    app.until("the sample project", lambda: app.printed("AZVIDEOCUT_PROJECT"))
    app.frame(3)

proof("azvideocut", "AzVideoCut", ["--sample"], videocut_ready, "Colour bars", None, None, check=bin_selected)
print("\n".join(results))
sys.exit(1 if any(r.startswith("FAIL") for r in results) else 0)
