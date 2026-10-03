#!/usr/bin/env python3
"""ABI guard, end to end: a program built against one api.json refuses a
libazul generated from another, before it reads a single struct, with a
message that names both ABI hashes (generator: doc/src/codegen/v2/abi_guard.rs).

Tests (each prints PASS / FAIL; the exit code is 1 when any fails):

  c_aborts_before_main_on_another_abi
      main.c includes azul.h; a stub `AzAbi_getHash` (the libazul side)
      returns another hash -> SIGABRT before `main`, both hashes on stderr.
  c_runs_against_its_own_abi
      the same program with a stub returning the header's own hash -> runs.
  cpp_aborts_before_main_on_another_abi
      the same through the C++ header (azul17.hpp by default, C++17).
  rust_app_aborts_on_a_libazul_with_another_abi
      a PREBUILT app (link-dynamic, the Rust binding) started against a copy
      of the prebuilt libazul whose `AzAbi_getHash` was patched to return
      0xdead -> SIGABRT, both hashes on stderr. (The stale-app case of
      2026-09-30, reproduced without a build.)
  rust_app_starts_on_its_own_libazul
      control: the same app against the unpatched libazul is still running
      after --seconds (no mismatch message).

The C / C++ tests need only a C / C++ compiler and target/codegen (no
libazul, no cargo). The Rust tests need a prebuilt app and its libazul; on
macOS the patched copy is re-signed ad hoc (`codesign -f -s -`). Apps run
through scripts/waves/tools/run_capped.sh when it exists (memory cap, one
run at a time), one at a time.

Usage:
  scripts/abi_guard_e2e.py [--only c,cpp,rust] [--codegen DIR] [--lib FILE]
                           [--app FILE] [--cpp-header NAME] [--seconds N] [--keep]
Defaults are taken from the checkout this script lives in (target/codegen,
target/azul-lib/libazul.{dylib,so}, target/release/AzCalculator).
"""

import argparse
import os
import platform
import re
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]

# What the generated check prints (abi_guard.rs: rust_message_format /
# c_message_format, REBUILD_HINT).
MISMATCH = "azul: ABI mismatch"
REBUILD_HINT = "rebuild the app against this libazul"

# The hash the patched / stubbed libazul reports.
WRONG_HASH = 0xDEAD

SYMBOL = "AzAbi_getHash"


def lib_name():
    return "libazul.dylib" if sys.platform == "darwin" else "libazul.so"


def lib_path_var():
    return "DYLD_LIBRARY_PATH" if sys.platform == "darwin" else "LD_LIBRARY_PATH"


def hex16(h):
    return f"{h:016x}"


# ---------------------------------------------------------------------------
# Reading the hash the generated code carries
# ---------------------------------------------------------------------------

def header_hash(azul_h):
    """`#define AZ_ABI_HASH ((uint64_t)0x...ULL)` in azul.h."""
    m = re.search(r"#define AZ_ABI_HASH \(\(uint64_t\)0x([0-9a-fA-F]+)ULL\)", azul_h.read_text())
    if not m:
        raise SystemExit(f"{azul_h}: no AZ_ABI_HASH - generated before the ABI guard existed?")
    return int(m.group(1), 16)
