#!/usr/bin/env python3
"""Memory of every azul app, headless: each app alone, then all at once (the shared libazul.dylib).

    scripts/waves/tools/run_capped.sh --cap-mb 2500 --seconds 1500 -- python3 scripts/waves/tools/memprobe.py alone
    scripts/waves/tools/run_capped.sh --cap-mb 5000 --seconds 900  -- python3 scripts/waves/tools/memprobe.py together

AZ_MEASURE_LIBDIR = the dylib's directory (default target/azul-lib), AZ_MEASURE_OUT = where results.json
goes (default $AZ_WORK/memprobe). Per app: RSS, `footprint` (private memory), and per-segment resident /
dirty MB of libazul.dylib and the app binary (vmmap). Together: the dylib __TEXT resident SYSTEM-WIDE
(mincore on the file) vs the sum over processes - the sharing saving.

2026-10-02 at 2e92c759b: per app footprint 73-130 MB (AzWidgets 211), dylib __TEXT resident 6-18 MB per
app of 47.9 MB; 20 apps together: 249 MB of dylib text mapped vs 16.1 MB resident -> ~233 MB saved;
private heap ~2 GB in total, about half of it malloc "reclaimable" (freed after startup, kept)."""
import ctypes, json, os, re, subprocess, sys, tempfile, time, urllib.request

ROOT = "/Users/fschutt/Development/azul"
LIBDIR = os.environ.get("AZ_MEASURE_LIBDIR", ROOT + "/target/azul-lib")
DYLIB = LIBDIR + "/libazul.dylib"
OUT = os.environ.get("AZ_MEASURE_OUT", os.path.join(os.environ.get("AZ_WORK", os.path.expanduser("~/Development/azul-work")), "memprobe"))
os.makedirs(OUT, exist_ok=True)
APPS = ["AzBuilder", "AzCalculator", "AzCalendar", "AzContacts", "AzDrive", "AzMail", "AzMaps", "AzMeet",
        "AzNotes", "AzPaint", "AzPhoto", "AzReview", "AzSetup", "AzSheets", "AzShells", "AzShow", "AzTasks",
        "AzVideoCut", "AzWidgets", "AzWriter"]
PER_APP_CAP_MB = 1500
TOTAL_CAP_MB = 4500


def op(port, name, timeout=5, **params):
    body = json.dumps(dict(op=name, **params)).encode()
    req = urllib.request.Request("http://127.0.0.1:%d/" % port, data=body, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read().decode())


def start(app, port):
    home = tempfile.mkdtemp(prefix="azmem-%s-" % app.lower())
    env = dict(os.environ, HOME=home, AZ_BACKEND="headless", AZ_DEBUG=str(port), DYLD_LIBRARY_PATH=LIBDIR)
    log = open(os.path.join(OUT, app + ".log"), "w")
    return subprocess.Popen([ROOT + "/target/release/" + app], env=env, stdout=log, stderr=subprocess.STDOUT, cwd=home)


def ready(p, port, timeout=40):
    t0 = time.time()
    while time.time() - t0 < timeout:
        if p.poll() is not None:
            return False
        try:
            if op(port, "get_state", timeout=3).get("status") == "ok":
                for _ in range(3):
                    op(port, "wait_frame", timeout=10)
                return True
        except Exception:
            time.sleep(0.5)
    return False


def rss_mb(pid):
    out = subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True).stdout.strip()
    return int(out) / 1024 if out else 0.0


def to_mb(s):
    n, unit = float(s.split()[0]), s.split()[1]
    return {"KB": n / 1024, "MB": n, "GB": n * 1024}.get(unit, n / 1048576)


def footprint_mb(pids):
    out = subprocess.run(["footprint"] + [x for pid in pids for x in ("-p", str(pid))], capture_output=True, text=True).stdout
    per = {int(m.group(1)): to_mb(m.group(2)) for m in re.finditer(r"\[(\d+)\]: 64-bit\s+Footprint: ([\d.]+ [KMG]?B)", out)}
    total = re.search(r"Summary Footprint: ([\d.]+ [KMG]?B)", out)
    return per, (to_mb(total.group(1)) if total else None), out


def vm_regions(pid):
    """Resident / dirty MB of libazul.dylib's segments and of the app binary's segments."""
    out = subprocess.run(["vmmap", str(pid)], capture_output=True, text=True).stdout
    res = {}

    def mb(x):
        return float(x[:-1]) * {"K": 1 / 1024, "M": 1, "G": 1024}[x[-1]] if x[-1] in "KMG" else float(x) / 1048576

    for line in out.splitlines():
        m = re.match(r"(__\w+)\s+\S+\s+\[\s*([\d.]+[KMG]?)\s+([\d.]+[KMG]?)\s+([\d.]+[KMG]?)\s+([\d.]+[KMG]?)\]\s+\S+\s+SM=\w+\s+(.*)$", line)
        if not m:
            continue
        seg, _vsz, resident, dirty, _swapped, path = m.groups()
        who = "dylib" if path.endswith("libazul.dylib") else ("app" if "/target/release/" in path else None)
        if who:
            r = res.setdefault("%s %s" % (who, seg), [0.0, 0.0])
            r[0] += mb(resident)
            r[1] += mb(dirty)
    return res


