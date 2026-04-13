# Review: layout/src/timer.rs

## Summary
- Lines: 626
- Public functions: ~34 (Timer: 9, TimerCallbackInfo: ~25 delegated + own)
- Public structs/enums: 4 (TimerCallback, Timer, TimerCallbackInfo, OptionTimer)
- Public type aliases: 1 (TimerCallbackType)
- Findings: 0 high, 3 medium, 0 low

## Findings

### [MEDIUM] Dead code — `OptionTimer` enum has no external callers
- **Location**: `timer.rs:666-689`
- **Details**: `OptionTimer` is only used internally in its own `impl` blocks. External grep shows it only in `doc/src/codegen/v2/lang_python.rs` (codegen string references), not actual usage.
- **Evidence**: `grep 'OptionTimer::' *.rs` → only hits in `timer.rs` itself and python codegen strings.
- **Recommendation**: Remove if not needed for FFI compatibility, or add `#[allow(dead_code)]` with a comment explaining it's for FFI.

### [MEDIUM] Dead code — `get_attached_node_size` / `get_attached_node_position` have no external callers
- **Location**: `timer.rs:305-313`
- **Details**: These two methods on `TimerCallbackInfo` are defined but never called outside this file.
- **Evidence**: `grep 'get_attached_node_size\|get_attached_node_position' *.rs` → only `timer.rs:305` and `timer.rs:310`.
- **Recommendation**: Keep if part of the public API contract for FFI users; otherwise remove.

### [MEDIUM] Magic number — `tick_millis` returns hardcoded `10`
- **Location**: `timer.rs:143`
- **Details**: When no interval is set, `tick_millis()` returns the magic number `10` (milliseconds). This is a default tick rate used by platform shells (Windows, macOS, Linux) to set OS timer intervals. It should be a named constant.
- **Recommendation**: `const DEFAULT_TIMER_TICK_MS: u64 = 10;`

## System Documentation
- System identified: yes — Timer / event-loop system
- Existing doc: `doc/guide/lifecycle.md` covers the event loop lifecycle broadly
- Doc needed: No dedicated timer system guide exists. A `doc/guide/timers.md` explaining timer creation, tick scheduling, delay/interval/timeout semantics, and how timers interact with the platform event loops would be valuable.
