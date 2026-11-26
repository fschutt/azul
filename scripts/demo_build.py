#!/usr/bin/env python3
"""demo_build.py: build the demo apps of ONE job against the prebuilt libazul of their target.

    demo_build.py desktop --leg linux|macos-arm64|macos-intel|windows --apps "AzA AzB" --lib <libazul.a|azul.lib>
                          --out <dir> --results <dir> [--target <triple>] [--deployment-target 11.0] [--run-check]
    demo_build.py android --target <triple> --apps "..." --libs <dir of lib artifacts> --out <dir> --results <dir>
    demo_build.py ios     --target <triple> --apps "..." --libs <dir of lib artifacts> --out <dir> --results <dir>
    (android, ios)        [--variant mobile-e2e]: the QA build against the artifact-only lib variant of the manifest (the engine's
                          debug server), staged as <Name>-e2e-<column>, its result a file of its own with "variant" set

Every app is its own `cargo build` against the library its target built once (scripts/demo_lib.sh): the
engine is never compiled here, and scripts/demo_gate.py reads cargo's JSON of every build and turns a
build that did compile it into a FAILED app. A failing app never stops the others: each leaves a
`<app>.<column>.result.json` {app, column, target, status built|failed|skipped, reason, artifact, sha256,
size, shipped_size, seconds, engine_crates, first_error} that scripts/demo_apps.py status merges into the
table of the run. Exit 0 whenever the script itself ran; the table carries the verdicts.

Standard library only (3.8+) apart from the manifest reader (scripts/demo_apps.py needs tomllib, 3.11+).
"""
import argparse
import gzip
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent
sys.path.insert(0, str(HERE))
import android_elf_check  # noqa: E402
import demo_apps  # noqa: E402
import demo_gate  # noqa: E402


# ---------------------------------------------------------------------------------------------
# small helpers
# ---------------------------------------------------------------------------------------------
def count_initializers(text):
    """The number of entries `xcrun dyld_info -inits` lists: the lines that start with an address (`0x00024068  <anon-00024068>`)."""
    return sum(1 for line in (text or "").splitlines() if re.match(r"^\s+0x[0-9A-Fa-f]+\s+\S", line))


def apple_initializers(path, which=shutil.which):
    """How many initializers an Apple binary lists (the entries of __init_offsets on iOS, of __mod_init_func on macOS), None where the
    tool is not (a Linux or Windows runner) or fails. The generated ABI guard's load-time entry is one of them (dyld_info prints a Rust
    function pointer anonymously): ONE is the expected evidence, none the failure to look for."""
    if which("xcrun") is None:
        return None
    try:
        p = subprocess.run(["xcrun", "dyld_info", "-inits", str(path)], capture_output=True, text=True, timeout=120)
    except (OSError, subprocess.SubprocessError):
        return None
    return count_initializers(p.stdout) if p.returncode == 0 else None


