#!/usr/bin/env python3
"""wp_e2_matrix.py: scripts/ci/e2e_matrix.py with the host apps of the scenario files of WP-E2 (AzMaps, AzCode) filled in.

    python3 scripts/ci/wp_e2_matrix.py desktop --repo . --bin AzMaps=demo-out/apps/AzMaps-e2e-linux --out DIR [--only maps_phone]
    python3 scripts/ci/wp_e2_matrix.py android --apk AzMaps-e2e-android-x86_64.apk --out DIR --shots DIR/shots --only maps_
    python3 scripts/ci/wp_e2_matrix.py ios --app AzMaps-e2e-ios-sim.app.zip --udid <udid> --out DIR --shots DIR/shots --only maps_

e2e_matrix.py names the app that hosts a file by examples/azul-widgets/ or by a table of file stems (PHONE_HOST_BY_STEM); the files of
examples/azul-maps/e2e and examples/azul-code/e2e are the DOM of their own app. WP-E1 is making `host_for` derive the host from the
folder's Cargo.toml; when that is on master this wrapper is dropped (it changes nothing in the harness: it fills the table before the
matrix runs, in this process only).
"""
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import e2e_matrix as matrix  # noqa: E402


def package_of(app_dir):
    """The cargo package name (`AzMaps`) of examples/<app_dir>/Cargo.toml, or None."""
    manifest = Path(__file__).resolve().parents[2] / "examples" / app_dir / "Cargo.toml"
    try:
        text = manifest.read_text(encoding="utf-8")
    except OSError:
        return None
    found = re.search(r'^name\s*=\s*"([^"]+)"', text, re.M)
    return found.group(1) if found else None


def fill_hosts():
    repo = Path(__file__).resolve().parents[2]
    for app_dir in ("azul-maps", "azul-code"):
        package = package_of(app_dir)
        if not package:
            continue
        for scenario in sorted((repo / "examples" / app_dir / "e2e").glob("*.json")):
            matrix.PHONE_HOST_BY_STEM[scenario.stem] = package


if __name__ == "__main__":
    fill_hosts()
    sys.exit(matrix.main(sys.argv[1:]))
