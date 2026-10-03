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


# ---------------------------------------------------------------------------
# Reading and patching the constant AzAbi_getHash returns
# ---------------------------------------------------------------------------

A64_RET = 0xD65F03C0
# Landing pads a hardened build may start the function with (BTI c / j / jc,
# PACIASP, PACIBSP): kept, the patch goes after them.
A64_PADS = {0xD503245F, 0xD503249F, 0xD50324DF, 0xD503233F, 0xD503237F}
X86_ENDBR64 = b"\xf3\x0f\x1e\xfa"


def body_start(data, off, arch):
    """Offset of the first instruction after a landing pad."""
    if arch == ARM64:
        while struct.unpack_from("<I", data, off)[0] in A64_PADS:
            off += 4
    elif data[off:off + 4] == X86_ENDBR64:
        off += 4
    return off


def decode_hash(data, off, arch):
    """The constant `AzAbi_getHash` returns (`AZ_ABI_HASH` of the build), or
    None when the code is not the expected load-constant-and-return."""
    off = body_start(data, off, arch)
    if arch == ARM64:
        value = 0
        for i in range(8):
            ins = struct.unpack_from("<I", data, off + 4 * i)[0]
            if ins == A64_RET:
                return value
            if ins & 0x1F != 0:  # every instruction must write x0
                return None
            imm, hw = (ins >> 5) & 0xFFFF, (ins >> 21) & 3
            if ins & 0xFF800000 == 0xD2800000:  # MOVZ x0, #imm, lsl #16*hw
                value = imm << (16 * hw)
            elif ins & 0xFF800000 == 0xF2800000:  # MOVK x0, #imm, lsl #16*hw
                value = (value & ~(0xFFFF << (16 * hw))) | (imm << (16 * hw))
            else:
                return None
        return None
    # x86_64: [push rbp; mov rbp, rsp;] movabs rax, imm64
    window = data[off:off + 16]
    at = window.find(b"\x48\xb8")
    if at < 0:
        return None
    return struct.unpack_from("<Q", window, at + 2)[0]


def patch_code(arch, value):
    """`return value;` (value < 2^16) in the arch's machine code."""
    if arch == ARM64:
        return struct.pack("<II", 0xD2800000 | (value << 5), A64_RET)  # MOVZ x0, #value; RET
    return b"\xb8" + struct.pack("<I", value) + b"\xc3"  # mov eax, imm32 (zero-extends); ret


def patched_copy(lib, dest_dir):
    """Copy `lib` into dest_dir with `AzAbi_getHash` returning WRONG_HASH.
    Returns (copy, the hash the unpatched library reports or None)."""
    data = bytearray(lib.read_bytes())
    off, arch = find_function(data)
    original = decode_hash(data, off, arch)
    code = patch_code(arch, WRONG_HASH)
    start = body_start(data, off, arch)
    data[start:start + len(code)] = code
    out = dest_dir / lib_name()
    out.write_bytes(bytes(data))
    out.chmod(0o755)
    if sys.platform == "darwin":
        # The patch broke the code signature; arm64 macOS kills a process
        # that maps an unsigned or mis-signed library. Re-sign it ad hoc.
        r = subprocess.run(["codesign", "--force", "--sign", "-", str(out)],
                           capture_output=True, text=True)
        if r.returncode != 0:
            raise SystemExit(f"codesign failed: {r.stderr.strip()}")
    return out, original


# ---------------------------------------------------------------------------
# The tests
# ---------------------------------------------------------------------------

def run(cmd, **kw):
    return subprocess.run([str(c) for c in cmd], capture_output=True, text=True, **kw)


def stub_object(work, value):
    """An object file defining `AzAbi_getHash` -> value: the libazul side."""
    src = work / f"stub_{value:x}.c"
    src.write_text("#include <stdint.h>\n"
                   f"uint64_t {SYMBOL}(void) {{ return UINT64_C({value:#x}); }}\n")
    obj = src.with_suffix(".o")
    r = run([os.environ.get("CC", "cc"), "-c", src, "-o", obj])
    if r.returncode != 0:
        raise SystemExit(f"cannot compile the stub: {r.stderr}")
    return obj


def cxx_compile(cmd):
    """Compile with the C++ compiler; on macOS retry with the SDK's libc++
    headers when the Command Line Tools' own copy is incomplete."""
    r = run(cmd)
    if r.returncode != 0 and sys.platform == "darwin" and "file not found" in r.stderr:
        sdk = run(["xcrun", "--show-sdk-path"]).stdout.strip()
        cxx_dir = Path(sdk) / "usr/include/c++/v1"
        if cxx_dir.is_dir():
            r = run(cmd[:1] + ["-nostdinc++", "-isystem", cxx_dir] + cmd[1:])
    return r


def expect_abort(name, exe, app_hash):
    """`exe` must abort before main, naming both hashes."""
    r = run([exe], timeout=60)
    problems = []
    if r.returncode not in (-6, 134):
        problems.append(f"exit {r.returncode}, not SIGABRT")
    if "main reached" in r.stdout:
        problems.append("main ran - the check did not run when the program loaded")
    for want in (MISMATCH, hex16(app_hash), hex16(WRONG_HASH), REBUILD_HINT):
        if want not in r.stderr:
            problems.append(f"stderr lacks {want!r}")
    return name, not problems, "; ".join(problems) + f"\n    stderr: {r.stderr.strip()[:400]}"


