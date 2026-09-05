# Recorder `0x40048` is a panic-reason register

A `NeverLift` symbol compiles to `store volatile <synth>, 0x40048; unreachable`.
Reaching one always ends the boot, and until now each discovery cost a relift to
identify. Enumerating the whole set shows why that was avoidable: **26 of the 29
NeverLift symbols are panic/abort entry points**, so `0x40048` does not name a
missing feature — it names *which panic the guest hit*, which points straight at
the real bug.

`std::thread::local::panic_access_error` (see `web-windows-tls.md`) was found
the slow way. With this table it would have been one lookup.

## ⚠ The addresses are build-specific

Synth addresses are assigned per build, and **AzWriter lifts itself**, so any
edit under `dll/src/web/` relinks the image and moves all of them (see
`web-windows-tls.md`). The table below is from one run; the *names* and their
relative weights persist, the *numbers* do not. Regenerate against the run you
are debugging:

```
python C:\rb\neverlift_reach.py <that run's scratch>   # ranks by caller count
python C:\rb\neverlift_names.py <that run's server.log>  # decodes to symbols
```

Read the names out of the log's own `dep: sub_<synth> → resolved=<name>@<native>`
lines, **not** `name-synth.py`, which resolves through the exe on disk — and the
exe is rebuilt between runs while a saved log is not.

## The set

"callers" is the number of lifted bodies that can reach the stub, which is a
decent proxy for how likely you are to land on it.

| synth | callers | symbol |
|---|---|---|
| `0xf05b70` | 1815 | `core::option::unwrap_failed` |
| `0xf06250` | 1758 | `core::panicking::panic_bounds_check` |
| `0xf05a0e` | 1365 | `alloc::alloc::handle_alloc_error` |
| `0xf05e50` | 1023 | `core::slice::index::slice_end_index_len_fail` |
| `0xf06210` | 705 | `core::panicking::panic` |
| `0xf05b90` | 654 | `core::option::expect_failed` |
| `0xf05e60` | 429 | `core::slice::index::slice_index_order_fail` |
| `0xf061f0` | 363 | `core::panicking::panic_fmt` |
| `0xf05e40` | 309 | `core::slice::index::slice_start_index_len_fail` |
| `0xea1080` | 207 | `hashbrown::raw::Fallibility::capacity_overflow` |
| `0xf06150` | 180 | `core::result::unwrap_failed` |
| `0xf049c0` | 69 | `std::thread::local::panic_access_error` |
| `0xf05b50` | 60 | `core::cell::panic_already_borrowed` |
| `0xf05d80` | 36 | `panic_const_div_by_zero` |
| `0xf062ec` | 18 | `core::panicking::assert_failed<usize,usize>` |
| `0xf05dc0` | 12 | `panic_const_rem_by_zero` |
| `0xf05d00` | 6 | `panic_const_div_overflow` |
| `0xefffc7` | 3 | `core::panicking::assert_failed<u8,u8>` |
| `0xf062b1` | 3 | `core::panicking::assert_failed<u64,u64>` |
| `0xf058c0` | 3 | `alloc::raw_vec::capacity_overflow` |
| `0xbfac40` | 3 | `hashbrown::raw::Fallibility::capacity_overflow` |

### The three that are not panics

| synth | callers | symbol | why it is NeverLift |
|---|---|---|---|
| `0xbface0` / `0xbfacc0` | 3 | `tfd::FileDialog::open_file` / `save_file` | native file dialogs; no wasm equivalent |
| `0xceb130` / `0xcebf10` | 3 | `gl_context_loader::…::delete_textures` / `delete_program` | native GL teardown |
| `0x6db9f0` | 3 | `brotli_decompressor::state::…::drop` | — |

These are the only entries where hitting the stub means "this genuinely cannot
run here" rather than "the guest panicked". Three more (`0x87fc10`, `0xaec100`,
`0xd50fd0`) had no `resolved=` line in the log sampled.

## How to use it

1. Boot traps. Read `0x40048` — `scripts/m9_e2e/tls-probe.js` prints it, and
   `state-regs.js` does **not**.
2. Check `0x40158` first. It is the unmatched-dispatch count, and a nonzero
   `0x40048` means something different when dispatch also failed: the
   `AZ_UNK_TRAP` path writes the *same* recorder with a PC, not a symbol.
3. Decode the symbol. A panic name tells you the *shape* of the bug —
   `panic_bounds_check` is an index computed wrong, `unwrap_failed` is a `None`
   where the guest expected `Some`, `panic_access_error` is thread-local access.
4. Only then go looking for the code that produced it.
