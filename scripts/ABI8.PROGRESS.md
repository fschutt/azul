# ABI8 - progress (branch wt/abi8, base 45c6bf98b)

Brief: scripts/waves/wave8/PLAN.md section "ABI8". Report: scripts/ABI8_2026_10_03.md.

## Audit (what exists, from LIFECYCLE 2026-10-01, commits 9d8d69e45 / 3ba642c65)
- doc/src/codegen/v2/abi_guard.rs: FNV-1a 64 over the sorted, doc-free ABI text of the IR.
- libazul exports `AzAbi_getHash` (checked: `nm -gU target/azul-lib/libazul.dylib` has `_AzAbi_getHash`).
- Rust binding (dll_api_external.rs / azul.rs, link-dynamic): `az_abi_check()` in every Constructor /
  StaticMethod wrapper and the two `From<..> for AzString` impls (all 21 prebuilt apps import `_AzAbi_getHash`).
- azul.h: `AZ_ABI_HASH`, `AzAbi_check()`, load-time call (GCC/Clang constructor; C++ static object
  elsewhere); C++ headers include azul.h. MSVC C: no load-time call.
- Python: the extension is libazul itself (`python-extension` = build-dll + link-static) - cannot disagree.

## Gaps found
1. Rust: value-less entry points that skip the check: `X::create_default()` (FunctionKind::Default, 742),
   `impl Default for X` -> `AzX_createDefault()` (740), enum-variant constructors (`none()`, `some(..)`, ...).
2. Rust: no load-time check (C has one); the check runs at the first checked call only.
3. MSVC C: no load-time check.
4. No runtime test of a real binding against a libazul with another hash (only `az_abi_check_hash` in-process).

## DONE
- e09b5811c progress (audit)
- 53f20424e RED gap 1 / 01f38b238 GREEN gap 1 (`is_first_call_kind`; impl Default body checks)
- 8f9b78d9a RED gap 2 / 8188e5625 GREEN gap 2 (`rust_load_time_check`: AZ_ABI_CHECK_AT_LOAD)

## IN PROGRESS
- Gap 4: scripts/abi_guard_e2e.py (NOT written yet). Verified by hand in /tmp/abi8/c: main.c including the
  prebuilt target/codegen/azul.h + stub.c (`uint64_t AzAbi_getHash(void){return STUB_HASH;}`), `clang -std=c11`,
  STUB_HASH=0xdead -> exit 134 with the message, before main (0.4 s, 96 MB).
- FOUND: azul17.hpp does not compile (`clang++ -std=c++17 -nostdinc++ -isystem <SDK>/usr/include/c++/v1`;
  this Mac's CLT c++/v1 dir is a stub, use the SDK's): azul17.hpp:87715 `AzChartKind_isBar(self)` hits azul.h's
  helper MACRO `#define AzChartKind_isBar(value) (*(value) == AzChartKind_Bar)` (azul.h:115671) - api.json's
  ChartKind has a method `isBar` that collides with the generated enum-variant test macro. One error only.
  Codegen bug in lang_c.rs (macro emitter) - fix if in reach (doc/src/codegen is ABI8's).

## NEXT
- Write scripts/abi_guard_e2e.py: tests (a) C header aborts before main on a mismatch / passes a match,
  (b) C++ (azul.h in C++ mode; azul17.hpp once the macro clash is fixed), (c) Rust app: copy
  target/azul-lib/libazul.dylib to a temp dir, patch `_AzAbi_getHash` (arm64 at file off 0x1280: movz/3x movk/ret
  -> `movz x0,#0xdead` 0xD2800000|0xdead<<5 ; `ret` 0xD65F03C0; x86_64: B8 AD DE 00 00 C3), `codesign -f -s -`,
  run AzCalculator via scripts/waves/tools/run_capped.sh -- env DYLD_LIBRARY_PATH=<tmp> AZ_BACKEND=headless <app>
  (SIP strips DYLD_* from /bin/bash, so set it through `env`), expect exit 134 + both hashes in the log.
- Then the ChartKind_isBar macro clash (lang_c.rs), RED test in doc crate.
- Gap 3: MSVC C `.CRT$XCU` entry (RED c_items test, GREEN).
- Report scripts/ABI8_2026_10_03.md.

## Decisions
- Do not add `AzAbi_getHash` to api.json (LIFECYCLE: it would change the hash it reports).
