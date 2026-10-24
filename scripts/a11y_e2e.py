#!/usr/bin/env python3
"""Accessibility end to end, for every app: what a screen reader and a keyboard user meet.

Each app starts on a fresh data folder under its debug server (headless by default, the way the
other E2E scripts start them), in `--language en` (or the language given). For each window the
script reads the accessibility tree the platform adapter holds (`get_accessibility_tree`) and
checks:

  1. the tree is the one a screen reader can walk: every element's parent lists it, and walking
     backwards (a node's parent and its place among the parent's children, the way VoiceOver finds
     the previous element) visits exactly the reverse of walking forwards. The walk filters the
     tree the way accesskit's adapters do (`accesskit_consumer::common_filter`: generic containers
     flattened, hidden subtrees and clipped-away children left out);
  2. every interactive element (it takes focus or a click) has a role and a non-empty name, and
     the name is translated text, not a translation key (`azdrive-add-drive`);
  3. the window root names the app's language (`--language de`: German);
  4. Tab and Shift+Tab visit the same stops, Shift+Tab in reverse order, each a named element
     the tree holds and a screen reader reaches;
  5. the tree stays valid across interactions: Tab, a resize across the layout breakpoints,
     the settings window (Cmd/Ctrl+,) opened and closed - no refused update, every focus in the
     tree;
  6. the app printed no `[azul][a11y...]` warning.

With `--real-window` the apps run in real windows (no AZ_BACKEND=headless) and the script also
walks the macOS accessibility tree itself, through the AX API (ApplicationServices, via
ctypes): AXChildren / AXParent consistency and the AXLanguage of the elements - what
VoiceOver reads. It needs the terminal to be allowed under System Settings > Privacy & Security
> Accessibility; without that it says so and skips the AX half. It never moves the real mouse or
types on the real keyboard.

Usage (after building libazul with the debug server and the apps):

    python3 scripts/a11y_e2e.py [--apps AzDrive,AzMail] [--language en|de] [--real-window]
        [--debug-port 8890] [--timeout 90] [--keep]
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time

import azlin_e2e as e2e
from azlin_e2e import Failure

# Every app, and the options it starts with on top of the shared `--data-dir <dir> --size
# <WxH> --language <l>` (`--sample` where the app has sample content to show).
APPS = {
    "AzCalculator": ["--sample"],
    "AzCalendar": [],
    "AzClock": ["--sample"],
    "AzCode": ["--sample"],
    "AzContacts": ["--sample"],
    "AzDashboard": [],
    "AzDrive": [],
    "AzERP": [],
    "AzPasswordManager": ["--sample"],
    "AzMail": ["--sample"],
    "AzMaps": [],
    "AzMeet": [],
    "AzMonitor": ["--sample"],
    "AzMusic": ["--sample"],
    "AzNews": [],
    "AzNotes": ["--sample"],
    "AzPaint": [],
    "AzPdf": [],
    "AzPhoto": ["--sample"],
    "AzPlayer": [],
    "AzReader": [],
    "AzReview": [],
    "AzSetup": [],
    "AzSheets": ["--sample"],
    "AzShow": ["--sample"],
    "AzTasks": ["--sample"],
    "AzTerm": ["--sample"],
    "AzVideoCut": ["--sample"],
    "AzWriter": ["--sample"],
}
# Apps whose data folder option is not `--data-dir`.
DATA_OPTION = {"AzTasks": "--data"}

# A string that is still a Fluent message id: lowercase words joined by hyphens.
KEY_LIKE = re.compile(r"^[a-z][a-z0-9]*(-[a-z0-9]+)+$")
# Roles that say nothing about what a control is.
SILENT_ROLES = {"Unknown", "GenericContainer"}
# Roles a placeholder may name (a text field's hint is what VoiceOver reads for it).
TEXT_ROLES = {"TextInput", "MultilineTextInput", "SearchInput", "PasswordInput", "EmailInput",
              "UrlInput", "PhoneNumberInput", "NumberInput", "ComboBox"}

INCLUDE, EXCLUDE_NODE, EXCLUDE_SUBTREE = "include", "exclude_node", "exclude_subtree"


# ---- the tree as an accesskit adapter filters it ----

class PlatformTree:
    """The published tree (`get_accessibility_tree`) filtered the way
    `accesskit_consumer::common_filter` filters it for every adapter."""

    def __init__(self, answer):
        self.root = answer["root"]
        self.focus = answer["focus"]
        self.rejection = answer.get("last_rejection")
        self.nodes = {n["id"]: n for n in answer["nodes"]}
        self._filter = {}

    def node(self, nid):
        return self.nodes.get(nid)

    def parent(self, nid):
        node = self.nodes.get(nid)
        return node.get("parent") if node else None

    def hidden(self, nid):
        # accesskit inherits `hidden`.
        for _ in range(4096):
            node = self.nodes.get(nid)
            if node is None:
                return False
            if node.get("hidden"):
                return True
            nid = node.get("parent")
        return False

    def base(self, nid):
        node = self.nodes[nid]
        if nid == self.focus and nid != self.root:
            return INCLUDE
        if self.hidden(nid):
            return EXCLUDE_SUBTREE
        if node["role"] in ("GenericContainer", "TextRun"):
            return EXCLUDE_NODE
        return None

    def without_parent_checks(self, nid):
        return self.base(nid) or INCLUDE

    def filtered_children(self, nid, filt):
        out = []
        for child in (self.nodes.get(nid) or {}).get("children") or []:
            if child not in self.nodes:
                continue
            result = filt(child)
            if result == INCLUDE:
                out.append(child)
            elif result == EXCLUDE_NODE:
                out.extend(self.filtered_children(child, filt))
        return out

    def filtered_parent(self, nid, filt):
        parent = self.parent(nid)
        for _ in range(4096):
            if parent is None:
                return None
            if filt(parent) == INCLUDE:
                return parent
            parent = self.parent(parent)
        return None

    def siblings(self, nid, filt, following):
        """The node's filtered siblings after (`following`) or before it, nearest first."""
        parent = self.parent(nid)
        if parent is None:
            return []
        kids = [c for c in self.nodes[parent].get("children") or [] if c in self.nodes]
        if nid not in kids:
            return []
        at = kids.index(nid)
        side = kids[at + 1:] if following else list(reversed(kids[:at]))
        out = []
        for sib in side:
            result = filt(sib)
            if result == INCLUDE:
                out.append(sib)
            elif result == EXCLUDE_NODE:
                inner = self.filtered_children(sib, filt)
                out.extend(inner if following else list(reversed(inner)))
        return out

    @staticmethod
    def intersects(a, b):
        return min(a[2], b[2]) > max(a[0], b[0]) and min(a[3], b[3]) > max(a[1], b[1])

    def filter(self, nid):
        if nid in self._filter:
            return self._filter[nid]
        result = self.base(nid)
        if result is None:
            result = INCLUDE
            parent = self.parent(nid)
            if parent is not None and self.filter(parent) == EXCLUDE_SUBTREE:
                result = EXCLUDE_SUBTREE
            else:
                clip = self.filtered_parent(nid, self.without_parent_checks)
                clip_node = self.nodes.get(clip) if clip is not None else None
                bbox = self.nodes[nid].get("bounds")
                if clip_node and clip_node.get("clips_children") and bbox and clip_node.get("bounds"):
                    pbox = clip_node["bounds"]

                    def first_inside(sibs):
                        if not sibs:
                            return False
                        b = self.nodes[sibs[0]].get("bounds")
                        return bool(b) and self.intersects(b, pbox)

                    if not self.intersects(bbox, pbox) and not (
                            first_inside(self.siblings(nid, self.without_parent_checks, True))
                            or first_inside(self.siblings(nid, self.without_parent_checks, False))):
                        result = EXCLUDE_SUBTREE
        self._filter[nid] = result
        return result

    def forward(self):
        out = []

        def visit(nid, depth=0):
            if depth > 512:
                return
            for child in self.filtered_children(nid, self.filter):
                out.append(child)
                visit(child, depth + 1)

        visit(self.root)
        return out

    def previous(self, nid):
        parent = self.filtered_parent(nid, self.filter)
        if parent is None:
            return None, None
        siblings = self.filtered_children(parent, self.filter)
        if nid not in siblings:
            return None, "element %s names %s as its parent, which does not list it" % (
                self.describe(nid), self.describe(parent))
        at = siblings.index(nid)
        if at == 0:
            return (None if parent == self.root else parent), None
        before = siblings[at - 1]
        for _ in range(4096):
            kids = self.filtered_children(before, self.filter)
            if not kids:
                break
            before = kids[-1]
        return before, None

    def describe(self, nid):
        node = self.nodes.get(nid) or {}
        return "#%s %s %r" % (nid, node.get("role"), node.get("name") or "")


