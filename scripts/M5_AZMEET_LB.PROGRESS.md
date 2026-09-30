# M5 AzMeet load balancer: progress

Branch `wt/m5-azmeet-lb` from `748f999af`. Task: rooms of 3+ through `IrohLoadBalancer` (capacity reports,
forwarding over a backbone, simulcast renditions from tile roles, three-clients.mjs, network panel).
Resumed once after a power loss (the scratchpad was wiped; the worktree was intact).
Added mid-task by the coordinator (user request): a meeting-server URL field on the start screen.

## DONE

- `2647d0cca` test(iroh): a room above its mesh cap forwards through its strongest peers (RED)
- `8fc8f78af` feat(iroh): IrohLoadBalancer::set_mesh_cap moves the everyone-forwards threshold
- `af1e06372` test(azmeet): who forwards whose media, which rendition each viewer gets, and the reports behind it (RED)
- `9f914b564` feat(azmeet): routing plan, rendition assignment, sync and relay wire, uplink estimate
- `cadcfc4e8` test(azmeet): three AzMeet processes route over a backbone and a far keyframe request reaches its origin (RED)
- `f7b404f5e` feat(azmeet): rooms of three and more forward over a backbone, each tile gets its rendition

## IN PROGRESS

- Meeting-server field (start screen): prefill saved > AZMEET_WORKER > built-in (pure `rooms::server_prefill`,
  RED unit test first), save on Enter / blur after a `GET /health` answers, status next to the field, demo only
  when nothing is configured and nothing answers; headless runs neither read nor write the settings file.

## NEXT

1. Server field: RED (rooms.rs tests + stubs), GREEN (rooms.rs + lib.rs glue), type-check with the harness.
2. Guide section, report `scripts/M5_AZMEET_LB_2026_09_29.md`.

## Type-check harness (scratchpad m5/, rebuilt after the power loss)

- `azul_stub.rs`: a stub `azul` crate with the generated signatures of every item lib.rs uses
  (`rustc --crate-type lib --crate-name azul --emit=metadata azul_stub.rs -o libazul.rmeta`), then
  `rustc --crate-type lib [--test] --crate-name azmeet --emit=metadata -A improper_ctypes_definitions
  --extern azul=libazul.rmeta examples/azul-meet/src/lib.rs`: the WHOLE lib.rs and its modules, lib and test
  builds, no errors, no warnings.

## Open questions / decisions

- The library's backbone rule let everyone forward up to 8 people, so a cap below 8 needed a library knob:
  `IrohLoadBalancer::set_mesh_cap` (api.json addition, listed in the report). No app-side twin of the rule.
- Pure modules are checked with `rustc --emit=metadata` only (no test binaries).
