#!/usr/bin/env python3
"""demo_publish.py: turn the files of a `demos` run into the assets of the GitHub release, and write companion.json.

    demo_publish.py assemble  --in <dir of downloaded demoapps-* artifacts> --out <dir> [--version 0.2.0] [--sha <git sha>]
    demo_publish.py companion --in <dir> --out companion.json [--version 0.2.0] [--sha <git sha>]

`assemble` writes into --out exactly the files the release page links (doc/src/dllgen/demo_apps.rs builds the
same names from scripts/demo_apps.toml): a desktop binary as <Name>-<os>[.exe].tar.gz (the macOS ones with the
unquarantine.sh and README the page's "developer cannot be verified" note refers to), a phone file as it is,
plus companion.json when AzCompanion's four files are there, and release-assets.json listing everything with its
sha256 and size. Nothing is uploaded here: the publish job of demos.yml runs `gh release upload` on --out, and
only when the run was asked to publish.

companion.json is what AzReview's installer reads before it downloads the phone app: {schema, app, version,
release, git, built, protocol_version, min_protocol_version, min_azreview_version, files: [{name, platform, sha256,
size}]}. It is UNSIGNED: the release signing key (scripts/sign-release.sh, the minisign chain the updater of
azul-layout verifies) lives on the owner's machine, not in CI, so signing it is the owner's step
(`scripts/sign-release.sh release --artifact companion.json ...`) and AzReview must treat an unsigned
manifest as unverified. The file says so itself ("signature": null).
"""
import argparse
import datetime
import hashlib
import io
import json
import pathlib
import re
import sys
import tarfile

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent
sys.path.insert(0, str(HERE))
import demo_apps  # noqa: E402

UNQUARANTINE = """#!/bin/bash
# Double-click me - I live next to {bin}. I clear the macOS
# download quarantine on that azul demo, then launch it.
cd "$(dirname "$0")" || exit 1
xattr -dr com.apple.quarantine . 2>/dev/null || true
chmod +x "./{bin}" 2>/dev/null || true
echo "Unquarantined ./{bin} - launching..."
exec "./{bin}"
"""

README_MACOS = """## How to run the demos on macOS ("developer cannot be verified" error)

macOS marks everything downloaded from the web as quarantined,
so to run the demo programs, you will need to strip the com.apple.quarantine attribute first with:

    chmod +x ./unquarantine.sh
    ./unquarantine.sh

"""

LIMITS = "examples/azul-review-link/src/limits.rs"


