#!/usr/bin/env python3
"""The gate of a demo app's build: the engine must NOT have been compiled.

Every demo app links the ONE prebuilt libazul of its target over the C ABI (azul-dll's `link-dynamic`:
the Rust facade, extern declarations only). The first mobile runs compiled azul-dll again for every app
and target, 6-11 minutes each; one manifest line (AzWidgets asking the facade for `e2e-scripting`, which
turns on `azul-layout/e2e-server`) did the same on the desktop. This reads cargo's own account of a
build, the JSON stream of `--message-format=json-render-diagnostics`, and fails when

  * azul-core, azul-layout or azul-css was compiled (the engine crates, whatever pulled them in), or
  * azul-dll itself was compiled with an engine feature (`link-static`, `cabi_internal`, `cabi_export`,
    `_internal_deps`, `dist`, `build-dll`, `engine`, `desktop`): anything beyond the facade.

`Compiling azul-dll` itself is expected: it is the Rust API crate, a few thousand lines of extern
declarations. It is judged by its features, not by its name.

    scripts/demo_gate.py <cargo-json-log> [--allow <crate> ...] [--json <summary.json>]

--allow names an engine crate an app may compile (a reason belongs next to it in scripts/demo_apps.toml).
Exit 0: clean; 1: the engine was compiled (the crates are printed); 2: no usable log.
"""
import json
import sys

ENGINE_CRATES = ("azul-core", "azul-layout", "azul-css")
ENGINE_FEATURES = (
    "link-static", "cabi_internal", "cabi_export", "_internal_deps", "dist", "build-dll", "engine", "desktop",
)


def package_name(package_id):
    """The package name of a cargo package id, in every spelling cargo has used:
         azul-core 0.0.21 (path+file:///x/core)          legacy
         path+file:///x/core#azul-core@0.0.21            named
         registry+https://...index#azul-core@0.0.21      named, from a registry
         path+file:///x/examples/azul-writer#0.1.0       no name: the directory is named after it
    """
    if " " in package_id and not package_id.startswith(("path+", "registry+", "git+", "sparse+")):
        return package_id.split(" ", 1)[0]
    source, _, fragment = package_id.partition("#")
    if "@" in fragment:
        return fragment.split("@", 1)[0]
    return source.rstrip("/").rsplit("/", 1)[-1]


def read_messages(path):
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        for line in fh:
            line = line.strip()
            if not line.startswith("{"):
                continue
            try:
                yield json.loads(line)
            except ValueError:
                continue


def check(path, allow=()):
    """-> {"compiled": n, "engine_crates": [...], "dll_features": [...], "dll_engine_features": [...]}"""
    compiled = set()
    engine = set()
    dll_features = set()
    saw_dll = False
    for msg in read_messages(path):
        if msg.get("reason") != "compiler-artifact":
            continue
        name = package_name(msg.get("package_id", ""))
        compiled.add(name)
        if name in ENGINE_CRATES and name not in allow:
            engine.add(name)
        if name == "azul-dll":
            saw_dll = True
            dll_features.update(msg.get("features") or [])
    dll_engine = sorted(f for f in dll_features if f in ENGINE_FEATURES)
    return {
        "compiled": len(compiled),
        "engine_crates": sorted(engine),
        "dll_features": sorted(dll_features),
        "dll_engine_features": dll_engine,
        "saw_dll": saw_dll,
    }


def main(argv):
    allow = []
    summary_path = None
    log = None
    args = list(argv)
    while args:
        a = args.pop(0)
        if a == "--allow" and args:
            allow.append(args.pop(0))
        elif a == "--json" and args:
            summary_path = args.pop(0)
        elif log is None:
            log = a
        else:
            print(__doc__, file=sys.stderr)
            return 2
    if log is None:
        print(__doc__, file=sys.stderr)
        return 2
    try:
        result = check(log, allow)
    except OSError as e:
        print(f"demo_gate: cannot read {log}: {e}", file=sys.stderr)
        return 2
    if result["compiled"] == 0:
        print(f"demo_gate: {log} holds no compiler-artifact message: nothing was built, nothing can be said", file=sys.stderr)
        return 2
    if summary_path:
        with open(summary_path, "w", encoding="utf-8") as fh:
            json.dump(result, fh, indent=2, sort_keys=True)
    bad = result["engine_crates"] or result["dll_engine_features"]
    if bad:
        print("demo_gate: the engine was compiled into this app (it must link the prebuilt libazul):", file=sys.stderr)
        for c in result["engine_crates"]:
            print(f"  compiled crate: {c}", file=sys.stderr)
        for f in result["dll_engine_features"]:
            print(f"  azul-dll built with the engine feature: {f}", file=sys.stderr)
        print(f"  azul-dll's features: {', '.join(result['dll_features']) or '(none)'}", file=sys.stderr)
        return 1
    print(f"demo_gate: ok - {result['compiled']} crates compiled, no engine crate "
          f"(azul-dll features: {', '.join(result['dll_features']) or 'none'})")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
