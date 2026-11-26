#!/usr/bin/env python3
"""demo_apps.py: the one reader of scripts/demo_apps.toml.

    demo_apps.py check                 the manifest, the workspace, the manifests of the apps and the
                                       release-page generator agree (fails with every disagreement)
    demo_apps.py plan [--e2e]          the build matrices of a `demos` run, as GITHUB_OUTPUT lines (--e2e: also the
                                       artifact-only QA variant of the phone libazul and the apps that ask for it)
    demo_apps.py assets [<package>]    the file names every app is built as, one per line
    demo_apps.py weights <status.json> the weight the measured build seconds of a run suggest for each app
    demo_apps.py status <dir> ...      merge the result files of a run into demo-status.json / .md

Python 3.11+ (tomllib); the build scripts that run on every runner (demo_build.py, demo_gate.py) need
only the standard library of older ones.
"""
import json
import os
import pathlib
import re
import sys

try:
    import tomllib
except ImportError:  # pragma: no cover
    sys.exit("demo_apps.py needs Python 3.11 or later (tomllib)")

ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "scripts" / "demo_apps.toml"

DESKTOP = ("linux", "macos-arm64", "macos-intel", "windows")
MOBILE = ("android", "ios")
PLATFORMS = DESKTOP + MOBILE
# What a platform is built as: the columns of the status table and the suffixes of the assets.
OS_SUFFIX = {"linux": "linux", "macos-arm64": "macos", "macos-intel": "macos-intel", "windows": "windows"}
BIN_EXT = {"windows": ".exe"}
MOBILE_TARGETS = {
    "android-arm64": "aarch64-linux-android",
    "android-x86_64": "x86_64-linux-android",
    "ios-device": "aarch64-apple-ios",
    "ios-sim": "aarch64-apple-ios-sim",
}
ARTIFACT_COLUMNS = DESKTOP + tuple(MOBILE_TARGETS)  # one status column per built file
LEG_RUNNER = {
    "linux": "ubuntu-22.04",
    "macos-arm64": "macos-15",
    "macos-intel": "macos-15",
    "windows": "windows-2022",
}
LEG_CODEGEN_OS = {"linux": "Linux", "macos-arm64": "macOS", "macos-intel": "macOS", "windows": "Windows"}
MAX_DESCRIPTION = 110


# ---------------------------------------------------------------------------------------------
# The manifest
# ---------------------------------------------------------------------------------------------
def load(path=MANIFEST):
    with open(path, "rb") as fh:
        m = tomllib.load(fh)
    for app in m.get("app", []):
        app.setdefault("name", app["package"])
        app.setdefault("bundle_id", "com.azul." + app["package"])
        app.setdefault("weight", 1)
        app.setdefault("docker", False)
        app.setdefault("features", [])
        app.setdefault("needs", [])
        app.setdefault("known_failing", {})
        app.setdefault("engine_ok", {})
        app.setdefault("e2e", False)
    return m


def apps_of(m, platform=None):
    return [a for a in m["app"] if platform is None or platform in a["platforms"]]


def staged_name(app, column, e2e=False):
    """The file a build produces for `column` (a desktop platform or a MOBILE_TARGETS key). `e2e`: the QA build against the
    artifact-only lib with the engine's debug server, <Name>-e2e-<column>: a phone file only, never a release asset."""
    p = app["package"]
    if column in DESKTOP:
        if e2e:
            raise ValueError(f"the QA variant is a phone build, not {column}")
        return f"{p}-{OS_SUFFIX[column]}{BIN_EXT.get(column, '')}"
    if e2e:
        p += "-e2e"
    return {
        "android-arm64": f"{p}-android-arm64.apk",
        "android-x86_64": f"{p}-android-x86_64.apk",
        "ios-device": f"{p}-ios-device.app.zip",
        "ios-sim": f"{p}-ios-sim.app.zip",
    }[column]


def artifact_only_libs(m):
    """{name: lib} of the library variants that are QA artifacts and never ship."""
    return {k: v for k, v in m.get("lib", {}).items() if v.get("artifact_only")}