def check_tree(tree, what, problems, language):
    """Checks 1-3 on one window's tree; appends to `problems`."""
    if tree.rejection:
        problems.append("%s: the last tree update was refused: %s" % (what, tree.rejection))
    forward = tree.forward()
    for nid in forward:
        parent = tree.filtered_parent(nid, tree.filter)
        if parent is not None and nid not in tree.filtered_children(parent, tree.filter):
            problems.append("%s: %s is not among its parent's children" % (what, tree.describe(nid)))
    if forward:
        backward, node, guard = [forward[-1]], forward[-1], 0
        while guard < 100000:
            guard += 1
            before, error = tree.previous(node)
            if error:
                problems.append("%s: walking backwards: %s" % (what, error))
                break
            if before is None:
                break
            backward.append(before)
            node = before
        backward.reverse()
        if backward != forward:
            first = next((i for i, (a, b) in enumerate(zip(backward, forward)) if a != b),
                         min(len(backward), len(forward)))
            problems.append("%s: walking backwards visits %d elements, forwards %d; they part at %s"
                            % (what, len(backward), len(forward),
                               tree.describe(forward[first]) if first < len(forward) else "the end"))
    reachable = set(forward)
    if tree.focus != tree.root and tree.focus not in reachable:
        problems.append("%s: the focus %s is not reachable from the window" % (
            what, tree.describe(tree.focus)))
    for nid in forward:
        node = tree.nodes[nid]
        if not (node.get("focusable") or node.get("clickable")):
            continue
        name = (node.get("name") or "").strip()
        if node["role"] in SILENT_ROLES:
            problems.append("%s: interactive %s has no role a screen reader can say" % (
                what, tree.describe(nid)))
        if not name and not (node["role"] in TEXT_ROLES and (node.get("placeholder") or "").strip()):
            problems.append("%s: interactive %s has no name" % (what, tree.describe(nid)))
        elif KEY_LIKE.match(name):
            problems.append("%s: %s is named by an untranslated key" % (what, tree.describe(nid)))
    root = tree.nodes.get(tree.root) or {}
    spoken = root.get("language") or ""
    if not spoken.lower().startswith(language.lower()):
        problems.append("%s: the window root names the language %r, the app runs in %r" % (
            what, spoken, language))
    return reachable


