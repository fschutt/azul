# LIFECYCLE - progress (branch wt/lifecycle, base 39092feee)

Task brief: ABI guard; threads of removed widgets; idle leftovers (FLIP settle,
RenderImageCallback frames, debug-server poll event-driven + PNG off the UI
thread, monitor change re-reads the frame interval); dark "Dark" segment.

## DONE
- 1. ABI guard: RED 9d8d69e45, GREEN 3ba642c65

## IN PROGRESS
- 2. worker threads of unmounted widgets

## NEXT
- 3a-d. idle leftovers
- 4. Segmented dark pair

## Decisions
- ABI hash = FNV-1a 64 over a canonical, sorted, doc-free text of the IR
  (types with field order/types/ref kinds/repr/variants, aliases, callback
  typedefs, every C function signature, constants). fn_body excluded
  (implementation, not ABI).
- Rust binding: every Constructor / StaticMethod wrapper and the
  `From<&str>/From<String> for AzString` impls call `az_abi_check()` (one
  relaxed atomic load after the first call), so the FIRST call into libazul
  is checked, not only App::create (AppConfig::create runs before it).
- C/C++: azul.h carries AZ_ABI_HASH + AzAbi_getHash + AzAbi_check and runs
  the check at load (GCC/Clang constructor; C++ static object for MSVC C++);
  opt-out AZ_NO_ABI_CHECK. Python: the extension IS libazul (same crate),
  no mismatch possible.

## Open questions
