#!/usr/bin/env python3
"""Strip .llvmbc/.llvmcmd (embedded LLVM bitcode) + COFF debug sections from an
MSVC-style static library (.lib) by processing each member with llvm-objcopy.

Why this exists: llvm-objcopy processes GNU/BSD `.a` archives natively but
rejects MSVC-kind archives (two "linker members") with "unsupported object
file format" — while it handles the individual COFF *members* just fine
(rust.yml uses it directly on the ELF/Mach-O `.a`s). So this script:

  1. parses the archive container itself,
  2. runs objcopy on every real COFF member,
  3. rewrites the archive with the shrunken members, patching the member
     offsets inside the two linker members (symbol names, order and count are
     unchanged — section removal never touches external symbols),
  4. re-parses the result and asserts no .llvmbc/.llvmcmd section survived.

The ThinLTO-optimized machine code (.text$mn etc.) is byte-identical — only
dead-weight metadata is removed (a prebuilt .lib consumer never runs Rust
ThinLTO, so the embedded bitcode is 100% unusable; see
scripts/RELEASE_SIZE_MEMORY_AUDIT_2026_07_04.md §2.4).

Usage: strip_coff_lib.py <objcopy-binary> <file.lib>
Exit 0 on success (file replaced in place), non-zero on any failure
(original file left untouched).
"""

import os
import struct
import subprocess
import sys
import tempfile

REMOVE_SECTIONS = [".llvmbc", ".llvmcmd"]
OBJCOPY_FLAGS = [f"--remove-section={s}" for s in REMOVE_SECTIONS] + ["--strip-debug"]

# COFF machine types rustc/MSVC emit (x64, x86, arm64, arm, arm64ec) + ANON_OBJECT.
COFF_MACHINES = {0x8664, 0x014C, 0xAA64, 0x01C0, 0x01C4, 0xA641, 0x0000}


def parse_archive(data):
    """Return list of (name, header_offset, header_bytes, member_data)."""
    if data[:8] != b"!<arch>\n":
        raise ValueError("not an ar archive")
    members = []
    off = 8
    while off + 60 <= len(data):
        hdr = data[off : off + 60]
        if hdr[58:60] != b"`\n":
            raise ValueError(f"bad member header magic at {off}")
        name = hdr[:16].decode("ascii").rstrip()
        size = int(hdr[48:58].decode("ascii").strip())
        body = data[off + 60 : off + 60 + size]
        if len(body) != size:
            raise ValueError(f"truncated member at {off}")
        members.append([name, off, hdr, body])
        off += 60 + size + (size & 1)
    return members


def coff_section_names(body):
    """Yield section names of a plain COFF object (empty for non-COFF)."""
    if len(body) < 20:
        return []
    machine, nsects = struct.unpack_from("<HH", body, 0)
    if machine not in COFF_MACHINES or machine == 0x0000:
        return []  # non-COFF or anon/import object — objcopy skips these too
    opt_size = struct.unpack_from("<H", body, 16)[0]
    names = []
    base = 20 + opt_size
    for i in range(nsects):
        o = base + i * 40
        if o + 40 > len(body):
            break
        names.append(body[o : o + 8].rstrip(b"\0").decode("latin1"))
    return names


def is_coff_object(body):
    if len(body) < 20:
        return False
    machine = struct.unpack_from("<H", body, 0)[0]
    return machine in COFF_MACHINES and machine != 0x0000


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    objcopy, libpath = sys.argv[1], sys.argv[2]
    data = open(libpath, "rb").read()
    members = parse_archive(data)

    # Identify special members: leading "/" entries (linker members) and "//"
    # (long-name string table). Everything else is a real object member.
    linker_member_idx = [i for i, m in enumerate(members) if m[0] == "/"]
    if not linker_member_idx or linker_member_idx[0] != 0:
        raise ValueError("no first linker member — not an MSVC-style .lib?")

    tmpdir = tempfile.mkdtemp(prefix="striplib")
    processed = skipped = 0
    for m in members:
        if m[0] in ("/", "//"):
            continue
        if not is_coff_object(m[3]):
            skipped += 1
            continue
        tmp = os.path.join(tmpdir, "member.obj")
        with open(tmp, "wb") as f:
            f.write(m[3])
        r = subprocess.run(
            [objcopy] + OBJCOPY_FLAGS + [tmp],
            capture_output=True,
            text=True,
        )
        if r.returncode != 0:
            print(f"warn: objcopy failed on member {m[0]}: {r.stderr.strip()}", file=sys.stderr)
            skipped += 1
            continue
        m[3] = open(tmp, "rb").read()
        processed += 1

    # Recompute member offsets (offset = position of the 60-byte header).
    old_to_new = {}
    off = 8
    for m in members:
        old_to_new[m[1]] = off
        off += 60 + len(m[3]) + (len(m[3]) & 1)

    # Patch first linker member: u32be count, count x u32be member offsets, names.
    flm = members[linker_member_idx[0]]
    body = bytearray(flm[3])
    (count,) = struct.unpack_from(">I", body, 0)
    for i in range(count):
        (old,) = struct.unpack_from(">I", body, 4 + 4 * i)
        struct.pack_into(">I", body, 4 + 4 * i, old_to_new[old])
    flm[3] = bytes(body)

    # Patch second linker member (if present): u32le nmembers, nmembers x u32le
    # offsets, u32le nsyms, nsyms x u16le indices, names.
    if len(linker_member_idx) > 1:
        slm = members[linker_member_idx[1]]
        body = bytearray(slm[3])
        (nmem,) = struct.unpack_from("<I", body, 0)
        for i in range(nmem):
            (old,) = struct.unpack_from("<I", body, 4 + 4 * i)
            struct.pack_into("<I", body, 4 + 4 * i, old_to_new[old])
        slm[3] = bytes(body)

    # Write the new archive (member headers keep original name/date/mode, only
    # the size field changes).
    out = bytearray(b"!<arch>\n")
    for name, _old_off, hdr, body in members:
        hdr = bytearray(hdr)
        hdr[48:58] = f"{len(body):<10d}".encode("ascii")
        out += hdr + body
        if len(body) & 1:
            out += b"\n"

    # Verify before replacing: re-parse, check linker-member offsets land on
    # headers, and assert the bitcode sections are really gone.
    reparsed = parse_archive(bytes(out))
    for m in reparsed:
        if m[0] == "/":
            (count,) = struct.unpack_from(">I" if m is reparsed[0] else "<I", m[3], 0)
            n = min(count, 4)
            for i in range(n):
                (o,) = struct.unpack_from(
                    ">I" if m is reparsed[0] else "<I", m[3], 4 + 4 * i
                )
                if bytes(out[o + 58 : o + 60]) != b"`\n":
                    raise ValueError(f"patched linker-member offset {o} is not a member header")
    leftovers = 0
    for m in reparsed:
        if m[0] in ("/", "//"):
            continue
        for s in coff_section_names(m[3]):
            if s in REMOVE_SECTIONS:
                leftovers += 1
    if leftovers:
        raise ValueError(f"{leftovers} bitcode sections survived — refusing to write")

    tmp_out = libpath + ".stripped"
    with open(tmp_out, "wb") as f:
        f.write(out)
    os.replace(tmp_out, libpath)
    print(
        f"{libpath}: {len(data)} -> {len(out)} bytes "
        f"({100 - len(out) * 100 // max(len(data), 1)}% smaller), "
        f"{processed} members stripped, {skipped} skipped"
    )


if __name__ == "__main__":
    main()
