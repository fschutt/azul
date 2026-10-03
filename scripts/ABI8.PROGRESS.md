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

## DONE (cont.)
- 8d7ef1110 lang_c.rs: no variant-checker macro shadows an api function (AzChartKind_isBar broke azul17.hpp);
  RED = existing bug_classes::azul_h_never_emits_one_name_as_macro_and_function_or_with_two_linkages
- ff19facfb..f96663e16 scripts/abi_guard_e2e.py (C, C++, Rust app with patched libazul). Ran on the prebuilt
  binaries: c 2/2 PASS, rust 2/2 PASS (AzCalculator aborts before its first line with both hashes),
  cpp FAILS on today's azul17.hpp (the macro clash, fixed by 8d7ef1110) and PASSES on a copy of azul.h
  without the macro (/tmp/abi8/c/inc).

- 2c583d92f RED gap 3 / 22c858698 GREEN gap 3 (azul.h `#elif defined(_MSC_VER)` arm: .CRT$XCU selectany entry
  + /include). Checked with clang --target=x86_64/i686-pc-windows-msvc -fms-extensions (object has the entry and
  the /include directive); cl.exe unverified.
- Simulated this branch's azul.h (/tmp/abi8/simulate_header.py -> /tmp/abi8/sim): e2e c 2/2, cpp PASS on
  azul03/11/14/17/20/23.hpp.

## IN PROGRESS
- CI wiring of scripts/abi_guard_e2e.py.

## NEXT
- CI: add `python3 scripts/abi_guard_e2e.py --only c,cpp` where target/codegen exists (rust.yml), and
  `--only rust` in the dll_tests job if it has a built app (check).
- Report scripts/ABI8_2026_10_03.md.

## Decisions
- Do not add `AzAbi_getHash` to api.json (LIFECYCLE: it would change the hash it reports).
