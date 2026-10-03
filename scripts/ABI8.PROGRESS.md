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
- (none yet)

## IN PROGRESS
- RED: abi_guard.rs test "every value-less entry point of the rust binding checks the abi".

## NEXT
- GREEN for gap 1 (rust_wrapper_checks covers Default + EnumVariantConstructor; impl Default body).
- Gap 2: Rust load-time check (`#[used]` init-section static), RED then GREEN.
- Gap 3: MSVC C `.CRT$XCU` entry.
- Gap 4: scripts/abi_guard_e2e.py (C / C++ with a stub `AzAbi_getHash`, Rust app with a patched libazul copy).

## Decisions
- Do not add `AzAbi_getHash` to api.json (LIFECYCLE: it would change the hash it reports).