def release_name(app, column):
    """The asset name on the GitHub release: the desktop binaries are tarred, the phone files are not."""
    n = staged_name(app, column)
    return n + ".tar.gz" if column in DESKTOP else n


def columns_of(app):
    """The built files of an app, in column order."""
    cols = []
    for p in app["platforms"]:
        if p in DESKTOP:
            cols.append(p)
        elif p == "android":
            cols += ["android-arm64", "android-x86_64"]
        elif p == "ios":
            cols += ["ios-device", "ios-sim"]
    return [c for c in ARTIFACT_COLUMNS if c in cols]


# ---------------------------------------------------------------------------------------------
# check
# ---------------------------------------------------------------------------------------------
def workspace_az_packages():
    """{package: dir} of the Az* packages the workspace lists (the root Cargo.toml's members)."""
    with open(ROOT / "Cargo.toml", "rb") as fh:
        members = tomllib.load(fh)["workspace"]["members"]
    found = {}
    for member in members:
        manifest = ROOT / member / "Cargo.toml"
        if not manifest.is_file():
            continue
        with open(manifest, "rb") as fh:
            name = tomllib.load(fh).get("package", {}).get("name", "")
        if re.fullmatch(r"Az[A-Z][A-Za-z0-9]*", name):
            found[name] = member
    return found


