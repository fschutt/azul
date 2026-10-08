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

## IN PROGRESS
- inventory (grep both repos; three read-only explore helpers collect rows)

## NEXT
- docs/HARDCODED.md (azul), iso/docs/HARDCODED.md (azul-apps)
- fixes, each RED test commit then fix commit
- local profile: azul-apps local/azlin-config.local.json + local/azlin-local.env, GETTING-STARTED.md

## Open questions
- AZCLOUD15 owns the `endpoints` section of examples/azul-appkit/src/azlin_config.rs; this task
  reads the shared config through a separate module and must be re-pointed at its typed accessor
  at integration.
