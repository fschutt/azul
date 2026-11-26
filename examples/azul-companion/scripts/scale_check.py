#!/usr/bin/env python3
"""The simulator's pointer mapping under a smaller phone: when the screen has no room for the phone, the window paints it
with `transform: scale()`, and the pointer must still mean the same thing on the phone. Checked through the debug port, with
synthetic events fed to the app (the user's real cursor is never touched):

  drag   a drag of 100 phone pixels across the pad sends the same pan (to 0.01) at 100, 75 and 50 percent;
  tap    a tap on the settings gear, at the place the gear is painted, opens the settings (the pad is gone) at 75 percent;
  edge   a drag that leaves the pad (the pointer stays captured by it): reported, not failed. Known engine gap: for a
         captured pointer outside the node `get_cursor_relative_to_node` measures against the UNtransformed rect.

    scale_check.py <companion_sim> [--port P]            the three checks, headless (the CPU renderer)
    scale_check.py <companion_sim> --window OUT.png      ONE real macOS window at the scale the screen asks for ("fit"): is
                                                         it laid out as big as it asked for (the system did not cut it down),
                                                         a drag and a tap at the painted places, a picture of the window

The pan the phone sent and the window's size and scale are read from the simulator's own trace (--trace). Exit code 0 when
everything checked holds.
"""
import base64, json, os, re, subprocess, sys, tempfile, time, urllib.request


def post(port, body, timeout=60):
    req = urllib.request.Request(f"http://127.0.0.1:{port}/", data=json.dumps(body).encode(), method="POST")
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.load(r)


class Sim:
    def __init__(self, binary, zoom, port, window):
        env = dict(os.environ, AZ_DEBUG=str(port), AZ_MODE="dark")
        if not window:
            env["AZ_BACKEND"] = "headless"
        self.port = port
        self.scale = 1.0 if zoom in ("fit", "100") else float(zoom) / 100.0
        self.log = tempfile.NamedTemporaryFile("w+", suffix=".log", delete=False)
        self.proc = subprocess.Popen([binary, "--zoom", zoom, "--trace"], env=env, stdout=self.log, stderr=subprocess.STDOUT)
        for _ in range(300):
            try:
                urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=1).read()
                break
            except Exception:
                time.sleep(0.1)
        post(port, {"op": "wait_frame"})
        time.sleep(1.0)  # the pen state and the picture arrive
        post(port, {"op": "wait_frame"})
        self.bezel = self.node("az-bezel")
        self.log.flush()
        self.log.seek(0)
        said = re.search(r"\[trace\] window (\d+)x(\d+) scale ([0-9.]+)", self.log.read())
        self.window = (float(said.group(1)), float(said.group(2))) if said else None
        if said:
            self.scale = float(said.group(3))

    def nodes(self):
        return post(self.port, {"op": "get_all_nodes_layout"})["data"]["value"]["nodes"]

    def node(self, ident):
        return next((n["rect"] for n in self.nodes() if n.get("id") == ident), None)

    def painted(self, x, y):
        """A point of the phone's layout, where the bezel's transform paints it (the origin is the bezel's top left)."""
        return self.bezel["x"] + (x - self.bezel["x"]) * self.scale, self.bezel["y"] + (y - self.bezel["y"]) * self.scale

    def drag(self, start, end, steps=10):
        (sx, sy), (ex, ey) = start, end
        post(self.port, {"op": "mouse_move", "x": sx, "y": sy})
        post(self.port, {"op": "mouse_down", "x": sx, "y": sy, "button": "left"})
        for i in range(1, steps + 1):
            post(self.port, {"op": "mouse_move", "x": sx + (ex - sx) * i / steps, "y": sy + (ey - sy) * i / steps})
            post(self.port, {"op": "wait_frame"})
        post(self.port, {"op": "mouse_up", "x": ex, "y": ey, "button": "left"})
        self.settle()

    def tap(self, point):
        x, y = point
        post(self.port, {"op": "mouse_move", "x": x, "y": y})
        post(self.port, {"op": "mouse_down", "x": x, "y": y, "button": "left"})
        post(self.port, {"op": "mouse_up", "x": x, "y": y, "button": "left"})
        self.settle()

    def screenshot(self, path):
        shot = post(self.port, {"op": "take_screenshot"})["data"]["value"]["data"]
        with open(path, "wb") as f:
            f.write(base64.b64decode(shot.split(",", 1)[1]))

    def settle(self):
        time.sleep(0.5)
        post(self.port, {"op": "wait_frame"})

    def last_pan(self):
        """The (x, y) of the last "pan end" line the simulated Mac logged, or None."""
        self.log.flush()
        self.log.seek(0)
        ends = re.findall(r"pan end #\d+\s+([-+]\d+\.\d+)\s+([-+]\d+\.\d+)", self.log.read())
        return (float(ends[-1][0]), float(ends[-1][1])) if ends else None

    def close(self):
        try:
            post(self.port, {"op": "close"})
        except Exception:
            pass
        time.sleep(0.4)
        try:
            self.proc.terminate()
            self.proc.wait(timeout=5)
        except Exception:
            self.proc.kill()