def check(m=None):
    m = m or load()
    errors = []
    err = errors.append
    apps = m["app"]
    names = [a["package"] for a in apps]
    for dup in sorted({n for n in names if names.count(n) > 1}):
        err(f"{dup} is listed twice")
    workspace = workspace_az_packages()
    for pkg in sorted(set(workspace) - set(names)):
        err(f"{pkg} is a package of the workspace ({workspace[pkg]}) but not in scripts/demo_apps.toml: "
            f"add it, with its platforms (an app that does not belong on the page says why in `known_failing`)")
    for pkg in sorted(set(names) - set(workspace)):
        err(f"{pkg} is in scripts/demo_apps.toml but is no AzXxx package of the workspace")
    libs = m.get("lib", {})
    for name, lib in artifact_only_libs(m).items():
        bad = [t for t in lib.get("targets", []) if t not in MOBILE_TARGETS]
        if bad or not lib.get("targets"):
            err(f"lib {name}: targets {lib.get('targets')} must be a non-empty subset of the phone columns {list(MOBILE_TARGETS)}")
        if "debug-server" not in str(lib.get("features", "")).split(","):
            err(f"lib {name} is artifact_only (the QA variant with the engine's debug server) and carries no debug-server feature")
    assets = {}
    for a in apps:
        p = a["package"]
        where = f"{p}:"
        d = ROOT / a.get("dir", "")
        if workspace.get(p) and a.get("dir") != workspace[p]:
            err(f"{where} dir is {a.get('dir')!r}, the workspace has it at {workspace[p]!r}")
        if not (d / "Cargo.toml").is_file():
            err(f"{where} {a.get('dir')}/Cargo.toml does not exist")
            continue
        if not re.fullmatch(r"Az[A-Z][A-Za-z0-9]*", p):
            err(f"{where} breaks the AzXxx convention (package, binary and asset stem are one string)")
        desc = a.get("description", "")
        if not desc or len(desc) > MAX_DESCRIPTION or "\n" in desc:
            err(f"{where} the description is empty, longer than {MAX_DESCRIPTION} characters or has a line break")
        bad = [x for x in a.get("platforms", []) if x not in PLATFORMS]
        if bad or not a.get("platforms"):
            err(f"{where} platforms {a.get('platforms')} must be a non-empty subset of {list(PLATFORMS)}")
        if any(x in MOBILE for x in a["platforms"]):
            lib = a.get("mobile_lib")
            if lib not in libs or lib == "desktop":
                err(f"{where} is built for a phone and needs mobile_lib = one of "
                    f"{[k for k in libs if k != 'desktop']}, not {lib!r}")
            elif libs[lib].get("artifact_only"):
                err(f"{where} mobile_lib {lib!r} is artifact_only: a shipped app never links a QA variant of the library")
        if a.get("e2e"):
            qa_columns = {c for lib_ in artifact_only_libs(m).values() for c in lib_.get("targets", [])}
            if not set(columns_of(a)) & qa_columns:
                err(f"{where} says e2e = true but no artifact_only lib builds a column of it "
                    f"({sorted(columns_of(a))} against {sorted(qa_columns)})")
        for k in a["known_failing"]:
            if k not in ARTIFACT_COLUMNS and k not in PLATFORMS:
                err(f"{where} known_failing names {k!r}, which is no platform")
        for k, why in a["known_failing"].items():
            if not str(why).strip():
                err(f"{where} known_failing.{k} needs the reason")
        for k, why in a["engine_ok"].items():
            if k not in ("azul-core", "azul-layout", "azul-css") or not str(why).strip():
                err(f"{where} engine_ok.{k}: names an engine crate (azul-core, azul-layout, azul-css) and says why the app compiles it")
        manifest = tomllib.loads((d / "Cargo.toml").read_text(encoding="utf-8"))
        # an app whose build is a known failure says why in the manifest, and is not held to what it lacks
        pending = bool(a["known_failing"])
        # the binary is named after the package (dll/tests/demo_naming_contract.rs: the asset name is built from it)
        bins = [b.get("name") for b in manifest.get("bin", [])]
        if bins and p not in bins and not pending:
            err(f"{where} declares [[bin]] {bins} and none is named {p}: the staged file and the page would not agree")
        # the app links the prebuilt libazul: its dependency on azul-dll is the link-dynamic facade, nothing more
        deps = [manifest.get("dependencies", {})]
        for target in manifest.get("target", {}).values():
            deps.append(target.get("dependencies", {}))
        for table in deps:
            dll = table.get("azul")
            if dll is None:
                continue
            feats = set(dll.get("features", []))
            engine = feats & {"link-static", "cabi_internal", "cabi_export", "e2e-scripting", "debug-server", "dist", "build-dll"}
            if engine:
                err(f"{where} asks azul-dll for {sorted(engine)}: that compiles the engine into the app "
                    f"(every app links the prebuilt libazul; the feature belongs in [lib.*] of the manifest)")
        for target, table in manifest.get("target", {}).items():
            if "azul" in table.get("dependencies", {}) and any(x in target for x in ("android", "ios")):
                err(f"{where} a target-specific azul-dll entry for {target}: the single [dependencies] entry serves every target")
        if any(x in MOBILE for x in a["platforms"]):
            crate_types = manifest.get("lib", {}).get("crate-type", [])
            if "cdylib" not in crate_types and "android" in a["platforms"] and not pending:
                err(f"{where} is built for Android and needs [lib] crate-type to include cdylib")
        for col in columns_of(a):
            for name in (staged_name(a, col), release_name(a, col)):
                other = assets.setdefault(name, p)
                if other != p:
                    err(f"{where} asset {name} is also {other}'s")
    wf = (ROOT / ".github" / "workflows" / "rust.yml").read_text(encoding="utf-8")
    for line in wf.splitlines():
        if re.search(r"for\s+demo\s+in\s+Az[A-Z]", line):
            err("rust.yml still loops over a hard-coded list of apps: " + line.strip())
    return errors


# ---------------------------------------------------------------------------------------------
# plan
# ---------------------------------------------------------------------------------------------
def pack(apps, groups):
    """Greedy balance of `apps` (weight descending) into at most `groups` bins -> [[package, ...], ...]."""
    groups = max(1, min(groups, len(apps)))
    bins = [[0, []] for _ in range(groups)]
    for a in sorted(apps, key=lambda a: (-a["weight"], a["package"])):
        lightest = min(bins, key=lambda b: (b[0], len(b[1])))
        lightest[0] += a["weight"]
        lightest[1].append(a["package"])
    return [sorted(b[1]) for b in bins if b[1]]


