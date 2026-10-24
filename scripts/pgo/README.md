# PGO + function order for libazul.dylib

BOLT is ELF-only (Linux `libazul.so`, a CI job later). On macOS the same win - hot code packed together, so
the OS maps fewer pages of the 72 MB dylib - is an ld64 `-order_file`; speed is PGO (`-Cprofile-use`).

    zsh scripts/pgo/chain.sh      # instrumented build -> train -> merge -> order file -> optimized build -> measure

Status (2026-10-02): the toolchain works (rustc 1.91, llvm-profdata 21.1.2: a tiny exe and a tiny cdylib
profile and merge fine). Every raw profile of the instrumented libazul.dylib came out TRUNCATED at its last
4 KB buffer: the shell ends the process with `std::process::exit` while its threads still run and the 25 MB
exit-time write raced them. Fixed in 4a3e172cb: `azul_layout::pgo::dump_profile()` (`__llvm_profile_dump`,
in a `--cfg azul_pgo` build), the debug-server op `dump_profile`, the e2e runner's exits. Not yet re-run.
Also seen: the instrumented `AZ_E2E` host segfaults at exit (exit 139) - a thread touching torn-down state.

Baseline (normal release build, 2e92c759b): the /e2e corpus (62 scenarios) runs in ~7 s on AzPaint
(40 pass / 22 fail); 20 apps together keep 16.1 MB of the dylib's 47.9 MB __TEXT resident; page-residency
(mincore) puts ~20 MB of the 38.7 MB __text on touched pages - page granularity, an upper bound.