def expect_run(name, exe):
    r = run([exe], timeout=60)
    ok = r.returncode == 0 and "main reached" in r.stdout and MISMATCH not in r.stderr
    return name, ok, f"exit {r.returncode}, stdout {r.stdout.strip()!r}, stderr {r.stderr.strip()[:400]!r}"


def test_c(work, codegen):
    app_hash = header_hash(codegen / "azul.h")
    main = work / "main_c.c"
    main.write_text('#include "azul.h"\n#include <stdio.h>\n'
                    'int main(void) { puts("main reached"); return 0; }\n')
    obj = work / "main_c.o"
    r = run([os.environ.get("CC", "cc"), "-std=c11", "-O0", "-I", codegen, "-c", main, "-o", obj])
    if r.returncode != 0:
        return [("c_compiles_azul_h", False, r.stderr[:2000])]
    results = []
    for value, name in ((WRONG_HASH, "c_aborts_before_main_on_another_abi"),
                        (app_hash, "c_runs_against_its_own_abi")):
        exe = work / name
        r = run([os.environ.get("CC", "cc"), obj, stub_object(work, value), "-o", exe])
        if r.returncode != 0:
            results.append((name, False, f"link: {r.stderr[:2000]}"))
        elif value == WRONG_HASH:
            results.append(expect_abort(name, exe, app_hash))
        else:
            results.append(expect_run(name, exe))
    return results


def test_cpp(work, codegen, header):
    app_hash = header_hash(codegen / "azul.h")
    m = re.match(r"azul(\d\d)\.hpp$", header)
    std = f"c++{m.group(1)}" if m else "c++17"
    main = work / "main_cpp.cpp"
    main.write_text(f'#include "{header}"\n#include <cstdio>\n'
                    'int main() { std::puts("main reached"); return 0; }\n')
    obj = work / "main_cpp.o"
    cxx = os.environ.get("CXX", "c++")
    r = cxx_compile([cxx, f"-std={std}", "-O0", "-I", codegen, "-c", main, "-o", obj])
    if r.returncode != 0:
        return [(f"cpp_compiles_{header}", False, r.stderr[:2000])]
    name = "cpp_aborts_before_main_on_another_abi"
    exe = work / name
    r = run([cxx, obj, stub_object(work, WRONG_HASH), "-o", exe])
    if r.returncode != 0:
        return [(name, False, f"link: {r.stderr[:2000]}")]
    return [expect_abort(name, exe, app_hash)]


def run_app(app, lib_dir, seconds, log):
    """Start the app against the libazul in lib_dir. Returns (exit code, output,
    aborted, still running when stopped)."""
    cmd = ["env", f"{lib_path_var()}={lib_dir}", "AZ_BACKEND=headless", app]
    capped = REPO / "scripts/waves/tools/run_capped.sh"
    if capped.exists():
        # `env` sets the variable for the app itself: macOS strips DYLD_*
        # from the environment of the protected /bin/bash the runner is.
        r = run([capped, "--cap-mb", "1500", "--seconds", seconds, "--log", log, "--"] + cmd)
        out = (log.read_text(errors="replace") if log.exists() else "") + r.stderr
        return r.returncode, out, r.returncode == 134, r.returncode == 124
    try:
        r = run(cmd, timeout=seconds)
    except subprocess.TimeoutExpired as e:
        out = b"".join(x for x in (e.stdout, e.stderr) if isinstance(x, bytes))
        return None, out.decode(errors="replace"), False, True
    return r.returncode, r.stdout + r.stderr, r.returncode in (-6, 134), False


def test_rust_app(work, lib, app, seconds):
    if not app.exists():
        return [("rust_app_exists", False, f"no prebuilt app at {app} (--app)")]
    if not lib.exists():
        return [("rust_libazul_exists", False, f"no prebuilt libazul at {lib} (--lib)")]
    stale_dir = work / "stale-libazul"
    stale_dir.mkdir()
    _copy, app_hash = patched_copy(lib, stale_dir)
    results = []
    name = "rust_app_aborts_on_a_libazul_with_another_abi"
    code, out, aborted, _ = run_app(app, stale_dir, max(seconds, 20), work / f"{name}.log")
    problems = [] if aborted else [f"exit {code}, not SIGABRT"]
    wants = [MISMATCH, hex16(WRONG_HASH), REBUILD_HINT] + ([hex16(app_hash)] if app_hash else [])
    problems += [f"output lacks {w!r}" for w in wants if w not in out]
    results.append((name, not problems, "; ".join(problems) + f"\n    output: {out.strip()[-600:]}"))

    name = "rust_app_starts_on_its_own_libazul"
    code, out, aborted, running = run_app(app, lib.parent, seconds, work / f"{name}.log")
    ok = not aborted and MISMATCH not in out and (running or code == 0)
    results.append((name, ok, f"exit {code}, output: {out.strip()[-600:]}"))
    return results
