#!/usr/bin/env python3
"""mobile_e2e.py: drive a phone app that carries the engine's debug server (a `mobile-e2e` libazul: features
debug-server + e2e-scripting) from outside, on an emulator or a simulator that is already running the app
(standard library only).

    mobile_e2e.py android --out DIR --port 8795 --package com.azul.AzWidgets [--scenario file.json ...]
    mobile_e2e.py ios     --out DIR --port 8795 --udid UDID --bundle com.azul.AzWidgets [--scenario file.json ...]

The app is started with the server's port in its environment (Android: `setprop debug.az.env AZ_DEBUG=<port>`
before the launch and `adb forward tcp:<port> tcp:<port>`; iOS Simulator: SIMCTL_CHILD_AZ_DEBUG=<port>, the
Simulator shares the host's loopback). This script then
  1. waits for the server and reads the window (get_state, list_windows, get_theme);
  2. takes the ENGINE's own frame (take_screenshot: a full repaint of the display list) and the DEVICE's frame
     (screencap / simctl io screenshot) of the same moment and keeps both, with the layout of every node
     (get_all_nodes_layout): the pair tells a layout/paint fault of the engine from a fault of the shell's
     present path (the engine frame is right and the device frame is not = the shell);
  3. runs each scenario file's tests through the server (`run_e2e_tests`, the format of tests/e2e/*.json and
     examples/*/e2e/*.json) and takes a device picture after each;
  4. touches: the server's in-process touch events (touch_start / touch_move / touch_end) as a swipe, on Android
     also `adb shell input swipe` / `input tap`, and says whether the page scrolled (get_scroll_states);
  5. the keyboard: a click on a text field and, on Android, whether the soft keyboard came up.
Everything lands in DIR like scripts/ci/mobile_qa.py's output (qa.json, qa.md, shots/, logs/).

    --audit-only          the phone-fit session of ANY app with a mobile-e2e build (Android): 1 and 2 above, then the layout audit
                          (scripts/ci/layout_audit.py: boxes over the window's edges, tappable and text boxes under the status and
                          navigation bars, touch targets under 48 dp, text boxes crossing an edge) in portrait AND landscape with
                          the engine's frame and the device's frame of each, the system dark mode asked of the engine (get_mode:
                          did the window's mode change, and did the picture), and the app's own script when --script names one.
    --script paint|writer|maps   a scripted check of one app: paint draws a stroke with a real swipe and undoes it (the app prints
                          AZPAINT_STROKES), writer taps into the page and types (the text in the accessibility tree, the keyboard,
                          whether the window shrinks above it, the ribbon against the width), maps taps the sidebar toggle at the
                          place the server names and double-taps the map (AZMAPS_SIDEBAR, AZMAPS_VIEW).
"""
import argparse
import base64
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mobile_qa as q  # noqa: E402
import layout_audit as la  # noqa: E402


class Server:
    def __init__(self, port, host="127.0.0.1"):
        self.url = f"http://{host}:{port}/"

    def op(self, op, timeout=40, **params):
        body = {"op": op}
        body.update(params)
        req = urllib.request.Request(self.url, data=json.dumps(body).encode("utf-8"), method="POST")
        with urllib.request.urlopen(req, timeout=timeout) as response:
            text = response.read().decode("utf-8") or "{}"
        try:
            return json.loads(text)
        except ValueError:
            return {"status": "error", "raw": text[:300]}

    def must(self, op, **params):
        answer = self.op(op, **params)
        if isinstance(answer, dict) and answer.get("status") == "error":
            raise RuntimeError(f"{op} {json.dumps(params)[:120]} failed: {json.dumps(answer)[:300]}")
        return answer

    def ready(self, seconds):
        t0 = time.time()
        last = ""
        while time.time() - t0 < seconds:
            try:
                answer = self.op("get_state", timeout=5)
                if isinstance(answer, dict) and answer.get("status") != "error":
                    return time.time() - t0, answer
                last = json.dumps(answer)[:200]
            except (urllib.error.URLError, OSError, TimeoutError) as e:
                last = str(e)[:200]
            time.sleep(1.0)
        return None, last


def find_data_url(obj):
    """The first 'data:image/png;base64,...' string anywhere in a debug-server answer."""
    if isinstance(obj, str):
        return obj if "base64," in obj else None
    if isinstance(obj, dict):
        for v in obj.values():
            r = find_data_url(v)
            if r:
                return r
    if isinstance(obj, list):
        for v in obj:
            r = find_data_url(v)
            if r:
                return r
    return None


