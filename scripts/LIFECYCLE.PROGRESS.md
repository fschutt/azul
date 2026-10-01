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

## IN PROGRESS
- 3b. RenderImageCallback whose inputs did not change must not request a
  frame (AzReview: new ImageRef every frame in prepare_frame_content).
  Last commit: b53b691f8. NEXT STEP: find prepare_frame_content /
  RenderImageCallback invocation in layout/dll, see why each frame mints a
  new ImageRef, write RED test, fix.

## NEXT
- 3c. debug-server poll event-driven + PNG encode off the UI thread
- 3d. monitor change re-reads the frame interval
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

- Thread ownership: only threads added by a node's OWN lifecycle callbacks
  (AfterMount / NodeResized / Updated) belong to the node; click / timer /
  write-back / BeforeUnmount threads stay the app's. Orphans are signalled
  at the unmount (remap_node_ids), retired by run_all_threads once
  finished (or detached after 2 s), never joined on the UI thread.

## Open questions
