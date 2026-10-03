# N2_APP_ID - progress

Branch `wt/n2-app-id`, base `d240a1b1d`. Implements `scripts/N1_NOTIFICATION_PLATFORMS_2026_09_29.md`
section 3 (`AppConfig::app_id`). Nothing compiled (house rule).

## DONE

- `45c413dc0` RED: `AppConfig::app_id` field + stub setters; `wire::{PlatformAppId,
  ResolvedAppIdentity, AppIdentity::resolve}` stub; `layout/tests/an_app_names_itself_with_app_id.rs`;
  bundle.rs `configured_identifier` / `default_bundle_id` stubs + tests; dll invariant
  `the_app_declares_its_app_id_when_it_is_created`.
- `524c59de1` fix: `set_app_id`, `AppIdentity::resolve`, `desktop::app_identity::{declare,
  current}`, `App::create` declares; build tools (`bundle_metadata`, `configured_identifier`,
  `default_bundle_id`, mobile `--package` default); guide sections.
- report `scripts/N2_APP_ID_2026_09_29.md` (committed with this file).

## IN PROGRESS

- none

## NEXT

- none: the parent compiles, runs the tests (report section 5) and the api.json autofix
  (report section 3).

## Open questions

- none