def azul_version():
    """AZUL_VERSION of rust.yml's env: the release the assets belong to (one place names it)."""
    text = (ROOT / ".github" / "workflows" / "rust.yml").read_text(encoding="utf-8")
    m = re.search(r'(?m)^  AZUL_VERSION:\s*"([^"]+)"', text)
    return m.group(1) if m else ""


def plan(m=None, e2e=False):
    """The matrices of a run. `e2e`: also the artifact-only QA variant (the input of the run asks for it)."""
    m = m or load()
    plan_cfg = m.get("plan", {})
    out = {"version": azul_version()}
    libs = m["lib"]
    desktop_targets = {
        "linux": dict(target="", os=LEG_RUNNER["linux"], lib_file="libazul.a"),
        "macos-arm64": dict(target="", os=LEG_RUNNER["macos-arm64"], deployment_target="11.0", lib_file="libazul.a"),
        "macos-intel": dict(target="x86_64-apple-darwin", os=LEG_RUNNER["macos-intel"], deployment_target="10.12", lib_file="libazul.a"),
        "windows": dict(target="", os=LEG_RUNNER["windows"], lib_file="azul.lib"),
    }
    family = {"linux": "linux", "macos-arm64": "macos", "macos-intel": "macos", "windows": "windows"}
    lib_jobs = {k: [] for k in ("linux", "macos", "windows", "android", "ios")}
    for leg in DESKTOP:
        if apps_of(m, leg):
            t = desktop_targets[leg]
            lib_jobs[family[leg]].append(dict(
                id=f"lib-{leg}", kind="desktop", leg=leg, variant="desktop", features=libs["desktop"]["features"],
                profile=libs["desktop"].get("profile", "prod-release"), codegen_os=LEG_CODEGEN_OS[leg],
                artifact=f"demolib-{leg}", **{k: v for k, v in t.items() if k != "lib_file"}))
    needed_variants = {}
    for a in m["app"]:
        if any(p in MOBILE for p in a["platforms"]):
            needed_variants.setdefault(a["mobile_lib"], []).append(a)
    for variant, vapps in sorted(needed_variants.items()):
        for col, target in MOBILE_TARGETS.items():
            kind = "android" if col.startswith("android") else "ios"
            if not any(col in columns_of(a) for a in vapps):
                continue
            features = libs[variant]["features"] + (",android-activity" if kind == "android" else "")
            lib_jobs[kind].append(dict(
                id=f"lib-{col}-{variant}", kind=kind, leg=col, variant=variant, features=features, target=target,
                os="ubuntu-22.04" if kind == "android" else "macos-15", codegen_os="Linux" if kind == "android" else "macOS",
                artifact=f"demolib-{col}-{variant}"))
    if e2e:
        wanting = [a for a in m["app"] if a.get("e2e")]
        for variant, lib in sorted(artifact_only_libs(m).items()):
            for col in lib["targets"]:
                if not any(col in columns_of(a) for a in wanting):
                    continue
                kind = "android" if col.startswith("android") else "ios"
                lib_jobs[kind].append(dict(
                    id=f"lib-{col}-{variant}", kind=kind, leg=col, variant=variant,
                    features=lib["features"] + (",android-activity" if kind == "android" else ""), target=MOBILE_TARGETS[col],
                    os="ubuntu-22.04" if kind == "android" else "macos-15", codegen_os="Linux" if kind == "android" else "macOS",
                    artifact=f"demolib-{col}-{variant}"))
    for k, v in lib_jobs.items():
        out["libs_" + k] = v
    # --- the desktop apps: one job per (leg, group)
    apps_by_family = {k: [] for k in ("linux", "macos", "windows")}
    for leg in DESKTOP:
        apps = apps_of(m, leg)
        if not apps:
            continue
        n = plan_cfg.get("groups", {}).get(leg, 4)
        groups = pack(apps, n)
        for i, group in enumerate(groups, 1):
            t = desktop_targets[leg]
            apps_by_family[family[leg]].append(dict(
                leg=leg, group=i, groups=len(groups), apps=" ".join(group), os=LEG_RUNNER[leg], os_suffix=OS_SUFFIX[leg],
                bin_ext=BIN_EXT.get(leg, ""), codegen_os=LEG_CODEGEN_OS[leg], lib_artifact=f"demolib-{leg}",
                lib_file=t["lib_file"], target=t["target"], deployment_target=t.get("deployment_target", ""),
                artifact=f"demoapps-{leg}-{i}", results_artifact=f"demoresults-{leg}-{i}"))
    for k, v in apps_by_family.items():
        out["apps_" + k] = v
    # --- the phone apps: one job per target, every app of it against its own variant of the library
    for kind in ("android", "ios"):
        entries = []
        for col, target in MOBILE_TARGETS.items():
            if not col.startswith(kind):
                continue
            apps = [a for a in m["app"] if col in columns_of(a)]
            if not apps:
                continue
            entries.append(dict(
                leg=col, kind=kind, target=target, apps=" ".join(a["package"] for a in apps),
                os="ubuntu-22.04" if kind == "android" else "macos-15", codegen_os="Linux" if kind == "android" else "macOS",
                lib_artifacts=" ".join(sorted({f"demolib-{col}-{a['mobile_lib']}" for a in apps})),
                artifact=f"demoapps-{col}", results_artifact=f"demoresults-{col}", variant=""))
        if e2e:   # the QA builds: the same app build against the artifact-only lib, in artifacts the shipped-files patterns never match
            for variant, lib in sorted(artifact_only_libs(m).items()):
                for col, target in MOBILE_TARGETS.items():
                    apps = [a for a in m["app"] if a.get("e2e") and col in columns_of(a)]
                    if col.startswith(kind) and col in lib["targets"] and apps:
                        entries.append(dict(
                            leg=col, kind=kind, target=target, apps=" ".join(a["package"] for a in apps),
                            os="ubuntu-22.04" if kind == "android" else "macos-15", codegen_os="Linux" if kind == "android" else "macOS",
                            lib_artifacts=f"demolib-{col}-{variant}", artifact=f"demoe2e-{col}",
                            results_artifact=f"demoresults-{col}-e2e", variant=variant))
        out["apps_" + kind] = entries
    # the buttons the boot tests press after the first screenshot: {package: label}
    out["smoke_taps"] = json.dumps({a["package"]: a["smoke"]["tap"] for a in m["app"]
                                    if (a.get("smoke") or {}).get("tap")}, sort_keys=True)
    out["smoke_expects"] = json.dumps({a["package"]: a["smoke"]["expect"] for a in m["app"]
                                       if (a.get("smoke") or {}).get("expect")}, sort_keys=True)
    return out


