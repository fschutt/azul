# AZMEET11 progress (relay E2E phase, relay-only transport, AZMEET_* as flags, waiting room)

Worktree: .claude/worktrees/agent-a26f308a0a897f39d (branch worktree-agent-a26f308a0a897f39d),
started from fix/input-bugs-2026-09-19 @ 86c0e821e. No cargo (the lead builds).

## DONE
- (none yet)

## IN PROGRESS
- C1 RED: `IrohConfig.relay_only` + `with_relay_only` (dll/src/desktop/extra/iroh/types.rs), the
  engine ignores it; RED tests in engine.rs.

## NEXT
- C2 engine honours relay_only (`Builder::clear_ip_transports`, no `bind_addr`; refused without a relay).
- C3 AzMeet: every AZMEET_* variable a `--flag` (args.rs table, env as fallback), `--relay-only`.
- C4 AzMeet relay-only endpoint (needs autofix of IrohConfig.relay_only + with_relay_only).
- C5 waiting room: `--screen waiting` without a server, `--shot` / `--size`, who is here, pattern preview.
- C6 scripts/iroh_relay_dev.py + relay phase in scripts/azmeet_e2e.py (flags instead of env).
- C7 two-clients.mjs, three-clients.mjs, meet-e2e.mjs, azmeet_cpu.py, fb1 probe onto the flags.

## Facts found
- iroh-relay 1.2.0: bin `iroh-relay` needs feature `server`; `--dev` = plain HTTP on [::]:3340, metrics on
  <http ip>:9090 (any path, OpenMetrics text: relayserver_bytes_recv_total, ..._bytes_sent_total,
  ..._accepts_total); a TOML `--config-path` sets `http_bind_addr` / `metrics_bind_addr`; `/healthz` answers.
- iroh 1.2.0: `Builder::clear_ip_transports()` removes the UDP sockets: relay-only (net_report skips QAD
  without IP transports). `Endpoint::bound_sockets()` lists the IP sockets (empty when relay-only).

## Open questions
- (none)