def find_key(obj, key):
    """The first value stored under `key` anywhere in a debug-server answer (None when absent)."""
    if isinstance(obj, dict):
        if key in obj:
            return obj[key]
        for v in obj.values():
            r = find_key(v, key)
            if r is not None:
                return r
    elif isinstance(obj, list):
        for v in obj:
            r = find_key(v, key)
            if r is not None:
                return r
    return None


def walk_nodes(obj, out=None):
    """Every dict that looks like a node with a rect, for the layout log."""
    out = [] if out is None else out
    if isinstance(obj, dict):
        if any(k in obj for k in ("rect", "bounds", "layout")) and any(k in obj for k in ("text", "tag", "id", "node_id", "class")):
            out.append(obj)
        for v in obj.values():
            walk_nodes(v, out)
    elif isinstance(obj, list):
        for v in obj:
            walk_nodes(v, out)
    return out


def layout_overflow(layout_answer, width):
    """The nodes of `get_all_nodes_layout` whose rect crosses the window's right edge: the page does not fit a phone's width
    (a toolbar that cannot wrap, a card or video of a fixed 480 px). -> (count, [(tag, id, classes, x, y, w, h), ...] the
    widest first, without the nodes inside an offender that starts at the same place)."""
    data = (layout_answer or {}).get("data") or {}
    nodes = ((data.get("value") or {}).get("nodes") if isinstance(data, dict) else None) or []
    over = []
    for n in nodes:
        r = n.get("rect")
        if not r or r.get("width", 0) <= 0:
            continue
        if r["x"] < width and r["x"] + r["width"] > width + 1.0:
            over.append((n.get("tag"), n.get("id"), n.get("classes") or [], r["x"], r["y"], r["width"], r["height"]))
    top = []
    for o in sorted(over, key=lambda t: (-t[5], t[4])):
        if not any(abs(o[3] - t[3]) < 1 and t[4] - 1 <= o[4] <= t[4] + t[6] for t in top):
            top.append(o)
    return len(over), top