def write_github_output(out):
    lines = []
    for key, value in out.items():
        text = json.dumps({"include": value}, separators=(",", ":")) if isinstance(value, list) else str(value)
        lines.append(f"{key}={text}")
    target = os.environ.get("GITHUB_OUTPUT")
    if target:
        with open(target, "a", encoding="utf-8") as fh:
            fh.write("\n".join(lines) + "\n")
    return lines


# ---------------------------------------------------------------------------------------------
# status
# ---------------------------------------------------------------------------------------------
CELL = {"built": "built", "reused": "reused", "failed": "FAILED", "skipped": "skipped"}


def human(n):
    n = float(n or 0)
    for unit in ("B", "KB", "MB", "GB"):
        if n < 1024 or unit == "GB":
            return f"{n:.0f} {unit}" if unit == "B" else f"{n:.1f} {unit}"
        n /= 1024
    return str(n)


def read_results(dirs):
    results = {}
    for d in dirs:
        for f in sorted(pathlib.Path(d).rglob("*.result.json")):
            try:
                r = json.loads(f.read_text(encoding="utf-8"))
            except ValueError:
                continue
            if r.get("variant"):    # a QA build (artifact only): not the shipped cell of the same column
                continue
            results[(r["app"], r["column"])] = r
    return results


def read_qa_builds(dirs):
    """The results of the artifact-only QA builds, [{app, column, variant, status, artifact, size, seconds, first_error}]."""
    found = {}
    for d in dirs:
        for f in sorted(pathlib.Path(d).rglob("*.result.json")):
            try:
                r = json.loads(f.read_text(encoding="utf-8"))
            except ValueError:
                continue
            if r.get("variant"):
                found[(r["app"], r["column"], r["variant"])] = {k: r.get(k) for k in (
                    "app", "column", "variant", "status", "reason", "artifact", "size", "seconds", "first_error")}
    return [found[k] for k in sorted(found)]


