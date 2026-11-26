#!/usr/bin/env python3
"""ios_phone_gpu.py: the GPU renderer on a real iPhone, the one thing the Simulator and the runners cannot show: the
Simulator's OpenGL ES is Apple's SOFTWARE renderer, a phone's is its GPU (standard library only).

    python3 scripts/ci/ios_phone_gpu.py <device> [--bundle com.azul.AzWidgets] [--modes cpu,gpu,auto] [--seconds 25] [--out ios-phone-gpu]

<device> is what `xcrun devicectl list devices` shows for the phone (its name or its identifier); it must be connected, trusted
and have the app INSTALLED (a build signed with your own development profile: `xcrun devicectl device install app --device
<device> MyApp.app`, see doc/guide/en/deploying/mobile.md). Needs Xcode 15 or newer on the Mac. Nothing is installed or deleted here.

For each renderer it starts the app (the one running is ended first) with AZ_BACKEND=<mode> and AZ_LOG=info, attaches to the app's
console for <seconds> (SCROLL THE PAGE WITH YOUR FINGER meanwhile: the engine writes its frame times only while frames come one
after another), and keeps what the engine said about its renderer: which one RAN and on which GL_RENDERER, why not the GPU when it
fell back (`gpu unavailable at <stage>`), the first GPU frame and the frame times of what you scrolled. The files land in --out
(a second run overwrites them). If the GPU run says `gpu unavailable at <stage>`, send that line and <out>/gpu-console.txt.

The same words are what scripts/ci/mobile_qa.py reads on the Simulator; the readers are its functions.
"""
import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mobile_qa as q  # noqa: E402

FIRST_FRAME_LINE = re.compile(r"\[iOS GPU\] first frame (\d+)x(\d+): (\d+) draw calls, drawn in (\d+) ms, (\d+) ms after the GPU path began")
RENDERER_LINE = re.compile(r"\[iOS GPU\] WebRender renderer created in (\d+) ms")
GL_STRINGS_LINE = re.compile(r"\[iOS GPU\] GL_VENDOR=(.*?) GL_RENDERER=(.*?) GL_VERSION=(.*?) GLSL=(.*)")


def report(mode, text):
    """What one run's console says about the renderer. -> {mode, ran, requested, device, fallbacks: [(stage, cause)],
    first_frame: {w, h, draw_calls, draw_ms, since_ms} | None, renderer_ms, gl_strings, frames: {median, p95, n, ...} | None}"""
    ran, requested, device = q.backend_ran(text)
    first = None
    for line in text.splitlines():
        m = FIRST_FRAME_LINE.search(line)
        if m:
            first = {"w": int(m.group(1)), "h": int(m.group(2)), "draw_calls": int(m.group(3)), "draw_ms": int(m.group(4)),
                     "since_ms": int(m.group(5))}
    renderer_ms = None
    gl_strings = None
    for line in text.splitlines():
        m = RENDERER_LINE.search(line)
        if m:
            renderer_ms = int(m.group(1))
        m = GL_STRINGS_LINE.search(line)
        if m:
            # the engine writes them with `{:?}`: in quotes
            gl_strings = {k: v.strip().strip('"') for k, v in zip(("vendor", "renderer", "version", "glsl"), m.groups())}
    return {"mode": mode, "ran": ran, "requested": requested, "device": device, "fallbacks": q.gpu_fallbacks(text), "first_frame": first,
            "renderer_ms": renderer_ms, "gl_strings": gl_strings, "frames": q.summarize_frames(q.swipe_windows(text))}


