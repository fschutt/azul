# LIFECYCLE - progress (branch wt/lifecycle, base 39092feee)

Task brief: ABI guard; threads of removed widgets; idle leftovers (FLIP settle,
RenderImageCallback frames, debug-server poll event-driven + PNG off the UI
thread, monitor change re-reads the frame interval); dark "Dark" segment.

## DONE
- 1. ABI guard: RED 9d8d69e45, GREEN 3ba642c65
- 2. threads of unmounted nodes: RED e379b2fb4, GREEN 6c3ffd0da
- 3a. FLIP settle: RED ee3c8f0fc, GREEN cead23ffb (energy criterion in
  SpringCurve::is_settled); RED 147c624e4, GREEN b53b691f8 (drop Move/Enter
  anims whose node left the tree)
- 3b. RenderImageCallback memo: RED 104fa4da4, GREEN b84a61c82
- e2e manager accounting for thread_owner: ea93eb0bf
- 3c part 1 debug wake: RED 44199da3b, GREEN 91800c36b (layout) +
  52450b5ec (dll loops) + 4a7ee6d5a (headless children guard)
- 3c part 2 PNG off UI thread: refactor d9d76151d, RED 57867bb64,
  GREEN 86fb64407
- 3d. monitor change re-paces running drivers: RED dd959d321, GREEN
  ac42c3b6f (frame_drivers_off_pace + repace_frame_drivers; macOS
  windowDidChangeScreen now detects the monitor)
- 4. Segmented dark pair: RED 2239d3f18, GREEN 75dd9c370 (system:accent-text
  resolves as a readable pair on system:accent; css color.rs)
- report: scripts/LIFECYCLE_2026_10_01.md

## IN PROGRESS
(none)

## NEXT
- nothing: the report scripts/LIFECYCLE_2026_10_01.md is committed. The
  parent compiles, runs the listed tests and the RED pass.

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

- Thread ownership: only threads added by a node's OWN lifecycle callbacks
  (AfterMount / NodeResized / Updated) belong to the node; click / timer /
  write-back / BeforeUnmount threads stay the app's. Orphans are signalled
  at the unmount (remap_node_ids), retired by run_all_threads once
  finished (or detached after 2 s), never joined on the UI thread.

## Open questions
