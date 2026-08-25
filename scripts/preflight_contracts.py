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
def _strip_rust_comments(src: str) -> str:
    """Drop // and /* */ so a check never matches prose about the bug it hunts."""
    src = re.sub(r"/\*.*?\*/", "", src, flags=re.S)
    return re.sub(r"//[^\n]*", "", src)


# Widget files whose runtime display/opacity override is NOT yet known to be
# cleared. This list is DEBT, not a blessing: the check exists to stop the shape
# from spreading, and every entry is a candidate instance of the bug below.
# Shrink it, never grow it.
OVERRIDE_LATCH_DEBT = {
    "alert.rs",
    "check_box.rs",
    "chip.rs",
    "modal.rs",
    "popover.rs",
    "radio_group.rs",
    "toast.rs",
    "tooltip.rs",
}


def check_widget_override_latch() -> None:
    """A widget that overrides display/opacity must be able to UNDO it.

    `info.set_css_property` records a USER OVERRIDE. `migrate_user_overrides_from`
    copies overrides onto every rebuild and they OUTRANK the cascade — forever.
    So a widget that hides something imperatively, while its own `dom()` also
    derives that property from state, permanently wins over `dom()`:

      * the Accordion latched a clicked-open section open for the life of the
        window, ignoring the state flag entirely;
      * `set_placeholder_visible` hid a TextInput's placeholder on the first
        keystroke and it never came back, even over an emptied field.

    Both were the same defect, found weeks apart, and the second was found only
    because a user reported a blank field. The remedy in both cases was
    `CssProperty::initial(...)` — clear the override when the callback asks for a
    rebuild (the fresh cascade is authoritative), keep the concrete value when it
    does not (there is no rebuild coming, so the override IS the mechanism).

    Checked syntactically: a file that overrides display/opacity must contain at
    least one `CssProperty::initial(`. That does not prove the clear is on the
    right path — only a test can — but it does prove the author knew the
    override has to be undone.
    """
    wdir = ROOT / "layout" / "src" / "widgets"
    if not wdir.is_dir():
        fail("override-latch", f"missing widget directory {wdir}")
        return
    for f in sorted(wdir.glob("*.rs")):
        src = _strip_rust_comments(f.read_text(encoding="utf-8", errors="replace"))
        # `set_css_property(node, CssProperty::const_display(..))`, possibly wrapped.
        overrides = re.findall(
            r"set_css_property\s*\([^;]{0,200}?(const_display|const_opacity|Display\s*\(|Opacity\s*\()",
            src,
            flags=re.S,
        )
        if not overrides:
            continue
        if "CssProperty::initial(" in src:
            if f.name in OVERRIDE_LATCH_DEBT:
                fail(
                    "override-latch",
                    f"{f.name} now clears its overrides — remove it from "
                    f"OVERRIDE_LATCH_DEBT so the check keeps holding it to that.",
                )
            continue
        if f.name in OVERRIDE_LATCH_DEBT:
            continue
        fail(
            "override-latch",
            f"{f.name} overrides display/opacity via set_css_property but never "
            f"calls CssProperty::initial(...) to clear it. A user override "
            f"outranks the cascade on EVERY later rebuild, so whatever dom() "
            f"derives for that property is dead from here on (the Accordion "
            f"latch, and the TextInput placeholder that never came back). "
            f"Clear it when the callback returns RefreshDom; keep the concrete "
            f"value when it does not.",
        )


# Widgets in the reference demo that own USER-EDITABLE state. Each must have its
# value fed back into `Showcase`, or an unrelated `RefreshDom` silently discards
# whatever the user did.
DEMO_STATEFUL_WIDGETS = (
    "TextInput",
    "NumberInput",
    "TextArea",
    "ColorInput",
    "Slider",
    "Switch",
    "CheckBox",
    "RadioGroup",
    "Segmented",
    "DropDown",
    "DatePicker",
    "TimePicker",
    "Stepper",
    "Pagination",
)


def check_demo_state_round_trip() -> None:
    """Every stateful widget in the demo must round-trip its value to the host.

    Widgets are rebuilt from host state on EVERY layout. A widget constructed
    from a literal with no `on_*` hook therefore snaps back to that literal the
    moment anything returns `RefreshDom` — and since almost every callback in
    the demo bumps a counter and returns one, "anything" means "another widget".

    This shape has now been found six times: DropDown had no setter at all, the
    DatePicker/TimePicker/ComboBox looked completely dead, the NumberInput
    merged typed text into a stale value ("3342"), and the TextInput silently
    lost everything typed into it while its placeholder stayed hidden. The demo
    is the reference every app is copied from, so it is the right place to pin
    the contract.
    """
    demo = ROOT / "examples" / "azul-widgets" / "src" / "lib.rs"
    if not demo.is_file():
        fail("demo-round-trip", f"missing demo source {demo}")
        return
    src = _strip_rust_comments(demo.read_text(encoding="utf-8", errors="replace"))
    for m in re.finditer(r"\b([A-Z][A-Za-z]*)::create\s*\(", src):
        name = m.group(1)
        if name not in DEMO_STATEFUL_WIDGETS:
            continue
        tail = src[m.end(): m.end() + 900]
        stop = tail.find(".dom()")
        span = tail if stop == -1 else tail[:stop]
        if not re.search(r"\.with_on_\w+", span):
            line = src[: m.start()].count("\n") + 1
            fail(
                "demo-round-trip",
                f"{name}::create(...) near line {line} of the AzWidgets demo has "
                f"no .with_on_* hook, so its value is never stored in Showcase. "
                f"The widget is rebuilt from host state every layout, so the "
                f"next RefreshDom from ANY other callback throws away whatever "
                f"the user typed or picked.",
            )


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
    check_widget_override_latch()
    check_demo_state_round_trip()
    check_sparse_checkout()

    if FAILURES:
        print("preflight contracts FAILED:\n", file=sys.stderr)
        for f in FAILURES:
            print(f"  {f}\n", file=sys.stderr)
        return 1
    print(
        "preflight contracts OK (naming, widget wiring, override latch, "
        "demo round-trip, sparse checkout, api.json)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
