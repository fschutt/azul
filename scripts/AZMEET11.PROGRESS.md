# AZMEET11 progress (relay E2E phase, relay-only transport, AZMEET_* as flags, waiting room)

Worktree: .claude/worktrees/agent-a26f308a0a897f39d (branch worktree-agent-a26f308a0a897f39d),
started from fix/input-bugs-2026-09-19 @ 86c0e821e. No cargo (the lead builds).

## DONE
- a64cb937b C1 RED: IrohConfig.relay_only + with_relay_only, RED tests in engine.rs.
- 6b96e5201 C2 GREEN: engine clears the IP transports for relay_only; refused without a relay.
- C3 AzMeet: every AZMEET_* variable a `--flag` (args.rs SWITCHES table, env as fallback, the
  switch wins; `--worker` also over the saved server via rooms::server_choice), `--relay-only` parsed.

- C4 AzMeet `--relay-only` endpoint (IrohConfig::with_relay_only - NEEDS the api.json autofix first),
  stdout `AZMEET_TRANSPORT <label>` at bind, `AZMEET_PATH <name> direct|relayed` when a path turns,
  statistics Network section starts with "Transport: ...".

## IN PROGRESS
- C5 waiting room.

## NEXT
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
