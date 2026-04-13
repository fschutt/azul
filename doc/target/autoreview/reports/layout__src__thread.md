# Review: layout/src/thread.rs

## Summary
- Lines: 1002
- Public functions: 4 (standalone) + numerous methods
- Public structs/enums: 15 (12 structs, 3 enums)
- Public type aliases: 7
- Findings: 4 high, 1 medium, 1 low

## Findings

### [HIGH] Dead Code — `thread_sleep_ms`, `thread_sleep_us`, `thread_sleep_ns` have zero call sites
- **Location**: `thread.rs:958`, `thread.rs:975`, `thread.rs:992`
- **Details**: All three sleep utility functions are never called anywhere in the codebase outside their own file.
- **Evidence**: `Grep pattern="thread_sleep_ms|thread_sleep_us|thread_sleep_ns" glob="*.rs"` returned only `layout/src/thread.rs`.
- **Recommendation**: Remove or wire these into the public API if they are intended for FFI consumers.

### [HIGH] Dead Code — `ThreadDestructorCallback`, `ThreadDestructorCallbackType` only used internally
- **Location**: `thread.rs:484-505`
- **Details**: `ThreadDestructorCallback` and `ThreadDestructorCallbackType` are `pub` but only referenced within `thread.rs` itself.
- **Evidence**: `Grep pattern="ThreadDestructorCallback\b" glob="*.rs"` returned only `layout/src/thread.rs`.
- **Recommendation**: Make these `pub(crate)` or remove if not needed for FFI.

### [HIGH] Dead Code — `LibraryReceiveThreadMsgCallback` and its type alias only used internally
- **Location**: `thread.rs:460-482`
- **Details**: `LibraryReceiveThreadMsgCallback` and `LibraryReceiveThreadMsgCallbackType` are `pub` but only used within `thread.rs`.
- **Evidence**: `Grep pattern="LibraryReceiveThreadMsgCallback" glob="*.rs"` returned only `layout/src/thread.rs`.
- **Recommendation**: Make `pub(crate)` or remove.

### [HIGH] Dead Code — `ThreadSendCallback`, `ThreadSenderDestructorCallback` and their type aliases only used internally
- **Location**: `thread.rs:215-296`
- **Details**: Both structs and their `*Type` aliases are `pub` but only referenced within `thread.rs`.
- **Evidence**: `Grep pattern="ThreadSendCallback\b" glob="*.rs"` and `Grep pattern="ThreadSenderDestructorCallback" glob="*.rs"` each returned only `layout/src/thread.rs`.
- **Recommendation**: Make `pub(crate)` or remove.

### [MEDIUM] Suspicious Drop — `run_destructor` flag set but never checked
- **Location**: `thread.rs:114-118` (`ThreadSender::drop`), `thread.rs:527-530` (`Thread::drop`)
- **Details**: Both `Drop` impls set `self.run_destructor = false` but never check the flag. In contrast, `core/src/refany.rs:188` actually checks `if !self.run_destructor`. The flag on `Thread`/`ThreadSender` appears vestigial — it is never read by any code. If it is intended for FFI consumers to check before calling a destructor, this should be documented.
- **Recommendation**: Either add the check or document the FFI contract clearly. If the flag truly is unused, remove it.

### [LOW] Module doc is adequate but could mention `create_thread_libstd` and sleep utilities
- **Location**: `thread.rs:1-4`
- **Details**: The module doc mentions "thread-related callback structures" but doesn't mention the sleep utilities or `create_thread_libstd` as entry points.
- **Recommendation**: Add a brief mention of key entry points.

## System Documentation
- System identified: yes — threading / background task system
- Existing doc: none (no `doc/guide/threading.md` or `doc/guide/tasks.md`)
- Doc needed: A guide covering how background threads are created (`Thread::create` / `create_thread_libstd`), the message-passing model (`ThreadSender` / `ThreadReceiver`), writeback callbacks, and how threads integrate with the event loop via `CallbackInfo::add_thread`. The `lifecycle.md` guide may touch on this but a dedicated threading guide would help.