def a11y_id(dom_id, node_id):
    return (int(dom_id) << 32) | (int(node_id) + 1)


def focused_id(app):
    state = app.value("get_focus_state")
    node = state.get("focused_node") if isinstance(state, dict) else None
    if not node:
        return None
    return a11y_id(node["dom_id"], node["node_id"])


def tab_walk(app, shift, limit=120):
    """The stops Tab (or Shift+Tab) visits from where the focus is, once round."""
    seen = []
    for _ in range(limit):
        app.key("tab", shift=shift, frames=2)
        nid = focused_id(app)
        if nid is None or nid in seen:
            break
        seen.append(nid)
    return seen


def check_tab(app, what, problems):
    """Check 4."""
    forward = tab_walk(app, shift=False)
    if not forward:
        return
    backward = tab_walk(app, shift=True)
    if sorted(forward) != sorted(backward):
        problems.append("%s: Tab visits %d stops, Shift+Tab %d (only Tab: %s; only Shift+Tab: %s)" % (
            what, len(forward), len(backward),
            [tree.describe(n) for n in set(forward) - set(backward)][:4],
            [tree.describe(n) for n in set(backward) - set(forward)][:4]))
    else:
        # Shift+Tab from the last Tab stop runs back through the others; rotated to start
        # from the first stop, it is the Tab order backwards.
        first = forward[0]
        rotated = backward[backward.index(first):] + backward[:backward.index(first)]
        expected = [first] + list(reversed(forward[1:]))
        if rotated != expected:
            problems.append("%s: Shift+Tab does not visit the Tab stops in reverse" % what)
    tree = PlatformTree(app.value("get_accessibility_tree"))
    reachable = set(tree.forward())
    for nid in forward:
        if nid not in tree.nodes:
            problems.append("%s: Tab lands on node %s, which the accessibility tree does not hold"
                            % (what, nid))
        elif nid not in reachable:
            problems.append("%s: Tab lands on %s, which a screen reader cannot reach" % (
                what, tree.describe(nid)))


