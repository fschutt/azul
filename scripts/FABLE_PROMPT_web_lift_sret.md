# Fable kickoff prompt — azul web-lift, delete the out-param workarounds

> Paste this as the opening message of a fresh Claude Fable session in
> `/Users/fschutt/Development/azul-mobile` (branch `web-lift-text-layout`).

---

You are picking up the azul **web backend** (native aarch64 dylib → wasm via a remill fork).
It already works: `examples/c/hello-world.c` lays out, renders its counter, and handles
clicks end-to-end on the lifted backend. Your predecessor (Claude Opus 4.8) left a complete
handoff — **read it first, fully**, before doing anything else:

    scripts/HANDOFF_FABLE_web_lift_2026_06_10.md

**Goal:** root-cause and fix the **class-B mis-lift** — multi-word `Vec`/struct **returns**
via the X8/sret register across internal lifted calls — so the `#[cfg(feature="web_lift")]`
**out-param workarounds** in azul source can be deleted (§4.A of the handoff). One transpiler/
remill fix removes them all. A minimal isolated reproduction (`az_sret_probe`) is staged for
you in §5, with three concrete hypotheses (H1 decoder/CFG gap · H2 LLVM-17 DCE of the X8
store · H3 `bl`→sub_ X8 threading) and a fast bisect (`AZ_OPT_LEVEL=O0` vs `Oz`).

**Hard constraints (do not violate):**
- The fix goes in the **transpiler** (`dll/src/web/`) or the **remill fork**
  (`/Users/fschutt/Development/azul/third_party/remill`), **never in azul source**. azul
  source only gets workarounds *removed*.
- **Commit only when the user explicitly asks.**
- **Check disk first** (`df -h /`) and purge `/var/folders/5x/*/T/azul-web-transpiler-*` +
  kill orphan `remill-lift`/`.bin` processes between relifts.
- **Analysis-first.** A full relift is ~15–30 min — exhaust static IR/objdump analysis before
  spending one. Standalone-lifting the staged witness (handoff §5, option 1) needs no relift.
- Two traps your predecessor flagged: (1) **`initializes` is a red herring** — it's an
  LLVM-20/21 analysis artifact; the real pipeline is LLVM 17 and never emits it. (2)
  rust-analyzer shows **stale** "expected 5 args found 6" errors on the out-param sites —
  trust `cargo check`, not rust-analyzer.

**First action:** read the handoff, then start with the cheap win in §7.1 — relift with the
`g115/g118/g120` HashMap bypasses deleted to confirm the just-landed EMPTY_GROUP fix
(`c0861ee07`) made them unnecessary (task #7). Then attack class B via the staged witness.

The hard infrastructure is done and proven (NEON decoders, build-std atomics, the
EMPTY_GROUP mirror, snprintf, font resolution, the full lift→link→harness loop). You are
hunting a small number of specific remill/transpiler semantic gaps, with a proven method
(standalone-lift → find the bad instr in objdump → fix the decoder/semantic) and a full
diagnostic toolkit (handoff §6).
