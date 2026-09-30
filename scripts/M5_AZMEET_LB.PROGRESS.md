# M5 AzMeet load balancer: progress

Branch `wt/m5-azmeet-lb` from `748f999af`. Task: rooms of 3+ through `IrohLoadBalancer` (capacity reports,
forwarding over a backbone, simulcast renditions from tile roles, three-clients.mjs, network panel).
Resumed once after a power loss (the scratchpad was wiped; the worktree was intact).

## DONE

- `2647d0cca` test(iroh): a room above its mesh cap forwards through its strongest peers (RED)
- `8fc8f78af` feat(iroh): IrohLoadBalancer::set_mesh_cap moves the everyone-forwards threshold
- `af1e06372` test(azmeet): who forwards whose media, which rendition each viewer gets, and the reports behind it (RED)
- `9f914b564` feat(azmeet): routing plan, rendition assignment, sync and relay wire, uplink estimate
- `cadcfc4e8` test(azmeet): three AzMeet processes route over a backbone and a far keyframe request reaches its origin (RED)

## IN PROGRESS

- lib.rs glue (GREEN). Line formats the scripts read are fixed by meet-e2e.mjs (readers checked against
  sample lines: scratchpad m5/readers_check.mjs).

## NEXT

1. lib.rs glue (GREEN): peer keys, syncs on connect / change / every tick, the plan via IrohLoadBalancer
   (`set_mesh_cap`), sending own media to the plan's children, forwarding (relay envelopes, relay windows,
   keyframe requests passed upstream), renditions per tile role (grid / speaker view, measured tile height),
   one encoder per rendition, camera consumers per rendition, the network panel column, ALPN azmeet/3.
2. Type-check harness for lib.rs (the M3 harness was lost with the scratchpad: rebuild it from the generated
   signatures in /Users/fschutt/Development/azul/target/codegen/dll_api_external.rs).
3. Guide section, report `scripts/M5_AZMEET_LB_2026_09_29.md`.

## Open questions / decisions

- The library's backbone rule let everyone forward up to 8 people, so a cap below 8 needed a library knob:
  `IrohLoadBalancer::set_mesh_cap` (api.json addition, listed in the report). No app-side twin of the rule.
- Pure modules are checked with `rustc --emit=metadata` only (no test binaries).
