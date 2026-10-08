# HARDCODE15 progress

Task: audit azul + azul-apps for hard-coded hosts / ports / paths / buckets / keys / timeouts, make
them configurable (shared Azlin config -> env -> flags; server side: ctl.toml / node.toml / wrangler
vars / env), a `local` profile for the fully local stack, docs/HARDCODED.md in both repos.

Worktrees:
- azul: this worktree (branch worktree-agent-a499dc27172d5847d, fast-forwarded to 5d78255a4)
- azul-apps: /Users/fschutt/Development/azul-apps-wt/hardcode15 (branch local/hardcode15)

No compiling here (house rule): the lead builds and runs the tests.

## DONE
azul-apps (local/hardcode15):
- bfb9180 RED / b598161 fix: `secrets push token-server` ran wrangler in iso/workers/token (gone);
  now iso/azworker/token (+ .gitignore, READMEs, wrangler comments)
- af5b954 RED / d268677 fix: `azctl token --base-url` (default http://<listen>) and
  `--issuer-year`; `dev up` passes its token URL (pay_url / account_url were empty natively)
- 478864f RED / 3a26f9e fix: CtlConfig::domain - AZLIN_DOMAIN > envs.<env>.domain > live
  s3.azlin.io / else s3-trial.azlin.io, for node provision, overflow up, lb sync, dns rdns, init
- d4ec1fb RED / 65f6a43 fix: AZLIN_PROVIDER_API=<base> sends Hetzner/Cloudflare calls to a
  stand-in server (<base>/<fixture folder><path>), also for --env dev
- dee8de5 RED / 3d32112 fix / 1824fc3: every wrangler [env.*] names its vars (live had none),
  [env.local] for token + watcher, .dev.vars ignored; ISSUER_KEYS from `azctl token keys`
- 4ba3759 RED / c43960e fix: node.toml `ntp = ["off"]` = no NTP (was: empty = Hetzner, 90 s
  boot wait offline); dev VMs write it
- a36c4a7 RED / 16c84e5 fix: ctl.toml envs.<env>.dns / .ntp for node provision
azul (this worktree):
- 372f1f238, 870c3532b: appkit shared_endpoint (endpoints.<name> of the shared config; stand-in
  for AZCLOUD15's typed section)
- 96582dc0d RED / 404dbbdc6 fix: AzCalendar --worker + endpoints.meet in the meeting server order
- d77ee39f8 RED / a68c5afc6 fix: AzMaps --tiles / AZMAPS_TILES / endpoints.tiles

Coordinator (2026-10-08): stay out of ctl/build.rs, image/build.sh, image/Dockerfile,
init/upgrade.rs, token_native.rs (I had already committed af5b954/d268677 there - reported),
the Cmd::Jwt arm of ctl/mod.rs, azworker/token/src/lib.rs; the iroh presets::N0 fix is the lead's.

## IN PROGRESS
- iso/docs/HARDCODED.md from the two azul-apps inventories (done: azctl, azinit/proto/token/
  workers); azul inventory helper still running

## NEXT
- docs/HARDCODED.md (azul), iso/docs/HARDCODED.md (azul-apps)
- fixes, each RED test commit then fix commit
- local profile: azul-apps local/azlin-config.local.json + local/azlin-local.env, GETTING-STARTED.md

## Open questions
- AZCLOUD15 owns the `endpoints` section of examples/azul-appkit/src/azlin_config.rs; this task
  reads the shared config through a separate module and must be re-pointed at its typed accessor
  at integration.
