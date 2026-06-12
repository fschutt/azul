# mechb_harness — native-aarch64 executor for remill-lifted functions

Built 2026-06-12 to root-cause web-lift "mechanism B" (the class-B gate:
`<[&str]>::join` returning a String whose len held a heap pointer).

## What it proved

1. `join_generic_copy h9c9d2f7abfe94f50` (bytes at dylib offset 0xb3dd90,
   `join_bytes.hex`, disasm in `join_h9c9d.disasm`), lifted standalone by the
   fork's `remill-lift-17` and executed NATIVELY on the M-series host via this
   harness, is **CORRECT**: input `["5"].join(" ")` → `{cap=1, ptr=heap, len=1}`,
   full memory-op trace clean. The lift was never broken.
2. The actual root cause was `classify_for_name` (dll/src/web/symbol_table.rs):
   crate `alloc` defaulted to `FnClass::Leaf` → join was NEVER LIFTED — its
   call sites hit a no-op env-import stub, so the caller read 24 bytes of stale
   stack garbage as the result Vec (`{ptr=1, len=<heap ptr>}` = leftovers, not
   a transposition). Fixed by defaulting `alloc` + `core` to `Recursable`.

## Reuse for the next suspected mis-lift

```sh
# 1. bytes of the fn (file offset == vmaddr for this dylib's __TEXT):
dd if=target/aarch64-apple-darwin/release/libazul.dylib bs=1 \
   skip=$((0x<addr>)) count=$((0x<size>)) | xxd -p | tr -d '\n' > bytes.hex
# 2. control lift:
third_party/remill-install/build/remill/bin/lift/remill-lift-17 \
   --arch aarch64 --address 0x<addr> --bytes "$(cat bytes.hex)" --ir_out fn.ll
# 3. regenerate probe.ll if remill's State layout changed (see probe.ll: it
#    re-declares the %struct.State type lines from fn.ll + off_* getters).
# 4. adapt harness.cpp: entry symbol, X0..X7/X8 inputs, callee stubs
#    (each `declare ptr @sub_<hex>` in fn.ll needs a stub; dotted duplicates
#    via __asm__("_sub_<hex>.N")), then:
clang++ -isysroot "$(xcrun --show-sdk-path)" -Wno-deprecated-declarations \
   -O1 harness.cpp fn.ll probe.ll -o repro && ./repro
```

The harness logs every `__remill_*memory*` op with symbolic region names
(SLICE/P/SEP/RET/STK/HEAP) — one run = a complete value-flow trace, which is
what static IR reading and isolated-instruction lifts could never give.
