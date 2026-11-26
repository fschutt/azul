#!/usr/bin/env python3
"""Mutation check of the link crate: a change to the source that no test notices is a test that is missing.

    python3 -I examples/azul-review-link/scripts/mutate.py <table> [--only N[,N...]] [--timeout SECONDS]

<table> is `client` (the phone client, the store and the discovery list: tests need no engine) or `iroh` (the transport adapter: its
tests run over the real iroh engine, so libazul and the generated bindings must be there, see gate_mac.sh in examples/azul-companion).

For each row of the table (mutations_<table>.py: file, old text, new text, what it does) the old text is replaced once, the table's
tests run, and the file is put back (also when the run is interrupted). The unmutated tests run first: if they fail, nothing is
said about the mutations. A row whose old text is gone is a stale table, a mutation that does not build is a broken row; both fail
the run, like a mutation that no test noticed. A row whose words start with "(no change" is a control, a change that must not matter:
its tests must pass. Exit status 0 only when every mutation was caught."""

import importlib.util
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent


def load(name):
    path = HERE / f"mutations_{name}.py"
    if not path.is_file():
        sys.exit(f"no table {name!r}: {path} does not exist")
    spec = importlib.util.spec_from_file_location(f"mutations_{name}", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def run_tests(table, timeout):
    """(verdict, text, failed test names): verdict is ok, failed, none (no test result line) or timeout."""
    env = dict(os.environ)
    env.update(table.ENV)
    try:
        done = subprocess.run(table.TEST, cwd=ROOT, env=env, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired as late:
        text = (late.stdout or b"").decode(errors="replace") if isinstance(late.stdout, bytes) else (late.stdout or "")
        return "timeout", text, []
    text = done.stdout + done.stderr
    results = re.findall(r"test result: (\w+)\. (\d+) passed; (\d+) failed", text)
    names = re.findall(r"^test (\S+) \.\.\. FAILED", text, re.M)
    if not results:
        return "none", text, names
    return ("failed" if any(r[0] == "FAILED" for r in results) else "ok"), text, names


def main(argv):
    if not argv or argv[0].startswith("-"):
        sys.exit(__doc__)
    table = load(argv[0])
    only = None
    timeout = 1200
    rest = iter(argv[1:])
    for arg in rest:
        if arg == "--only":
            only = {int(n) for n in next(rest, "").split(",") if n}
        elif arg == "--timeout":
            timeout = int(next(rest, "1200"))
        else:
            sys.exit(f"unknown option {arg!r}\n{__doc__}")

    print(f"the unmutated tests: {' '.join(table.TEST)}", flush=True)
    verdict, text, names = run_tests(table, timeout)
    if verdict != "ok":
        print(text[-3000:])
        sys.exit(f"the tests of the unmutated source do not pass ({verdict}): nothing can be said about the mutations")
    print("  ok", flush=True)

    caught = escaped = stale = broken = 0
    for index, (file, old, new, what) in enumerate(table.M):
        if only is not None and index not in only:
            continue
        path = ROOT / file
        original = path.read_text()
        if old not in original:
            stale += 1
            print(f"{index:3d} STALE (the old text is gone): {what}", flush=True)
            continue
        path.write_text(original.replace(old, new, 1))
        try:
            verdict, text, names = run_tests(table, timeout)
        finally:
            path.write_text(original)
        if what.startswith("(no change"):
            # a control: a change that must not matter (the tests pass), so that a harness that fails every run cannot look like a good one
            if verdict == "ok":
                print(f"{index:3d} control  {what}: the tests pass, as they must", flush=True)
            else:
                broken += 1
                print(f"{index:3d} BROKEN   {what}: the tests of a change that changes nothing said {verdict}", flush=True)
        elif verdict == "failed":
            caught += 1
            print(f"{index:3d} caught   {what}: {len(names)} failing, e.g. {names[0].split('::')[-1][:70] if names else '-'}", flush=True)
        elif verdict == "timeout":
            caught += 1
            print(f"{index:3d} caught   {what}: the tests hang (killed after {timeout} s)", flush=True)
        elif verdict == "ok":
            escaped += 1
            print(f"{index:3d} ESCAPED  {what}", flush=True)
        else:
            broken += 1
            errors = [line for line in text.splitlines() if line.startswith("error")][:2]
            print(f"{index:3d} BROKEN   {what}: no test ran: {' | '.join(errors)}", flush=True)
    print(f"caught {caught}, escaped {escaped}, stale {stale}, broken {broken}")
    return 0 if escaped == stale == broken == 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
