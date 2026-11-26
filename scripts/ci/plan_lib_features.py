#!/usr/bin/env python3
"""plan_lib_features.py <plan.txt> <key> <variant> <leg>: the cargo features of one lib of `scripts/demo_apps.py plan --e2e`.

    python3 scripts/ci/plan_lib_features.py demo-out/plan.txt libs_ios mobile-e2e ios-sim

prints the `features` string of the entry of the `libs_ios=` line whose variant and leg match (exit 1 and nothing printed when there
is none). Standard library only."""
import json
import sys


def main(argv):
    if len(argv) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    path, key, variant, leg = argv
    with open(path, "r", encoding="utf-8") as fh:
        for line in fh:
            name, _, value = line.rstrip("\n").partition("=")
            if name != key:
                continue
            for entry in json.loads(value).get("include", []):
                if entry.get("variant") == variant and entry.get("leg") == leg:
                    print(entry["features"])
                    return 0
    print(f"plan_lib_features: no {variant!r} lib for leg {leg!r} under {key!r} in {path}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
