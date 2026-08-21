#!/usr/bin/env python3
"""Fast checks for the bug CLASSES that shipped a broken 0.2.0 release.

Every one of these failures had the same shape: a name or a capability is
produced in one place and consumed in another, and nothing asserts the two
agree. None is visible to a compiler, and each cost a full CI run — or a whole
release — to discover:

  * `azul-writer` built a binary called `azwriter`; CI staged
    `target/release/<package>`, missed, and a `[reuse]` branch reported success.
    Three desktop downloads 404'd for an entire version.
  * api.json bound `MapWidget.dom` to azul-layout's PLACEHOLDER, so the map
    panned and never painted a tile on every desktop platform, while
    `map_widget_dom` — which wires the tile worker — had zero callers.
    `VideoWidget.dom` had the identical bug.
  * deploy_pages checks out with cone-mode off; a script it runs by path was
    not in the sparse list, so the deploy died on a missing file. The same trap
    silently disabled the registry mirrors for two months in 2026.

This runs in the preflight tier: checkout + python, no compilation, ~1 second.
The equivalent Rust tests (dll/tests/*_contract.rs) still exist and are more
precise; this exists so a doomed run dies in the first two minutes instead of
after an hour of building artifacts nobody can publish.

Exit 0 = all contracts hold. Exit 1 = at least one is broken, with the reason.
"""

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
FAILURES: list[str] = []


def fail(check: str, msg: str) -> None:
    FAILURES.append(f"[{check}] {msg}")


# --------------------------------------------------------------------------
# 1. Demo naming: package == [[bin]] name == release asset stem, all AzXxx.
# --------------------------------------------------------------------------
DEMO_DIRS = {
    "AzWidgets": "azul-widgets",
    "AzMaps": "azul-maps",
    "AzPaint": "azul-paint",
    "AzMeet": "azul-meet",
    "AzWriter": "azul-writer",
}


def check_demo_naming() -> None:
    for pkg, d in DEMO_DIRS.items():
        manifest = ROOT / "examples" / d / "Cargo.toml"
        if not manifest.is_file():
            fail("demo-naming", f"{pkg}: examples/{d}/Cargo.toml is missing")
            continue
        text = manifest.read_text()
        m = re.search(r'(?m)^name = "([^"]+)"', text)
        if not m or m.group(1) != pkg:
            fail(
                "demo-naming",
                f"examples/{d} declares package '{m.group(1) if m else '?'}', expected '{pkg}'",
            )
        # [[bin]] name, if declared, must equal the package
        in_bin = False
        for line in text.splitlines():
            t = line.strip()
            if t.startswith("["):
                in_bin = t == "[[bin]]"
                continue
            if in_bin and t.startswith("name"):
                got = t.split("=", 1)[1].strip().strip('"')
                if got != pkg:
                    fail(
                        "demo-naming",
                        f"{pkg} builds a binary named '{got}'. CI stages the binary "
                        f"cargo reports, but the release page links '<package>-<os>' — "
                        f"a mismatch is a 404 that no job reports as an error.",
                    )
                in_bin = False
        if not (pkg.startswith("Az") and len(pkg) > 2 and pkg[2].isupper()):
            fail("demo-naming", f"package '{pkg}' breaks the AzXxx convention")

    wf = (ROOT / ".github/workflows/rust.yml").read_text()
    expected = " ".join(DEMO_DIRS)
    if expected not in wf:
        fail(
            "demo-naming",
            f"the workflow's demo loop no longer reads '{expected}' — this checker "
            f"and the workflow disagree about what ships",
        )


