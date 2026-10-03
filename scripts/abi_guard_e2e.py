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


# ---------------------------------------------------------------------------
# Finding AzAbi_getHash in a libazul binary (Mach-O thin / fat, ELF64 LE)
# ---------------------------------------------------------------------------

ARM64, X86_64 = "arm64", "x86_64"
MACHO_CPU = {0x0100000C: ARM64, 0x01000007: X86_64}
ELF_MACHINE = {183: ARM64, 62: X86_64}


def host_arch():
    m = platform.machine().lower()
    return ARM64 if m in ("arm64", "aarch64") else X86_64


def find_function(data):
    """(file offset of `AzAbi_getHash`'s first instruction, arch)."""
    magic_be = struct.unpack_from(">I", data, 0)[0]
    if magic_be in (0xCAFEBABE, 0xCAFEBABF):  # fat Mach-O: pick the host's slice
        fat64 = magic_be == 0xCAFEBABF
        (n,) = struct.unpack_from(">I", data, 4)
        pos = 8
        for _ in range(n):
            if fat64:
                cpu, _sub, off, _size, _align, _res = struct.unpack_from(">iiQQII", data, pos)
                pos += 32
            else:
                cpu, _sub, off, _size, _align = struct.unpack_from(">iiIII", data, pos)
                pos += 20
            if MACHO_CPU.get(cpu & 0xFFFFFFFF) == host_arch():
                return find_in_macho(data, off)
        raise SystemExit("fat libazul without a slice for this machine")
    if struct.unpack_from("<I", data, 0)[0] == 0xFEEDFACF:
        return find_in_macho(data, 0)
    if data[:4] == b"\x7fELF":
        return find_in_elf(data)
    raise SystemExit("libazul is neither Mach-O 64 nor ELF")


def find_in_macho(data, base):
    _magic, cpu, _sub, _ftype, ncmds, _size, _flags, _res = struct.unpack_from("<IiiIIIII", data, base)
    arch = MACHO_CPU.get(cpu & 0xFFFFFFFF)
    if arch is None:
        raise SystemExit(f"Mach-O cpu type {cpu:#x} is not arm64 / x86_64")
    segments, symtab = [], None
    pos = base + 32
    for _ in range(ncmds):
        cmd, cmdsize = struct.unpack_from("<II", data, pos)
        if cmd == 0x19:  # LC_SEGMENT_64
            vmaddr, vmsize, fileoff, filesize = struct.unpack_from("<QQQQ", data, pos + 24)
            segments.append((vmaddr, vmsize, fileoff, filesize))
        elif cmd == 0x2:  # LC_SYMTAB
            symtab = struct.unpack_from("<IIII", data, pos + 8)
        pos += cmdsize
    if symtab is None:
        raise SystemExit("Mach-O without LC_SYMTAB")
    symoff, nsyms, stroff, _strsize = symtab
    want = ("_" + SYMBOL).encode()
    for i in range(nsyms):
        strx, ntype, _sect, _desc, value = struct.unpack_from("<IBBHQ", data, base + symoff + 16 * i)
        if ntype & 0x0E != 0x0E:  # N_SECT: defined here
            continue
        start = base + stroff + strx
        if data[start:start + len(want) + 1] != want + b"\0":
            continue
        for vmaddr, vmsize, fileoff, filesize in segments:
            if vmaddr <= value < vmaddr + min(vmsize, filesize):
                return base + fileoff + (value - vmaddr), arch
        raise SystemExit(f"_{SYMBOL} at {value:#x} is in no file-backed segment")
    raise SystemExit(f"libazul does not export _{SYMBOL} - built before the ABI guard existed?")


def find_in_elf(data):
    if data[4] != 2 or data[5] != 1:
        raise SystemExit("only 64-bit little-endian ELF is supported")
    (machine,) = struct.unpack_from("<H", data, 18)
    arch = ELF_MACHINE.get(machine)
    if arch is None:
        raise SystemExit(f"ELF machine {machine} is not arm64 / x86_64")
    phoff, shoff = struct.unpack_from("<QQ", data, 32)
    phentsize, phnum, shentsize, shnum = struct.unpack_from("<HHHH", data, 54)
    sections = [struct.unpack_from("<IIQQQQIIQQ", data, shoff + i * shentsize) for i in range(shnum)]
    value = None
    for sh in sections:
        _name, sh_type, _flags, _addr, off, size, link, _info, _align, entsize = sh
        if sh_type not in (2, 11) or not entsize:  # SHT_SYMTAB, SHT_DYNSYM
            continue
        str_off = sections[link][4]
        for j in range(size // entsize):
            st_name, st_info, _other, shndx, st_value, _size = struct.unpack_from("<IBBHQQ", data, off + j * entsize)
            if shndx == 0 or st_info & 0xF != 2:  # defined STT_FUNC
                continue
            end = data.index(b"\0", str_off + st_name)
            if data[str_off + st_name:end] == SYMBOL.encode():
                value = st_value
                break
        if value is not None:
            break
    if value is None:
        raise SystemExit(f"libazul does not export {SYMBOL} - built before the ABI guard existed?")
    for i in range(phnum):
        p_type, _flags, p_offset, p_vaddr, _paddr, p_filesz, _memsz, _align = struct.unpack_from(
            "<IIQQQQQQ", data, phoff + i * phentsize)
        if p_type == 1 and p_vaddr <= value < p_vaddr + p_filesz:  # PT_LOAD
            return p_offset + (value - p_vaddr), arch
    raise SystemExit(f"{SYMBOL} at {value:#x} is in no PT_LOAD segment")