# ---- the real window, through the macOS AX API ----

class AX:
    """The macOS accessibility API (ApplicationServices) through ctypes: what VoiceOver reads."""

    def __init__(self):
        import ctypes
        self.c = ctypes
        self.app_services = ctypes.cdll.LoadLibrary(
            "/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices")
        self.cf = ctypes.cdll.LoadLibrary(
            "/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation")
        vp = ctypes.c_void_p
        self.app_services.AXIsProcessTrusted.restype = ctypes.c_bool
        self.app_services.AXUIElementCreateApplication.restype = vp
        self.app_services.AXUIElementCreateApplication.argtypes = [ctypes.c_int]
        self.app_services.AXUIElementCopyAttributeValue.restype = ctypes.c_int32
        self.app_services.AXUIElementCopyAttributeValue.argtypes = [vp, vp, ctypes.POINTER(vp)]
        self.cf.CFStringCreateWithCString.restype = vp
        self.cf.CFStringCreateWithCString.argtypes = [vp, ctypes.c_char_p, ctypes.c_uint32]
        self.cf.CFGetTypeID.restype = ctypes.c_ulong
        self.cf.CFGetTypeID.argtypes = [vp]
        self.cf.CFArrayGetTypeID.restype = ctypes.c_ulong
        self.cf.CFStringGetTypeID.restype = ctypes.c_ulong
        self.cf.CFArrayGetCount.restype = ctypes.c_long
        self.cf.CFArrayGetCount.argtypes = [vp]
        self.cf.CFArrayGetValueAtIndex.restype = vp
        self.cf.CFArrayGetValueAtIndex.argtypes = [vp, ctypes.c_long]
        self.cf.CFStringGetCString.restype = ctypes.c_bool
        self.cf.CFStringGetCString.argtypes = [vp, ctypes.c_char_p, ctypes.c_long, ctypes.c_uint32]
        self.cf.CFEqual.restype = ctypes.c_bool
        self.cf.CFEqual.argtypes = [vp, vp]
        self.cf.CFRetain.restype = vp
        self.cf.CFRetain.argtypes = [vp]
        self._names = {}

    def trusted(self):
        return bool(self.app_services.AXIsProcessTrusted())

    def name(self, attr):
        if attr not in self._names:
            self._names[attr] = self.cf.CFStringCreateWithCString(None, attr.encode(), 0x08000100)
        return self._names[attr]

    def attr(self, element, attr):
        out = self.c.c_void_p()
        err = self.app_services.AXUIElementCopyAttributeValue(element, self.name(attr), self.c.byref(out))
        return out.value if err == 0 else None

    def string(self, value):
        if not value or self.cf.CFGetTypeID(value) != self.cf.CFStringGetTypeID():
            return None
        buf = self.c.create_string_buffer(4096)
        return buf.value.decode("utf-8", "replace") if self.cf.CFStringGetCString(
            value, buf, 4096, 0x08000100) else None

    def children(self, element):
        array = self.attr(element, "AXChildren")
        if not array or self.cf.CFGetTypeID(array) != self.cf.CFArrayGetTypeID():
            return []
        # Each child is retained: the array is never released while the walk runs.
        return [self.cf.CFRetain(self.cf.CFArrayGetValueAtIndex(array, i))
                for i in range(self.cf.CFArrayGetCount(array))]

    def application(self, pid):
        return self.app_services.AXUIElementCreateApplication(pid)


