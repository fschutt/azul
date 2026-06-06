/*
 * azul-remill — C ABI wrapper around remill + LLVM + LLD that
 * replaces the per-fn `remill-lift-17` + `opt` + `llc` + `wasm-ld`
 * subprocess chain with in-process library calls. Eliminates the
 * requirement that the running binary find external LLVM tools at
 * load time — everything is statically linked into libazul.dylib.
 *
 * All output buffers are heap-allocated; caller must release with
 * az_remill_free / az_remill_free_buf.
 *
 * Thread safety: NOT reentrant. LLVM's global TargetRegistry +
 * LLD's static state mean only one pipeline can run at a time.
 * Callers serialize via Mutex.
 */
#ifndef AZUL_REMILL_H
#define AZUL_REMILL_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ── az_remill_lift ──────────────────────────────────────────────
 * Lift native code bytes (at virtual address `address`) into LLVM
 * IR text. Mirrors `remill-lift-17 --arch ARCH --os OS --address ADDR
 * --bytes HEX --ir_out PATH` but with in-memory output.
 */
int az_remill_lift(const char *arch_name,
                   const char *os_name,
                   uint64_t address,
                   const uint8_t *bytes,
                   size_t bytes_len,
                   char **ir_out,
                   size_t *ir_len_out,
                   char **err_out);

/* ── az_remill_lift_batch ────────────────────────────────────────
 * Lift N functions in one call, sharing one `LoadArchSemantics`
 * (~30 ms) and one `TraceLifter` across every item. Output is N
 * separate IR strings — one per item — each with the lifted body
 * for that item's entry in its own fresh `lifted_<i>` module.
 * Cross-item bl targets become extern declarations resolved at
 * wasm-ld time; intra-item references stay as direct calls.
 *
 * Per-fn cost drops from ~50 ms (single-shot via `az_remill_lift`)
 * to ~5 ms once `LoadArchSemantics` is amortized over the batch.
 *
 * Output `ir_outs[i]` (length `ir_lens_out[i]`) for `i in [0,
 * item_count)`. The caller must free each `ir_outs[i]` via
 * `az_remill_free` and `ir_outs` itself via `az_remill_free_buf`
 * (cast as `uint8_t *`).
 *
 * Each item's `bytes` is its function body; `addresses[i]` is the
 * canonical virtual address where the entry is assumed to live.
 * The manager's `LiftMemory` spans every item's byte range so the
 * lifter can find executable bytes when one item's `bl` lands on
 * another item's entry; bl targets that fall in the GAPS between
 * items still resolve as extern declarations.
 */
int az_remill_lift_batch(const char *arch_name,
                         const char *os_name,
                         const uint64_t *addresses,
                         const uint8_t *const *bytes_ptrs,
                         const size_t *bytes_lens,
                         size_t item_count,
                         const char *extra_data,
                         char ***ir_outs,
                         size_t **ir_lens_out,
                         char **err_out);

/* ── az_remill_compile_to_wasm32_obj ─────────────────────────────
 * Parse one or more LLVM IR text inputs into separate Modules, link
 * them together via llvm::Linker (handles cross-module type / global
 * / linkonce_odr conflicts properly — text concatenation can't),
 * then run opt -O2 followed by llc -mtriple=wasm32 -O2 -filetype=obj.
 * Output is a wasm32 ELF-shaped object (the intermediate `.o` that
 * wasm-ld consumes).
 *
 * `ir_strs[i]` of length `ir_lens[i]` for `i in [0, ir_count)`. The
 * first module is the "destination" and subsequent modules are
 * linked into it; this matches the semantics of `llvm-link a.ll b.ll`.
 *
 * `ir_strs` is NOT freed; copy if you need long-lived references.
 */
int az_remill_compile_to_wasm32_obj(const char *const *ir_strs,
                                    const size_t *ir_lens,
                                    size_t ir_count,
                                    uint8_t **obj_out,
                                    size_t *obj_len_out,
                                    char **err_out);

/* ── az_remill_wasm_link ─────────────────────────────────────────
 * Link a batch of wasm32 .o objects into a final .wasm via lld's
 * wasm driver. Equivalent to:
 *
 *   wasm-ld --no-entry --allow-undefined
 *     [--import-memory] [--import-table]
 *     --initial-memory=<bytes>
 *     --export=<name> ...
 *     <obj1> <obj2> ... -o <out.wasm>
 *
 * Objects are passed as in-memory byte buffers; the linker writes
 * each to a temp file before invoking lld (lld's API takes file
 * paths, not memory). Output wasm is read back into a heap buffer.
 *
 * `objs`        — array of pointers to obj byte buffers
 * `obj_lens`    — array of obj sizes (same length as `objs`)
 * `obj_count`   — number of objs
 * `exports`     — array of C strings to emit as `--export=NAME`
 * `export_count`
 * `import_memory` — emit `--import-memory` (per-cb wasms share mini's heap)
 * `import_table`  — emit `--import-table` (per-cb wasms share fn table)
 * `initial_memory_bytes` — `--initial-memory=N` (set to 0 to omit)
 */
int az_remill_wasm_link(const uint8_t *const *objs,
                        const size_t *obj_lens,
                        size_t obj_count,
                        const char *const *exports,
                        size_t export_count,
                        int import_memory,
                        int import_table,
                        uint32_t initial_memory_bytes,
                        uint8_t **wasm_out,
                        size_t *wasm_len_out,
                        char **err_out);

/* Release strings/buffers returned by the above. Safe to call with NULL. */
void az_remill_free(char *ptr);
void az_remill_free_buf(uint8_t *ptr);

#ifdef __cplusplus
}
#endif

#endif /* AZUL_REMILL_H */
