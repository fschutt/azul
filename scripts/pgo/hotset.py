#!/usr/bin/env python3
"""The dylib's hot code from PAGE RESIDENCY (no instrumentation): functions overlapping a resident page
of __text (mincore, while the workload runs). Page granularity - an upper bound of the hot set.
Usage: hotset.py <libazul.dylib> <out order list>"""
import ctypes, os, re, subprocess, sys
dylib, out = sys.argv[1], sys.argv[2]
page = 16384
text = subprocess.run(["otool", "-l", dylib], capture_output=True, text=True).stdout
m = re.search(r"segname __TEXT\n\s+vmaddr (0x[0-9a-f]+)\n\s+vmsize (0x[0-9a-f]+)\n\s+fileoff (\d+)", text)
seg_addr, seg_size, seg_off = int(m.group(1), 16), int(m.group(2), 16), int(m.group(3))
sec = re.search(r"sectname __text\n\s+segname __TEXT\n\s+addr (0x[0-9a-f]+)\n\s+size (0x[0-9a-f]+)", text)
text_addr, text_size = int(sec.group(1), 16), int(sec.group(2), 16)
libc = ctypes.CDLL(None)
libc.mmap.restype = ctypes.c_void_p
libc.mmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_long]
libc.mincore.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_char_p]
fd = os.open(dylib, os.O_RDONLY)
addr = libc.mmap(None, seg_size, 1, 1, fd, seg_off)
vec = ctypes.create_string_buffer((seg_size + page - 1) // page)
libc.mincore(addr, seg_size, vec)
resident = [bool(b & 1) for b in vec.raw]
syms = sorted((int(p[0], 16), p[2]) for p in (l.split() for l in subprocess.run(["nm", "-n", "-U", dylib], capture_output=True, text=True).stdout.splitlines())
              if len(p) >= 3 and p[1] in ("T", "t") and text_addr <= int(p[0], 16) < text_addr + text_size)
hot, hot_bytes = [], 0
for i, (a, name) in enumerate(syms):
    end = syms[i + 1][0] if i + 1 < len(syms) else text_addr + text_size
    if any(resident[pg] for pg in range((a - seg_addr) // page, (max(a, end - 1) - seg_addr) // page + 1)):
        hot.append(name)
        hot_bytes += end - a
res_pages = sum(resident[(text_addr - seg_addr) // page:(text_addr + text_size - seg_addr + page - 1) // page])
print("__text %.1f MB, %d functions; resident %.1f MB; functions on resident pages: %d = %.1f MB"
      % (text_size / 1048576, len(syms), res_pages * page / 1048576, len(hot), hot_bytes / 1048576))
open(out, "w").write("\n".join(hot) + "\n")