def check_real_window(ax, pid, what, problems, language, limit=3000):
    """AXChildren / AXParent consistency and AXLanguage over the app's first window."""
    app_el = ax.application(pid)
    windows = []
    for _ in range(40):
        windows = ax.children(app_el)
        windows = [w for w in windows if ax.string(ax.attr(w, "AXRole")) == "AXWindow"]
        if windows:
            break
        time.sleep(0.25)
    if not windows:
        problems.append("%s: the AX API shows no window" % what)
        return
    window = windows[0]
    stack, visited, order = [window], 0, []
    languages = set()
    while stack and visited < limit:
        element = stack.pop()
        visited += 1
        kids = ax.children(element)
        for kid in kids:
            parent = ax.attr(kid, "AXParent")
            if not parent or not ax.cf.CFEqual(parent, element):
                role = ax.string(ax.attr(kid, "AXRole"))
                title = ax.string(ax.attr(kid, "AXTitle")) or ax.string(ax.attr(kid, "AXDescription"))
                problems.append("%s: AX element %s %r is a child whose AXParent is another element: "
                                "VoiceOver cannot move back from it" % (what, role, title or ""))
        for kid in kids:
            lang = ax.string(ax.attr(kid, "AXLanguage"))
            if lang:
                languages.add(lang)
        order.append(element)
        stack.extend(reversed(kids))
    if languages and not any(l.lower().startswith(language.lower()) for l in languages):
        problems.append("%s: the AX elements name the languages %s, the app runs in %r" % (
            what, sorted(languages), language))
    if not languages:
        problems.append("%s: no AX element answers AXLanguage" % what)


# ---- one app ----

def a11y_warnings(app):
    lines = []
    for path in {app.out_path, app.err_path}:
        try:
            with open(path, encoding="utf-8", errors="replace") as f:
                lines.extend(l.rstrip("\n") for l in f if "[azul][a11y" in l)
        except OSError:
            pass
    # One per finding: the node index is not part of what to fix.
    return sorted({re.sub(r"node \d+", "node N", l) for l in lines})


class RealWindowApp(e2e.App):
    """An app in a real window: its debug server, but no AZ_BACKEND=headless."""

    def __init__(self, tag, binary, args, port, logs, timeout):
        self.tag = tag
        self.port = port
        self.deadline = time.time() + timeout
        self.out_path = os.path.join(logs, "%s.stdout" % tag)
        self.err_path = os.path.join(logs, "%s.stderr" % tag)
        self.capped = False
        env = dict(os.environ)
        env.pop("AZ_BACKEND", None)
        env["AZ_DEBUG"] = str(port)
        self.process = subprocess.Popen([binary] + list(args), env=env, stdin=subprocess.DEVNULL,
                                        stdout=open(self.out_path, "wb"),
                                        stderr=open(self.err_path, "wb"))


def start(name, binary, opts, logs, real):
    data_dir = tempfile.mkdtemp(prefix="a11y-%s-" % name.lower())
    common = [DATA_OPTION.get(name, "--data-dir"), data_dir, "--size", "1200x800"]
    # An app that does not take --language yet starts in its own; the language check then
    # expects English.
    attempts = [common + APPS[name] + ["--language", opts.language], common + APPS[name]]
    for argv in attempts:
        if real:
            app = RealWindowApp(name, binary, argv, opts.debug_port, logs, opts.timeout)
        else:
            app = e2e.App(name, binary, argv, opts.debug_port, logs, opts.timeout)
        try:
            app.until("the debug server", lambda: app.op("wait_frame") is not None)
            app.frame(3)
            return app, data_dir, argv
        except Failure:
            app.stop()
            if "--language" not in argv:
                raise
    raise Failure("%s did not start" % name)