def sha256_of(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def shipped_size(path):
    """The bytes the release serves for a desktop binary: the gzip of it (the asset is a .tar.gz). A phone file
    is served as it is (an .apk and a .zip are compressed already)."""
    p = pathlib.Path(path)
    if p.suffix in (".apk", ".zip"):
        return p.stat().st_size
    with open(p, "rb") as fh:
        return len(gzip.compress(fh.read(), 6))


def tail(path, lines=60):
    try:
        text = pathlib.Path(path).read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""
    return "\n".join(text.splitlines()[-lines:])


def first_error(json_log, stderr_log):
    """The first thing cargo said was wrong, in one line: a compiler error with its place, else the first
    `error` line of what it printed (a failed build script, a missing package)."""
    try:
        for msg in demo_gate.read_messages(json_log):
            if msg.get("reason") != "compiler-message":
                continue
            m = msg.get("message") or {}
            if m.get("level") != "error":
                continue
            text = (m.get("message") or "").strip().replace("\n", " ")
            spans = [s for s in (m.get("spans") or []) if s.get("is_primary")]
            where = f" ({spans[0]['file_name']}:{spans[0]['line_start']})" if spans else ""
            pkg = demo_gate.package_name(msg.get("package_id", ""))
            return f"{pkg}: {text}{where}"[:300]
    except OSError:
        pass
    for line in tail(stderr_log, 400).splitlines():
        if line.startswith("error"):
            return line.strip()[:300]
    last = [l for l in tail(stderr_log, 5).splitlines() if l.strip()]
    return (last[-1].strip()[:300]) if last else "no output"


def executable_of(json_log, package, kind="bin"):
    """The file cargo reports for the package's target of `kind` (scripts/cargo_bin_path.py, in-process)."""
    found = None
    for msg in demo_gate.read_messages(json_log):
        if msg.get("reason") != "compiler-artifact":
            continue
        target = msg.get("target") or {}
        if kind not in (target.get("kind") or []):
            continue
        if demo_gate.package_name(msg.get("package_id", "")) != package:
            continue
        if kind == "bin":
            exe = msg.get("executable")
        else:
            exe = next((f for f in (msg.get("filenames") or []) if f.endswith((".so", ".dylib", ".dll"))), None)
        if exe:
            found = exe
    return found


class Job:
    """What every build of one job shares: where things go and how a result is written."""

    def __init__(self, args):
        self.out = pathlib.Path(args.out)
        self.results = pathlib.Path(args.results)
        self.logs = self.out / "logs"
        for d in (self.out, self.results, self.logs):
            d.mkdir(parents=True, exist_ok=True)
        self.manifest = demo_apps.load()
        self.apps = {a["package"]: a for a in self.manifest["app"]}

    def result(self, app, column, target, status, reason="", artifact=None, seconds=0, gate=None, first_error="", variant="",
               initializers=None):
        r = {
            "app": app["package"], "column": column, "target": target, "status": status, "reason": reason,
            "artifact": None, "sha256": None, "size": None, "shipped_size": None, "seconds": int(seconds),
            "engine_crates": (gate or {}).get("engine_crates", []), "first_error": first_error,
            "dll_features": (gate or {}).get("dll_features", []),
        }
        if variant:     # a QA build (artifact only): its own file, never the shipped cell of the column
            r["variant"] = variant
        if initializers is not None:   # an Apple binary: how many initializers dyld_info lists (one is the ABI guard's load-time entry)
            r["initializers"] = int(initializers)
        if artifact:
            p = pathlib.Path(artifact)
            r.update(artifact=p.name, sha256=sha256_of(p), size=p.stat().st_size, shipped_size=shipped_size(p))
        (self.results / f"{app['package']}.{column}{'-e2e' if variant else ''}.result.json").write_text(
            json.dumps(r, indent=2, sort_keys=True), encoding="utf-8")
        mark = {"built": "OK  ", "failed": "FAIL", "skipped": "skip"}.get(status, status)
        extra = f"  {r['size']} bytes" if r["size"] else ""
        print(f"[{mark}] {app['package'] + ('-e2e' if variant else ''):<18} {column:<14} {int(seconds):>5}s{extra}  {first_error or reason}", flush=True)
        return r

    def run_cargo(self, cmd, env, tag):
        """Run a cargo command with its JSON on one file and its text on another. -> (code, json path, stderr path)"""
        jpath = self.logs / f"{tag}.json"
        epath = self.logs / f"{tag}.stderr.txt"
        with open(jpath, "wb") as jf, open(epath, "wb") as ef:
            code = subprocess.run(cmd, cwd=ROOT, env=env, stdout=jf, stderr=ef).returncode
        return code, jpath, epath

    def keep_failure_log(self, tag, jpath, epath):
        """A failed build keeps the end of its text; a good one drops its (large) logs."""
        keep = self.logs / f"{tag}.failure.txt"
        keep.write_text(tail(epath, 300), encoding="utf-8")
        for p in (jpath, epath):
            try:
                p.unlink()
            except OSError:
                pass

    def drop_logs(self, *paths):
        for p in paths:
            try:
                pathlib.Path(p).unlink()
            except OSError:
                pass


# ---------------------------------------------------------------------------------------------
# desktop
# ---------------------------------------------------------------------------------------------
def strip_binary(path, leg):
    if leg == "windows":
        return  # the MSVC build keeps its symbols in a .pdb next to the exe, not in it
    if shutil.which("strip") is None:
        return
    cmd = ["strip", "-x", str(path)] if leg.startswith("macos") else ["strip", "--strip-unneeded", str(path)]
    subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if leg.startswith("macos") and shutil.which("codesign"):
        # strip drops the signature the linker wrote; an arm64 Mac refuses to start an executable without one
        subprocess.run(["codesign", "--force", "--sign", "-", str(path)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def check_macos_binary(path, leg, deployment_target):
    """An Intel build must be x86_64 and ask for no newer macOS than the page promises (the arm64 one: 11.0)."""
    if not leg.startswith("macos") or shutil.which("lipo") is None:
        return ""
    archs = subprocess.run(["lipo", "-archs", str(path)], capture_output=True, text=True).stdout.strip()
    want = "x86_64" if leg == "macos-intel" else "arm64"
    if archs != want:
        return f"{pathlib.Path(path).name} is '{archs}', not {want}"
    dump = subprocess.run(["otool", "-l", str(path)], capture_output=True, text=True).stdout.splitlines()
    minos = ""
    for i, line in enumerate(dump):
        if "LC_BUILD_VERSION" in line or "LC_VERSION_MIN_MACOSX" in line:
            for follow in dump[i + 1:i + 8]:
                parts = follow.split()
                if len(parts) == 2 and parts[0] in ("minos", "version"):
                    minos = parts[1]
                    break
            break
    if deployment_target and minos and minos != deployment_target:
        return f"{pathlib.Path(path).name} needs macOS {minos}, not {deployment_target}"
    return ""


def run_check(path, seconds=6):
    """Start the staged app under a virtual display for a few seconds: alive at the end is a pass. Informational."""
    if shutil.which("xvfb-run") is None:
        return "skipped: no xvfb-run"
    try:
        p = subprocess.run(["xvfb-run", "-a", "timeout", "-s", "KILL", str(seconds), str(path)],
                           capture_output=True, text=True, timeout=seconds + 30)
    except subprocess.TimeoutExpired:
        return "ok (still running)"
    # timeout -s KILL ends the child with 137 when it had to
    if p.returncode in (124, 137, -9):
        return "ok (still running after %ds)" % seconds
    err = (p.stderr or p.stdout or "").strip().splitlines()
    return f"exited {p.returncode} before {seconds}s: {(err[-1] if err else '')[:160]}"


def desktop(args, job):
    leg = args.leg
    lib = pathlib.Path(args.lib).resolve()
    if not lib.is_file():
        sys.exit(f"demo_build: {lib} is not a file")
    env = dict(os.environ)
    env["AZ_LINK_PATH"] = str(lib)
    env["CARGO_PROFILE_RELEASE_LTO"] = "off"
    env["CARGO_PROFILE_RELEASE_DEBUG"] = "0"
    if args.deployment_target:
        env["MACOSX_DEPLOYMENT_TARGET"] = args.deployment_target
    checks = {}
    for pkg in args.apps.split():
        app = job.apps[pkg]
        started = time.time()
        tag = f"{pkg}.{leg}"
        cmd = ["cargo", "build", "--release", "-p", pkg, "--bin", pkg, "--message-format=json-render-diagnostics"]
        if args.target:
            cmd += ["--target", args.target]
        for f in app["features"]:
            cmd += ["--features", f]
        code, jpath, epath = job.run_cargo(cmd, env, tag)
        gate = None
        try:
            gate = demo_gate.check(str(jpath), allow=tuple(app["engine_ok"]))
        except OSError:
            pass
        if code != 0:
            err = first_error(jpath, epath)
            job.keep_failure_log(tag, jpath, epath)
            job.result(app, leg, args.target, "failed", "cargo build failed", seconds=time.time() - started,
                       gate=gate, first_error=err)
            continue
        exe = executable_of(jpath, pkg)
        if not exe or not pathlib.Path(exe).is_file():
            job.keep_failure_log(tag, jpath, epath)
            job.result(app, leg, args.target, "failed", "cargo named no executable it produced",
                       seconds=time.time() - started, gate=gate, first_error=f"no executable for {pkg}")
            continue
        engine_bad = bool(gate and (gate["engine_crates"] or gate["dll_engine_features"]))
        if engine_bad:
            # the gate: this build compiled the engine instead of linking the prebuilt libazul
            job.result(app, leg, args.target, "failed", "the engine was compiled into the app", seconds=time.time() - started,
                       gate=gate, first_error="engine crates compiled: " + ", ".join(gate["engine_crates"] + gate["dll_engine_features"]))
            job.drop_logs(jpath, epath)
            continue
        dst = job.out / demo_apps.staged_name(app, leg)
        shutil.copy2(exe, dst)
        os.chmod(dst, 0o755)
        strip_binary(dst, leg)
        problem = check_macos_binary(dst, leg, args.deployment_target)
        if problem:
            job.result(app, leg, args.target, "failed", problem, seconds=time.time() - started, gate=gate, first_error=problem)
            dst.unlink()
            job.drop_logs(jpath, epath)
            continue
        job.result(app, leg, args.target, "built", seconds=time.time() - started, artifact=dst, gate=gate,
                   initializers=apple_initializers(dst) if leg.startswith("macos") else None)
        job.drop_logs(jpath, epath)
        if args.run_check:
            checks[pkg] = run_check(dst)
            print(f"      starts: {pkg}: {checks[pkg]}", flush=True)
    if checks:
        (job.results / f"startcheck.{leg}.json").write_text(json.dumps(checks, indent=2, sort_keys=True), encoding="utf-8")


# ---------------------------------------------------------------------------------------------
# phones
# ---------------------------------------------------------------------------------------------
def lib_file(libs_dir, artifact, kind):
    d = pathlib.Path(libs_dir) / artifact
    f = d / ("libazul.so" if kind == "android" else "libazul.a")
    return f if f.is_file() else None


def ios_plist_env(app):
    """The Info.plist texts of an app as the environment build-ios.sh reads them: AZ_IOS_LOCAL_NETWORK (the Local Network prompt, with
    NSBonjourServices _irohv1._udp), AZ_IOS_USAGE (one NS...UsageDescription=text per line) and AZ_IOS_BONJOUR_SERVICES."""
    env = {}
    ios = app.get("ios") or {}
    if ios.get("local_network"):
        env["AZ_IOS_LOCAL_NETWORK"] = ios["local_network"]
    if ios.get("usage"):
        env["AZ_IOS_USAGE"] = "\n".join(f"{k}={v}" for k, v in sorted(ios["usage"].items()))
    if ios.get("bonjour_services"):
        env["AZ_IOS_BONJOUR_SERVICES"] = ",".join(ios["bonjour_services"])
    return env


def strings_dir(app, work):
    """<work>/<lang>.lproj/InfoPlist.strings from the manifest's [app.ios.strings.<lang>]."""
    strings = (app.get("ios") or {}).get("strings") or {}
    if not strings:
        return None
    for lang, table in strings.items():
        d = pathlib.Path(work) / f"{lang}.lproj"
        d.mkdir(parents=True, exist_ok=True)
        lines = []
        for key, value in sorted(table.items()):
            esc = str(value).replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n")
            lines.append(f'"{key}" = "{esc}";')
        (d / "InfoPlist.strings").write_text("\n".join(lines) + "\n", encoding="utf-8")
    return pathlib.Path(work)


def version_info():
    run = os.environ.get("GITHUB_RUN_NUMBER", "1")
    sha = os.environ.get("GITHUB_SHA", "")[:7] or "local"
    return {"code": run, "name": f"0.2.0-{sha}", "build": run}


def phone(args, job, kind):
    column = {"aarch64-linux-android": "android-arm64", "x86_64-linux-android": "android-x86_64",
              "aarch64-apple-ios": "ios-device", "aarch64-apple-ios-sim": "ios-sim"}[args.target]
    ver = version_info()
    variant = args.variant
    for pkg in args.apps.split():
        app = job.apps[pkg]
        started = time.time()
        tag = f"{pkg}.{column}" + ("-e2e" if variant else "")
        artifact_name = f"demolib-{column}-{variant or app['mobile_lib']}"
        lib = lib_file(args.libs, artifact_name, kind)
        if lib is None:
            job.result(app, column, args.target, "failed", f"the library {artifact_name} did not arrive",
                       seconds=time.time() - started, first_error=f"no libazul in {artifact_name}", variant=variant)
            continue
        env = dict(os.environ)
        env["CARGO_PROFILE_RELEASE_DEBUG"] = "0"
        json_log = job.logs / f"{tag}.json"
        err_log = job.logs / f"{tag}.stderr.txt"
        work = job.logs / f"{tag}.work"
        work.mkdir(exist_ok=True)
        if kind == "android":
            env.update(AZ_ANDROID_NO_DEPLOY="1", AZ_ANDROID_LINK_LIB=str(lib), AZ_ANDROID_BUILD_JSON=str(json_log),
                       LABEL=app["name"] + (" e2e" if variant else ""), VERSION_NAME=ver["name"], VERSION_CODE=ver["code"])
            perms = (app.get("android") or {}).get("permissions") or []
            if perms:
                env["AZ_ANDROID_PERMISSIONS"] = ",".join(perms)
            cmd = ["bash", "scripts/build-android.sh", args.target, pkg, app["bundle_id"], pkg]
        else:
            env.update(AZ_IOS_DRYRUN="1", AZ_IOS_LINK_LIB=str(lib), AZ_IOS_BUILD_JSON=str(json_log), APP_NAME=pkg,
                       AZ_IOS_SYMBOL_JSON=str(job.results / f"{pkg}.{column}{'-e2e' if variant else ''}.symbols.json"),
                       BUNDLE_ID=app["bundle_id"], DISPLAY_NAME=app["name"] + (" e2e" if variant else ""), VERSION="0.2.0",
                       BUILD=ver["build"])
            env.update(ios_plist_env(app))
            sd = strings_dir(app, work / "strings")
            if sd:
                env["AZ_IOS_STRINGS_DIR"] = str(sd)
            cmd = ["bash", "scripts/build-ios.sh", args.target, pkg]
        with open(err_log, "wb") as ef:
            code = subprocess.run(cmd, cwd=ROOT, env=env, stdout=ef, stderr=subprocess.STDOUT).returncode
        gate = None
        try:
            gate = demo_gate.check(str(json_log), allow=tuple(app["engine_ok"]))
        except OSError:
            pass
        if code != 0:
            err = first_error(json_log, err_log)
            job.keep_failure_log(tag, json_log, err_log)
            shutil.rmtree(work, ignore_errors=True)
            job.result(app, column, args.target, "failed", f"{cmd[1]} failed", seconds=time.time() - started, gate=gate, first_error=err,
                       variant=variant)
            continue
        if gate and (gate["engine_crates"] or gate["dll_engine_features"]):
            job.result(app, column, args.target, "failed", "the engine was compiled into the app", seconds=time.time() - started, gate=gate,
                       first_error="engine crates compiled: " + ", ".join(gate["engine_crates"] + gate["dll_engine_features"]),
                       variant=variant)
            job.drop_logs(json_log, err_log)
            shutil.rmtree(work, ignore_errors=True)
            continue
        dst = job.out / demo_apps.staged_name(app, column, e2e=bool(variant))
        try:
            if kind == "android":
                abi = {"aarch64-linux-android": "arm64-v8a", "x86_64-linux-android": "x86_64"}[args.target]
                apk = ROOT / "target" / "android-bundle" / f"{pkg}-{abi}" / "aligned.apk"
                verify_apk(apk, abi)
                shutil.copy2(apk, dst)
            else:
                app_dir = ROOT / "target" / "ios-bundle" / args.target / f"{pkg}.app"
                if not app_dir.is_dir():
                    raise RuntimeError(f"build-ios.sh exited 0 but produced no {app_dir}")
                if dst.exists():
                    dst.unlink()
                subprocess.run(["zip", "-qry", str(dst.resolve()), app_dir.name], cwd=app_dir.parent, check=True)
        except Exception as e:  # noqa: BLE001  (a bad bundle is this app's failure, not the job's)
            job.result(app, column, args.target, "failed", "the bundle is not sound", seconds=time.time() - started,
                       gate=gate, first_error=str(e)[:300], variant=variant)
            job.drop_logs(json_log, err_log)
            shutil.rmtree(work, ignore_errors=True)
            continue
        job.result(app, column, args.target, "built", seconds=time.time() - started, artifact=dst, gate=gate, variant=variant,
                   initializers=apple_initializers(ROOT / "target" / "ios-bundle" / args.target / f"{pkg}.app" / pkg) if kind == "ios" else None)
        job.drop_logs(json_log, err_log)
        shutil.rmtree(work, ignore_errors=True)


def verify_apk(apk, abi):
    """What the phone checks before it runs anything (the old job's checks, kept)."""
    if not apk.is_file():
        raise RuntimeError(f"no APK at {apk}")
    sdk = os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT") or ""
    bt = pathlib.Path(sdk) / "build-tools" / "34.0.0"
    listing = subprocess.run(["unzip", "-l", str(apk)], capture_output=True, text=True).stdout
    for needle in (f"lib/{abi}/libazul.so", "classes.dex"):
        if needle not in listing:
            raise RuntimeError(f"the APK has no {needle}")
    # a device with 16 KiB pages (Android 15 and later) cannot load a library whose segments are aligned to 4 KiB, NDK 27's default:
    # every library of the APK has p_align >= 0x4000 (the rustflags of .cargo/config.toml), the failing file is named
    problems = android_elf_check.check_apk(str(apk))
    if problems:
        raise RuntimeError("; ".join(problems))
    if (bt / "apksigner").exists():
        r = subprocess.run([str(bt / "apksigner"), "verify", str(apk)], capture_output=True, text=True)
        if r.returncode != 0:
            raise RuntimeError("the APK is not validly signed: " + (r.stderr or r.stdout).strip()[:200])
    if (bt / "aapt2").exists():
        badging = subprocess.run([str(bt / "aapt2"), "dump", "badging", str(apk)], capture_output=True, text=True).stdout
        if "launchable-activity" not in badging:
            raise RuntimeError("the APK declares no launchable activity")


# ---------------------------------------------------------------------------------------------
def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="kind", required=True)
    d = sub.add_parser("desktop")
    d.add_argument("--leg", required=True, choices=demo_apps.DESKTOP)
    d.add_argument("--lib", required=True)
    d.add_argument("--target", default="")
    d.add_argument("--deployment-target", default="")
    d.add_argument("--run-check", action="store_true")
    for name in ("android", "ios"):
        p = sub.add_parser(name)
        p.add_argument("--target", required=True)
        p.add_argument("--libs", required=True)
        p.add_argument("--variant", default="")
    for p in sub.choices.values():
        p.add_argument("--apps", required=True)
        p.add_argument("--out", required=True)
        p.add_argument("--results", required=True)
    args = ap.parse_args(argv)
    job = Job(args)
    unknown = [a for a in args.apps.split() if a not in job.apps]
    if unknown:
        sys.exit(f"demo_build: not in scripts/demo_apps.toml: {unknown}")
    variant = getattr(args, "variant", "")
    if variant and variant not in demo_apps.artifact_only_libs(job.manifest):
        sys.exit(f"demo_build: --variant {variant!r} is no artifact_only lib of scripts/demo_apps.toml "
                 f"({sorted(demo_apps.artifact_only_libs(job.manifest))}): a shipped build takes the lib its app names")
    if args.kind == "desktop":
        desktop(args, job)
    else:
        phone(args, job, args.kind)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
