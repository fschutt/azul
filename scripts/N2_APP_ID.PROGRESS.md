# N2_APP_ID - progress

Branch `wt/n2-app-id`, base `d240a1b1d`. Implements `scripts/N1_NOTIFICATION_PLATFORMS_2026_09_29.md`
section 3 (`AppConfig::app_id`). Nothing compiled (house rule).

## DONE

(none yet)

## IN PROGRESS

- RED commit: `AppConfig::app_id` field + stub setters; `wire::{PlatformAppId, ResolvedAppIdentity,
  AppIdentity::resolve}` stub; `layout/tests/an_app_names_itself_with_app_id.rs`; bundle.rs
  `configured_identifier` / `default_bundle_id` stubs + tests; dll invariant
  `the_app_declares_its_app_id_when_it_is_created`.

## NEXT

1. Fix: `set_app_id`, `AppIdentity::resolve`, `desktop::app_identity::{declare, current}`,
   `App::create` calls `declare`.
2. Build tools: `configured_identifier` (shared line scan with `configured_icons`),
   `default_bundle_id`, `mobile::run::Target::resolve` default.
3. Guide: `doc/guide/en/system/windowing.md` "App identity" section, `deploying/mobile.md`.
4. Report `scripts/N2_APP_ID_2026_09_29.md`.

## Open questions

- none
