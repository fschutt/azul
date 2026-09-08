# JSON state hydration on the web backend

Status: design note · Scope: make non-POD app state cross the server→client
boundary correctly, which is what currently blocks AzWriter from booting.

## The failure

AzWriter boots to `AzStartup_init → state ptr` and then traps with
`memory access out of bounds` on the first store through a model field.

The `<script id="az-hydrate">` payload carries three things:

```json
{ "type_id": "15986894982550938563",
  "json": { "export_path": "C:\\…\\azul-doc-export.pdf", "exported": false },
  "size": 32,
  "bytes": "3500000000000000d0b275a0e5010000…" }
```

`loader_js.rs` (hydrate path) always restores the **raw byte image** when
`bytes` is present. For hello-world's `{ counter: u32 }` that is exactly
right. For `DocState { export_path: String, exported: bool }` it is not:
`d0b275a0e5010000` is the host heap pointer `0x1E5A075B2D0`, meaningless in
the guest, and the first deref walks outside linear memory.

Raw bytes are only valid for plain-old-data models. The presence of a
`json` OBJECT means the app registered reflection (`set_serialize_fn` /
`set_deserialize_fn`), i.e. the state is meant to travel as JSON.

## Why the JSON path never ran

The mechanism is half-built:

- `EventloopState::state_deserializer` exists;
  `AzStartup_registerStateDeserializer(state, fn_addr)` sets it
  (`eventloop.rs`), and `AzStartup_init`'s doc already describes
  "deserializer if registered, else raw-bytes fallback".
- Nothing reads `state_deserializer`, and `loader_js.rs` never calls the
  setter — the export exists but is dead.
- `AzStartup_hydrate` builds the `RefAny` by copying `data_size` bytes and
  explicitly zeroes `serialize_fn` / `deserialize_fn` on the new
  `RefCountInner`.
- The app's `main()` never runs in wasm, so `data.set_deserialize_fn(…)`
  (azul-writer `main.rs`) has no effect client-side. The address is known
  only on the server.

## The four pieces

1. **Server** — include the model's `deserialize_fn` in the az-hydrate
   payload, translated to a SYNTH address (the same translation mirrored
   data gets). Emit it only when reflection is actually registered.
2. **Loader** — when the payload has both a `json` object and a
   `deserialize_fn`: copy the JSON **text** into guest memory, call
   `AzStartup_registerStateDeserializer(state, fnAddr)`, then the new
   `AzStartup_hydrateJson(typeIdLo, typeIdHi, jsonPtr, jsonLen)`. Keep the
   raw-bytes path only for payloads without reflection, and make the
   choice explicit in the console line so a mis-hydrate is visible.
3. **eventloop** — implement `AzStartup_hydrateJson`: build an `AzString`
   over the guest JSON bytes, invoke the registered deserializer through
   the existing indirect-call bridge (the same route callbacks take), and
   keep the returned `RefAny` as the app state. On a missing/failing
   deserializer, return 0 loudly rather than falling back to raw bytes.
4. **Lift coverage** — the deserializer is reached only through a
   function pointer, so the transitive walk must be seeded with it, the
   way discovered callback addresses already are. Without that its body is
   never lifted and the indirect call lands on an unlifted target.

## Notes

- This is the same reflection bridge the boundary-API redesign relies on
  (`doc/web-boundary-apis-plan.md` §4.1: RefAnys cross the JS boundary via
  serialize/deserialize), so the work is shared rather than throwaway.
- Test hook: `tests/e2e/azwriter_boot.json` already asserts the document
  view renders; it becomes meaningful the moment hydration is correct.
