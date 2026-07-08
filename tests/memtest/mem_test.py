#!/usr/bin/env python3
# Memory test for the azul Python (pyo3) binding. See tests/memtest/README.md.
#
# Kept deliberately simple: the harness (scripts/run_memtest.sh) does the work.
#   - runs this under gdb  -> any SIGSEGV (double-free / UAF) fails the test
#   - runs it twice with a small and a large AZ_MEMTEST_N and compares peak RSS
#     -> RSS that scales with N is a LEAK
# So each per-language file only has to: exercise the create/consume/DROP paths
# in a loop N times and exit 0. No in-test measurement, no event loop (App.run
# needs a display and hangs headless).
import os
from azul import *

class Model:
    def __init__(self, counter):
        self.counter = counter

N = int(os.environ.get("AZ_MEMTEST_N", "200000"))

# 1. The consume-by-value DROP path: App.create consumes AppConfig, whose nested
#    SystemStyle was one of the 7 types that bitwise-cloned + double-freed.
app = App.create(Model(5), AppConfig.create())
del app

# 2. Leak loop: create/destroy a droppable object N times (AppConfig drops the
#    nested SystemStyle every iteration).
for _ in range(N):
    c = AppConfig.create()
    del c

print(f"memtest python OK (N={N})")
