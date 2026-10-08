#!/usr/bin/env python3
"""REFACTOR13: stack frame sizes of the recursive layout functions (aarch64 Mach-O).

    scripts/refactor/frame_sizes.py <binary or .rlib> [name ...]

Reads the prologue of every function whose demangled path is in
`azul_layout::solver3` and whose last segment is one of NAMES (default: the
four functions of the recursive block-layout path plus every `layout_bfc*` /
`bfc_*` helper) and prints how far it moves `sp`: the stack-probe target
(`sub x9, sp, #N, lsl #12`), every `sub sp, sp, #imm` outside the probe loop,
and the register-save pushes (`stp .., [sp, #-N]!`) separately.

The frame figure is `sub sp` + probe, the same quantity the lead measured by
hand with `otool -tv -p <symbol>`; pushes are listed but not included.
"""
import re
import subprocess
import sys

DEFAULT = [
    "layout_bfc",
    "calculate_layout_for_subtree",
    "calculate_layout_for_subtree_fragment",
    "layout_formatting_context",
]
HELPER_PREFIXES = ("layout_bfc", "bfc_")


def demangle(sym):
    """Legacy Rust mangling `__ZN<len><ident>...17h<hash>E` -> list of segments."""
    s = sym.lstrip("_")
    if not s.startswith("ZN"):
        return None
    i, segs = 2, []
    while i < len(s) and s[i] != "E":
        m = re.match(r"\d+", s[i:])
        if not m:
            return None
        n = int(m.group(0))
        i += len(m.group(0))
        segs.append(s[i : i + n])
        i += n
    if segs and re.fullmatch(r"h[0-9a-f]{16}", segs[-1]):
        segs = segs[:-1]
    return segs


def imm(text):
    return int(text, 16) if text.startswith("0x") else int(text)


def frame_of(binary, sym):
    # `otool -p` disassembles from the symbol to the end of the section: read
    # only the prologue and stop it.
    proc = subprocess.Popen(["otool", "-tv", "-p", sym, binary], stdout=subprocess.PIPE, text=True)
    out = []
    for line in proc.stdout:
        out.append(line.rstrip("\n"))
        if len(out) > 60:
            break
    proc.kill()
    proc.wait()
    insns = [l.split("\t", 1)[1] for l in out if "\t" in l and re.match(r"^[0-9a-f]+\t", l)]
    frame, pushes, probe_loop_pending = 0, 0, False
    for ins in insns[:40]:
        op = ins.split("\t")
        mnemonic, args = op[0], (op[1] if len(op) > 1 else "")
        if mnemonic in ("bl", "b", "ret", "br", "blr", "cbz", "cbnz", "tbz", "tbnz"):
            break
        m = re.match(r"x\d+, x\d+, \[sp, #-(0x[0-9a-f]+|\d+)\]!", args)
        if mnemonic == "stp" and m:
            pushes += imm(m.group(1))
            continue
        m = re.match(r"x9, sp, #(0x[0-9a-f]+|\d+), lsl #12", args)
        if mnemonic == "sub" and m:
            frame += imm(m.group(1)) * 4096
            probe_loop_pending = True
            continue
        m = re.match(r"sp, sp, #(0x[0-9a-f]+|\d+)(, lsl #12)?$", args)
        if mnemonic == "sub" and m:
            amount = imm(m.group(1)) * (4096 if m.group(2) else 1)
            if probe_loop_pending and m.group(2) and imm(m.group(1)) == 1:
                probe_loop_pending = False  # the probe loop's 4 KiB step
                continue
            frame += amount
    return frame, pushes


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    binary, names = sys.argv[1], sys.argv[2:] or DEFAULT
    syms = subprocess.run(["nm", binary], capture_output=True, text=True).stdout.splitlines()
    seen = {}
    for line in syms:
        parts = line.split()
        if len(parts) < 3 or parts[1] not in ("t", "T"):
            continue
        segs = demangle(parts[2])
        if not segs or segs[:2] != ["azul_layout", "solver3"] or any("test" in s for s in segs):
            continue
        last = segs[-1]
        if last in names or (not sys.argv[2:] and last.startswith(HELPER_PREFIXES)):
            seen.setdefault(parts[2], segs)
    rows = []
    for sym, segs in seen.items():
        frame, pushes = frame_of(binary, sym)
        rows.append(("::".join(segs[1:]), frame, pushes))
    for path, frame, pushes in sorted(rows, key=lambda r: -r[1]):
        print(f"{frame:>8} B  (+{pushes} B saved regs)  {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