def e2e_session(args):
    run = q.Run(args.out, args.platform, {"label": args.label, "port": args.port, "kind": "e2e"})
    if args.platform == "android":
        dev = q.Android(run, args.package, args.activity, args.serial)
    else:
        dev = q.IOS(run, args.udid, args.bundle, args.exe or args.bundle.rsplit(".", 1)[-1], None)
    srv = Server(args.port)
    state = {"ready": False, "first": None}

    def device_shot(step, name):
        sig, _w, _h, fname = dev.capture(name)
        step.shots.append(fname)
        return sig

    # ---- the device is fresh: the launcher baseline, so "not the launcher" means something
    def s_baseline(s):
        if args.apk:
            if args.platform == "android":
                out = dev.adb("install", "-r", "-t", args.apk, timeout=240)
                s.check("adb install succeeds", "Success" in out, out.strip()[-160:])
                dev.adb("logcat", "-c")
                dev.sh("settings put system accelerometer_rotation 0")
                dev.sh("settings put secure show_ime_with_hard_keyboard 1")
            else:
                rc, _o, err = dev.simctl("install", args.udid, args.apk, timeout=180)
                s.check("simctl install succeeds", rc == 0, err.strip()[:160])
        if args.platform == "android":
            dev.home()
            dev.settle(2.0)
        sig, w, h, f = dev.capture("home-baseline")
        dev.home_sig = sig
        s.shots.append(f)
        s.note(f"screen {w}x{h}")

    run.step("e00-baseline", "the launcher before the app starts", s_baseline)

    # ---- start the app with the server on
    def s_start(s):
        if args.platform == "android":
            dev.adb("forward", f"tcp:{args.port}", f"tcp:{args.port}")
            dev.sh(f"setprop debug.az.env 'AZ_DEBUG={args.port};AZ_LOG=info;AZ_DUMP_PRESENT=1'")
            out = dev.launch(cold=True)
        else:
            out = dev.launch(extra_env={"AZ_DEBUG": str(args.port), "AZ_DUMP_PRESENT": "1"}, cold=True)
            out = json.dumps({k: v for k, v in out.items() if k != "console"})
        s.note(f"launch: {str(out).strip()[:160]}")
        t = dev.wait_for_front(timeout=args.first_frame_timeout)
        s.check("the app shows a first frame", t is not None, f"after {t:.1f} s" if t is not None else "no frame")
        took, answer = srv.ready(args.server_timeout)
        s.check("the debug server answers get_state", took is not None, f"after {took:.1f} s" if took is not None else f"never: {answer}")
        state["ready"] = took is not None
        if took is not None:
            (run.logs_dir / "get_state.json").write_text(json.dumps(answer, indent=2), encoding="utf-8")
            state["get_state"] = answer
            s.note("get_state: " + json.dumps(answer)[:300])
            dpi = find_key(answer, "dpi")
            if isinstance(dpi, (int, float)) and dpi > 0:
                state["scale"] = dpi / 96.0
                s.note(f"the window's dpi {dpi} = {state['scale']:.3f} device pixels per logical pixel")

    run.step("e01-start-with-the-server", "start the app with the debug server on; the server answers", s_start)
    if not state["ready"]:
        run.write()
        return run

    # ---- the engine's frame against the device's frame
    def s_frames(s):
        for op in ("list_windows", "get_theme"):
            try:
                s.note(f"{op}: " + json.dumps(srv.op(op))[:200])
            except Exception as e:  # noqa: BLE001
                s.note(f"{op}: {e}")
        try:
            srv.op("wait_settled", timeout_ms=3000, timeout=15)
        except Exception as e:  # noqa: BLE001
            s.note(f"wait_settled: {e}")
        dev.settle(1.0)
        device_sig = device_shot(s, "device-frame")
        eng = srv.op("take_screenshot", timeout=60)
        url = find_data_url(eng)
        s.check("take_screenshot returns the engine's own frame", bool(url), json.dumps(eng)[:160] if not url else "")
        if url:
            png = base64.b64decode(url.split("base64,", 1)[1])
            (run.shots_dir / "engine-frame.png").write_bytes(png)
            s.shots.append("engine-frame.png")
            s.note(f"engine frame {len(png)} bytes; compared with the device frame offline (scripts/ci/mobile_gallery.py)")
        for op, fname in (("get_html_string", "dom.html.json"), ("get_scroll_states", "scroll-states.json"),
                          ("get_accessibility_tree", "a11y-tree.json")):
            try:
                (run.logs_dir / fname).write_text(json.dumps(srv.op(op, timeout=60)), encoding="utf-8")
            except Exception as e:  # noqa: BLE001
                s.note(f"{op}: {e}")
        try:
            layout = srv.op("get_all_nodes_layout", timeout=60)
            (run.logs_dir / "nodes-layout.json").write_text(json.dumps(layout), encoding="utf-8")
            s.note(f"{len(walk_nodes(layout))} nodes with a rect logged")
            width = find_key(state.get("get_state"), "logical_width") or 0
            if width:
                count, top = layout_overflow(layout, float(width))
                first = ", ".join(f"{t[0]}{'#' + t[1] if t[1] else ''} x {t[3]:.0f}..{t[3] + t[5]:.0f} at y {t[4]:.0f}" for t in top[:4])
                s.check(f"the page fits the window's width ({float(width):.0f} logical px): no node crosses the right edge", count == 0,
                        f"{count} nodes cross it; widest: {first}", required=False)
                (run.logs_dir / "layout-overflow.json").write_text(json.dumps({"width": width, "count": count, "top": top[:40]}), encoding="utf-8")
        except Exception as e:  # noqa: BLE001
            s.note(f"get_all_nodes_layout: {e}")
        try:
            # the startup timeline (GET /startup, when the engine has one): marks in ms since process start, for the startup work
            with urllib.request.urlopen(f"http://127.0.0.1:{args.port}/startup", timeout=15) as response:
                text = response.read().decode("utf-8", "replace")
            (run.logs_dir / "startup.json").write_text(text, encoding="utf-8")
            s.note(f"GET /startup: {len(text)} bytes kept in logs/startup.json")
        except Exception as e:  # noqa: BLE001
            s.note(f"GET /startup: {str(e)[:100]} (the engine has no startup timeline in this build)")
        state["first"] = device_sig

    run.step("e02-engine-frame-and-device-frame", "the engine's full-repaint frame and the device's frame of the same moment, plus every node's layout", s_frames)

    # ---- the layout audit: boxes, bars and touch targets in both orientations, each with the engine's frame and the device's frame
    audits = []

    def engine_frame(s, name):
        eng = srv.op("take_screenshot", timeout=60)
        url = find_data_url(eng)
        s.check(f"take_screenshot returns the engine's own frame ({name})", bool(url), json.dumps(eng)[:160] if not url else "")
        if url:
            png = base64.b64decode(url.split("base64,", 1)[1])
            (run.shots_dir / f"{name}.png").write_bytes(png)
            s.shots.append(f"{name}.png")

    def audit_moment(s, orient):
        """The dumps of this moment and orientation (kept as logs/<name>-<orientation>.*, so that layout_audit.py can read them
        again), both pictures, and the audit's lists."""
        try:
            srv.op("wait_settled", timeout_ms=3000, timeout=15)
        except Exception as e:  # noqa: BLE001
            s.note(f"wait_settled: {str(e)[:100]}")
        dev.settle(1.0)
        dumps = {}
        for op, name in (("get_state", "get_state"), ("get_all_nodes_layout", "nodes-layout"), ("get_accessibility_tree", "a11y-tree"),
                         ("get_scroll_states", "scroll-states"), ("get_mode", "mode")):
            try:
                dumps[name] = srv.op(op, timeout=90)
                (run.logs_dir / f"{name}-{orient}.json").write_text(json.dumps(dumps[name]), encoding="utf-8")
            except Exception as e:  # noqa: BLE001
                s.note(f"{op}: {str(e)[:120]}")
        window = dev.sh("dumpsys window | grep -E 'InsetsSource|mAppBounds|mMaxBounds|mCurrentFocus|mDisplayRotation' | head -n 160", timeout=60)
        dev.save_log(f"dumpsys-window-{orient}.txt", window)
        device_shot(s, f"device-frame-{orient}")
        engine_frame(s, f"engine-frame-{orient}")
        bars = la.parse_system_bars(window)
        s.note(f"bars the window system names: {[(b['name'], list(b['rect'])) for b in bars['bands']]}; app bounds {bars['app']}; screen {bars['screen']}")
        result = la.audit(dumps.get("nodes-layout"), dumps.get("a11y-tree"), dumps.get("get_state"), bars, orient)
        (run.logs_dir / f"layout-audit-{orient}.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
        s.check(f"the layout and accessibility dumps of the {orient} window could be audited", "error" not in result, result.get("error", ""))
        if "error" in result:
            return result
        s.note(la.summary_line(result))
        s.check(f"{orient}: no box crosses the window's right or left edge", result["edge_right"]["count"] + result["edge_left"]["count"] == 0,
                f"{result['edge_right']['count']} cross the right edge ({result['edge_right']['outer']} outermost), {result['edge_left']['count']} the left", required=False)
        u = result["under_bars"]
        s.check(f"{orient}: no tappable node lies under a system bar", u["tappable"]["count"] == 0, f"{u['tappable']['count']} do", required=False)
        s.check(f"{orient}: no text lies under a system bar", u["text"]["count"] == 0, f"{u['text']['count']} do", required=False)
        t = result["small_targets"]
        s.check(f"{orient}: every tappable node is at least {la.MIN_TARGET_DP:.0f} dp in both directions", t["small"] == 0,
                f"{t['small']} of {t['tappable']} are smaller", required=False)
        return result

    def write_audit_report():
        if audits:
            (run.out / "layout-audit.md").write_text(la.report_markdown(audits, args.label), encoding="utf-8")
            (run.out / "layout-audit.json").write_text(json.dumps(audits, indent=2), encoding="utf-8")
            run.meta["layout_audit"] = " | ".join(
                f"{a['orientation']}: edge {a['edge_right']['count']}, bars {a['under_bars']['tappable']['count']}+{a['under_bars']['text']['count']}, "
                f"small {a['small_targets']['small']}/{a['small_targets']['tappable']}" for a in audits if "error" not in a)

    def s_audit_portrait(s):
        audits.append(audit_moment(s, "portrait"))
        write_audit_report()

    def s_audit_landscape(s):
        dev.rotate(1)
        dev.settle(3.0)
        t = dev.wait_for_front(timeout=30)
        s.check("the app is in front after the rotation", t is not None, f"after {t:.1f} s" if t is not None else "no")
        audits.append(audit_moment(s, "landscape"))
        write_audit_report()
        dev.rotate(0)
        dev.settle(3.0)

    if args.platform == "android":
        run.step("e03-layout-audit-portrait", "the layout audit of the portrait window: edges, bars, touch targets, both pictures", s_audit_portrait)
        run.step("e04-layout-audit-landscape", "the same after a rotation to landscape", s_audit_landscape)

    # ---- the system's dark mode as the engine sees it: (a) the window's mode did not change, or (b) it did and the picture did not
    def s_dark(s):
        before = device_shot(s, "dark-before")
        mode0 = srv.op("get_mode")
        dev.sh("cmd uimode night yes")
        dev.settle(6.0)
        try:
            srv.op("wait_frame")
        except Exception as e:  # noqa: BLE001
            s.note(f"wait_frame: {str(e)[:100]}")
        mode1 = srv.op("get_mode")
        after = device_shot(s, "dark-after")
        engine_frame(s, "dark-engine-frame")
        choice, was, now = find_key(mode1, "mode"), find_key(mode0, "resolved"), find_key(mode1, "resolved")
        diff = q.sig_diff(after, before)
        s.note(f"get_mode before: {json.dumps(mode0)[:160]}; after the night mode: {json.dumps(mode1)[:160]}; the picture differs {diff:.3f}")
        if choice not in (None, "system"):
            verdict = f"the app pins its mode to {choice}: it does not follow the system on purpose"
        elif now != "dark":
            verdict = "(a) the window's mode did not change: the shell's UiMode push (or the engine's adoption of it) did not arrive"
        elif diff <= q.SAME_BELOW:
            verdict = "(b) the window's mode changed to dark and the picture did not: the app looks the same in both modes"
        else:
            verdict = "the window's mode and the picture both changed"
        run.meta["dark_mode"] = verdict[:120]
        s.note("DARK MODE VERDICT: " + verdict)
        s.check("the engine's mode followed the system's night mode (resolved=dark)", now == "dark" or choice not in (None, "system"),
                f"resolved {was} -> {now}; mode choice {choice}")
        s.check("the picture changed with the mode", diff > q.SAME_BELOW or choice not in (None, "system"), f"differs {diff:.3f}", required=False)
        dev.sh("cmd uimode night no")
        dev.settle(4.0)

    if args.platform == "android" and args.audit_only:
        run.step("e30-system-dark-mode", "cmd uimode night yes: did the engine's mode change (get_mode), did the picture", s_dark)

    # ---- the scripted check of one app (--script)
    def tap_node(s, selector):
        """A REAL touch on the node the server names (its window-space box, logical pixels, times the pixel ratio): (x, y) in device pixels."""
        answer = srv.op("get_node_layout", selector=selector, timeout=30)
        box = find_key(answer, "screen_rect") or find_key(answer, "rect")
        if not isinstance(box, dict):
            return None
        scale = state.get("scale") or 1.0
        x, y = int((box["x"] + box["width"] / 2) * scale), int((box["y"] + box["height"] / 2) * scale)
        dev.tap(x, y)
        return x, y

    def s_script_paint(s):
        w, h = dev.dims
        mark = dev.now_mark()
        before = device_shot(s, "paint-before")
        x0, y0, x1, y1 = (int(w * 200 / 1080), int(h * 900 / 2400), int(w * 800 / 1080), int(h * 1400 / 2400))
        dev.swipe(x0, y0, x1, y1, 300)
        dev.settle(1.5)
        drawn = device_shot(s, "paint-after-stroke")
        log = dev.adb("logcat", "-d", "-v", "threadtime", timeout=60)
        counts = q.marker_values(log, "AZPAINT_STROKES", since=mark)
        s.note(f"AZPAINT_STROKES after the swipe: {counts}")
        s.check("a swipe on the canvas makes exactly one stroke (the app prints AZPAINT_STROKES 1)", counts[-1:] == ["1"], f"printed {counts}")
        s.check("the picture shows it", q.sig_diff(drawn, before) > q.SAME_BELOW, f"differs {q.sig_diff(drawn, before):.3f}")
        found = dev.find_label("Undo")
        s.check("the accessibility tree names the phone toolbar's 'Undo' with a position", bool(found), str(found))
        if not found:
            return
        mark2 = dev.now_mark()
        dev.tap(*found)
        dev.settle(1.5)
        undone = device_shot(s, "paint-after-undo")
        counts2 = q.marker_values(dev.adb("logcat", "-d", "-v", "threadtime", timeout=60), "AZPAINT_STROKES", since=mark2)
        s.note(f"AZPAINT_STROKES after Undo: {counts2}")
        s.check("Undo takes the stroke away (AZPAINT_STROKES 0)", counts2[-1:] == ["0"], f"printed {counts2}")
        s.check("the picture is the empty canvas again", q.sig_diff(undone, before) <= q.SAME_BELOW * 2, f"differs from before {q.sig_diff(undone, before):.3f}", required=False)

    def s_script_writer(s):
        w, h = dev.dims
        before = device_shot(s, "writer-before")
        height0 = (find_key(srv.op("get_state"), "logical_height") or 0)
        dev.tap(int(w * 0.5), int(h * 0.45))          # into the page
        dev.settle(1.5)
        up = dev.ime_shown()
        dev.sh("input text hello")
        dev.settle(2.0)
        typed = device_shot(s, "writer-typed")
        engine_frame(s, "writer-engine-frame")
        height1 = (find_key(srv.op("get_state"), "logical_height") or 0)
        xml = dev.sh("uiautomator dump /sdcard/qa-ui.xml >/dev/null && cat /sdcard/qa-ui.xml", timeout=60)
        dev.save_log("writer-ui.xml", xml)
        s.check("a tap into the page raises the keyboard", up, "the window system says " + ("shown" if up else "hidden"))
        s.check("the typed text 'hello' is in the accessibility tree", "hello" in xml.lower(), f"{len(xml)} bytes of the tree")
        s.check("the picture changed after typing", q.sig_diff(typed, before) > q.SAME_BELOW, f"differs {q.sig_diff(typed, before):.3f}")
        s.note(f"window height {height0:.0f} dp without the keyboard, {height1:.0f} dp with it: "
               + ("the window shrinks above the keyboard" if height1 and height0 and height1 < height0 - 50 else "the window keeps its height (the keyboard covers it or the page insets itself)"))
        portrait = next((a for a in audits if a and a.get("orientation") == "portrait" and "error" not in a), None)
        if portrait:
            ribbon = [e for e in portrait["edge_right"]["top"] if e["y"] < portrait["window_dp"][1] * 0.25]
            s.check("the ribbon (the top quarter of the window) fits the width", not ribbon, f"{len(ribbon)} boxes cross the right edge: "
                    + ", ".join(f"{e['node']} {e.get('text', '')[:18]!r}" for e in ribbon[:4]), required=False)
        dev.back()
        dev.settle(1.0)

    def s_script_maps(s):
        dev.settle(15.0)
        views0 = q.marker_values(dev.adb("logcat", "-d", "-v", "threadtime", timeout=60), "AZMAPS_VIEW")
        before = device_shot(s, "maps-15s")
        mark = dev.now_mark()
        at = tap_node(s, "#sidebar-toggle")
        s.check("the server names the sidebar toggle (#sidebar-toggle) with a box", bool(at), str(at))
        dev.settle(3.0)
        sidebar = device_shot(s, "maps-sidebar-tapped")
        marks = q.marker_values(dev.adb("logcat", "-d", "-v", "threadtime", timeout=60), "AZMAPS_SIDEBAR", since=mark)
        s.note(f"AZMAPS_SIDEBAR after one tap on the toggle: {marks}; the picture differs {q.sig_diff(sidebar, before):.3f}")
        s.check("one tap on the toggle changes the sidebar (AZMAPS_SIDEBAR, and the picture)", bool(marks) or q.sig_diff(sidebar, before) > q.SAME_BELOW,
                f"printed {marks}")
        w, h = dev.dims
        mark2 = dev.now_mark()
        dev.tap(int(w * 0.5), int(h * 0.62))
        time.sleep(0.12)
        dev.tap(int(w * 0.5), int(h * 0.62))
        dev.settle(3.0)
        zoomed = device_shot(s, "maps-double-tap")
        views = q.marker_values(dev.adb("logcat", "-d", "-v", "threadtime", timeout=60), "AZMAPS_VIEW", since=mark2)
        s.note(f"AZMAPS_VIEW before the double tap {views0[-1:]}; after {views[-2:]}")
        try:
            z0 = float(views0[-1].split()[2]) if views0 else None
            z1 = float(views[-1].split()[2]) if views else None
        except (ValueError, IndexError):
            z0 = z1 = None
        s.check("a double tap on the map zooms in", z0 is not None and z1 is not None and z1 > z0,
                f"zoom {z0} -> {z1}; the picture differs {q.sig_diff(zoomed, sidebar):.3f}")

    scripts = {"paint": s_script_paint, "writer": s_script_writer, "maps": s_script_maps}
    if args.script and args.platform == "android":
        run.step(f"e40-script-{args.script}", f"the scripted check of the {args.script} app", scripts[args.script])

    full_step = run.step if not args.audit_only else (lambda *_a, **_k: None)   # the rest of the session is for AzWidgets

    # ---- touch (BEFORE the scenarios: they scroll the page to its end, where a swipe up has nothing left to scroll)
    def s_touch(s):
        srv.op("scroll_into_view", text="Azul Widget Showcase", block="start", behavior="instant")
        srv.op("wait_frame")
        dev.settle(1.0)
        before = device_shot(s, "before-touch-swipe")
        w, h = dev.dims
        try:
            st0 = srv.op("get_scroll_states")
        except Exception as e:  # noqa: BLE001
            st0 = {"error": str(e)}
        (run.logs_dir / "scroll-before.json").write_text(json.dumps(st0), encoding="utf-8")
        # the server's coordinates are the window's logical pixels
        s.note("window: " + json.dumps(srv.op("get_state"))[:200])
        x, y0, y1 = w * 0.5, h * 0.7, h * 0.3
        scale = args.logical_scale or state.get("scale") or 1.0
        srv.op("touch_start", id=1, x=x / scale, y=y0 / scale)
        srv.op("wait_frame")
        for k in range(1, 9):
            srv.op("touch_move", id=1, x=x / scale, y=(y0 + (y1 - y0) * k / 8) / scale)
            srv.op("wait_frame")           # one move per frame: the engine reads the touch STATE once a frame
        srv.op("touch_end", id=1)
        srv.op("wait_frame")
        dev.settle(1.5)
        after = device_shot(s, "after-touch-swipe")
        s.check("an in-process touch swipe scrolls the page", q.sig_diff(after, before) > q.SAME_BELOW, f"differs {q.sig_diff(after, before):.3f}")
        st1 = srv.op("get_scroll_states")
        (run.logs_dir / "scroll-after.json").write_text(json.dumps(st1), encoding="utf-8")
        s.check("the scroll states changed", st1 != st0, "", required=False)
        if args.platform == "android":
            dev.swipe(w * 0.5, h * 0.3, w * 0.5, h * 0.7, 350)   # a real finger the other way, through the OS
            dev.settle(1.5)
            back = device_shot(s, "after-adb-swipe")
            s.check("an adb swipe (a real touch through Android's input) scrolls it back", q.sig_diff(back, after) > q.SAME_BELOW, f"differs {q.sig_diff(back, after):.3f}")

    full_step("e05-touch", "in-process touch events as a swipe (and on Android a real one through adb)", s_touch)

    # ---- the scenario files
    for i, path in enumerate(args.scenario or []):
        name = Path(path).stem

        def s_scenario(s, path=path, name=name):
            tests = json.loads(Path(path).read_text(encoding="utf-8"))
            if isinstance(tests, dict):
                tests = tests.get("tests", [tests])
            answer = srv.op("run_e2e_tests", timeout=240, tests=tests)
            (run.logs_dir / f"scenario-{name}.json").write_text(json.dumps(answer, indent=2), encoding="utf-8")
            data = answer.get("data") if isinstance(answer, dict) else None
            results = (data or {}).get("results") if isinstance(data, dict) else None
            if results is None and isinstance(data, dict) and isinstance(data.get("value"), dict):
                results = data["value"].get("results")
            s.check("the server ran the scenario", results is not None, json.dumps(answer)[:200] if results is None else f"{len(results)} test(s)")
            for r in results or []:
                steps = r.get("steps") or []
                bad = [st for st in steps if st.get("status") not in ("pass", "ok")]
                s.check(f"test {r.get('name', '?')[:70]}", r.get("status") == "pass",
                        f"{len(steps)} steps" + (f"; first failing: {json.dumps(bad[0])[:240]}" if bad else ""))
            dev.settle(1.0)
            device_shot(s, f"after-{name}")

        full_step(f"e{10 + i}-scenario-{name}", f"the scenario file {path} through the server", s_scenario)

    # ---- the server survives the lifecycle
    def s_lifecycle(s):
        if args.platform == "android":
            dev.rotate(1)
            dev.settle(3.0)
            device_shot(s, "landscape")
            try:
                s.note("state in landscape: " + json.dumps(srv.op("get_state"))[:240])
                s.check("the server answers in landscape", True)
            except Exception as e:  # noqa: BLE001
                s.check("the server answers in landscape", False, str(e)[:160])
            dev.rotate(0)
            dev.settle(3.0)
            dev.home()
            time.sleep(10)
            dev.resume()
            t = dev.wait_for_front(timeout=30)
            s.check("back in front after 10 s in the background", t is not None, f"after {t:.1f} s" if t is not None else "no")
        else:
            dev.open_other_app()
            time.sleep(10)
            dev.launch(cold=False)
            t = dev.wait_for_front(timeout=30)
            s.check("back in front after 10 s in the background", t is not None, f"after {t:.1f} s" if t is not None else "no")
        dev.settle(1.5)
        device_shot(s, "after-lifecycle")
        try:
            srv.op("get_state", timeout=15)
            s.check("the server still answers", True)
        except Exception as e:  # noqa: BLE001
            s.check("the server still answers", False, str(e)[:160])

    full_step("e50-lifecycle-with-the-server", "rotation and ten seconds in the background with the server running", s_lifecycle)

    # ---- keyboard
    def s_keyboard(s):
        # the scenarios before scrolled the page about: put the field on the screen first (an off-screen node has no position)
        try:
            srv.op("scroll_into_view", text="TextInput", block="center", behavior="instant")
            srv.op("wait_frame")
            dev.settle(1.0)
        except Exception as e:  # noqa: BLE001
            s.note(f"scroll_into_view: {e}")
        if args.platform == "android":
            x, y, how = dev.tap_label("TextInput")      # a real touch through Android's input, at the node's place
            if x is None:
                s.check("the accessibility tree names the TextInput field with a position", False, "not found")
                return
            s.note(f"the TextInput field is at ({x}, {y}) in the accessibility tree")
            dev.settle(2.5)
            shown = dev.ime_shown()
            s.check("a touch on a text field raises the soft keyboard", shown, "the window system says " + ("shown" if shown else "hidden"))
            device_shot(s, "text-field-touched")
        srv.op("text_input", text="hello azul")      # in process: into the focused node
        srv.op("wait_frame")
        dev.settle(1.0)
        device_shot(s, "text-typed")
        if args.platform == "android" and dev.ime_shown():
            dev.back()
            dev.settle(1.5)
            s.check("the keyboard hides again", not dev.ime_shown(), "")
        s.check("the process is alive after the keyboard", dev.alive(), f"pid {dev.pid() or '-'}")

    full_step("e60-keyboard", "click a text field, type, hide the keyboard", s_keyboard)

    def s_logs(s):
        if args.platform == "android":
            full = dev.adb("logcat", "-d", "-v", "threadtime", timeout=120)
            dev.save_log("logcat.txt", full)
            present = [l for l in full.splitlines() if "present" in l and "rows" in l]
            s.note(f"{len(present)} present lines (AZ_DUMP_PRESENT): first: {present[0].split('RustStdoutStderr:')[-1].strip()[:100] if present else '-'}")
            bad = q.fatal_lines(full, args.package)
            s.check("no fatal line of the app in the log", not bad, bad[0][:200] if bad else "")
        else:
            consoles = sorted(run.logs_dir.glob("console-*.txt"))
            text = "\n".join(c.read_text(errors="replace") for c in consoles)
            bad = [l for l in text.splitlines() if "panicked" in l or "unexpected panic" in l]
            s.check("no panic on the console", not bad, bad[0][:200] if bad else "")

    run.step("e70-logs", "the log of the whole session", s_logs)
    run.write()
    return run


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("platform", choices=["android", "ios"])
    ap.add_argument("--out", required=True)
    ap.add_argument("--port", type=int, default=8795)
    ap.add_argument("--package", default="com.azul.AzWidgets")
    ap.add_argument("--activity", default="")
    ap.add_argument("--serial", default="")
    ap.add_argument("--udid", default="")
    ap.add_argument("--bundle", default="com.azul.AzWidgets")
    ap.add_argument("--exe", default="")
    ap.add_argument("--label", default="AzWidgets e2e")
    ap.add_argument("--apk", default="", help="the APK (Android) or .app directory (iOS) to install first")
    ap.add_argument("--scenario", action="append", default=[])
    ap.add_argument("--first-frame-timeout", type=int, default=60)
    ap.add_argument("--server-timeout", type=int, default=60)
    ap.add_argument("--logical-scale", type=float, default=0.0, help="device pixels per logical pixel of the window (the touch ops take logical pixels)")
    ap.add_argument("--audit-only", action="store_true", help="the phone-fit session of any app: frames, the layout audit in both orientations, the dark mode, --script")
    ap.add_argument("--script", choices=["", "paint", "writer", "maps"], default="", help="the scripted check of one app (Android)")
    args = ap.parse_args(argv)
    run = e2e_session(args)
    failed = [s.name for s in run.steps if not s.ok]
    print(f"\nE2E {'PASS' if not failed else 'FAIL'}: {len(run.steps) - len(failed)}/{len(run.steps)} steps"
          + (f"; failed: {', '.join(failed)}" if failed else ""))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