def verdict(reports):
    """The sentence(s) a person reads. -> [lines]. The rule is the one the QA harness holds the Simulator to, for a HARDWARE GPU:
    cpu runs cpu, gpu runs gpu, auto runs gpu."""
    lines = []
    by = {r["mode"]: r for r in reports}
    for mode, r in by.items():
        words = f"requested {mode}: ran {r['ran']}" + (f" on {r['device']}" if r["device"] else "")
        if r["fallbacks"]:
            words += "; fell back: " + "; ".join(f"at {stage}: {cause}" for stage, cause in r["fallbacks"])
        lines.append(words)
    cpu, gpu, auto = by.get("cpu"), by.get("gpu"), by.get("auto")
    if cpu is not None and cpu["ran"] not in ("cpu", "unknown"):
        lines.append(f"FINDING: requested cpu, the {cpu['ran']} renderer ran")
    if gpu is not None:
        if gpu["ran"] == "gpu":
            device = gpu["device"] or "?"
            if q.device_is_software(device):
                lines.append(f"FINDING: the GPU path ran on a SOFTWARE renderer ({device}): this is not the phone's GPU")
            else:
                lines.append(f"PASS: the GPU path ran on {device}")
        elif gpu["ran"] == "cpu":
            why = gpu["fallbacks"][0] if gpu["fallbacks"] else None
            lines.append("FAIL: requested gpu, ran cpu" + (f" (stage {why[0]}: {why[1]})" if why else " (no fallback line: this build has no GPU path)"))
        else:
            lines.append("FAIL: no `[render] backend=` line in the console: the app did not start, or this build does not write one")
    if auto is not None and auto["ran"] != "unknown":
        if auto["ran"] == "gpu":
            lines.append("PASS: auto took the GPU on the phone")
        else:
            why = auto["fallbacks"][0] if auto["fallbacks"] else None
            lines.append("FAIL: auto ran the CPU renderer on a phone" + (f" ({why[0]}: {why[1]})" if why else ""))
    for r in reports:
        f = r["frames"]
        if f:
            draw = f" (the draw alone: median {f['draw_median']:.1f} / p95 {f['draw_p95']:.1f} ms)" if f.get("draw_median") is not None else ""
            lines.append(f"{r['mode']}: scrolled frames median {f['median']:.1f} ms / p95 {f['p95']:.1f} ms over {f['n']} frames{draw}")
        elif r["ran"] != "unknown":
            lines.append(f"{r['mode']}: no frame times (scroll the page for several seconds with your finger: fewer than 8 frames a second say nothing)")
        ff = r["first_frame"]
        if ff:
            lines.append(f"{r['mode']}: first GPU frame {ff['w']}x{ff['h']}, {ff['draw_calls']} draw calls, drawn in {ff['draw_ms']} ms, "
                         f"{ff['since_ms']} ms after the GPU path began (renderer made in {r['renderer_ms']} ms)")
    return lines


def run_mode(device, bundle, mode, seconds):
    """Start the app with AZ_BACKEND=<mode> and keep its console for `seconds`. The client of `devicectl` is stopped then (the
    app keeps running: the next start ends it). -> the console text"""
    env = json.dumps({"AZ_BACKEND": mode, "AZ_LOG": "info", "RUST_BACKTRACE": "1"})
    cmd = ["xcrun", "devicectl", "device", "process", "launch", "--device", device, "--terminate-existing",
           "--environment-variables", env, "--console", bundle]
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors="replace")
    try:
        out, _ = proc.communicate(timeout=seconds)
    except subprocess.TimeoutExpired:
        proc.terminate()
        try:
            out, _ = proc.communicate(timeout=10)
        except subprocess.TimeoutExpired:
            out = ""
    return out or ""


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("device", help="the phone: its name or identifier from `xcrun devicectl list devices`")
    ap.add_argument("--bundle", default="com.azul.AzWidgets", help="the installed app's bundle identifier")
    ap.add_argument("--modes", default="cpu,gpu,auto", help="comma list of AZ_BACKEND values to run, in order")
    ap.add_argument("--seconds", type=int, default=25, help="how long to keep each run's console (scroll the page meanwhile)")
    ap.add_argument("--out", default="ios-phone-gpu", help="where the consoles and the summary are written")
    args = ap.parse_args(argv)
    modes = [m.strip() for m in args.modes.split(",") if m.strip() in ("cpu", "gpu", "auto")]
    if not modes:
        print("--modes names none of cpu, gpu, auto")
        return 2
    seconds = max(8, min(args.seconds, 300))
    try:
        listed = subprocess.run(["xcrun", "devicectl", "list", "devices"], capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.TimeoutExpired) as e:
        print(f"xcrun devicectl does not run here ({e}): this needs a Mac with Xcode 15 or newer")
        return 2
    if listed.returncode != 0 or args.device not in listed.stdout:
        print(f"`xcrun devicectl list devices` does not show {args.device!r}:\n{listed.stdout or listed.stderr}")
        return 2
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    reports = []
    for mode in modes:
        print(f"================ AZ_BACKEND={mode}: starting {args.bundle}; SCROLL THE PAGE for {seconds} s ================", flush=True)
        t0 = time.time()
        text = run_mode(args.device, args.bundle, mode, seconds)
        (out / f"{mode}-console.txt").write_text(text, encoding="utf-8")
        rep = report(mode, text)
        reports.append(rep)
        engine = [l.strip() for l in text.splitlines() if "[render]" in l or "[iOS GPU]" in l]
        print("\n".join(engine[:14]) or "(the engine wrote no [render] line: is the app installed, and is this a libazul with the GPU path?)")
        print(f"({time.time() - t0:.0f} s)", flush=True)
    lines = verdict(reports)
    (out / "summary.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
    (out / "summary.json").write_text(json.dumps(reports, indent=2), encoding="utf-8")
    print("\n================ summary ================")
    print("\n".join(lines))
    print(f"files: {out}/{{{','.join(modes)}}}-console.txt, summary.txt, summary.json")
    if any(r["mode"] == "gpu" and r["fallbacks"] for r in reports):
        print("the GPU run fell back: send the `gpu unavailable at <stage>` line above and gpu-console.txt")
    return 0 if not any(l.startswith(("FAIL", "FINDING")) for l in lines) else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