def check_app(name, binary, opts, logs, ax):
    problems = []
    app, data_dir, argv = start(name, binary, opts, logs, opts.real_window)
    language = opts.language if "--language" in argv else "en"
    try:
        tree = PlatformTree(app.value("get_accessibility_tree"))
        check_tree(tree, name, problems, language)
        check_tab(app, name, problems)

        # Interactions: a resize across the breakpoints, the settings window.
        for size in ((560, 700), (1200, 800)):
            app.must("resize", width=size[0], height=size[1])
            app.frame(3)
            check_tree(PlatformTree(app.value("get_accessibility_tree")),
                       "%s at %dx%d" % (name, size[0], size[1]), problems, language)
        known = set(app.window_ids())
        app.key("comma", primary=True, frames=4)
        for wid in [w for w in app.window_ids() if w not in known]:
            window = e2e.InWindow(app, wid)
            check_tree(PlatformTree(window.value("get_accessibility_tree")),
                       "%s settings window" % name, problems, language)
        app.key("escape", frames=3)
        after = PlatformTree(app.value("get_accessibility_tree"))
        check_tree(after, "%s after closing the settings" % name, problems, language)
        focus = focused_id(app)
        if focus is not None and focus not in after.nodes:
            problems.append("%s: the focus names node %s, which the tree does not hold" % (name, focus))

        if ax is not None:
            check_real_window(ax, app.process.pid, name, problems, language)

        for line in a11y_warnings(app):
            problems.append("%s printed: %s" % (name, line[:300]))
    finally:
        app.stop()
        if not opts.keep:
            shutil.rmtree(data_dir, ignore_errors=True)
    return problems


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--apps", help="comma-separated app names (default: every app)")
    parser.add_argument("--language", default="en")
    parser.add_argument("--real-window", action="store_true")
    parser.add_argument("--debug-port", type=int, default=8890)
    parser.add_argument("--timeout", type=int, default=90)
    parser.add_argument("--keep", action="store_true")
    opts = parser.parse_args()

    names = [n.strip() for n in opts.apps.split(",")] if opts.apps else list(APPS)
    unknown = [n for n in names if n not in APPS]
    if unknown:
        print("unknown apps: %s (known: %s)" % (unknown, ", ".join(APPS)))
        return 2

    ax = None
    if opts.real_window:
        if sys.platform != "darwin":
            print("--real-window reads the macOS AX API; this is %s" % sys.platform)
            return 2
        ax = AX()
        if not ax.trusted():
            print("[a11y-e2e] this terminal may not read other apps' accessibility trees: allow it "
                  "under System Settings > Privacy & Security > Accessibility; the AX half is "
                  "skipped")
            ax = None

    logs = tempfile.mkdtemp(prefix="a11y-e2e-")
    report = {}
    for name in names:
        try:
            binary = e2e.find_binary(name, None, name.upper() + "_BIN")
        except Failure as e:
            report[name] = ["SKIP: %s" % e]
            print("[a11y-e2e] %s: SKIP (%s)" % (name, e), flush=True)
            continue
        started = time.time()
        try:
            problems = check_app(name, binary, opts, logs, ax)
        except Failure as e:
            problems = ["FAIL: %s" % e]
        report[name] = problems
        print("[a11y-e2e] %s: %s (%.1fs)" % (
            name, "ok" if not problems else "%d problem(s)" % len(problems), time.time() - started),
            flush=True)
        for p in problems[:40]:
            print("    " + p, flush=True)
    with open(os.path.join(logs, "report.json"), "w") as f:
        json.dump(report, f, indent=1)
    failed = [n for n, p in report.items() if p and not p[0].startswith("SKIP")]
    print("[a11y-e2e] %d apps, %d with problems; report and logs in %s" % (
        len(report), len(failed), logs))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
