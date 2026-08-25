#!/usr/bin/env python3
"""
env_guard.py — which environment variables build-time code is allowed to read.

THE THREAT. Credential theft through the build is the most productive move an
attacker gets from a poisoned dependency, because CI hands the build a fully
populated environment and `cargo` passes that environment, unfiltered, to every
`build.rs` and every proc macro in the tree. `rustdecimal` (2022) branched on
`GITLAB_CI` before detonating. `mysten-metrics` 9.0.3 (2026) exfiltrated the
environment plus `~/.cargo/credentials` and SSH keys from a build script. The
code doing this does not need to be in a crate you chose — it needs to be in
anything, anywhere in the tree, at any depth.

WHAT THIS GATE DOES. It builds the set of variable names that exist in (or can
be injected into) azul's CI, classifies each one, then scans every build script
and proc-macro source in a `cargo vendor` tree for reads of them:

    env!("X")  option_env!("X")  env::var("X")  env::var_os("X")
    env::vars()  env::vars_os()                 <- whole-environment sweep
    getenv("X")  GetEnvironmentVariable         <- C sources compiled by cc

The interesting half of the name set does NOT come from `os.environ`: a secret
only appears in the environment of the job it is wired into, and a scan running
in a different job would never see it. So the names are also parsed straight out
of `.github/workflows/*.yml` — every `secrets.NAME` the repository can inject,
whether or not this job has it. A dependency reading `CARGO_REGISTRY_TOKEN`
fails here even on a run where that secret was never set.

IT MUST RUN BEFORE ANY BUILD. `cargo vendor` downloads sources without
executing anything; that is the whole reason this gate is possible. Once a
build has started, the build scripts have already run and the credentials have
already been read. This is a preflight, not a post-check.

Usage:
    env_guard.py --vendor vendor/ [--workflows .github/workflows]
                 [--scope compile-time|all] [--strict] [--json OUT]
    env_guard.py --emit-env-allowlist      # `env -i` prefix for a hardened build
    env_guard.py --print-classes           # the classification table

Exit 0 when no build-time code reads a forbidden variable; exit 1 otherwise.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import sc_common as sc  # noqa: E402

# ---------------------------------------------------------------------------
# CLASSIFICATION
# ---------------------------------------------------------------------------
#
# ALLOWED is cargo's documented build-script contract plus the toolchain and
# platform variables a native build legitimately needs. It is an ALLOWLIST: a
# name that is not matched here is not automatically fatal, but it is reported,
# because "why does this crate want that" is the question worth asking.

ALLOWED = [
    # cargo's build-script contract
    r'^CARGO$', r'^CARGO_(MANIFEST_DIR|MANIFEST_LINKS|MANIFEST_PATH|MAKEFLAGS)$',
    r'^CARGO_PKG_', r'^CARGO_FEATURE_', r'^CARGO_CFG_', r'^CARGO_BIN_', r'^CARGO_CRATE_NAME$',
    r'^CARGO_PRIMARY_PACKAGE$', r'^CARGO_ENCODED_RUSTFLAGS$', r'^CARGO_TARGET_',
    r'^OUT_DIR$', r'^TARGET$', r'^HOST$', r'^NUM_JOBS$', r'^OPT_LEVEL$',
    r'^DEBUG$', r'^PROFILE$', r'^DEP_', r'^RUSTC$', r'^RUSTC_WRAPPER$',
    r'^RUSTC_WORKSPACE_WRAPPER$', r'^RUSTC_LINKER$', r'^RUSTDOC$', r'^RUSTFLAGS$',
    r'^RUSTDOCFLAGS$', r'^CARGO_HOME$', r'^RUSTUP_HOME$', r'^RUSTUP_TOOLCHAIN$',
    # native toolchain
    r'^(CC|CXX|AR|AS|LD|NM|RANLIB|STRIP|OBJCOPY|OBJDUMP|WINDRES|DLLTOOL)(_.*)?$',
    r'^(CFLAGS|CXXFLAGS|LDFLAGS|CPPFLAGS|ASMFLAGS|ARFLAGS)(_.*)?$',
    r'^PKG_CONFIG', r'^SYSROOT$', r'^SDKROOT$', r'^MACOSX_DEPLOYMENT_TARGET$',
    r'^IPHONEOS_DEPLOYMENT_TARGET$', r'^DEVELOPER_DIR$', r'^XCODE',
    r'^ANDROID_(NDK|SDK|HOME|API)', r'^NDK_HOME$', r'^EMSDK', r'^EMCC',
    r'^LIBCLANG_PATH$', r'^CLANG_PATH$', r'^BINDGEN_', r'^LLVM_CONFIG',
    r'^VCPKG', r'^VCINSTALLDIR$', r'^VSINSTALLDIR$', r'^WINDOWSSDK', r'^WINSDK',
    r'^(LIB|INCLUDE|LIBPATH)$', r'^MSYSTEM$', r'^MSVC',
    # benign platform / locale / paths
    r'^PATH$', r'^HOME$', r'^USERPROFILE$', r'^TMPDIR$', r'^TEMP$', r'^TMP$',
    r'^LANG$', r'^LC_', r'^TZ$', r'^TERM$', r'^SHELL$', r'^PWD$', r'^OSTYPE$',
    r'^SYSTEMROOT$', r'^WINDIR$', r'^PROGRAMFILES', r'^PROCESSOR_',
    r'^NUMBER_OF_PROCESSORS$', r'^COMSPEC$', r'^PATHEXT$', r'^HOSTTYPE$',
    # reproducible-builds / docs.rs conventions
    r'^SOURCE_DATE_EPOCH$', r'^DOCS_RS$',
]

# CI_DETECT — reading these is how a payload decides it is on a build server
# worth robbing rather than a developer laptop. rustdecimal keyed on GITLAB_CI.
# There is no legitimate reason for a *library's* build script to care.
CI_DETECT = [
    r'^CI$', r'^CONTINUOUS_INTEGRATION$', r'^BUILD_(ID|NUMBER|URL)$',
    r'^GITHUB_(ACTIONS|WORKFLOW|RUN_ID|RUN_NUMBER|JOB|ACTOR|REPOSITORY|EVENT_NAME|SHA|REF)$',
    r'^GITLAB_CI$', r'^TRAVIS', r'^CIRCLECI$', r'^JENKINS', r'^HUDSON',
    r'^BUILDKITE', r'^TF_BUILD$', r'^TEAMCITY_', r'^APPVEYOR', r'^DRONE',
    r'^SEMAPHORE', r'^CODEBUILD_', r'^BITBUCKET_', r'^WOODPECKER_',
]

# FORBIDDEN — a read of any of these from build-time code fails the build,
# unconditionally. There is no configuration in which a dependency's build
# script has business reading a credential.
FORBIDDEN = [
    r'TOKEN', r'SECRET', r'PASSWORD', r'PASSWD', r'CREDENTIAL', r'API_?KEY',
    r'PRIVATE_?KEY', r'ACCESS_?KEY', r'AUTH', r'SESSION', r'COOKIE', r'BEARER',
    r'^AWS_', r'^AZURE_', r'^GCP_', r'^GOOGLE_APPLICATION_CREDENTIALS$',
    r'^SSH_', r'^GPG_', r'^GNUPG', r'^NPM_', r'^PYPI_', r'^TWINE_',
    r'^DOCKER_(AUTH|PASSWORD)', r'^KUBECONFIG$', r'^VAULT_',
    r'^ACTIONS_RUNTIME_TOKEN$', r'^ACTIONS_ID_TOKEN_REQUEST_',
    r'^CARGO_REGISTRY_TOKEN$', r'^CARGO_REGISTRIES_.*_TOKEN$',
    r'^GH_TOKEN$', r'^GITHUB_TOKEN$',
    # azul's own signing/publishing material, by name
    r'^APPLE_(CERT|ID|TEAM_ID|APP_PASSWORD)', r'^OSSRH_', r'^MAVEN_GPG_',
    r'^AUR_', r'^RUBYGEMS_', r'^NUGET_', r'^LUAROCKS_', r'^AZUL_APT_GPG',
]

_ALLOWED_RE = re.compile("|".join(ALLOWED))
_CI_RE = re.compile("|".join(CI_DETECT))
_FORBIDDEN_RE = re.compile("|".join(FORBIDDEN), re.IGNORECASE)


def classify(name: str) -> str:
    """forbidden > ci-detect > allowed > unknown. Order matters: a name can
    match several patterns and the most dangerous reading must win —
    GITHUB_TOKEN is both a GITHUB_* name and a credential."""
    if _FORBIDDEN_RE.search(name):
        return "forbidden"
    if _CI_RE.match(name):
        return "ci-detect"
    if _ALLOWED_RE.match(name):
        return "allowed"
    return "unknown"


# ---------------------------------------------------------------------------
# Where the names come from
# ---------------------------------------------------------------------------

def workflow_secret_names(workflow_dir: Path) -> set[str]:
    """Every `secrets.NAME` any workflow can inject.

    This is the half of the name set `os.environ` cannot provide. A secret is
    only in the environment of the job it is wired into, so a preflight job —
    which deliberately has no secrets — would otherwise scan against an empty
    list and pass everything.
    """
    names: set[str] = set()
    if not workflow_dir.is_dir():
        return names
    for wf in sorted(workflow_dir.rglob("*.y*ml")):
        text = wf.read_text(encoding="utf-8", errors="replace")
        # Every `secrets.NAME` reference, plus the env-var NAME each one is
        # bound to — `CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}`
        # contributes both, and they are not always spelled the same.
        #
        # The binding pattern REQUIRES `secrets.` inside the expression. It used
        # to match any `NAME: ${{ ... }}` line, which swept up
        # `LD_LIBRARY_PATH: ${{ github.workspace }}/target/debug` and made
        # clang-sys — which legitimately reads LD_LIBRARY_PATH to find libclang —
        # a hard build failure. An over-broad secret list does not fail safe: it
        # trains people to add exemptions to a security gate.
        names |= set(re.findall(r'secrets\.([A-Za-z_][A-Za-z0-9_]*)', text))
        names |= set(re.findall(
            r'(?m)^\s{2,}([A-Za-z_][A-Za-z0-9_]*)\s*:\s*\$\{\{[^}]*\bsecrets\.', text))
    names.discard("GITHUB_TOKEN")   # re-added below; it is always available
    names.add("GITHUB_TOKEN")
    return names


# ---------------------------------------------------------------------------
# Reading the sources
# ---------------------------------------------------------------------------

READ_PATTERNS = [
    ("env!", re.compile(r'\benv!\s*\(\s*"([A-Za-z_][A-Za-z0-9_]*)"')),
    ("option_env!", re.compile(r'\boption_env!\s*\(\s*"([A-Za-z_][A-Za-z0-9_]*)"')),
    ("env::var", re.compile(r'\benv::var(?:_os)?\s*\(\s*"([A-Za-z_][A-Za-z0-9_]*)"')),
    ("var()", re.compile(r'(?<![:\w])var(?:_os)?\s*\(\s*"([A-Za-z_][A-Za-z0-9_]*)"')),
    ("getenv", re.compile(r'\bgetenv\s*\(\s*"([A-Za-z_][A-Za-z0-9_]*)"')),
    ("GetEnvironmentVariable", re.compile(
        r'\bGetEnvironmentVariable[AW]?\s*\(\s*"?([A-Za-z_][A-Za-z0-9_]*)"?')),
]
ENUMERATE_RE = re.compile(r'\b(?:std::)?env::vars(?:_os)?\s*\(|\benviron\b')
DYNAMIC_RE = re.compile(r'\benv::var(?:_os)?\s*\(\s*(?!")')

SCANNABLE = {".rs", ".c", ".h", ".cc", ".cpp", ".cxx", ".hpp", ".m", ".mm"}


def strip_comments(text: str) -> str:
    """Comments and `#[test]`/`#[cfg(test)]` items are not build-time code."""
    return sc.normalise_source(text)


def scan_crate(crate: sc.VendoredCrate, scope: str,
               build_deps: set[str] | None = None) -> dict | None:
    """Every environment read in this crate's build-time code.

    `build_deps` is the transitive `[build-dependencies]` closure. Those crates
    are ordinary libraries — no build script, not proc macros — but they are
    linked INTO build scripts and run with the same authority. Scoping to
    "build scripts and proc macros" alone reports `turso_core` as reading
    nothing unusual while `built`, its build dependency, calls `env::vars_os()`
    over the entire environment on its behalf.
    """
    build_deps = build_deps or set()
    paths = sc.build_script_paths(crate)
    kind = "build-script"
    if not paths and sc.manifest_facts(crate)["proc_macro"]:
        paths, kind = sc.proc_macro_paths(crate), "proc-macro"
    if not paths and crate.name in build_deps:
        paths, kind = sc.proc_macro_paths(crate), "build-dependency"
    if scope == "all" and not paths:
        src = crate.path / "src"
        paths, kind = (sorted(src.rglob("*.rs")) if src.is_dir() else []), "library"
    if not paths:
        return None

    reads: dict[str, set[str]] = {}
    enumerates: list[str] = []
    dynamic: list[str] = []
    for p in paths:
        if p.suffix not in SCANNABLE:
            continue
        try:
            text = strip_comments(p.read_text(encoding="utf-8", errors="replace"))
        except OSError:
            continue
        rel = str(p.relative_to(crate.path)).replace(os.sep, "/")
        for label, pat in READ_PATTERNS:
            for name in pat.findall(text):
                reads.setdefault(name, set()).add(f"{rel} ({label})")
        if ENUMERATE_RE.search(text):
            enumerates.append(rel)
        if DYNAMIC_RE.search(text):
            dynamic.append(rel)
    if not reads and not enumerates and not dynamic:
        return None
    return {
        "name": crate.name, "version": crate.version, "kind": kind,
        "reads": {k: sorted(v) for k, v in sorted(reads.items())},
        "enumerates": sorted(set(enumerates)),
        "dynamic": sorted(set(dynamic)),
    }


# ---------------------------------------------------------------------------

def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--vendor", type=Path, default=Path("vendor"))
    ap.add_argument("--workflows", type=Path, default=Path(".github/workflows"))
    ap.add_argument("--scope", choices=("compile-time", "all"), default="compile-time",
                    help="compile-time = build scripts + proc macros (default)")
    ap.add_argument("--strict", action="store_true",
                    help="also fail on CI-provider detection and whole-env enumeration")
    ap.add_argument("--allow-enumerate", action="append", default=[], metavar="CRATE",
                    help="crate permitted to call env::vars() (repeatable)")
    ap.add_argument("--json", type=Path)
    ap.add_argument("--report-only", action="store_true")
    ap.add_argument("--emit-env-allowlist", action="store_true",
                    help="print an `env -i ...` prefix that passes only allowed vars")
    ap.add_argument("--print-classes", action="store_true")
    ap.add_argument("--metadata", type=Path,
                    help="cargo metadata JSON; without it the build-dependency "
                         "closure is computed by invoking cargo")
    ap.add_argument("--no-build-deps", action="store_true")
    args = ap.parse_args()

    if args.print_classes:
        for label, pats in (("ALLOWED", ALLOWED), ("CI_DETECT", CI_DETECT),
                            ("FORBIDDEN", FORBIDDEN)):
            print(f"=== {label} ({len(pats)} patterns) ===")
            for p in pats:
                print(f"  {p}")
            print()
        return 0

    if args.emit_env_allowlist:
        keep = [f'{k}={v}' for k, v in sorted(os.environ.items())
                if classify(k) == "allowed"]
        print("env -i \\\n  " + " \\\n  ".join(
            f"'{kv}'" for kv in keep) + " \\\n  cargo build --locked")
        return 0

    secret_names = workflow_secret_names(args.workflows)
    live_names = set(os.environ)

    build_deps: set[str] = set()
    if not args.no_build_deps:
        build_deps = sc.build_dependency_closure(sc.load_metadata(args.metadata))

    findings = []
    for crate in sc.walk_vendor(args.vendor):
        f = scan_crate(crate, args.scope, build_deps)
        if f:
            findings.append(f)

    violations, notices = [], []
    for f in findings:
        for name, sites in f["reads"].items():
            cls = classify(name)
            # A name literally wired into this repository's workflows is
            # forbidden regardless of shape: AUR_USERNAME is not "secret-shaped"
            # but it is still ours and no dependency should be reading it.
            if name in secret_names:
                cls = "forbidden"
            rec = {**{k: f[k] for k in ("name", "version", "kind")},
                   "var": name, "class": cls, "sites": sites,
                   "live_in_this_env": name in live_names}
            if cls == "forbidden":
                violations.append(rec)
            elif cls in ("ci-detect", "unknown"):
                (violations if (args.strict and cls == "ci-detect") else notices).append(rec)
        if f["enumerates"]:
            rec = {**{k: f[k] for k in ("name", "version", "kind")},
                   "var": "*", "class": "enumerate", "sites": f["enumerates"],
                   "live_in_this_env": True}
            if f["name"] in args.allow_enumerate:
                notices.append({**rec, "class": "enumerate (allowlisted)"})
            elif args.strict:
                violations.append(rec)
            else:
                notices.append(rec)

    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(json.dumps(
            {"findings": findings, "violations": violations, "notices": notices,
             "secret_names": sorted(secret_names)}, indent=2), encoding="utf-8")

    # ---- report ----
    md = ["## 🔑 Environment access from build-time code", "",
          f"Scanned `{len(findings)}` crates that read the environment at build time, "
          f"against `{len(secret_names)}` secret names this repository's workflows can "
          f"inject and `{len(live_names)}` variables live in this job.", ""]
    if violations:
        md += ["### ❌ Forbidden reads", "", "| crate | kind | variable | class | where |",
               "|---|---|---|---|---|"]
        for v in violations:
            md.append(f'| `{v["name"]} {v["version"]}` | {v["kind"]} | `{v["var"]}` '
                      f'| {v["class"]} | {", ".join(v["sites"][:3])} |')
        md.append("")
    if notices:
        md += [f"<details><summary><b>Notices ({len(notices)}) — "
               f"non-standard or CI-shaped names</b></summary>", "",
               "| crate | variable | class |", "|---|---|---|"]
        for n_ in notices:
            md.append(f'| `{n_["name"]}` | `{n_["var"]}` | {n_["class"]} |')
        md += ["", "</details>", ""]
    sc.summary("\n".join(md))

    print(f"crates reading the environment at build time: {len(findings)}")
    print(f"  forbidden reads : {len(violations)}")
    print(f"  notices         : {len(notices)}")
    for v in violations:
        sc.annotate("error", f'{v["name"]} {v["version"]} ({v["kind"]}) reads '
                             f'{v["var"]} [{v["class"]}] at {v["sites"][0]}')
    if not violations:
        print("no build-time code reads a credential-shaped or repository-secret variable")
    return 0 if (not violations or args.report_only) else 1


if __name__ == "__main__":
    sys.exit(main())