def read_smoke(dirs):
    """{(package, column): result} from the boot tests: <dir>/<package>-<column>/result.json (scripts/ci/*_smoke.sh)."""
    found = {}
    for d in dirs:
        for f in sorted(pathlib.Path(d).rglob("result.json")):
            try:
                r = json.loads(f.read_text(encoding="utf-8"))
            except ValueError:
                continue
            if "alive" not in r:
                continue
            for column in ("android-x86_64", "ios-sim"):
                stem = str(r.get("app", ""))
                if stem.endswith("-" + column):
                    found[(stem[: -len(column) - 1], column)] = r
    return found


def read_startchecks(dirs):
    """{(package, leg): text} from the desktop start checks (demo_build.py --run-check: startcheck.<leg>.json)."""
    found = {}
    for d in dirs:
        for f in sorted(pathlib.Path(d).rglob("startcheck.*.json")):
            leg = f.name[len("startcheck."):-len(".json")]
            try:
                for package, text in json.loads(f.read_text(encoding="utf-8")).items():
                    found[(package, leg)] = text
            except ValueError:
                continue
    return found


def read_libs(dirs):
    """The libazul.info.json files the library jobs leave: [{kind, target, features, seconds, size, ...}]."""
    libs = []
    for d in dirs:
        for f in sorted(pathlib.Path(d).rglob("libazul.info.json")):
            try:
                libs.append(json.loads(f.read_text(encoding="utf-8")))
            except ValueError:
                continue
    return libs


def boot_text(r):
    if r is None:
        return None
    if "pass" in r:   # the verdict of scripts/ci/smoke_verdict.py: the one pass line
        if not r["pass"]:
            return "BOOT FAILED: " + str(r.get("reason") or "no reason given")[:110]
        text = "boots"
    elif r.get("alive") and not r.get("fatal"):
        text = "boots" + ("" if r.get("screen_nonblank") else " (blank screen)")
    else:
        return "BOOT FAILED: " + str(r.get("fatal") or "not alive after the wait")[:90]
    exp = r.get("expect")
    if exp:
        text += f", \"{exp.get('text')}\" " + ("shown" if exp.get("found") else "NOT SHOWN")
    tap = r.get("tap")
    if tap:
        text += f", \"{tap.get('label')}\" pressed {tap.get('found')}: " + ("fine" if tap.get("alive") and not tap.get("fatal") else "CRASHED")
    return text


def status(dirs, m=None, extra=None):
    """-> (json-able dict, markdown). `extra`: {"artifacts": n, "runner_minutes": x, "smoke": {...}}"""
    m = m or load()
    extra = extra or {}
    results = read_results(dirs)
    smoke = read_smoke(dirs)
    starts = read_startchecks(dirs)
    rows = []
    total_files = total_bytes = total_shipped = 0
    unexplained = []
    for a in m["app"]:
        row = {"app": a["package"], "description": a["description"], "cells": {}}
        for col in ARTIFACT_COLUMNS:
            if col not in columns_of(a):
                continue
            known = a["known_failing"].get(col) or a["known_failing"].get(col.split("-")[0])
            r = results.get((a["package"], col))
            if r is None:
                cell = {"status": "missing", "reason": "no result file: the job did not run or did not report"}
            else:
                cell = {k: r.get(k) for k in ("status", "reason", "artifact", "sha256", "size", "shipped_size", "seconds",
                                               "engine_crates", "first_error")}
            cell["staged"] = staged_name(a, col)      # the file the build stages and the asset the release serves: a failed or
            cell["release"] = release_name(a, col)    # missing cell has no `artifact`, and the page's link pruning needs the names
            if known:
                cell["known"] = known
            boot = boot_text(smoke.get((a["package"], col)))
            if boot:
                cell["boot"] = boot
            start = starts.get((a["package"], col))
            if start:
                cell["start"] = start
            if cell["status"] in ("built", "reused"):
                total_files += 1
                total_bytes += cell.get("size") or 0
                total_shipped += cell.get("shipped_size") or 0
            elif not known and cell["status"] in ("failed", "missing"):
                unexplained.append(f"{a['package']}/{col}")
            row["cells"][col] = cell
        rows.append(row)
    boot_failed = sorted(f"{r['app']}/{c}" for r in rows for c, cell in r["cells"].items()
                         if str(cell.get("boot", "")).startswith("BOOT FAILED"))
    libs = read_libs(dirs)
    summary = {
        "libs": libs,
        "assets": total_files, "bytes": total_bytes, "shipped_bytes": total_shipped,
        "artifacts_of_the_run": extra.get("artifacts"), "runner_minutes": extra.get("runner_minutes"),
        "unexplained_failures": unexplained, "boot_failures": boot_failed, "smoke": extra.get("smoke", {}),
        "qa_builds": read_qa_builds(dirs),
    }
    data = {"schema": 1, "apps": rows, "summary": summary}
    return data, render_markdown(data)


