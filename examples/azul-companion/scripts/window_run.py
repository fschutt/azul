#!/usr/bin/env python3
"""A scripted run of the companion simulator through its debug port: a drag of synthetic mouse moves over the pad (the
user's real cursor is never touched: the events are fed to the app, not to the system), the time each move takes until
the next frame, the engine's own per-layout times from its log, and one screenshot of the window from inside the app.

    window_run.py <companion_sim> <out.png> [--headless] [--moves N] [--port P] [--extra ARGS...]

Without --headless the window is a real macOS window (the CPU renderer): it opens, is driven for a few seconds and closes.
Prints one JSON object with the numbers.
"""
import json, os, re, statistics, subprocess, sys, time, urllib.request, base64, tempfile


def post(port, body, timeout=60):
    req = urllib.request.Request(f"http://127.0.0.1:{port}/", data=json.dumps(body).encode(), method="POST")
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.load(r)


def percentile(values, p):
    values = sorted(values)
    if not values:
        return None
    return values[min(len(values) - 1, int(round(p / 100 * (len(values) - 1))))]


def main():
    args = sys.argv[1:]
    if len(args) < 2:
        print(__doc__)
        return 2
    binary, out_png = args[0], args[1]
    headless = "--headless" in args
    moves = int(args[args.index("--moves") + 1]) if "--moves" in args else 120
    port = int(args[args.index("--port") + 1]) if "--port" in args else 8793
    extra = args[args.index("--extra") + 1:] if "--extra" in args else []
    env = dict(os.environ, AZ_DEBUG=str(port), AZ_MODE="dark", AZ_LOG="debug")
    if headless:
        env["AZ_BACKEND"] = "headless"
    log = tempfile.NamedTemporaryFile("w+", suffix=".log", delete=False)
    proc = subprocess.Popen([binary, *extra], env=env, stdout=log, stderr=subprocess.STDOUT)
    result = {"headless": headless, "moves": moves}
    try:
        for _ in range(200):
            try:
                urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=1).read()
                break
            except Exception:
                time.sleep(0.1)
        post(port, {"op": "wait_frame"})
        time.sleep(1.0)  # the pen state and the thumbnail arrive
        post(port, {"op": "wait_frame"})
        nodes = post(port, {"op": "get_all_nodes_layout"})["data"]["value"]["nodes"]
        pad = next(n for n in nodes if n.get("id") == "pad")["rect"]
        cx, cy = pad["x"] + pad["width"] / 2, pad["y"] + pad["height"] / 2
        result["pad"] = pad
        log.flush()
        log.seek(0, os.SEEK_END)
        mark = log.tell()
        post(port, {"op": "mouse_move", "x": cx, "y": cy})
        post(port, {"op": "mouse_down", "x": cx, "y": cy, "button": "left"})
        times = []
        for i in range(moves):
            x = cx + 90 * __import__("math").sin(i / 12.0)
            y = cy + 60 * __import__("math").sin(i / 17.0)
            t0 = time.perf_counter()
            post(port, {"op": "mouse_move", "x": x, "y": y})
            post(port, {"op": "wait_frame"})
            times.append((time.perf_counter() - t0) * 1000.0)
        post(port, {"op": "mouse_up", "x": cx, "y": cy, "button": "left"})
        result["move_plus_frame_ms"] = {
            "min": round(min(times), 1), "p50": round(percentile(times, 50), 1), "p90": round(percentile(times, 90), 1),
            "p99": round(percentile(times, 99), 1), "max": round(max(times), 1),
        }
        time.sleep(0.3)
        shot = post(port, {"op": "take_screenshot"})["data"]["value"]["data"]
        with open(out_png, "wb") as f:
            f.write(base64.b64decode(shot.split(",", 1)[1]))
        result["screenshot"] = out_png
        log.flush()
        log.seek(mark)
        text = log.read()
        phases = [float(m) for m in re.findall(r"\[phases\] total ([0-9.]+)ms", text)]
        if phases:
            result["layout_pass_ms"] = {
                "count": len(phases), "p50": round(percentile(phases, 50), 1), "p90": round(percentile(phases, 90), 1),
                "max": round(max(phases), 1),
            }
        present = [float(m) for m in re.findall(r"present[^\n]*?([0-9.]+)\s?ms", text)]
        if present:
            result["present_ms"] = {"count": len(present), "p50": round(percentile(present, 50), 1), "max": round(max(present), 1)}
        post(port, {"op": "close"})
        time.sleep(0.5)
    finally:
        try:
            proc.terminate()
        except Exception:
            pass
        try:
            proc.wait(timeout=5)
        except Exception:
            proc.kill()
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