def sha256_of(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def staged_files(in_dir, manifest):
    """{staged name: (app, column, path)} for every file of the run that is a build of a manifest app."""
    by_name = {}
    for app in manifest["app"]:
        for col in demo_apps.columns_of(app):
            by_name[demo_apps.staged_name(app, col)] = (app, col)
    found = {}
    for path in sorted(pathlib.Path(in_dir).rglob("*")):
        if path.is_file() and path.name in by_name and "logs" not in path.parts:
            found[path.name] = (*by_name[path.name], path)
    return found


def add_bytes(tar, name, data, mode):
    info = tarfile.TarInfo(name)
    info.size = len(data)
    info.mode = mode
    info.mtime = 0
    tar.addfile(info, io.BytesIO(data))


def tar_desktop(app, col, src, dst):
    """<Name>-<os>[.exe] -> <same>.tar.gz; a macOS binary travels with the script that clears its quarantine."""
    name = src.name
    with tarfile.open(dst, "w:gz", format=tarfile.GNU_FORMAT, compresslevel=6) as tar:
        info = tar.gettarinfo(str(src), arcname=name)
        info.mode = 0o755
        info.mtime = 0
        info.uid = info.gid = 0
        info.uname = info.gname = ""
        with open(src, "rb") as fh:
            tar.addfile(info, fh)
        if col.startswith("macos"):
            add_bytes(tar, "unquarantine.sh", UNQUARANTINE.format(bin=name).encode(), 0o755)
            add_bytes(tar, "README-macOS.txt", README_MACOS.encode(), 0o644)


def protocol_constants():
    """PROTOCOL_VERSION, MIN_PROTOCOL_VERSION and MIN_AZREVIEW_VERSION of the phone link crate (None when absent)."""
    try:
        text = (ROOT / LIMITS).read_text(encoding="utf-8")
    except OSError:
        text = ""
    def grab(name, kind):
        m = re.search(rf"pub const {name}\s*:\s*{kind}\s*=\s*(\"[^\"]*\"|\d+)\s*;", text)
        if not m:
            return None
        v = m.group(1)
        return v.strip('"') if v.startswith('"') else int(v)
    return grab("PROTOCOL_VERSION", "u16"), grab("MIN_PROTOCOL_VERSION", "u16"), grab("MIN_AZREVIEW_VERSION", "&str")


def app_version(manifest, package):
    app = next(a for a in manifest["app"] if a["package"] == package)
    with open(ROOT / app["dir"] / "Cargo.toml", "rb") as fh:
        return demo_apps.tomllib.load(fh)["package"]["version"]


def companion_json(files, manifest, version, sha):
    """companion.json for the AzCompanion files among `files` ({name: (app, col, path)}), or None."""
    mine = {n: v for n, v in files.items() if v[0]["package"] == "AzCompanion"}
    if not mine:
        return None
    protocol, min_protocol, min_azreview = protocol_constants()
    return {
        "schema": 1,
        "app": "AzCompanion",
        "name": next(a["name"] for a in manifest["app"] if a["package"] == "AzCompanion"),
        "version": app_version(manifest, "AzCompanion"),
        "release": version,
        "git": sha,
        "built": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "protocol_version": protocol,
        "min_protocol_version": min_protocol,
        "min_azreview_version": min_azreview,
        "files": [
            {"name": n, "platform": col, "sha256": sha256_of(path), "size": path.stat().st_size}
            for n, (_, col, path) in sorted(mine.items())
        ],
        "signature": None,
        "signature_note": "unsigned: the release signing key is not in CI (scripts/sign-release.sh, on the owner's machine)",
    }


def assemble(in_dir, out_dir, version, sha):
    manifest = demo_apps.load()
    files = staged_files(in_dir, manifest)
    out = pathlib.Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    listing = []
    for name, (app, col, path) in sorted(files.items()):
        if col in demo_apps.DESKTOP:
            dst = out / demo_apps.release_name(app, col)
            tar_desktop(app, col, path, dst)
        else:
            dst = out / name
            dst.write_bytes(path.read_bytes())
        listing.append({"name": dst.name, "app": app["package"], "column": col,
                        "sha256": sha256_of(dst), "size": dst.stat().st_size})
    comp = companion_json(files, manifest, version, sha)
    if comp:
        dst = out / "companion.json"
        dst.write_text(json.dumps(comp, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        listing.append({"name": "companion.json", "app": "AzCompanion", "column": "manifest",
                        "sha256": sha256_of(dst), "size": dst.stat().st_size})
    (out / "release-assets.json").write_text(json.dumps(listing, indent=2) + "\n", encoding="utf-8")
    return listing


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("command", choices=("assemble", "companion"))
    ap.add_argument("--in", dest="in_dir", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--version", default=demo_apps.azul_version())
    ap.add_argument("--sha", default="")
    args = ap.parse_args(argv)
    if args.command == "assemble":
        listing = assemble(args.in_dir, args.out, args.version, args.sha)
        total = sum(x["size"] for x in listing)
        print(f"{len(listing)} release assets, {total} bytes, in {args.out}")
        for x in listing:
            print(f"  {x['size']:>12}  {x['name']}")
        return 0
    manifest = demo_apps.load()
    comp = companion_json(staged_files(args.in_dir, manifest), manifest, args.version, args.sha)
    if comp is None:
        print("demo_publish: no AzCompanion file in the run, no companion.json", file=sys.stderr)
        return 1
    pathlib.Path(args.out).write_text(json.dumps(comp, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"wrote {args.out}: {len(comp['files'])} files")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