# --------------------------------------------------------------------------
# 2. Widget wiring: a *_widget_dom in azul-dll must be reachable from api.json.
# --------------------------------------------------------------------------
def check_widget_wiring() -> None:
    api = (ROOT / "api.json").read_text()
    glue = list((ROOT / "dll/src/unified").rglob("*.rs")) + list(
        (ROOT / "dll/src/desktop/extra").rglob("*.rs")
    )
    for f in glue:
        for line in f.read_text(errors="ignore").splitlines():
            t = line.strip()
            if not t.startswith("pub fn "):
                continue
            name = re.match(r"pub fn ([A-Za-z0-9_]+)", t)
            if not name:
                continue
            name = name.group(1)
            if not name.endswith("_widget_dom"):
                continue
            if name not in api:
                fail(
                    "widget-wiring",
                    f"{name} exists to wire a background worker, and api.json never "
                    f"routes a widget's dom() through it. The widget ships bound to "
                    f"azul-layout's placeholder: it renders, then never receives data, "
                    f"silently, on every platform.",
                )

    # And the reverse: the two known widgets must not be bound to the placeholder.
    for widget, fn in (("MapWidget", "map_widget_dom"), ("VideoWidget", "video_widget_dom")):
        at = api.find(f'"{widget}"')
        if at == -1:
            continue
        blk = api[at : at + 20000]
        dom_at = blk.find('"dom": {')
        if dom_at == -1:
            continue
        body_at = blk.find('"fn_body"', dom_at)
        if body_at == -1:
            continue
        body = blk[body_at : body_at + 200]
        if fn not in body:
            fail(
                "widget-wiring",
                f"{widget}.dom is not routed through {fn} — that is the placeholder "
                f"binding that made the map pan without ever painting a tile.",
            )


# --------------------------------------------------------------------------
# 3. deploy_pages runs scripts by path; sparse checkout must include them.
# --------------------------------------------------------------------------
def check_sparse_checkout() -> None:
    wf = (ROOT / ".github/workflows/rust.yml").read_text()
    at = wf.find("sparse-checkout: |")
    if at == -1:
        fail("sparse-checkout", "deploy_pages declares no sparse-checkout block")
        return
    end = wf.find("sparse-checkout-cone-mode:", at)
    listed = {
        l.strip()
        for l in wf[at:end].splitlines()[1:]
        if l.strip() and not l.strip().startswith("#")
    }

    start = wf.find("\n  deploy_pages:")
    if start == -1:
        fail("sparse-checkout", "no deploy_pages job found")
        return
    body = wf[start + 1 :]
    nxt = re.search(r"\n  (?!-)(?!#)[A-Za-z0-9_]+:\n", body)
    region = body[: nxt.start()] if nxt else body

    invoked = set()
    for line in region.splitlines():
        t = line.strip()
        if t.startswith("#"):
            continue
        for pat in ("python3 scripts/", "bash scripts/", "sh scripts/", "./scripts/"):
            i = t.find(pat)
            if i != -1:
                name = re.match(r"[A-Za-z0-9_.\-]+", t[i + len(pat) :])
                if name:
                    invoked.add("scripts/" + name.group(0))

    if not invoked:
        fail("sparse-checkout", "found no scripts invoked in deploy_pages — matcher broken")
        return
    for s in sorted(invoked - listed):
        fail(
            "sparse-checkout",
            f"deploy_pages runs {s} by path but does not check it out. cone-mode is "
            f"off, so the file is simply ABSENT: the step dies with 'No such file or "
            f"directory', or — if guarded by || true — passes while doing nothing, "
            f"which is how the registry mirrors were dead for two months behind a "
            f"green deploy.",
        )


# --------------------------------------------------------------------------
# 4. api.json must stay parseable — a broken fn_body ships in 30+ bindings.
# --------------------------------------------------------------------------
def check_api_json_parses() -> None:
    try:
        json.load(open(ROOT / "api.json"))
    except Exception as e:  # noqa: BLE001 - report any parse failure verbatim
        fail("api-json", f"api.json does not parse: {e}")


def main() -> int:
    check_api_json_parses()
    check_demo_naming()
    check_widget_wiring()
    check_sparse_checkout()

    if FAILURES:
        print("preflight contracts FAILED:\n", file=sys.stderr)
        for f in FAILURES:
            print(f"  {f}\n", file=sys.stderr)
        return 1
    print("preflight contracts OK (naming, widget wiring, sparse checkout, api.json)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
