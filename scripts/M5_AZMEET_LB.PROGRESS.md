# M5 AzMeet load balancer: progress

Branch `wt/m5-azmeet-lb` from `748f999af`. Task: rooms of 3+ through `IrohLoadBalancer` (capacity reports,
forwarding over a backbone, simulcast renditions from tile roles, three-clients.mjs, network panel).

## DONE

- `2647d0cca` test(iroh): a room above its mesh cap forwards through its strongest peers (RED)
- `8fc8f78af` feat(iroh): IrohLoadBalancer::set_mesh_cap moves the everyone-forwards threshold

## IN PROGRESS

- `examples/azul-meet/src/routes.rs` (pure: plan tree, assignments, sync + relay wire, capacity estimate) RED

## NEXT

1. routes.rs GREEN; video_wire.rs header + controls carry the rendition height (RED, GREEN).
2. three-clients.mjs (RED), two-clients.mjs regexes for the rendition in the lines.
3. lib.rs glue: syncs, plan, forwarding, renditions, network panel (GREEN).
4. Guide section, report `scripts/M5_AZMEET_LB_2026_09_29.md`.

## Open questions / decisions

- The library's backbone rule let everyone forward up to 8 people, so a cap below 8 needed a library knob:
  `IrohLoadBalancer::set_mesh_cap` (api.json addition, listed in the report). No app-side twin of the rule.
