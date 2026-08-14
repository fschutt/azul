//go:build unix

package main

// Why this file exists
// ────────────────────
// libazul's C structs carry Rust `NonNull::dangling()` sentinels — small
// non-null values like 0x8 — in the pointer fields of empty Vecs. That is
// correct on the Rust side (a dangling-but-aligned pointer is what an empty
// Vec holds), and harmless to C, which never dereferences them.
//
// It is not harmless to Go. `AzApp_run(&app, window)` puts a by-value
// AzWindowCreateOptions on the goroutine stack, and when that stack later
// grows, the runtime walks every slot it believes is a pointer and aborts on
// anything that is neither nil nor a plausible address:
//
//	runtime.adjustpointers → runtime.adjustframe → runtime.copystack
//
// which is precisely the macOS arm64 crash the AZ_E2E job hit. `invalidptr=0`
// is the mitigation the Go runtime documentation names for this exact case
// ("only useful if you are using cgo and have pointers to C memory on the
// stack"). It cannot be set with a `//go:debug` directive, so the process
// re-execs itself once with it — the same guard `hello-world-idiomatic`
// carries, kept in its own unix-tagged file here so the Windows build (which
// has no `syscall.Exec` and does not hit the crash) is untouched.
//
// Launch with GODEBUG=invalidptr=0 already set and this is a no-op.

import (
	"os"
	"strings"
	"syscall"
)

func init() {
	if strings.Contains(os.Getenv("GODEBUG"), "invalidptr=0") {
		return
	}
	exe, err := os.Executable()
	if err != nil {
		return // Can't re-exec; fall through and hope the stack never grows.
	}
	god := os.Getenv("GODEBUG")
	if god != "" {
		god += ","
	}
	god += "invalidptr=0"

	env := make([]string, 0, len(os.Environ())+1)
	for _, kv := range os.Environ() {
		if !strings.HasPrefix(kv, "GODEBUG=") {
			env = append(env, kv)
		}
	}
	env = append(env, "GODEBUG="+god)

	// Replaces this process on success, so nothing below runs. On failure we
	// continue unguarded, which is exactly today's behaviour.
	_ = syscall.Exec(exe, os.Args, env)
}
