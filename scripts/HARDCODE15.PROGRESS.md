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
  `--issuer-year`; `dev up` passes its token URL (token_native.rs is now the lead's file)
- 478864f RED / 3a26f9e fix: CtlConfig::domain - AZLIN_DOMAIN > envs.<env>.domain > live
  s3.azlin.io / else s3-trial.azlin.io, for node provision, overflow up, lb sync, dns rdns, init
- d4ec1fb RED / 65f6a43 fix: AZLIN_PROVIDER_API=<base> sends Hetzner/Cloudflare calls to a
  stand-in server (<base>/<fixture folder><path>), also for --env dev
- dee8de5 RED / 3d32112 fix / 1824fc3 / fbb01a9: every wrangler [env.*] names its vars (live had
  none), [env.local] for token (8081, in place of the native one) + watcher, .dev.vars ignored
- 4ba3759 RED / c43960e fix: node.toml `ntp = ["off"]` = no NTP (was: empty = Hetzner, 90 s
  boot wait offline); dev VMs write it
- a36c4a7 RED / 16c84e5 fix: ctl.toml envs.<env>.dns / .ntp for node provision
- 0072bf6, 691f6ff: iso/docs/HARDCODED.md; d54314a: local/ profile (mkprofile.py, template,
  env file, tests); ca5563d: GETTING-STARTED "The fully local stack"
azul (this worktree):
- 372f1f238, 870c3532b: appkit shared_endpoint (endpoints.<name> of the shared config; stand-in
  for AZCLOUD15's typed section)
- 96582dc0d RED / 404dbbdc6 fix: AzCalendar --worker + endpoints.meet in the meeting server order
- d77ee39f8 RED / a68c5afc6 fix: AzMaps --tiles / AZMAPS_TILES / endpoints.tiles
- 943f9160b RED / 84fa9cfcc fix: AZ_E2E_ALLOW_HTTP (layout/src/request.rs)
- d2bab5687 RED / 5677e5cc9 fix: AzTasks + AzCalendar's task store honour AZLIN_DATA
- 999636fe7: browse.py isolation (--data-dir, AZLIN_CONFIG=off), feed_server.py free port
- db34c5c6e RED / cd15020e4 fix: AzBuilder debug port 8765 (was 8080 = sqld)
- 4dc618c7e, 062935ecc: docs/HARDCODED.md

Coordinator (2026-10-08): stay out of ctl/build.rs, image/build.sh, image/Dockerfile,
init/upgrade.rs, token_native.rs, the Cmd::Jwt arm of ctl/mod.rs, azworker/token/src/lib.rs; the
iroh presets::N0 fix is the lead's. Told the lead: sntp is done here; local/ file names may
collide with the lead's local/ scripts on local/infra15.

## NEXT (for whoever continues)
- Integration with AZCLOUD15: AzCalendar's shared_meeting_server() -> azlin_config's
  resolve_endpoints(.., Endpoint::Meet) (keeps the profile's local 8790 default); shared_endpoint
  stays for `tiles` (not an Endpoint) or becomes Endpoint::Tiles.
- OPEN rows in both HARDCODED.md files (BOOT_DISK, data disks, AZ_KEYRING_SERVICE, AzCalendar's
  events folder (USER), AzDrive home under the root (USER), issuer year from the clock (USER)).

## Open questions
- See docs/HARDCODED.md section 10 and iso/docs/HARDCODED.md status USER rows.
