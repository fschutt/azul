#!/usr/bin/env python3
"""android_elf_check.py: can a device with 16 KiB memory pages load these Android libraries?

    android_elf_check.py <app.apk | lib.so> [...]

Android 15 and later run on devices whose kernel uses 16 KiB pages, and Google requires apps that target Android 15 (targetSdk
35) and later to support them from 2027-02-01. A native library whose PT_LOAD segments are aligned to 4 KiB (p_align 0x1000, the
default of NDK 27's linker) cannot be mapped there; the libraries of an APK are stored compressed, so the compatibility mode does
not rescue them either. Every PT_LOAD segment of every library must have p_align >= 0x4000: the library is linked with
`-Wl,-z,max-page-size=16384 -Wl,-z,common-page-size=16384` (the rustflags of the Android targets in .cargo/config.toml, which
reach the library of the lib job and the library of every app alike).

An APK is read for every lib/<abi>/*.so in it (and the name of a failing one is `<apk>!lib/<abi>/<file>.so`); a library the
APK stores UNCOMPRESSED must also start on a 16 KiB boundary of the zip (`zipalign -P 16`, build-tools 35 and later; the
libraries of the demo APKs are compressed, so this holds for them without it). The ELF headers are read here, in Python, so the
gate needs no NDK on the runner that runs it (llvm-readelf -lW shows the same p_align column).

Exit 0 when every file passes, 1 when one does not (a line each, with the file name), 2 when a file cannot be read or no file
is given.
"""
import os
import struct
import sys
import zipfile

MIN_ALIGN = 0x4000
ZIP_ALIGN = 16384
PT_LOAD = 1


def load_alignments(data):
    """The p_align of every PT_LOAD segment of an ELF file (bytes). Raises ValueError for what is no readable ELF."""
    if data[:4] != b"\x7fELF":
        raise ValueError("not an ELF file")
    if len(data) < 52 or data[5] != 1:
        raise ValueError("not a readable little-endian ELF file")
    if data[4] == 2:        # ELF64: e_phoff at 32, e_phentsize and e_phnum at 54; Elf64_Phdr has p_align at 48
        if len(data) < 64:
            raise ValueError("cut off in the ELF header")
        (phoff,) = struct.unpack_from("<Q", data, 32)
        phentsize, phnum = struct.unpack_from("<HH", data, 54)
        align_at, align_format, align_size = 48, "<Q", 8
    elif data[4] == 1:      # ELF32: e_phoff at 28, e_phentsize and e_phnum at 42; Elf32_Phdr has p_align at 28
        (phoff,) = struct.unpack_from("<I", data, 28)
        phentsize, phnum = struct.unpack_from("<HH", data, 42)
        align_at, align_format, align_size = 28, "<I", 4
    else:
        raise ValueError("neither ELF32 nor ELF64")
    aligns = []
    for i in range(phnum):
        at = phoff + i * phentsize
        if phentsize < align_at + align_size or at + phentsize > len(data):
            raise ValueError("cut off in the program headers")
        (kind,) = struct.unpack_from("<I", data, at)
        if kind == PT_LOAD:
            aligns.append(struct.unpack_from(align_format, data, at + align_at)[0])
    return aligns


def check_elf(name, data):
    """The problems of one library, as lines that start with its name: [] when it can be loaded on 16 KiB pages."""
    try:
        aligns = load_alignments(data)
    except (ValueError, struct.error) as e:
        return [f"{name}: {e}"]
    if not aligns:
        return [f"{name}: no PT_LOAD segment"]
    bad = sorted({a for a in aligns if a < MIN_ALIGN})
    if bad:
        return [f"{name}: PT_LOAD p_align {', '.join(hex(a) for a in bad)} is below 0x4000, so a device with 16 KiB pages cannot "
                f"load it (link with -Wl,-z,max-page-size=16384: the rustflags of the Android targets in .cargo/config.toml)"]
    return []


def stored_data_offset(path, info):
    """Where the data of a zip entry starts in the file (after its local header, whose extra field may differ from the central one)."""
    with open(path, "rb") as fh:
        fh.seek(info.header_offset)
        head = fh.read(30)
    name_length, extra_length = struct.unpack_from("<HH", head, 26)
    return info.header_offset + 30 + name_length + extra_length


def check_apk(path):
    """The problems of every native library of an APK: [] when each can be loaded on 16 KiB pages."""
    try:
        archive = zipfile.ZipFile(path)
    except (zipfile.BadZipFile, OSError) as e:
        return [f"{path}: not a readable APK: {e}"]
    base = os.path.basename(path)
    problems, libraries = [], 0
    with archive:
        for info in archive.infolist():
            if not (info.filename.startswith("lib/") and info.filename.endswith(".so")):
                continue
            libraries += 1
            name = f"{base}!{info.filename}"
            try:
                data = archive.read(info)
            except Exception as e:  # noqa: BLE001  (a torn or corrupt entry is this library's failure)
                problems.append(f"{name}: cannot be read: {e}")
                continue
            problems += check_elf(name, data)
            if info.compress_type == zipfile.ZIP_STORED:
                offset = stored_data_offset(path, info)
                if offset % ZIP_ALIGN:
                    problems.append(f"{name}: stored uncompressed at offset {offset}, not on a 16 KiB boundary (zipalign -P 16)")
    if not libraries:
        problems.append(f"{base}: no native library (lib/<abi>/*.so) in the APK")
    return problems


def main(argv):
    if not argv:
        print(__doc__, file=sys.stderr)
        return 2
    problems, unreadable = [], 0
    for arg in argv:
        try:
            with open(arg, "rb") as fh:
                head = fh.read(4)
                data = head + fh.read() if head[:2] != b"PK" else b""
        except OSError as e:
            print(f"android_elf_check: {arg}: {e}", file=sys.stderr)
            unreadable += 1
            continue
        problems += check_apk(arg) if head[:2] == b"PK" else check_elf(arg, data)
    for line in problems:
        print(line)
    if unreadable:
        return 2
    if problems:
        return 1
    print(f"android_elf_check: ok ({len(argv)} file(s), every PT_LOAD segment aligned to at least 16 KiB)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
