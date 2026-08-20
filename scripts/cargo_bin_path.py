#!/usr/bin/env python3
"""Print the executable cargo actually produced for a package.

`target/release/<package>` is a GUESS, and it is wrong whenever a crate
renames its binary. `examples/azul-writer` declares `[[bin]] name =
"azwriter"`, so `target/release/azul-writer` has never existed — the CI
staging loop looked for it, did not find it, took a branch that printed
"[reuse] ... published binary kept" and stayed green while the 0.2.0
release shipped ZERO azul-writer assets on all three desktop OSes.

cargo already answers this exactly: with `--message-format=json` every
`compiler-artifact` message for a bin target carries an `executable`
field holding the real path. This reads that stream and prints the path.

Usage:
    cargo build --release -p <pkg> --message-format=json-render-diagnostics \
        > build.json
    python3 scripts/cargo_bin_path.py build.json <pkg>

Exits 1 (printing nothing to stdout) when the stream contains no
executable for that package — which means the build produced no binary
and the caller must treat it as a failure, never as "nothing changed".
"""
import json
import sys


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: cargo_bin_path.py <cargo-json-log> <package-name>", file=sys.stderr)
        return 2
    log_path, package = sys.argv[1], sys.argv[2]

    found = []
    with open(log_path, "r", encoding="utf-8", errors="replace") as fh:
        for line in fh:
            line = line.strip()
            if not line.startswith("{"):
                continue
            try:
                msg = json.loads(line)
            except ValueError:
                continue
            if msg.get("reason") != "compiler-artifact":
                continue
            exe = msg.get("executable")
            if not exe:
                continue
            target = msg.get("target") or {}
            if "bin" not in (target.get("kind") or []):
                continue
            # package_id spellings cargo has used: "azul-writer 0.2.0 (path+file:///...)"
            # and the newer "path+file:///...#azul-writer@0.2.0". Match either
            # without matching a package that merely CONTAINS the name.
            pid = msg.get("package_id", "")
            if not (pid.startswith(package + " ") or ("#" + package + "@") in pid
                    or pid.endswith("#" + package)):
                continue
            found.append(exe)

    if not found:
        print(
            f"no bin artifact for package '{package}' in {log_path} — "
            f"the build produced no executable",
            file=sys.stderr,
        )
        return 1
    # A package may declare several bins; the last artifact cargo reported is
    # the one it finished with. Announce the ambiguity rather than hiding it.
    if len(set(found)) > 1:
        print(f"note: '{package}' produced {len(set(found))} bins: {found}", file=sys.stderr)
    print(found[-1])
    return 0


if __name__ == "__main__":
    sys.exit(main())
