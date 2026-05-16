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

/* ── az_remill_compile_to_wasm32_obj ─────────────────────────────
 * Parse LLVM IR text, run opt -O2, then llc -mtriple=wasm32 -O2
 * -filetype=obj. Output is a wasm32 ELF-shaped object (the
 * intermediate `.o` that wasm-ld consumes).
 *
 * `ir_str` is NOT freed; copy if you need a long-lived reference.
 */
int az_remill_compile_to_wasm32_obj(const char *ir_str,
                                    size_t ir_len,
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