def render_markdown(data):
    cols = [c for c in ARTIFACT_COLUMNS if any(c in r["cells"] for r in data["apps"])]
    head = "| app | " + " | ".join(cols) + " | engine crates | seconds | size (stripped / shipped) |"
    lines = ["### Demo apps", "", head, "|---|" + "---|" * (len(cols) + 3)]
    for r in data["apps"]:
        cells = []
        secs = 0
        engine = 0
        sizes = []
        for c in cols:
            cell = r["cells"].get(c)
            if cell is None:
                cells.append("-")
                continue
            st = CELL.get(cell["status"], "MISSING")
            if cell["status"] == "failed":
                st = "FAILED: " + (cell.get("first_error") or cell.get("reason") or "?")[:70].replace("|", "/")
            if cell.get("known") and cell["status"] in ("failed", "missing"):
                st = "known: " + str(cell["known"])[:60].replace("|", "/")
            if cell.get("boot") and cell["status"] in ("built", "reused"):
                st += ", " + cell["boot"]
            if cell.get("start") and cell["status"] in ("built", "reused"):
                st += ", starts" if str(cell["start"]).startswith("ok") else ", NO START: " + str(cell["start"])[:60]
            cells.append(st)
            secs += int(cell.get("seconds") or 0)
            engine += len(cell.get("engine_crates") or [])
            if cell.get("size"):
                sizes.append(f"{human(cell['size'])}/{human(cell.get('shipped_size'))}")
        lines.append(f"| {r['app']} | " + " | ".join(cells) + f" | {engine} | {secs} | {'; '.join(sizes[:2])}{' ...' if len(sizes) > 2 else ''} |")
    files = [(r["app"], c, cell) for r in data["apps"] for c, cell in r["cells"].items() if cell.get("artifact")]
    if files:
        lines += ["", "### Files (what the release page links)", "",
                  "| asset | platform | stripped | shipped | seconds | engine crates | sha256 |", "|---|---|---|---|---|---|---|"]
        for app, col, cell in files:
            lines.append(f"| {cell['artifact']} | {col} | {human(cell.get('size'))} | {human(cell.get('shipped_size'))} | "
                         f"{cell.get('seconds') or 0} | {len(cell.get('engine_crates') or [])} | `{str(cell.get('sha256') or '')[:12]}` |")
    s = data["summary"]
    lines += ["", f"**{s['assets']} files, {human(s['bytes'])} stripped, {human(s['shipped_bytes'])} as shipped**; "
                  f"artifacts of the run: {s['artifacts_of_the_run']}; runner minutes: {s['runner_minutes']}."]
    if s.get("qa_builds"):
        lines += ["", "### QA builds with the engine's debug server (artifacts of the run, never published)", "",
                  "| app | column | status | size | seconds |", "|---|---|---|---|---|"]
        for q in s["qa_builds"]:
            what = q["status"] if q["status"] == "built" else f"{q['status']}: " + str(q.get("first_error") or q.get("reason") or "?")[:70].replace("|", "/")
            lines.append(f"| {q.get('artifact') or q['app']} | {q['column']} | {what} | {human(q.get('size'))} | {q.get('seconds') or 0} |")
    if s["libs"]:
        lines += ["", "### The libazul every app of a target links (built once)", "",
                  "| target | features | seconds | size |", "|---|---|---|---|"]
        for l in sorted(s["libs"], key=lambda l: (l.get("kind", ""), l.get("target", ""), l.get("features", ""))):
            lines.append(f"| {l.get('target') or 'host'} ({l.get('kind')}) | {l.get('features')} | {l.get('seconds')} | {human(l.get('size'))} |")
    if s["smoke"]:
        lines += ["", "### Boot tests", ""] + [f"- {k}: {v}" for k, v in sorted(s["smoke"].items())]
    if s["boot_failures"]:
        lines += ["", "DO NOT BOOT: " + ", ".join(s["boot_failures"])]
    if s["unexplained_failures"]:
        lines += ["", "UNEXPLAINED: " + ", ".join(s["unexplained_failures"])]
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------------------------------------
def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, rest = argv[0], argv[1:]
    if cmd == "check":
        errors = check()
        for e in errors:
            print("demo_apps: " + e, file=sys.stderr)
        if not errors:
            m = load()
            print(f"demo_apps: ok - {len(m['app'])} apps, "
                  f"{sum(len(columns_of(a)) for a in m['app'])} built files")
        return 1 if errors else 0
    if cmd == "plan":
        out = plan(e2e="--e2e" in rest)
        for line in write_github_output(out):
            print(line)
        return 0
    if cmd == "assets":
        m = load()
        for a in m["app"]:
            if rest and a["package"] != rest[0]:
                continue
            for c in columns_of(a):
                print(f"{a['package']}\t{c}\t{staged_name(a, c)}\t{release_name(a, c)}")
        return 0
    if cmd == "weights":
        # demo_apps.py weights demo-status.json: the weight (1 light .. 3 heavy) the measured build seconds of each app
        # suggest, from the mean over its desktop builds, next to the weight in the manifest
        data = json.loads(pathlib.Path(rest[0]).read_text(encoding="utf-8"))
        mean = {}
        for row in data["apps"]:
            secs = [c["seconds"] for col, c in row["cells"].items() if col in DESKTOP and c.get("status") == "built" and c.get("seconds")]
            if secs:
                mean[row["app"]] = sum(secs) / len(secs)
        if not mean:
            print("no desktop build with seconds in " + rest[0])
            return 1
        ordered = sorted(mean.values())
        third = lambda v: 1 if v <= ordered[len(ordered) // 3] else (2 if v <= ordered[2 * len(ordered) // 3] else 3)
        have = {a["package"]: a["weight"] for a in load()["app"]}
        for app, v in sorted(mean.items(), key=lambda kv: -kv[1]):
            mark = "" if third(v) == have.get(app) else "   <- manifest says " + str(have.get(app))
            print(f"{app:20} {v:7.0f} s   suggested weight {third(v)}{mark}")
        return 0
    if cmd == "status":
        dirs = [d for d in rest if not d.startswith("--")]
        extra = {}
        for flag in (x for x in rest if x.startswith("--")):
            k, _, v = flag[2:].partition("=")
            extra[k] = json.loads(v) if v[:1] in "[{0123456789" else v
        data, md = status(dirs, extra=extra)
        pathlib.Path("demo-status.json").write_text(json.dumps(data, indent=2, sort_keys=True), encoding="utf-8")
        pathlib.Path("demo-status.md").write_text(md, encoding="utf-8")
        summary = os.environ.get("GITHUB_STEP_SUMMARY")
        if summary:
            with open(summary, "a", encoding="utf-8") as fh:
                fh.write(md)
        print(md)
        return 1 if data["summary"]["unexplained_failures"] else 0
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