def dylib_text_resident_mb():
    """The dylib's __TEXT pages resident system-wide (mincore over the file)."""
    libc = ctypes.CDLL(None, use_errno=True)
    libc.mmap.restype = ctypes.c_void_p
    libc.mmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_long]
    libc.mincore.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_char_p]
    libc.munmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
    text = subprocess.run(["otool", "-l", DYLIB], capture_output=True, text=True).stdout
    size = int(re.search(r"segname __TEXT\n\s+vmaddr \S+\n\s+vmsize (0x[0-9a-f]+)", text).group(1), 16)
    fd = os.open(DYLIB, os.O_RDONLY)
    addr = libc.mmap(None, size, 1, 1, fd, 0)
    page = 16384
    vec = ctypes.create_string_buffer((size + page - 1) // page)
    libc.mincore(addr, size, vec)
    resident = sum(1 for b in vec.raw if b & 1)
    libc.munmap(addr, size)
    os.close(fd)
    return resident * page / 1048576, size / 1048576


def measure(procs):
    pids = {app: p.pid for app, (p, port) in procs.items()}
    per_fp, combined, raw = footprint_mb(list(pids.values()))
    rows = {app: {"rss": rss_mb(pid), "footprint": per_fp.get(pid), "regions": vm_regions(pid)} for app, pid in pids.items()}
    return rows, combined, raw


def stop(p, port=None):
    # A clean exit first: an instrumented (PGO) library writes its profile only on a clean dump.
    if port is not None and p.poll() is None:
        for name, t in (("dump_profile", 60), ("close", 3)):
            try:
                op(port, name, timeout=t)
            except Exception:
                pass
        try:
            p.wait(timeout=8)
        except Exception:
            pass
    if p.poll() is None:
        p.terminate()
        try:
            p.wait(timeout=3)
        except subprocess.TimeoutExpired:
            p.kill()


def main():
    phase = sys.argv[1] if len(sys.argv) > 1 else "both"
    results = {"dylib_mb": os.path.getsize(DYLIB) / 1048576,
               "binaries_mb": {a: os.path.getsize(ROOT + "/target/release/" + a) / 1048576 for a in APPS}}
    if phase in ("alone", "both"):
        alone = {}
        for i, app in enumerate(APPS):
            port = 9810 + i
            t0 = time.time()
            p = start(app, port)
            ok = ready(p, port)
            startup = time.time() - t0
            time.sleep(2)
            if ok and rss_mb(p.pid) < PER_APP_CAP_MB:
                rows, _, _ = measure({app: (p, port)})
                alone[app] = dict(rows[app], startup_s=startup)
                print("alone %-13s rss %6.1f MB  footprint %6.1f MB  startup %.1fs" % (app, rows[app]["rss"], rows[app]["footprint"] or -1, startup), flush=True)
            else:
                alone[app] = {"error": "not ready" if not ok else "over cap"}
                print("alone %-13s %s" % (app, alone[app]["error"]), flush=True)
            stop(p, port)
            time.sleep(1)
        results["alone"] = alone
    if phase in ("together", "both"):
        procs = {}
        try:
            for i, app in enumerate(APPS):
                port = 9840 + i
                p = start(app, port)
                procs[app] = (p, port)
                if not ready(p, port):
                    print("together %-13s not ready" % app, flush=True)
                if sum(rss_mb(pp.pid) for pp, _ in procs.values() if pp.poll() is None) > TOTAL_CAP_MB:
                    print("TOTAL CAP %d MB hit after %s" % (TOTAL_CAP_MB, app), flush=True)
                    break
            time.sleep(3)
            live = {a: v for a, v in procs.items() if v[0].poll() is None}
            rows, combined, raw = measure(live)
            text_res, text_size = dylib_text_resident_mb()
            results["together"] = {"rows": rows, "combined_footprint": combined, "dylib_text_resident_union": text_res,
                                   "dylib_text_size": text_size, "n": len(live)}
            open(os.path.join(OUT, "footprint_together.txt"), "w").write(raw)
            mapped = sum(r["regions"].get("dylib __TEXT", [0, 0])[0] for r in rows.values())
            print("together: %d apps, sum rss %.1f MB, sum footprint %.1f MB; dylib __TEXT mapped-resident summed %.1f MB vs %.1f MB system-wide (of %.1f MB)"
                  % (len(live), sum(r["rss"] for r in rows.values()), sum((r["footprint"] or 0) for r in rows.values()), mapped, text_res, text_size), flush=True)
        finally:
            for p, port in procs.values():
                stop(p, port)
    json.dump(results, open(os.path.join(OUT, "results_%s.json" % phase), "w"), indent=1)


main()