def pad_centre(sim):
    pad = sim.node("pad")
    return pad["x"] + pad["width"] / 2, pad["y"] + pad["height"] / 2, pad


def real_window(binary, port, out_png):
    sim = Sim(binary, "fit", port, True)
    ok = True
    try:
        body = sim.nodes()[0]["rect"]
        asked = sim.window
        print(f"window asked for {asked}, laid out as {round(body['width'])}x{round(body['height'])}, phone scale {sim.scale}")
        cut = asked is None or abs(body["width"] - asked[0]) > 1.0 or abs(body["height"] - asked[1]) > 1.0
        print(f"  window as big as it asked for: {'FAIL (the system cut it down)' if cut else 'PASS'}")
        ok &= not cut
        cx, cy, pad = pad_centre(sim)
        sim.drag(sim.painted(cx, cy), sim.painted(cx + 100.0, cy))
        pan = sim.last_pan()
        print(f"  drag of 100 phone px, pan sent: {pan}")
        ok &= pan is not None and abs(pan[0] + 0.16) <= 0.01
        sim.screenshot(out_png)
        targets = [n for n in sim.nodes() if "az-target" in (n.get("classes") or []) and n["rect"]["x"] < sim.bezel["x"] + sim.bezel["width"]]
        top = min(t["rect"]["y"] for t in targets)
        gear = max((t for t in targets if abs(t["rect"]["y"] - top) < 6), key=lambda t: t["rect"]["x"])["rect"]
        sim.tap(sim.painted(gear["x"] + gear["width"] / 2, gear["y"] + gear["height"] / 2))
        good = sim.node("pad") is None
        print(f"  tap on the gear where it is painted: {'PASS' if good else 'FAIL'}")
        ok &= good
    finally:
        sim.close()
    return 0 if ok else 1


def main():
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    binary = args[0]
    window = False
    port = int(args[args.index("--port") + 1]) if "--port" in args else 8820
    if "--window" in args:
        return real_window(binary, port, args[args.index("--window") + 1])
    ok = True

    inside, outside = {}, {}
    for n, zoom in enumerate(["100", "75", "50"]):
        sim = Sim(binary, zoom, port + n, window)
        try:
            cx, cy, pad = pad_centre(sim)
            sim.drag(sim.painted(cx, cy), sim.painted(cx + 100.0, cy))
            inside[zoom] = sim.last_pan()
        finally:
            sim.close()
        sim = Sim(binary, zoom, port + 10 + n, window)
        try:
            cx, cy, pad = pad_centre(sim)
            sim.drag(sim.painted(cx, cy), sim.painted(cx + 250.0, cy))  # leaves the pad by about 70 pixels
            outside[zoom] = sim.last_pan()
        finally:
            sim.close()
    print("drag of 100 phone px, pan sent:", inside)
    base = inside.get("100")
    for zoom in ("75", "50"):
        got = inside.get(zoom)
        good = base is not None and got is not None and abs(got[0] - base[0]) <= 0.01 and abs(got[1] - base[1]) <= 0.01
        print(f"  drag at {zoom}%: {'PASS' if good else 'FAIL'}")
        ok &= good

    print("drag of 250 phone px that leaves the pad, pan sent:", outside)
    base = outside.get("100")
    for zoom in ("75", "50"):
        got = outside.get(zoom)
        same = base is not None and got is not None and abs(got[0] - base[0]) <= 0.01 and abs(got[1] - base[1]) <= 0.01
        print(f"  edge at {zoom}%: {'same as at 100%' if same else 'DIFFERENT (known engine gap, not counted)'}")

    sim = Sim(binary, "75", port + 20, window)
    try:
        targets = [n for n in sim.nodes() if "az-target" in (n.get("classes") or []) and n["rect"]["x"] < sim.bezel["x"] + sim.bezel["width"]]
        top = min(t["rect"]["y"] for t in targets)
        gear = max((t for t in targets if abs(t["rect"]["y"] - top) < 6), key=lambda t: t["rect"]["x"])["rect"]
        sim.tap(sim.painted(gear["x"] + gear["width"] / 2, gear["y"] + gear["height"] / 2))
        good = sim.node("pad") is None
        print(f"  tap on the gear where it is painted at 75%: {'PASS' if good else 'FAIL'}")
        ok &= good
    finally:
        sim.close()
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
