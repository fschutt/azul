# DEDUP review - AREA: OFFICE (2026-10-02)

Reviewed commit: `53c978b33` (branch fix/input-bugs-2026-09-19), range `112cf8342..HEAD`.
Scope: examples/azul-{sheets,show,photo,videocut,paint,drive,meet,maps,calculator,setup,builder,shells,widgets,storage,appkit}
and the widgets they use or copy. READ-ONLY review: nothing built, nothing edited in the repo.
Status: COMPLETE. 31 duplication findings (D), 7 reuse candidates (R), 18 API exports (A), 6 semantics items (S), 14 next-wave items (N).

## Findings

### 1. DUPLICATION

#### D1. Flora accent ramps exist three times (SHOW copy, shells copy, the pre-existing flora stones)
- Evidence:
  - `examples/azul-show/src/themes.rs:13-64` - `struct Stone { name, accent, deep, soft, glow }` + `STONES: [Stone; 5]`
    ("Stone", Leaf, Plum, Clay, Slate) + `PAPER` (#F4F2EA) / `INK`.
  - `layout/src/widgets/shells/theme_scope.rs:56-142` (in range, `d5e1428f6`) - `ShellThemeAccentColors { accent, deep, soft,
    glow, on_accent }`, `ShellThemeAccent::light()` table at :102-117, `colors(dark)` at :124, `ON_ACCENT` = #F4F2EA.
  - `layout/src/widgets/themes/flora.rs:4080-4135` (PRE-range, `28002c50f`) - `FloraStone { stone, deep, soft, glow }`,
    `STONE_ACCENT` (= LIGHT_ACC/DEEP/SOFT/GLOW), `STONE_LEAF`, `STONE_CLAY`, `STONE_SLATE`, `STONE_AMBER` (no Plum).
- Confirmation: compared the 20 hex values one by one: SHOW `STONES[i]` == `theme_scope::light()` row i for all 5 families
  (identical, only the first family is named "Stone" in SHOW vs `Blue` in the enum); `theme_scope` Leaf/Clay/Slate rows ==
  `flora::STONE_LEAF/CLAY/SLATE`, Blue row == flora `LIGHT_ACC/DEEP/SOFT/GLOW`; SHOW `PAPER` == `theme_scope::ON_ACCENT`.
  api.json (`ShellThemeAccent` at ~172368) exports only the enum: no `colors`, no `name`/`from_name`, no `ShellThemeAccentColors`.
- Proposal: (a) build `ShellThemeAccent::light()` from `flora::STONE_*` (add `STONE_PLUM` to flora.rs next to the others) so
  the widget layer has one table; (b) export `ShellThemeAccentColors` (struct) + `ShellThemeAccent::colors(dark: bool)`,
  `ShellThemeAccent::name()`, `ShellThemeAccent::from_name(String) -> OptionShellThemeAccent` and an `ALL` accessor
  (e.g. `ShellThemeAccent::all() -> ShellThemeAccentVec` or `from_index(u8)`) in module `shells`; (c) SHOW's `themes::STONES`
  becomes `ShellThemeAccent::all().map(|a| a.colors(false))`, `PAPER` = `.on_accent`.
- Effort S. Risk low (const data; theme_scope tests already pin `#44684F`). Benefits: AzShow (deck themes), any app that
  paints accent-coloured content outside CSS (charts in AzSheets, AzPhoto swatches, AzVideoCut clip tints).

#### D2. Twelve hand-written command-line parsers; `azul_appkit::args` exists but only Calculator/Contacts use it
- Evidence: `examples/azul-{sheets(204),show(238),photo(187),videocut(137),drive(322),meet(147),writer(233),calendar(278),
  mail(203),notes(192),tasks(209)}/src/args.rs` + a 12th inside `examples/azul-setup/src/model.rs:277-380`
  (`Args::parse`, same loop, `--screen/--theme/--mode/--frame/--step`) vs `examples/azul-appkit/src/args.rs:115-300` (`AppSpec`, `AppArgs::parse`,
  `Theme`, `ModePref`, `help(spec)`). `git grep azul_appkit` -> only azul-calculator and azul-contacts depend on it.
- Confirmation: read sheets/show/photo/videocut/drive/meet/writer parsers: all are the same loop (`split_once('=')` for
  `--x=v`, a `value(what)` closure that advances `i`, `"-h" | "--help" => Err(HELP)`, `--screen/--theme/--mode/--size/
  --sample`, "unknown option" fallthrough) - renamed/diverged, not byte-identical. Divergences: Sheets/Photo/Show/VideoCut/
  Drive `--mode` accepts only `light|dark` (no `system`), Meet accepts `system`; Show/Writer have `--shot`/`--shot-delay-ms`
  (appkit has them), the others do not; Photo has `--data`, appkit `--data-dir`; Meet's parse is a free fn returning
  `Args{help:bool}` instead of `Err(HELP)`; each app has its own `Theme`/`Mode` enum (Sheets `args::Theme{Flat,Flora}`,
  VideoCut `args.theme: Option<String>` compared to `"flora"` at lib.rs:2916). ~2,300 lines incl. tests.
- Why the apps could not adopt it: appkit's `AppArgs::parse` rejects any switch it does not know
  (`other if other.starts_with('-') => Err("unknown option")`, args.rs:275-276), so app switches (`--open`, `--project`,
  `--slide`, `--layout`, `--name`, `--export-dir`, `--frame-log`) have no place.
- Proposal: add `AppSpec::switches: &'static [Switch { name, value: Option<&str>, help }]` and an
  `AppArgs::extra: Vec<(String, Option<String>)>` (or a `parse_with(spec, argv, |name, value| -> Result<bool>)` hook);
  then move every app's `args.rs` to a ~30-line `AppSpec` + typed accessors. Unify the `--mode` vocabulary on
  `system|light|dark` and `--data-dir` (Photo's `--data`).
- Effort M (mechanical per app, tests move with it). Risk low (E2E scripts pass these switches - keep names). Benefits:
  all 14 apps; one `--help` format; `--shot` for every app (screenshot regression F8).

#### D3. Data-root resolution: 7 copies that disagree on the folder name (S3-split blocker)
- Evidence: `examples/azul-appkit/src/data.rs:26-38` (`--data-dir` > `$AZLIN_DATA` > `<data>/Azlin`),
  `examples/azul-sheets/src/storage.rs:144-149` (`$AZSHEETS_DATA` > `<data>/Azlin` > `./azsheets-data`),
  `examples/azul-show/src/storage.rs:199-208` (`$AZSHOW_DATA` > `<data>/azul` > `./azul-data`),
  `examples/azul-videocut/src/store.rs:68-73` (`$AZVIDEOCUT_DATA` > `<data>/azul` > `./azul-data`),
  `examples/azul-photo/src/lib.rs:363-367` (`--data` > `$AZPHOTO_DATA` > `<data>/Azul` > `./Azul`),
  plus calendar `event.rs:619`, mail `account.rs:301`, notes `lib.rs:325`, tasks `lib.rs:409`. Each also re-wraps
  `FilePath::get_data_dir()` in its own `user_data_dir()` (sheets lib.rs:3022, photo lib.rs:335, show lib.rs:514,
  videocut lib.rs:2908, appkit ui.rs:118, calendar lib.rs:1048, mail lib.rs:927).
- Confirmation: read all five office copies. Same 3-step shape; DIVERGED results: on macOS AzSheets writes to
  `~/Library/Application Support/Azlin/sheets/`, AzShow and AzVideoCut to `.../azul/{show,videocut}/`, AzPhoto to
  `.../Azul/photo/` (on a case-sensitive FS `azul` != `Azul`). The per-user bucket layout in the cloud-storage memo has
  ONE root with one folder per app; three apps currently put their files outside it.
- Proposal: every app calls `azul_appkit::data::data_root(flag, env(AZLIN_DATA), FilePath::get_data_dir())` and keeps
  its per-app env var only as a test override (or drops it: `--data-dir` covers tests). Fix the three roots now (S).
- Effort S. Risk: existing local test data under `azul/`/`Azul/` is orphaned (dev-only). Benefits: Show, VideoCut,
  Photo, Sheets (+ Calendar/Mail/Notes/Tasks outside this area).

#### D4. "List every page of a drive folder" loop copied 7x although `azul_storage::ops::list_all` exists; AzShow lacks it
- Evidence: `examples/azul-storage/src/ops.rs:10-22` (`list_all`, recursive, all pages - in range `03f4c553b`);
  copies: `examples/azul-appkit/src/files.rs:65-80` (same, returns keys, adds a 10,000-page guard and a sort),
  `examples/azul-videocut/src/store.rs:93-110`, `examples/azul-photo/src/storage.rs:231-244` (recursive) and
  `:275-300` (folder), `examples/azul-sheets/src/storage.rs:116-135` (folder), `examples/azul-notes/src/store.rs:136-145`,
  `examples/azul-tasks/src/store.rs:44-52`.
  BUG: `examples/azul-show/src/storage.rs:126-129` (`Job::List`) reads ONE page (`drive.list(&ListRequest::folder(..))`,
  `page.next` ignored): on an S3 bucket (1000 keys/page) decks beyond the first page vanish from File > Open.
- Confirmation: read all loops; recursive ones are `list_all` modulo the mapping; folder ones need a `folders` collector
  that `ops` does not offer.
- Proposal: `azul_storage::ops::list_folder_all(drive, prefix) -> Result<(Vec<ObjectInfo>, Vec<String>)>` beside
  `list_all`; appkit's `files::list_all` becomes `ops::list_all(..).map(keys)` (move the page guard into `ops`); the
  app loops call the two helpers; Show's `Job::List` switches to `list_folder_all` (fixes the bug).
- Effort S. Risk low (tests exist in azul-storage `tests/ops.rs`). Benefits: Show (bug), Photo, Sheets, VideoCut,
  appkit, Notes, Tasks.

#### D5. "Recent documents" listing = the same 4-step shape in Sheets / Show / Photo / VideoCut
- Evidence: `examples/azul-sheets/src/storage.rs:114-138` (`list`: folder -> `<id>.xlsx` -> sidecar json -> modified,
  sort newest first), `examples/azul-show/src/storage.rs:124-152` (`Job::List`: folder -> `deck.json` -> `head` for
  modified -> sort newest first), `examples/azul-photo/src/storage.rs:272-302` (`list`: folder -> `head(doc.json)` +
  `get(doc.json)` -> sort newest first), `examples/azul-videocut/src/store.rs:92-110` (`list_projects`, ids only).
- Confirmation: read all four; identical sort key `b.modified.cmp(&a.modified).then_with(title/name)` in Sheets, Show,
  Photo. Photo does `head` AND `get` per document (2 round trips per doc on S3), Show `get` + `head`.
- Proposal: `azul_appkit::files::recent_documents(drive, app_folder, layout: DocLayout{FolderWith(&str)|FileWithExt(&str)},
  read_meta: fn(&[u8]) -> Option<(String /*title*/, ..)>) -> Vec<DocEntry{id,title,modified}>` (one `list_folder_all`,
  `ObjectInfo.modified` from the listing instead of a `head` per doc). The Backstage "Open" pane that renders it is
  per app today (Sheets lib.rs backstage `"Open"`, Show backstage.rs, Photo start screen) - see R3.
- Effort M. Risk low. Benefits: Sheets, Show, Photo, VideoCut (+ Writer, Notes).

#### D6. One-shot "job on an azul Thread with a write-back" boilerplate in 8 apps
- Evidence: `examples/azul-photo/src/jobs.rs:142-175` (`JobInit{job:Option<Job>}`, `Done{outcome:Option<Outcome>}`,
  `job_thread`, `spawn`), `examples/azul-drive/src/jobs.rs:244-262` + `:717` (same names, plus `send`),
  `examples/azul-videocut/src/lib.rs:327-370` + `:541-570` (`job_thread` with drive/files), `examples/azul-sheets/src/lib.rs:829-890`
  (`job_thread`, `spawn_job`) and `:577-610` (`wait_thread`), `examples/azul-show/src/lib.rs:275-310` (`spawn_storage`,
  `storage_thread`),
  `examples/azul-appkit/src/ui.rs:276-342` (`FileReply`, `take_reply`, `file_thread`, `spawn_file_jobs`), plus
  azul-notes `jobs.rs:28-70`, azul-tasks `jobs.rs:59-160`.
- Confirmation: read photo/drive/videocut/sheets/appkit copies: same 3 items, renamed; the only variation is what
  `run(job)` closes over (a drive, a file cache).
- Proposal (two options): (a) appkit `jobs::spawn<J: Send + 'static, O: 'static>(info, app, job, run: fn(J) -> O,
  done: WriteBackCallbackType)` with a generic `extern "C" fn job_thread::<J, O>` and `jobs::take::<O>(&mut RefAny)`;
  (b) azul API: `Thread::create_job(data: RefAny, run: JobCallback, writeback: WriteBackCallback)` /
  `CallbackInfo::spawn_job(..)` so C/Python apps get it too. (a) is S and app-local; (b) is the real fix.
- Effort S (a) / M (b). Risk low. Benefits: Photo, Drive, VideoCut, Sheets, Show, appkit apps, Notes, Tasks, Mail.

#### D7. File ids: four mints, and Photo / Show / VideoCut use the DETERMINISTIC `Uuid::v4()` (overwrite bug)
- Evidence: `examples/azul-appkit/src/data.rs:70-111` (`new_uuid`: RandomState + clock + pid + counter -> v4 text;
  random), `examples/azul-sheets/src/storage.rs:22-26` + `Cargo.toml:28` (the `uuid` crate's `new_v4`; random),
  `examples/azul-photo/src/lib.rs:164-167` (`new_uuid` = `azul::uuid::Uuid::v4()`), `examples/azul-show/src/
  commands.rs:47-50` (`new_deck_id` = `Uuid::v4()`), `examples/azul-videocut/src/lib.rs:2839,2858,2874` (`Uuid::v4()`
  for the sample, a new project and `--sample`). Outside the area: calendar `event.rs:698` and tasks `state.rs:170`
  (seeded copies), notes `jobs.rs:296` (v4) - see the EDITORS report B1.
- Confirmation: `layout/src/uuid.rs:20-45` documents that `Uuid::v4` is a pure function of a process-local tick ("the
  first Uuid::v4 in every process is 00000000-0000-4000-...", "do not use as a database key that two processes might
  mint"). BUG: AzPhoto mints its start document's id at `lib.rs:399` (`PhotoState::new(doc, &name, &new_uuid())`)
  before any window exists, i.e. the same id on every launch -> Save writes `photo/<same-id>/doc.json` and its tiles
  over the previous session's document (and `storage::save` then DELETES tiles the new doc lacks, storage.rs:231-244).
  AzShow (`commands.rs:463,470`, `lib.rs:521`) and AzVideoCut (`lib.rs:2839,2858`) mint after a run-dependent number
  of marker ids, so collisions across runs are likely for the same click path (e.g. `--sample` at startup: VideoCut
  `lib.rs:2874`, Show `lib.rs:521` -> the same sample id every run).
- Proposal: export a random mint (`Uuid::random() -> String`, std: RandomState + clock seed into the existing
  `Uuid::from_seed`) in module `uuid` - or, app-side today, `azul_appkit::data::new_uuid()`; switch Photo, Show,
  VideoCut (and Notes) now; Sheets can drop the `uuid` crate once the export lands.
- Effort S. Risk none (ids are opaque). PRIORITY: high (silent data loss in Photo on the second launch).
  Benefits: Photo, Show, VideoCut (+ Notes, Calendar, Tasks, Contacts, Sheets).

#### D8. `reborrow_info` copied 3x (identical) - and not needed: `CallbackInfo` is `Copy`
- Evidence: `examples/azul-sheets/src/lib.rs:2544-2554`, `examples/azul-writer/src/lib.rs:630-638`,
  `examples/azul-notes/src/ui.rs:1235-1245` (all build `CallbackInfo { ref_data, hit_dom_node, cursor_relative_to_item,
  cursor_in_viewport, changes }` to hand `Pdf::from_dom_in_callback` an owned info). `examples/azul-show/src/commands.rs:342`
  passes `*info` instead. api.json `callbacks.CallbackInfo` derives `Debug, Copy, Clone`.
- Confirmation: the three bodies are byte-identical (diffed by eye, 7 lines each); Show proves `*info` compiles against
  the generated crate.
- Proposal: delete the three helpers, pass `*info` (no API change needed for the info). Also the A4 page size `794.0 x 1123.0` literal repeats (Sheets lib.rs:2590,
  Notes ui.rs:1278, Writer lib.rs:651-652, Show computes its own): add `Pdf::A4_WIDTH_PX/A4_HEIGHT_PX` consts or a
  `PageSize::a4()` in module `pdf`.
- Effort S. Risk none. Benefits: Sheets, Writer, Notes.

#### D9. Ribbon builder helpers (`large/small/toggle/column/row/group/tab` + `fold(with_item)`) copied in 6 apps
- Evidence: `examples/azul-show/src/ribbon.rs:31-60`, `examples/azul-sheets/src/lib.rs:1172-1214`,
  `examples/azul-drive/src/ui_ribbon.rs:23-90`, `examples/azul-tasks/src/chrome.rs:286-320`,
  `examples/azul-writer/src/ribbon_ui.rs:133-160`, `examples/azul-calendar/src/chrome.rs:94`. api.json:
  `RibbonColumn`/`RibbonRow`/`RibbonGroup` have only `create` + `add_item/with_item`, `RibbonTab` only `with_group`.
- Confirmation: read Show/Sheets/Drive/Tasks copies: `column(items) = RibbonItem::Column(items.into_iter().fold(
  RibbonColumn::create(), |c, it| c.with_item(it)))` is textually identical in all four; `large/small/toggle` differ
  only in how the click payload is made (`command(app, cmd)` / `action_ref(app, a)` / `CommandRef{..}`). Drive adds a
  disabled reason (`why_not`) - Show/Sheets/Tasks never disable a ribbon button.
- Proposal (API): `RibbonColumn::create_with_items(RibbonItemVec)`, `RibbonRow::create_with_items(..)`,
  `RibbonGroup::with_items(RibbonItemVec)`, `RibbonTab::with_groups(RibbonGroupVec)`, and `RibbonItem::large(RibbonButton)`
  / `RibbonItem::small(..)` constructors in module `widgets` (autofix). The per-app click-payload helper stays app code
  (see R1 for the command table that would also carry `why_not`).
- Effort S. Risk none. Benefits: Show, Sheets, Drive, Tasks, Writer, Calendar, Mail.

#### D10. Snapshot undo/redo stacks: three diverged copies (Photo / VideoCut / Show)
- Evidence: `examples/azul-photo/src/raster/history.rs:10-119` (`History { states, current, limit }`, labels, coalescing
  key, `jump(index)` for the History panel, `drain(0..excess)`), `examples/azul-videocut/src/model.rs:23,414-560`
  (`History { undo: Vec<(&str, Sequence)>, redo }`, `HISTORY_LIMIT = 200`, `undo.remove(0)`),
  `examples/azul-show/src/editor.rs:14,74-125` (`undo: Vec<Deck>`, `redo`, `UNDO_DEPTH = 100`, `undo.remove(0)`,
  no labels). Op-based (different by design, not merged): Writer `lib.rs:89-113,600-720`, Paint `lib.rs:139-183`
  (stroke stack, pre-range), Sheets (IronCalc's own undo), Tasks `state.rs:56` (last delete only).
- Confirmation: read all three snapshot copies. Same idea (push the state before / pop into the other stack / clear redo
  on edit / cap). Diverged: only Photo coalesces runs (a dragged slider = one step) and can jump to a state; only Photo
  and VideoCut keep labels ("Undo Move" menu text); Show and VideoCut drop the oldest with `Vec::remove(0)` (O(n) on
  every edit at the cap - a 100-deep Vec<Deck> memmove per edit once full); Show coalesces only adorner drags, through
  an ad-hoc `transforming` flag (`editor.rs:319-333`) - Photo's coalescing key is the general form of that flag.
- Proposal: one generic `UndoHistory<T: Clone>` (Photo's shape: `new(label, state)`, `push(label, state,
  coalesce: Option<&str>)`, `undo() -> Option<&T>`, `redo()`, `jump(i)`, `labels()`, `can_undo/can_redo`,
  `with_limit(n)`, VecDeque inside) in `examples/azul-appkit/src/history.rs` (Rust generic, no FFI needed). A
  `HistoryPanel` list widget is optional (Photo's `ui.rs:707 history_panel` is a ListView of labels).
- Effort S-M. Risk low (each app's tests cover undo/redo). Benefits: Photo, VideoCut, Show (gets coalescing + labels);
  later Paint, Tasks.

#### D11. `#rrggbb` formatting / parsing written 8 times; azul has it (`color_input::color_to_hex/color_from_hex`) but does not export it
- Evidence: format: `examples/azul-photo/src/ui.rs:83-85`, `examples/azul-show/src/model.rs:54-60` (`Color::hex`),
  `examples/azul-show/src/render.rs:64-70` (`css_color`, rgba() when translucent), `examples/azul-widgets/src/forms.rs:350-352`,
  `examples/azul-writer/src/palette.rs:228-230`, `layout/src/widgets/shells/theme_scope.rs:141-147` (uppercase, `#RRGGBBAA`);
  parse: `examples/azul-sheets/src/model.rs:184-192` (`parse_hex`, 6 digits only), `examples/azul-show/src/model.rs:64-80`
  (`Color::parse`, 3/6/8 digits), `layout/src/widgets/map_themes.rs:211` (`parse_hex_rgb`). Reference implementation:
  `layout/src/widgets/color_input.rs:586-625` (`color_to_hex`: `#rrggbb` or `#rrggbbaa`; `color_from_hex`: 3/4/6/8 digits,
  `#` optional, trimmed - tested at :3340).
- API today: `ColorU::to_hash()` always emits 8 digits (`#rrggbbff`, css/src/props/basic/color.rs:889) and
  `ColorU::from_str(String)` returns BLACK on a parse error (api.json fn_body `..ok().unwrap_or(BLACK)`), so an app
  cannot tell "black" from "invalid" - which is why Sheets/Show validate with their own parsers.
- Confirmation: read every copy; the 5 formatters are `format!("#{:02x}{:02x}{:02x}", r, g, b)` (identical modulo
  input type); parsers diverge in accepted lengths (Sheets 6 only; Show 3/6/8; ColorInput 3/4/6/8).
- Proposal: export `ColorU::to_hex() -> String` (= `color_input::color_to_hex`) and `ColorU::parse_hex(String) ->
  OptionColorU` (= `color_from_hex`), plus `ColorU::try_from_css(String) -> OptionColorU` (parse_css_color without the
  BLACK fallback); move the two fns from color_input.rs into `css/src/props/basic/color.rs` as `ColorU` methods
  (non-UI, core/css). Apps keep their own colour structs only where they serialize (Show's `model::Color`).
- Effort S. Risk none. Benefits: Photo, Show, Sheets, Writer, AzWidgets, shells.

#### D12. Standard dialogs re-built by hand in VideoCut / Photo / Drive / appkit while DIALOGS shipped them (exported)
- Context: `layout/src/widgets/standard_dialogs.rs` (`MessageBox`, `AboutDialog`, `ProgressDialog`, `LoginDialog`,
  `FindReplaceDialog`) are in api.json (module `widgets`, checked: `create`, `with_*`, `with_on_event`, `dom`). Users:
  `git grep -w` -> AboutDialog/MessageBox only in azul-setup + azul-widgets; ProgressDialog/FindReplaceDialog only in
  azul-widgets. Same night, so the apps could not know - but each copy now has its own look and a11y.
- Evidence (copies):
  - About: `examples/azul-videocut/src/lib.rs:1726-1744` (`about_dialog`: a `Dialog` with name + text + Close),
    `examples/azul-drive/src/ui_dialogs.rs:1019-1045` (`about`: version, blurb, settings path, a key list sentence),
    `examples/azul-show/src/backstage.rs:286-290`, `examples/azul-sheets/src/lib.rs:1921-1925` (About pane lines),
    `examples/azul-appkit/src/about.rs:13-41` + `ui.rs:18,573` (`TODO(DIALOGS)`: "azul's About dialog plugs in here").
  - Progress: `examples/azul-videocut/src/lib.rs:1607-1672` (`export_dialog`: `ProgressBar` + status line + Cancel /
    Export now / Close in a modal `Dialog`) = `ProgressDialog` (bar + percent, Cancel held via `can_cancel`).
  - Message box: `examples/azul-drive/src/ui_dialogs.rs:283-330` (`Popup::ConfirmDelete` / `ConfirmForget`: question
    text, "This cannot be undone.", Cancel + Danger button; the "ask again" lives in Options as `confirm_delete`) =
    `MessageBox { kind: Warning|Question, detail, buttons, dont_ask }`.
  - Find: `examples/azul-sheets/src/lib.rs:1758-1767` (side panel: one "Find what" field, Enter = next; no Replace,
    no Match case, no status) = `FindReplaceDialog` (find/replace fields, Match case / Whole word, status line,
    Find previous / next / Replace / Replace all; `ops.rs` already has the engine-side find). AzShow's find/replace is
    "not built" (SHOW report) - it should start from the dialog.
  - Modal frame: `examples/azul-photo/src/ui.rs:889-918` (`sheet_frame`: absolute overlay `rgba(0,0,0,0.35)`, card,
    title, Cancel/OK, role Dialog) = `Modal`/`Dialog` (VideoCut and Drive use `Dialog`); Drive's
    `ui_dialogs.rs:667-681` `inline_sheet` is a test-only twin (`AZDRIVE_DIALOGS=inline`).
- Confirmation: read every copy listed; none uses dialog_kit's look, so flat/flora + dark differ per app (Photo's sheet
  uses its private `Palette` - see D16).
- Proposal: Sheets Find panel -> `FindReplaceDialog` body in the side panel (Replace via `ops`); VideoCut About ->
  `AboutDialog` in its `Dialog`; VideoCut export -> `ProgressDialog` (keep the
  "Export now" form as the pre-step); Drive ConfirmDelete/ConfirmForget -> `MessageBox` with `with_dont_ask` wired to
  `settings.confirm_delete`; Photo `sheet_frame` -> `Modal` + `dialog_kit` buttons; appkit's About section ->
  `AboutDialog` built from `about::AboutInfo` (resolves `TODO(DIALOGS)`), then Sheets/Show/Drive About panes call appkit.
- Effort S each (M total). Risk low (E2E scripts click by text/id: keep ids `confirm-delete`, `sheet-ok`, `sheet-cancel`).
  Benefits: VideoCut, Drive, Photo, Sheets, Show, appkit apps.

#### D13. Settings / Options pages: every app hand-builds "Appearance + Data folder + Shortcuts + About" on ShellSettingsLayout
- Evidence: `examples/azul-appkit/src/ui.rs:58,592-718` (the kit's page: Appearance with Segmented Flat/Flora and
  System/Light/Dark applied via `set_theme/set_mode` AND saved to `<app>/settings.json`, Data, Shortcuts, About),
  `examples/azul-sheets/src/lib.rs:1896-1919` (Options: 4 plain Buttons Flat/Flora/Light/Dark, no System, nothing
  saved; "Files" = data folder lines), `examples/azul-show/src/backstage.rs:258-301` (Segmented with System; Keyboard =
  6 text lines; About), `examples/azul-videocut/src/lib.rs:1675-1724` (Playback/Export/Storage/About, no Appearance section - theme and mode
  are two flip toggles, `on_theme_toggle`/`on_mode_toggle` at lib.rs:2816-2830),
  `examples/azul-drive/src/ui_dialogs.rs:779-1018` (View/Navigation/Drives/About, own settings.json in the CONFIG dir),
  `examples/azul-meet/src/ui.rs:869-935` (Devices/Video/Appearance/About), `examples/azul-photo/src/ui.rs:1005-1025` (Flat/Flora/Light/Dark Buttons in a hand-made sheet).
  The theme switch itself is re-coded in each: sheets `lib.rs:2518-2521`, videocut `lib.rs:2816-2830` (a toggle that
  flips flat<->flora), photo `commands.rs:380-392`, show `commands.rs:488-500`, meet `lib.rs:4574-4590`, drive
  `lib.rs:1578-1585`.
- Confirmation: read each page. Divergence that users will notice: theme/mode persist only in the appkit apps (Calculator,
  Contacts); Sheets/Show/Photo/VideoCut/Meet/Drive forget the choice on restart (Drive applies `--theme/--mode` only,
  `lib.rs:1576-1585`); Sheets/VideoCut/Photo cannot go back to "System" once a mode is picked. Shortcut lists are free text in Show/Sheets/Drive (drift from the real key handlers), a table
  in appkit (`shortcuts.rs`).
- Proposal: (1) appkit's `settings_page` re-based on azul's `ShellSettingsDialog` (exported, table-driven: `ShellSetting`
  with `Choice`/`Toggle`/`Path`/`Shortcut` values, search, Apply/OK/Instant) instead of raw `ShellSettingsLayout`;
  (2) every office app takes appkit's Kit (`create_kit`, `app_config`, `settings_page(kit, app_sections)`,
  `handle_key`) - Appearance/Data/Shortcuts/About come for free and persist in `<app>/settings.json` under the data root
  (S3-ready, see D3); (3) app sections become `ShellSetting` rows.
- Effort M (appkit rebase) + S per app. Risk: E2E scripts that click "Flat"/"Dark" buttons (Sheets) need the segmented
  labels. Benefits: Sheets, Show, Photo, VideoCut, Meet, Drive (+ Writer, Calendar, Mail, Notes, Tasks).

#### D14. "Primary modifier" = `ctrl || meta` in 25 key handlers (21 files); azul's platform rule (`KeyboardState::primary_down`) is not exported
- Evidence: `examples/azul-sheets/src/lib.rs:2966-2967`, `examples/azul-show/src/lib.rs:425-426`,
  `examples/azul-drive/src/actions.rs:574-578` (+ `ui_view.rs` x3, `ui_panes.rs`), `examples/azul-appkit/src/ui.rs:441`,
  `examples/azul-videocut/src/lib.rs` (x2), `examples/azul-photo/src/canvas.rs:388`, `examples/azul-meet/src/lib.rs`,
  `examples/azul-calculator/src/ui.rs`, `examples/azul-shells/src/lib.rs` (and 10 more files outside the area: calendar,
  contacts, mail, notes, tasks). Core has the rule: `core/src/window.rs:653-659` (`KeyboardState::primary_down`: Cmd on
  macOS, Ctrl elsewhere) and `core/src/menu.rs:47` (`accelerator_matches`, exact chord match, `LWin` = primary);
  api.json `dom.KeyboardState` and `dom.KeyModifiers` export NO functions; only `app.HotkeyModifiers::primary()`.
- Confirmation: `git grep "ctrl || .*meta"` over examples/ = 21 files, 25 sites. `KeyModifiers.meta` is
  `super_down()` = LWin/RWin (`core/src/window.rs:643-645,677`), so in every copy the Windows/Super key counts as Ctrl
  on Windows/Linux (any Win+<key> the OS does not grab reaches the app as Ctrl+<key>), and on macOS Ctrl+S saves
  as well as Cmd+S - `primary_down()` is exactly the rule they meant.
- Proposal (API): export `KeyboardState::primary_down() -> bool`, `shift_down`, `alt_down`, `ctrl_down`, `super_down`,
  `is_key_down(VirtualKeyCode)`, and `VirtualKeyCodeCombo::matches(keyboard: KeyboardState, pressed: VirtualKeyCode)
  -> bool` (= `accelerator_matches`) in module `dom`; or `KeyModifiers::primary() -> bool` if the modifiers struct is
  the preferred surface. Then the apps' tables become `[(combo, Action)]` (see R1).
- Effort S (API) + S per app. Risk low. Benefits: every app.

#### D15. Image sizing / scaling / RawImage plumbing re-done in VideoCut, Photo, Paint, Meet
- Evidence:
  - `examples/azul-videocut/src/render.rs:95-103` `fit_within` vs `core/src/image_scale.rs:659-672` `fit_within`
    (in range, `faa5d84c2`). Diverged edge cases: VideoCut uses f32 and `max(1)` on every input, so an empty source
    or box yields 1x1; core returns `(0, 0)` and returns the size unchanged when it already fits (no float rounding).
    Same rounding otherwise. 6 call sites in VideoCut (decode.rs:251,285, lib.rs:262,379,464, render.rs:87).
  - `examples/azul-videocut/src/render.rs:163-180` `scale_to` (nearest sample) vs core `resample_rgba`
    (area-average/bilinear, image_scale.rs:362) - the monitor and the bin thumbnails are nearest-neighbour today.
  - `examples/azul-photo/src/raster/transform.rs:264-310` `resize` + `box_average` vs core `sample`/`resample_rgba`
    (Photo works on premultiplied tiles; core on a `SrcImage`; same algorithm family, not a copy).
  - RGBA8 `RawImage { pixels: RawImageData::U8(..), width, height, premultiplied_alpha, data_format: RGBA8, tag }`
    literal: videocut `lib.rs:718-728`, photo `codec.rs:109-118` and `:149`, `raster/brush.rs:116`, meet `lib.rs:1742-1751`,
    paint `lib.rs:291`, `:507`, `:572`, widgets `forms.rs:220`.
- Confirmation: read both `fit_within` bodies side by side (quoted above) and `scale_to`; the 9 `RawImage {..}`
  literals were found with `git grep "RawImage {"` and each read.
- API today: `RawImage::thumbnail(max_w, max_h)` is exported (api.json ~105008) and AzDrive uses it
  (`examples/azul-drive/src/jobs.rs:426-433`); `fit_within`, `resample_rgba` and an RGBA8 constructor are not.
- Proposal: export `RawImage::create_rgba8(width: u32, height: u32, pixels: U8Vec, premultiplied: bool) -> RawImage`,
  `RawImage::resized(width: u32, height: u32) -> OptionRawImage` (= `resample_rgba`), and a free/static
  `RawImage::fit_within(width, height, max_w, max_h) -> LogicalSize`-like pair (or `ImageSize::fit_within`) in module
  `image`; VideoCut deletes `fit_within` and uses `thumbnail`/`resized` for the bin and the monitor; the others use
  `create_rgba8`.
- Effort S. Risk low (VideoCut's 1x1-minimum semantics: check callers that pass 0). Benefits: VideoCut, Photo, Meet,
  Paint, AzWidgets, Review.

#### D16. AzPhoto paints its chrome from a private light/dark `Palette` instead of the app theme
- Evidence: `examples/azul-photo/src/ui.rs:36-69` (`Palette { chrome, panel, line, text, muted, selected, ruler }`,
  `LIGHT`/`DARK` hex constants) used by every panel builder (`ui.rs:113-1040`), while the window root is a
  `ShellThemeScope` + `CanvasShell` (themed by flat/flora). Writer has the same pattern (`examples/azul-writer/src/
  palette.rs:21`, pre-dates the shells). VideoCut does it right: `system:` colours (`lib.rs:1030-1047`).
- Confirmation: read Photo's palette and its users: flat and flora give the same panel colours; only light/dark switch.
- Proposal: replace the palette with `system:` colours / `var(--az-accent*)` (what VideoCut and Drive do) or with a
  `CanvasShell`-provided panel look; keep only the canvas workspace grey (`ViewColors`) app-side.
- Effort S-M. Risk: visual only. Benefits: Photo (theme coherence with the other Office apps).

#### D17. Wheel-to-steps accumulators: CellGrid and TimePicker (SHEETS-report twin, verified)
- Evidence: `layout/src/widgets/cell_grid.rs:1782-1793` (`wheel_steps(travel, delta, px_per_step)`, clamp
  `WHEEL_MAX_STEPS`) + `:1795-1815` (thread-local `WHEEL_TRAVEL (x, y)`), `layout/src/widgets/time_picker.rs:992-1027`
  (`SCROLL_ACCUM (owner, travel)`, `take_scroll_steps(which, dy)`, clamp to +-1, no_std fallback).
- Confirmation: read both. Same accumulate / divide / keep-the-remainder loop; diverged in cap (CellGrid many steps per
  event, TimePicker one), owner reset (TimePicker resets the travel when another column scrolls; CellGrid never
  resets - a half notch left from grid A carries into grid B), and no_std (only TimePicker has a fallback; CellGrid's
  `take_wheel` has its own cfg arm).
- Proposal: `pub(crate) fn wheel_steps(travel: &mut f32, delta: f32, px_per_step: f32, max_steps: i64) -> i64` in a
  small `widgets/wheel.rs` (or `roving.rs` neighbour) + one thread-local keyed by an owner id (`NodeId`-hash), used by
  both; Timeline (`timeline.rs`, wheel zoom/scroll) is the next candidate.
- Effort S. Risk low (both have tests). Benefits: CellGrid, TimePicker, Timeline.

#### D18. TSV encoded three times (widget + twice inside AzSheets) with two quoting rules
- Evidence: `layout/src/widgets/cell_grid.rs:1839-1849` (`cells_to_tsv` + `tsv_field`, hand-written, for the system
  clipboard), `examples/azul-sheets/src/model.rs:207-222` (`tsv_of`, the `csv` crate, used by lib.rs:2240 for the
  ribbon's Paste) and `examples/azul-sheets/src/ironcalc_engine.rs:240-254` (`tsv_of` again: csv crate + `flexible(true)`
  + explicit `\n` terminator, used by the engine's sort/remove-duplicates paste at :340). The two app copies differ
  only in `flexible` (model.rs errors - and returns "" silently - on ragged rows) and the explicit terminator.
- Confirmation: read both. Widget (`tsv_field`, cell_grid.rs:1830-1836, Excel's rule): quote only when the field
  holds a tab/newline/CR or STARTS with a double quote; rows joined by `\n`, no trailing newline. csv crate
  (QuoteStyle::Necessary): quotes any field containing a double quote anywhere, and ends every record with `\n`.
  Text `say "hi"` -> widget keeps it bare, the app writes it quoted with doubled quotes, so the system clipboard
  (Ctrl+C) and the TSV the ribbon's Paste hands IronCalc follow different rules for the same cells.
- Proposal: one TSV rule for both: export `CellGrid::tsv_of_rows(StringVecVec) -> String` (the widget's rule) or
  make `model::tsv_of` use `QuoteStyle` matching Excel's rule; have the ribbon's Paste build its TSV with the widget's rule. Add a
  round-trip test (widget TSV -> IronCalc paste) in AzSheets.
- Effort S. Risk low. Benefits: AzSheets (clipboard consistency).

#### D19. XML/HTML text escapers: 7 private copies (3 added in range)
- Evidence: `layout/src/widgets/cell_grid.rs:1851-1863` (`html_escape`, no `'`; in range), `layout/src/e2e/builder.rs:
  2731-2744` (`escape_xml`, 5 entities; 2026-09-29), `layout/src/managers/notification.rs:620-635` (`xml_escape`, 5 +
  drops C0 controls; 2026-09-28), `examples/azul-mail/src/compose.rs:693-705` (`escape_html`; in range),
  `layout/src/xml/mod.rs:1203-1215` (`escape`, 5 entities, pre-range), `dll/src/web/html_render.rs:1090-1115`
  (`html_escape` 3 entities + `html_escape_attr` 4).
- Confirmation: read all; identical `match c { '&' => "&amp;", '<' => "&lt;", '>' => "&gt;", ... }` loops with 3-5
  arms; only notification.rs drops control characters (the correct behaviour for XML 1.0).
- Proposal: `azul_core::xml::escape_xml(s: &str, attr: bool) -> String` (5 entities + drop C0 except \t\n\r) used by
  all six; export as `Xml::escape_text(String) -> String` in module `xml` so apps (Mail compose, Sheets' HTML export,
  Show's future .pptx writer, Drive's WebDAV/S3 XML) stop writing their own.
- Effort S. Risk low. Benefits: CellGrid, e2e, notifications, web backend, AzMail, future exporters.

#### D20. Click / Shift / Ctrl list-selection model written three times (one of them is an exported widget type)
- Evidence: `layout/src/widgets/message_list.rs:423-490` (`MessageListSelection { rows: U32Vec, anchor }`,
  `apply(index, shift, ctrl)`: Shift = anchor..=index, Ctrl = toggle + new anchor, plain = one; exported with
  `create/apply/contains/len/is_empty`), `examples/azul-show/src/editor.rs:135-175` (`rail_select(index, shift, ctrl)`,
  same three rules on `selected_slides: Vec<usize>` + `anchor`), `examples/azul-drive/src/model.rs:143-240`
  (`Selection::click/toggle/extend/add_range`, key-based, plus Ctrl+Shift = add the range).
- Confirmation: read all three. Diverged: Show's Ctrl-toggle refuses to empty the selection (keeps one slide - the
  rail always has a current slide); Drive adds Ctrl+Shift range-add and works on keys + a visible order; the widget
  type allows an empty selection. `ThumbnailStripEvent` (Show's rail) and `TimelineEvent` already carry `shift`/`ctrl`,
  so each consumer re-applies the rule.
- Proposal: move `MessageListSelection` to `list_view.rs` as `ListSelection` (keep `MessageListSelection` as a type
  alias for one release), add `apply_range_add(index)` (Ctrl+Shift) and `with_keep_one(bool)`; ThumbnailStrip /
  ListView / TreeView consumers (Show rail, Drive file view, VideoCut clips) use it. Drive's key-based variant maps keys
  to indices of the visible order first.
- Effort S-M. Risk low. Benefits: Show, Drive, VideoCut, Mail, any list app.

#### D21. Storage glue that would block "durable data = files in the user's S3 bucket"
- Evidence and what blocks:
  - AzSheets builds a fresh `LocalDrive::new(root)` inside every job: `examples/azul-sheets/src/lib.rs:564, 574, 840,
    843, 858` (the state holds `data_root: PathBuf`, not a `Drive`), and reports paths as `root.join(&key).display()`
    (lib.rs:577, 862). Swapping in an `S3Drive` touches 5 sites + the messages.
  - AzShow carries a `PathBuf` root in every job (`commands.rs:94` `spawn_storage(info, app, s.data_root.clone(), job)`)
    and opens `storage::local_drive(root)` per job (`lib.rs:303`).
  - AzShow lists one page only (D4 bug) - invisible decks on a bucket with >1000 keys.
  - Data roots outside `Azlin/` (D3): Show/VideoCut `azul/`, Photo `Azul/`.
  - AzPhoto, AzShow, AzVideoCut mint deterministic ids (D7) - on a shared bucket two devices WILL pick the same
    `photo/<id>/` folder (both start at tick 0).
  - Per-device config, deliberately local (fine, just know it): Drive `<config>/azul-drive/settings.json` +
    `<config>/azul-storage/drives.json` (credentials; written from the UI callback per DRIVE2), Meet
    `<config>/AzMeet/settings.txt` via `std::fs` (`examples/azul-meet/src/lib.rs:4049-4085`, written synchronously in
    the server-check write-back at :4034).
  - Ready already: Photo and VideoCut hold `Arc<dyn Drive>` (`photo/src/lib.rs:368`, `videocut/src/lib.rs:2913`);
    appkit's `files` jobs take `&dyn Drive`.
- Confirmation: read every construction site listed (`git grep LocalDrive` over the six apps) and the Show/Sheets
  job enums; Photo/VideoCut were checked to hold the drive once.
- Proposal: Sheets and Show keep ONE `Arc<dyn Drive>` in their state (as Photo/VideoCut do) and pass it into jobs;
  user-facing locations come from a `Drive::describe(key) -> String` (LocalDrive: the file path; S3Drive:
  `s3://bucket/key`) instead of `root.join(key)` - add that to azul-storage's trait with a default. Together with
  D3/D4/D7 that is the whole S3-readiness checklist for the Office apps.
- Effort S (Sheets/Show) + S (`describe`). Risk low. Benefits: Sheets, Show (+ Drive/Mail messages).

#### D22. Export pipelines: three different destinations for "Export"
- Evidence: Show `commands.rs:325-375` (PDF rendered in the callback, then `FileDialog::save_bytes(name, mime,
  bytes)`; PNG via `take_screenshot_of_node` + `save_bytes`), Sheets `lib.rs:2557-2600` (PDF rendered in the callback,
  then written to `exports/<title>.pdf` IN THE DATA ROOT by a worker, no dialog; CSV the same, lib.rs:571-581), Photo
  `commands.rs:485-510` + `jobs.rs:118-136` (save dialog `FileDialog::save_file` -> path -> worker `std::fs::write`; or
  `--export-dir`), VideoCut (export bytes `Mp4Muxer::finish` -> `drive.put("videocut/<id>/exports/..")`), Writer / Notes
  (PDF via `reborrow_info`, D8).
- Confirmation: read Show, Sheets, Photo export code; VideoCut from its report + `lib.rs` Export job.
- Proposal (decision for the user): one rule for every Office app - "Export" = a save dialog on desktop
  (`FileDialog::save_bytes`, which Show proves works with bytes) and, when the app runs against S3, ALSO
  `exports/<name>` in the bucket. Implement once in appkit as `export::deliver(info, bytes, name, mime, drive:
  Option<&dyn Drive>)` running the write on a job (D6). The A4 constant (D8) and the "render a DOM to PDF in the callback"
  step stay app code.
- Effort S-M. Risk low. Benefits: Show, Sheets, Photo, VideoCut, Writer, Notes.

#### D23. Trivial helpers re-written per app although a dependency already has them
- Evidence / confirmation / proposal per helper (each read in full):
- `now_secs()` (4 identical lines: `SystemTime::now().duration_since(UNIX_EPOCH).map(as_secs).unwrap_or(0)`):
  `examples/azul-calculator/src/ui.rs:238-243`, `examples/azul-contacts/src/ui.rs:201-206`, `examples/azul-sheets/src/
  storage.rs:84-89`, `examples/azul-drive/src/actions.rs:613-623` (via ms) - Sheets and Drive depend on azul-storage
  directly, Calculator/Contacts through azul-appkit (which can re-export it), and azul-storage exports
  `azul_storage::time::now_unix()` (`time.rs:11-17`, identical body).
- AzWidgets section modules each define the same `keep(data, put)` (downcast `Showcase`, apply, `interactions += 1`,
  RefreshDom): `examples/azul-widgets/src/forms.rs:929-938`, `dialogs.rs:144-153`, `mail.rs:152-161` - one generic
  `keep_with(data, |s| &mut s.forms, put)` in `lib.rs` (DIALOGS report twin, verified).
- Photo's menu `cmd_key()` (`examples/azul-photo/src/ui.rs:168-175`, used at :295: `cfg!(target_os = "macos")` picks LWin vs
  LControl at COMPILE time) re-implements what `menu::accelerator_matches` already does at RUN time (`LWin` = Cmd on
  macOS, Ctrl elsewhere, core/src/menu.rs:36-45, `mac_shortcut_conventions()` also covers X11-on-Mac): use `LWin`
  unconditionally.
- Proposal: delete the copies and call the existing helper (`azul_storage::time::now_unix`, one `keep_with`, `LWin`).
- Effort S. Risk none. Benefits: Calculator, Contacts, Sheets, Drive, AzWidgets, Photo.

#### D24. Byte sizes formatted 4 ways ("1.5 KB" vs "324 GB" vs "12 KB" vs "N KB") - DRIVE2/FB2 twin, verified
- Evidence: `layout/src/widgets/tile.rs:142-160` (`format_bytes`: 1024 steps, ONE decimal below 10, none above:
  "324 GB"; used by `wizard_pages.rs:63,1079,1093-1094,1282,1331`), `examples/azul-drive/src/browse.rs:259-277`
  (`format_size(Option<u64>)`: always one decimal: "324.0 GB", "" for a folder; ~20 call sites in Drive),
  `examples/azul-tasks/src/detail.rs:584-591` (`size_text`: integer KB, one-decimal MB, nothing above MB),
  `examples/azul-photo/src/lib.rs:298` and `:318` (inline `bytes / 1024` "KB").
- Confirmation: read all; same loop in tile/drive (identical UNITS array and 1024 division), diverged only in the
  decimal rule. Not exported (`format_bytes` absent from api.json), so apps cannot call it.
- Proposal: move `format_bytes` out of the Tile widget file into a small non-UI home (`core` - e.g.
  `azul_core::fmt::format_bytes` - or `layout/src/widgets/format.rs`) and export it (e.g. `DiskSpace::format_bytes(u64)
  -> String` static in module `file`, next to `DiskSpace`); Drive's `format_size` becomes `bytes.map(format_bytes)`
  (decide which decimal rule Explorer parity wants: Explorer shows "324 GB" and "1.5 KB" - the widget's rule).
- Effort S. Risk: Drive E2E expectations on "x.0 KB" strings. Benefits: Drive, Tasks, Photo, Setup (via the wizard),
  VideoCut (export size), Mail (attachments).

#### D25. "Every word of the query, any case" search matcher re-written per app
- Evidence: `examples/azul-appkit/src/shortcuts.rs:72-85` (`matches`), `examples/azul-tasks/src/views.rs:230-250`
  (`search_matches`), `examples/azul-notes/src/model.rs` (test at :939 names the same rule), and in widgets
  `layout/src/widgets/dialog_kit.rs:380-415` (`find_ignore_case` / `contains_ignore_case`, whole-query substring, used
  by settings_layout.rs:666 and settings_dialog) + `layout/src/widgets/shells/command_palette.rs:227`
  (`palette_matches`, in-order subsequence). Drive filters by `entry.name.to_lowercase().contains(..)`
  (`browse.rs:367`), Sheets' Find by `to_lowercase().contains` (`ops.rs:257`).
- Confirmation: read appkit + tasks + dialog_kit + palette; three different rules for "search" across one suite (a
  multi-word query matches word-wise in appkit's shortcut list but only as one substring in ShellSettingsLayout).
- Proposal: one exported `String`-level helper pair in core (e.g. `azul_core::text::matches_words(haystack, query)` and
  `find_ignore_case(hay, needle) -> Option<(usize, usize)>` for highlighting) used by settings_layout/settings_dialog,
  appkit, and the apps; keep `palette_matches` as the palette's own fuzzy rule.
- Effort S. Risk low. Benefits: settings dialog, appkit, Tasks, Notes, Drive.

#### D26. Civil-date arithmetic (leap year, days in month, days<->civil, weekday) in 5 places
- Evidence: `examples/azul-calculator/src/datecalc.rs:26-115` (`is_leap`, `days_in_month`, `Date::days` =
  Hinnant `days_from_civil`, `from_days`, `weekday`, `add_months`), `examples/azul-storage/src/time.rs:20-58`
  (private `civil_from_days`, `days_from_civil`, `is_leap`, `days_in_month` - byte-identical bodies to the
  calculator's except `i64` vs `i32`), `layout/src/widgets/date_picker.rs:413-450` (pre-range: `is_leap`,
  `days_in_month` with `_ => 30`, Sakamoto `weekday`), outside the area `examples/azul-calendar/src/rrule.rs:169` and
  `examples/azul-tasks/src/recur.rs:287` (`days_in_month`).
- Confirmation: diffed calculator vs storage `is_leap`/`days_in_month` by eye: identical logic; date_picker differs
  only in the invalid-month fallback (30 vs 0).
- Proposal: a small `azul_core::date` (`CivilDate { year: i32, month: u8, day: u8 }`: `from_days/days/weekday/
  add_months/add_days/days_in_month/is_leap`), used by DatePicker and azul-storage's `time`, exported as
  `CivilDate` in module `time` so AzCalculator's date mode and the apps without chrono stop carrying their own.
  Calendar/Tasks may keep chrono.
- Effort S. Risk low (each copy has tests to port). Benefits: Calculator, DatePicker, azul-storage, Calendar, Tasks.

#### D27. Hand-made selectable rows (thumbnail + two lines) where ThumbnailStrip / Tile / ListView exist
- Evidence: VideoCut media bin `examples/azul-videocut/src/lib.rs:1245-1300` (`media_pane`: div rows, image thumb
  64x36, name + "timecode - WxH - codec" line, MouseUp -> `on_bin_click`, double-click -> `on_bin_open`, selected =
  CSS only - no role, no keyboard, no Tab stop), Show open list `backstage.rs:200-217` (div rows + an Open button),
  Photo start screen `ui.rs:846-888`. Widgets that already do this with a11y + keyboard: `ThumbnailStrip`
  (`content: Dom` thumb + `label` + `selected`, Column/Grid layout, drag reorder, one Tab stop; built for Show's rail
  the same night), `Tile` (`icon/title/detail/on_click/on_double_click/is_selected`, Drive's Content view),
  `ListView` rows.
- Confirmation: read the VideoCut and Show rows; neither sets an accessibility role (grep `with_accessibility` in
  `media_pane`: none).
- Proposal: VideoCut's bin -> `ThumbnailStrip` with `ThumbnailStripLayout::Column` (thumb = the bin image, label =
  name, a `detail` line would need one new field `ThumbnailItem::detail`) - also gives it drag-out-to-timeline later via
  the strip's drag events; Show/Photo open lists -> R3.
- Effort S. Risk low (E2E clicks bin items by text). Benefits: VideoCut (a11y + keyboard), Show, Photo.

#### D28. Office status-bar zoom wiring copied Writer -> Show -> Sheets (struct literals with `callable: OptionRefAny::None`)
- Evidence: `examples/azul-writer/src/editor_ui.rs:268-297` (`StatusBarZoom::office_2013().with_percent(..)`, then
  `zoom.on_zoom_out = Some(button_click(..))`, `zoom.on_slider_change = Some(SliderOnValueChange { data, callback:
  SliderOnValueChangeCallback { cb, callable: OptionRefAny::None } })`, `button_click` = a `ButtonOnClick { .. }`
  literal), `examples/azul-show/src/views.rs:650-707` (`click` = the same literal; the same 9-line slider literal),
  `examples/azul-sheets/src/lib.rs:1656-1690` (`zoom_click` = the same literal; no slider). The status-bar segment
  set ("ENGLISH (UNITED STATES)", word/slide count, view switcher) is also Writer's, copied into Show.
- Confirmation: read the three; Show/Writer slider literals are identical modulo the data variable.
- Bug found while comparing: Show (`commands.rs:568`, `views.rs:669`) and Sheets (`lib.rs:931`) allow 10..400 %, but
  `StatusBarZoom::office_2013()` fixes the slider window at 10..190 (`statusbar.rs:1375-1386`) and nothing sets
  `min/max` - above 190 % the thumb sits at the end and the first slider drag snaps the zoom back below 190 %.
- API gap (root cause): api.json `StatusBarZoom` exports only `office_2013` + `with_percent`; `ButtonOnClick` and
  `SliderOnValueChange` have no constructor (`widgets.ButtonOnClick: ctors=[]`). Proposal: `StatusBarZoom::with_range(min,
  max)`, `with_on_zoom_in(data, cb)`, `with_on_zoom_out(data, cb)`, `with_on_slider_change(data, cb)`, `with_label(bool)`;
  and a generic `create(data: RefAny, callback: <X>CallbackType)` constructor for every `impl_widget_callback!` wrapper
  (`ButtonOnClick::create`, `SliderOnValueChange::create`, ...) so no app writes the `callable` field.
- Effort S. Risk none. Benefits: Writer, Show, Sheets (+ any app with a status bar).

#### D29. E2E debug-server client copied into every app's script although `scripts/azlin_e2e.py` is the shared one
- Evidence: `scripts/azlin_e2e.py:27-140` (`Failure`, `repo_roots`, `find_binary(name, explicit, env_var)`, `strings`,
  `dicts`, `tail`, `class App` with `op/must/...`; imported by azcalculator/azcontacts/azsetup E2Es) vs private copies
  of the same block in `scripts/azsheets_e2e.py:48-160`, `scripts/azshow_e2e.py:47-160`, `scripts/azphoto_e2e.py:41-140`,
  `scripts/azvideocut_e2e.py:45-135`, `scripts/azdrive_e2e.py:56-165`, `scripts/azmeet_e2e.py:52-140` (`Process`
  variant) and, outside the area, calendar/mail/notes/tasks; `examples/azul-shells/scripts/shells_e2e.py` (imported by
  AzSetup's E2E).
- Confirmation: `diff` of azshow_e2e.py:47-160 vs azsheets_e2e.py:48-161 = only the app name, the env var name, a
  wait timeout (3 s vs 5 s) and `App.__init__`'s argument order differ (~110 identical lines).
- Proposal: every script imports `azlin_e2e` (`find_binary("AzShow", args.bin, "AZSHOW_BIN")`, `App(tag, binary,
  args, port, logs, timeout, extra_env)`); app-specific helpers (`frame`, `classes`, `value`) move into `azlin_e2e.App`
  when two scripts use them. AzShells' script imports it instead of being imported.
- Effort S-M (mechanical; scripts never ran, so do it before they are debugged). Risk low. Benefits: 10 E2E scripts.

#### D30. AzDrive's Details view is a hand-built data table because ListView lacks multi-select and column resize
- Evidence: `examples/azul-drive/src/ui_view.rs:701-906` (`column_parts`, `details_header`: per-column div with sort
  arrow, edge drag to resize (`ColumnDrag`), double-click to fit; `details_row`), `:907-948` (`content_row`).
  `layout/src/widgets/list_view.rs` (exported: `with_columns`, `with_sorted_by`, `with_on_column_click`,
  `with_on_row_click`, `with_selected_row` - ONE row, `with_on_lazy_load_scroll`) has no column widths/resize, no
  multi-selection, no per-row check box. CellGrid (in range) carries its own header-edge resize + auto-fit
  (`cell_grid.rs:2105-2140`, `CellGridDragKind::ResizeColumn`, `AutoFitColumn`).
- Confirmation: read Drive's header/row builders and ListView's API (apiq). Two column-resize implementations now
  exist (Drive app, CellGrid widget) and ListView has none.
- Proposal: extend ListView with `column_widths: ListViewColumnWidthVec` + `on_column_resize(index, width)` +
  `AutoFit` on double-click (lift CellGrid's drag code into a shared `header_resize` helper), `selection:
  ListSelection` (D20) + `with_check_boxes(bool)`; then Drive's Details view and AzContacts' list adopt it.
- Effort M-L. Risk medium (ListView is used widely). Benefits: Drive, Contacts, Mail (message list is separate),
  Tasks list, any records app (RecordsShell).

#### D31. File-extension -> media-type tables: azul-storage's (11 types) vs AzMail's (35 types) - S3 uploads get the poor one
- Evidence: `examples/azul-storage/src/s3.rs:238-256` (`content_type_for(key)`, private; the Content-Type of every
  S3 PUT from Drive/Sheets/Show/Photo/VideoCut), `examples/azul-mail/src/compose.rs:546-584` (`mime_type_for`, 35
  types incl. docx/xlsx/pptx/mp4/mov/webp/heic/vcf), plus per-purpose classifiers in AzDrive: `browse.rs:159-194`
  (`kind_of`: "PDF Document", "PNG image" for the Type column) and `preview.rs:55-77` (`PreviewKind` by extension).
- Confirmation: read both mime tables: same `rsplit_once('.')` + lowercase + `match` shape; storage's lacks every
  Office type and video, so `.xlsx` (AzSheets), `.mp4` (AzVideoCut exports), `.pptx` uploaded to a bucket are stored
  as `application/octet-stream` (a browser download of a shared link then loses the type).
- Proposal: one `azul_storage::mime::media_type_for(name) -> &'static str` (Mail's table, `pub`), used by S3 PUTs,
  AzMail attachments and `FileDialog::save_bytes` callers (Show passes literal mime strings); AzDrive's `kind_of` and
  `PreviewKind` can key off the same table (`media_type_for(name).split('/').next()`).
- Effort S. Risk low. Benefits: every app writing to S3, Mail, Drive, Show.

### 2. REUSE CANDIDATES

#### R1. One command table per app feeding ribbon, menu, command palette, shortcut help and key dispatch
- Evidence of the need: per-app command enums with parallel lookups - Tasks `examples/azul-tasks/src/chrome.rs:41-170`
  (`Command::label/icon/shortcut`, already feeds the ribbon AND `ShellCommandPalette`), Photo `ui.rs:166-338`
  (`menu_table`: label + `Command` + key chord, feeds the native `Menu`), Drive `actions.rs` (`Action` + `why_not`
  disabled reasons) + `keys.rs:1-310` (own Key/Mods/Command table), Show `app.rs:270` (`Command`) + `lib.rs:386-450`
  (`editor_shortcut`, `on_window_key`), Sheets `lib.rs:2963-2998` (`on_window_key` match) + Action enum. Shortcut help is free text in
  Show `backstage.rs:275-282`, Sheets `lib.rs:1924`, Drive `ui_dialogs.rs:1038-1042`, and a table in appkit
  `shortcuts.rs`. `ShellCommandPalette` (exported; "the menu, the ribbon, a toolbar and the palette are all views of ONE
  list of commands", command_palette.rs:1-3) is used only by Tasks, Notes and the AzShells demo.
- Confirmation: read Tasks' `Command` impl (label/icon/shortcut), Photo's `menu_table` + `native_menu`, Drive's
  `keys.rs` header and `ui_ribbon.rs` button helpers, Show's `editor_shortcut`, Sheets' `on_window_key`, and
  `ShellCommandPalette`/`ShellPaletteCommand` in api.json (`create`, `with_shortcut/icon/category`).
- Proposal: `examples/azul-appkit/src/commands.rs`: `trait AppCommand: Copy + 'static { fn all() -> &'static [Self];
  fn label(self) -> &'static str; fn icon(self) -> &'static str; fn category(self) -> &'static str;
  fn shortcut(self) -> &'static [VirtualKeyCode] /* LWin = primary */; }` + helpers `ribbon_button(app, cmd, on_click,
  disabled: Option<&str>) -> RibbonButton`, `menu_item(..) -> StringMenuItem` (accelerator from `shortcut`),
  `palette_commands() -> Vec<ShellPaletteCommand>`, `shortcut_rows() -> Vec<appkit::Shortcut>`,
  `command_for_key(keyboard, pressed) -> Option<Self>` (needs D14's `VirtualKeyCodeCombo::matches` export). Rust-generic,
  so appkit (not api.json) is the right home; a C/Python app uses the palette/menu types directly.
- Adopters: Sheets, Show, Photo, Drive, VideoCut (+ Tasks, Notes, Writer, Mail). Each app keeps its own enum and its
  `why_not(state, cmd)`.
- Effort M. Risk low (pure refactor; E2E clicks by label stay). Benefits: Sheets, Show, Photo, Drive, VideoCut,
  Tasks, Notes, Writer, Mail.

#### R2. `PropertyGrid` widget for inspector panels, built from the settings dialog's value model
- Evidence: Photo `ui.rs:112-160` (`number`, `slider`, `check`, `segments`: label + control + `Field` enum routed to
  `commands::on_number/on_slider/on_check/on_segment`), Show `views.rs:500-548` (`frame_field`: label + NumberInput
  -> `FrameField` -> `on_frame_field`), VideoCut `lib.rs:1420-1440` (`effect_field`: NumberInput -> `EffectField`).
  The value model already exists: `layout/src/widgets/shells/settings_dialog.rs:191-300` (`ShellSettingValue {Toggle,
  Choice, Number+unit, Text, Path, Color, Shortcut, Slider, Radio}`) and its `control()` builder (:1121-1256) +
  `row()` (:1257) with dialog_kit's look.
- Confirmation: read the three app helpers and `settings_dialog::control` (:1121-1256): the same controls
  (NumberInput/Slider/Switch/DropDown/ColorInput) with a label and one change event.
- Proposal: `layout/src/widgets/property_grid.rs`: `PropertyGrid { rows: PropertyRowVec, label_width: f32, theme }`,
  `PropertyRow { id: String, label, value: ShellSettingValue (rename to `PropertyValue`, keep alias), group: String,
  disabled_reason: String }`, one `on_change(PropertyGridEvent { id, index, value })` callback; settings_dialog's
  `control()` moves there and settings_dialog calls it. Adopters: Photo (Properties / tool options), Show (format
  pane), VideoCut (effect controls), Sheets (a future Format Cells), AzWidgets demo.
- Effort M. Risk medium (settings_dialog refactor; its tests cover the controls). Benefits: Photo, Show, VideoCut,
  Sheets, AzWidgets, AzSetup (settings dialog keeps working).

#### R3. `RecentDocuments` / backstage "Open" pane
- Evidence: Show `backstage.rs:189-229` (rows with title + "N slides - show/<id>" + Open, ShellEmptyState when empty,
  Refresh/Sample buttons), Sheets `lib.rs:1844-1872` (Browse button + Link button per workbook + "No workbooks"),
  Writer `backstage_ui.rs:219-295` (Office-2013 places column + `recent_row` + Browse), Photo `ui.rs:846-888`
  (`start_screen`). The data side is D5.
- Confirmation: read the four Open/start panes: four layouts for one list; none shows the modified date it sorts by
  (Photo shows the size, Show the slide count); Sheets/Show/Photo rows are buttons (Tab-reachable, but no list role or
  arrow keys); Writer's `recent_row`s are static sample rows without a callback (`backstage_ui.rs:275-283`).
- Proposal: `layout/src/widgets/recent_documents.rs`: `RecentDocuments { items: RecentDocumentVec { title, detail,
  icon, modified_unix: u64, pinned }, empty_title, empty_detail, browse_label, on_open(index), on_browse, on_pin }` with
  the date grouping Outlook/Office use ("Today", "Yesterday", "Older" - message_list.rs already groups by day). Adopters:
  Sheets, Show, Photo, Writer, VideoCut (project picker), Notes.
- Effort M. Risk low. Benefits: the adopters above.

#### R4. Zoom / pan viewport model (`ZoomView`) from AzPhoto
- Evidence: `examples/azul-photo/src/view.rs:17-130` (`ZOOM_STEPS` 1 %..3200 %, `View { zoom, pan_x, pan_y, width,
  height }`, `fit`, `center`, `doc_to_view/view_to_doc`, `zoom_about(zoom, vx, vy)`, `step(dir)`), status-bar zoom
  clamps in Show `commands.rs:568` / `views.rs:669` (10..400), Sheets `lib.rs:931` (10..400), Writer `lib.rs:824-847`
  (10..190, +-10 steps), VideoCut `lib.rs:1940` (`zoom_about` on the time axis). `StatusBarZoom` (statusbar.rs:1351-1395)
  has `percent/min/max` but no step rule, so each app invents its own (+-10 linear in Writer/Show, Office uses
  10,25,50,75,100,...).
- Confirmation: read Photo's `View`, the zoom clamps in Show/Sheets/Writer and `StatusBarZoom`'s fields.
- Proposal: a no-UI `ZoomView` in `core` (or `layout/src/widgets/zoom.rs`): Photo's `View` + `next_step(dir)` over a
  steps table; export `StatusBarZoom::with_range(min, max)`, `with_on_zoom_in/out(data, cb)`, `with_on_slider(data, cb)`
  and `StatusBarZoom::step(percent, dir) -> f32` so Show/Sheets/Writer stop hand-building it (see A3).
- Effort S-M. Risk low. Benefits: Photo, Show, Sheets, Writer, Paint, VideoCut.

#### R5. The Office-app skeleton = appkit, extended (args + data root + settings + jobs + storage + about)
- Evidence: D2, D3, D6, D7, D13 are the same five concerns re-solved per app; appkit already has `args`, `data`,
  `settings`, `files`, `about`, `ui::Kit` but only Calculator/Contacts use it (they were written the same night).
- Confirmation: see the confirmations of D2, D3, D6, D7, D13; `git grep azul_appkit` lists only Calculator/Contacts.
- Proposal: extend appkit with (a) app switches (D2), (b) `jobs::spawn` (D6), (c) `files::list_folder_all` +
  `recent_documents` (D4/D5), (d) a held `Arc<dyn Drive>` in the Kit (LocalDrive today, S3Drive later - the ONE place
  that changes for the S3 split), (e) `history::UndoHistory<T>` (D10); then move Sheets, Show, Photo, VideoCut, Drive,
  Meet onto it one app per commit. Office-app wave item "Office scaffold" in the ledger is exactly this.
- Effort L (spread over apps). Risk low per step. Benefits: every app.

#### R6. `LayerList` - tree rows with per-row toggle icons (eye / lock / expand) and drag reorder
- Evidence: Photo `ui.rs:605-648` (layers: depth indent, visibility + lock + expand icon buttons, kind icon, name,
  "Blend 50 %" badge, press/release drag via `LayerPress/LayerRelease`), Timeline track headers
  (`layout/src/widgets/timeline.rs`, "mute/show and lock toggles as link Buttons named 'Hide V1' / 'Lock V1'"),
  Show's selection pane (not built; PowerPoint's has the same eye column). `TreeView` (extended in range with a drop
  hook) has depth + expand + drop but no per-row toggles.
- Confirmation: read Photo's `layers_panel` (ui.rs:568-662) and TreeView's public API (depth, expand, drop hook).
- Proposal: `layout/src/widgets/layer_list.rs` - `LayerList { rows: LayerRowVec { id: u64, depth: u32, icon, label,
  detail, visible: bool, locked: bool, expandable, expanded, selected }, on_event(LayerListEvent { kind: Select|
  ToggleVisible|ToggleLock|ToggleExpanded|Move{to, into}|Rename, id, shift, ctrl }) }` on TreeView's keyboard/drop
  code; Timeline's header column can render with it. Adopters: Photo, Show (selection pane), VideoCut (track list),
  a future AzDraw.
- Effort M. Risk low. Benefits: Photo, Show, VideoCut, AzDraw.

#### R7. `ColorPalette` - Office's colour gallery (theme row + standard row + No colour + More colours...)
- Evidence (the workarounds): Sheets cycles a sheet tab through 5 fixed colours per click (`examples/azul-sheets/src/
  lib.rs:2391-2399`, `COLORS` + `(i + 1) % len`) and has fixed `InkRed` / `FillYellow` / `FillGreen` actions
  (`lib.rs:2254-2257`); Photo's swatch panel is 12 plain divs with a MouseUp callback, no role / keyboard
  (`examples/azul-photo/src/ui.rs:520-532`, `state.rs:321-335`); Show builds swatch cells for ribbon galleries
  (`ribbon.rs:165-172`) and "Accent / Accent 2 / Accent 3 / None" buttons in the format pane (`views.rs:550-606`).
  `ColorInput` (exported) is ONE swatch that opens a picker window - no palette. SHEETS lists ColorPicker / BorderPicker
  as backlog widgets.
- Confirmation: read the Sheets/Photo/Show swatch code and `color_input.rs`'s module doc (one swatch + picker
  window, no palette).
- Proposal: `layout/src/widgets/color_palette.rs`: `ColorPalette { theme_colors: ColorUVec (with tint/shade rows
  generated by `ColorU::lighten/darken`, already exported), standard_colors: ColorUVec, recent: ColorUVec, selected:
  OptionColorU, allow_none: bool, more_label: String }`, `on_pick(OptionColorU)`, "More colours..." opens `ColorInput`'s
  picker; usable inline (Photo panel), in a `RibbonGallery` drop-down (Show/Sheets/Writer font colour + fill), and in a
  Popover (Sheets tab colour). Theme row for Office apps = `ShellThemeAccent::colors` (A1) or the document theme
  (Show's `ColorScheme`).
- Adopters / benefits: Sheets, Show, Photo, Writer, Paint, Notes (highlight). Effort M. Risk low.

### 3. API.JSON EXPOSURE (names only - add through `azul-doc autofix`)

Checked against api.json at `53c978b33` with a script over `api["0.2.0"]["api"][module]["classes"]`.

| # | Export | Module | Rust source | Why (who works around it) | Finding |
|---|---|---|---|---|---|
| A1 | `ShellThemeAccentColors` (struct), `ShellThemeAccent::colors(dark: bool)`, `::name() -> String`, `::from_name(String) -> OptionShellThemeAccent`, `::all()` | shells | `layout/src/widgets/shells/theme_scope.rs:56-142` | AzShow copies the 5 ramps (`themes.rs:30-64`) | D1 |
| A2 | `ButtonOnClick::create(RefAny, ButtonOnClickCallbackType)`, `SliderOnValueChange::create(..)` (generic: every `impl_widget_callback!` wrapper gets `create`) | widgets | `layout/src/widgets/button.rs`, `slider.rs` (macro) | Writer/Show/Sheets write `{ data, callback: { cb, callable: OptionRefAny::None } }` literals | D28 |
| A3 | `StatusBarZoom::with_range(min, max)`, `with_on_zoom_in/out(RefAny, cb)`, `with_on_slider_change(RefAny, cb)`, `with_label(bool)` | widgets | `statusbar.rs:1351-1400` | same 3 apps; 400 % zoom outside the 10..190 slider | D28 |
| A4 | `RibbonColumn/RibbonRow::with_items(RibbonItemVec)`, `RibbonGroup::with_items`, `RibbonTab::with_groups(RibbonGroupVec)`, `RibbonItem::large/small(RibbonButton)` | widgets | `ribbon.rs:2574-3070` | 6 apps fold `with_item` | D9 |
| A5 | `KeyboardState::primary_down/shift_down/ctrl_down/alt_down/super_down/is_key_down(VirtualKeyCode)`; `VirtualKeyCodeCombo::matches(KeyboardState, VirtualKeyCode)` (= `menu::accelerator_matches`) | dom (+ menu) | `core/src/window.rs:600-665`, `core/src/menu.rs:47` | 25 handlers write `ctrl \|\| meta` (Win key = Ctrl bug) | D14 |
| A6 | `ColorU::to_hex() -> String`, `ColorU::parse_hex(String) -> OptionColorU`, `ColorU::try_from_css(String) -> OptionColorU` | css | move `color_input.rs:586-625` into `css/src/props/basic/color.rs` | 8 private hex helpers; `from_str` returns BLACK on error | D11 |
| A7 | `RawImage::create_rgba8(w, h, U8Vec, premultiplied)`, `RawImage::resized(w, h) -> OptionRawImage`, `RawImage::fit_within(w, h, max_w, max_h)` (static, returns a size pair struct) | image | `core/src/image_scale.rs:362,659` | VideoCut `fit_within`/`scale_to`, 9 `RawImage {..}` literals | D15 |
| A8 | `Uuid::random() -> String` (seeded from the OS/clock into `from_seed`), `Uuid::is_valid(String) -> bool` | uuid | `layout/src/uuid.rs` | Photo/Show/VideoCut mint deterministic file ids | D7 |
| A9 | `Xml::escape_text(String) -> String` / `escape_attr` | xml | new `azul_core::xml::escape_xml` | 7 private escapers incl. AzMail | D19 |
| A10 | `format_bytes(u64) -> String` as e.g. `DiskSpace::format_bytes` (static) | file | `layout/src/widgets/tile.rs:142` (move out of the widget) | Drive/Tasks/Photo own formatters | D24 |
| A11 | `Button::with_disabled(reason: String)` / `set_disabled`, `Button::with_toggled(bool)` (aria-pressed) | widgets | `button.rs` (RibbonButton already has both) | Calculator draws a dimmed `div` for a disabled key (`ui.rs:610-613`, no button, no a11y state) and uses `ButtonType::Primary` as "toggled" (SMALLAPPS); Photo dims a still-clickable tool with `opacity: 0.45` (`ui.rs:443`); dialog_kit's `row_button` fakes it for every dialog (DIALOGS) | new |
| A12 | `ThumbnailItem::detail` (second line) | widgets | `thumbnail_strip.rs:107-125` | VideoCut bin rows | D27 |
| A13 | `CivilDate` (from_days/days/weekday/add_months/days_in_month/is_leap) | time | new `azul_core::date` | Calculator date mode, DatePicker, azul-storage | D26 |
| A14 | `matches_words(haystack, query) -> bool` (+ `find_ignore_case`) as a `String`-level helper | str | `dialog_kit.rs:380-415` | appkit/Tasks/Notes search | D25 |
| A15 | Reading the system clipboard outside a paste event (`CallbackInfo::request_clipboard_text()` -> an event/write-back) | callbacks | dll clipboard | AzSheets' ribbon Paste keeps an internal clipboard; AzCalculator pastes only via the Paste event (both reports) | not verified in code beyond the reports |
| A16 | `CallbackInfo::prevent_window_close()` (veto a `CloseRequested`) | callbacks | dll window state flags | AzMail clears `flags.close_requested` by hand (`ui_compose.rs:764-767`); Office apps have no unsaved-changes guard | N14 |
| A17 | `Pdf::A4_WIDTH_PX` / `A4_HEIGHT_PX` consts (or `PdfPageSize::a4()`) | pdf | dll pdf module | `794.0, 1123.0` literals in Sheets/Notes/Writer | D8 |
| A18 | `Uuid::marker()` / `short_marker()` as the honest names of today's `v4` / `short` (keep old names as deprecated aliases) | uuid | `layout/src/uuid.rs` | the name `v4` misled 3 apps | S1 |

Not api.json (Rust example crates, for the record): `azul_storage::ops::list_folder_all` (D4), `Drive::describe(key)`
(D21), `azul_storage::mime::media_type_for` (D31), appkit `jobs`, `history::UndoHistory`, `commands::AppCommand`,
`export::deliver`, `guard_close` (D6, D10, R1, D22, N14).

Already exported and simply unused by the apps (no API work, adoption only): `AboutDialog`, `MessageBox`,
`ProgressDialog`, `FindReplaceDialog`, `ShellSettingsDialog`, `PathInput`, `ShortcutRecorder` (D12/D13),
`RawImage::thumbnail` (VideoCut), `Uuid` (wrong one, D7), `ShellCommandPalette` (R1), `MessageListSelection` (D20),
`Timeline::format_timecode` (VideoCut uses it - good).

### 4. SEMANTICS / PLACEMENT

#### S1. `Uuid::v4` is named like an RFC random v4 but is a deterministic per-process counter
- Evidence: `layout/src/uuid.rs:20-45` (module doc: "These ids are DETERMINISTIC, not random"; "the first Uuid::v4
  in every process is 00000000-0000-4000-..."), exported as `uuid.Uuid.v4`. Three Office apps took the name at face
  value (D7).
- Confirmation: read `layout/src/uuid.rs:1-80` and the three call sites (D7).
- Proposal: rename the marker mint to `Uuid::marker()` / `Uuid::short_marker()` (keep `v4` one release as a
  deprecated alias with a doc warning in the generated bindings) and add `Uuid::random()` (A8). Effort S, Risk: API
  rename (autofix + bindings regen). Benefits: every app that stores ids.

#### S2. One accent family, three names: `Blue` / "Stone" / `STONE_ACCENT`
- Evidence: `ShellThemeAccent::Blue` (`theme_scope.rs:44`), AzShow's `STONES[0].name = "Stone"` (`themes.rs:32`),
  flora's `STONE_ACCENT` + `FloraStone.stone` field (`flora.rs:4084-4100`) - and `ShellThemeAccentColors.accent` vs
  `FloraStone.stone` for the same face colour. Proposal: with D1, name the field `accent` in both structs and let AzShow
  show `ShellThemeAccent::name()` ("blue") capitalised. Effort S.
- Confirmation: read the three definitions. Risk none (names only). Benefits: AzShow, theme code readers.

#### S3. `ColorU::to_hash()` means "to hex string"
- Evidence: `css/src/props/basic/color.rs:889-891` (`format!("#{:02x}{:02x}{:02x}{:02x}")`), exported as `to_hash`.
  "hash" reads as a hash value. Proposal: `to_hex()` (A6), `to_hash` kept as an alias. Effort S.
- Confirmation: read the body. Risk: an alias keeps old bindings working. Benefits: every app formatting colours.

#### S4. Generic helpers living inside widget files (so they cannot be shared or exported cleanly)
- Evidence / confirmation (each read; callers found with grep):
- `format_bytes` in `layout/src/widgets/tile.rs:142` (used by wizard_pages too) -> a format util (D24).
- `find_ignore_case`, `contains_ignore_case`, `percent_text`, `line`, `highlighted_line` in
  `layout/src/widgets/dialog_kit.rs:340-415` (used by shells/settings_layout.rs:666 and settings_dialog) - the string
  helpers belong in a text util (D25); the Dom builders may stay in dialog_kit.
- `ShellSettingValue` (`shells/settings_dialog.rs:191`) is a general property-value model (R2) living in a shell file.
- `MessageListSelection` (`message_list.rs:423`) is a general list selection named after the mail list (D20).
- `wheel_steps` private to cell_grid.rs (D17).
- Proposal: move each to the home named in the linked finding. Effort S each. Risk low (crate-internal moves).
  Benefits: whoever needs the helper next (wizard pages, settings, CellGrid, Timeline).

#### S5. Shared library crates live under `examples/`
- Evidence / confirmation (read the Cargo.toml files):
- `examples/azul-storage` (the S3 / LocalDrive library every app depends on) and `examples/azul-appkit` (the app
  skeleton) are libraries, not examples; AzShow depends on the AzWriter APP crate for its rich-text IR
  (`examples/azul-show/Cargo.toml:22-24`, `AzWriter = { path = "../azul-writer" }`, building AzWriter's cdylib too) -
  the IR should become azul's RichTextEditor model (see the EDITORS report A2). Proposal: keep the paths for now (the
  release tooling copies `examples/`), but document in `examples/README` which crates are shared libraries; break the
  app->app dependency when the RichTextEditor lands. Effort S (doc) / M (IR move).
- Risk low. Benefits: build clarity; AzShow and AzWriter decoupled.

#### S6. "Theme" overloaded inside AzShow and the app-folder constant spelled four ways
- Evidence / confirmation (read; constants found with grep):
- AzShow: `model::Theme` (deck design: colours + fonts, `model.rs:140`), `themes.rs` (deck theme presets), the app
  theme flat/flora (`--theme`, `commands.rs:488-500`). Suggest `DeckTheme` for the document one.
- The app's folder in the bucket: Sheets `storage::DIR = "sheets"`, Show `APP_FOLDER = "show"`, VideoCut
  `APP_FOLDER = "videocut"`, Photo `PREFIX = "photo/"`, appkit `AboutInfo::app_folder` - fold into appkit's
  `data::app_key/app_prefix` (D3/R5). Effort S.
- Risk none. Benefits: AzShow readers; the S3 key layout in one place (appkit).

### 5. OTHER NEXT-WAVE ITEMS (bugs, dead code, TODOs, gaps)

- N1 (BUG, high). AzPhoto start document id is the same on every launch -> overwrite + stale-tile deletion of the
  previous session's document: `examples/azul-photo/src/lib.rs:399` + `:164-167`, `storage.rs:231-244`. Same class:
  Show `lib.rs:521` / `commands.rs:463,470`, VideoCut `lib.rs:2839,2858,2874`. Fix = D7.
- N2 (BUG). AzShow's deck list ignores `page.next` (`examples/azul-show/src/storage.rs:126-129`) - D4.
- N3 (BUG). Status-bar zoom slider fixed to 10..190 while Show/Sheets zoom to 400 % (`statusbar.rs:1375-1386`,
  `show/src/commands.rs:568`, `sheets/src/lib.rs:931`) - D28.
- N4 (BUG). `ctrl || meta` = shortcuts also fire on the Windows/Super key, and on Ctrl as well as Cmd on macOS
  (21 files) - D14.
- N5 (latent, small). `examples/azul-sheets/src/model.rs:207-222` `tsv_of` returns "" silently when rows differ in
  length (no `flexible(true)`, unlike the engine's twin `ironcalc_engine.rs:240`); today the ribbon's internal
  clipboard (`inputs_of(area)`, lib.rs:2234-2240) is always rectangular, so it only bites once a ragged source exists.
- N6 (dead code). `examples/azul-drive/src/browse.rs:447-473` `up`, `crumbs`, `upload_key` are called only by their
  own tests (`browse.rs:843-870`); the UI uses `crumbs_of` / `place_up` / `fileops::check_name` (DRIVE2 said so;
  verified with grep over `examples/azul-drive/src`). Delete with their tests.
- N7 (TODO, resolvable now). `TODO(DIALOGS)` at `examples/azul-appkit/src/about.rs:5`, `ui.rs:18` and `ui.rs:573`: `AboutDialog` exists and is exported - D12.
- N8 (rule drift). AzMeet writes its settings file with `std::fs` + rename inside a write-back on the UI thread
  (`examples/azul-meet/src/lib.rs:4034` -> `save_server` `:4077-4085`); every other app puts file I/O on a Thread.
- N9 (stale report note). DIALOGS said `examples/azul-shells/src/lib.rs` uses `VirtualKeyCode` without importing it;
  it is imported now (`lib.rs:28`). Nothing to do.
- N10 (small). VideoCut's `image_of` (`lib.rs:718-728`) marks the straight-alpha `Canvas` (render.rs:18 says
  "straight alpha") as `premultiplied_alpha: true`; harmless while every picture is opaque (composited over black),
  wrong the day a transparent layer is shown in a monitor.
- N11 (gap). Sheets' formula autocomplete is a row of Link buttons (`examples/azul-sheets/src/lib.rs:1548-1575`): no
  keyboard selection, no popup; ComboBox has no type-to-filter (`combobox.rs:23` "TODO2"). A `SuggestionPopup`
  (anchored list, Up/Down/Enter/Escape) would serve the formula bar, AzMail recipients and the address bar.
- N12 (gap). Photo's crop / free-transform handles are drawn into the bitmap (`view.rs` overlays) and "free transform
  handles" are listed as not built (PHOTO report); `SelectionAdorner` (built for Show the same night: handles, rotate,
  guides, snapping, nudge) is the widget to put over Photo's canvas node for Move/Transform/Crop.
- N13 (consistency). Theme/mode choice is not persisted in Sheets/Show/Photo/VideoCut/Meet/Drive (D13) and Photo's chrome
  ignores flat vs flora (D16).
- N14 (gap, data loss). No Office app asks before its window closes with unsaved work: Photo tracks `modified`
  (`state.rs:296`), Show `Editor::dirty` (`editor.rs:78-82`), Sheets keeps an undo state, VideoCut a revision - none
  registers `WindowEventFilter::CloseRequested` (`git grep CloseRequested` -> only AzCalendar `editor_ui.rs:186` and
  AzMail `ui_compose.rs:358`). AzMail's veto is a trick: clear `state.flags.close_requested` and
  `modify_window_state` (`ui_compose.rs:764-767`, "a cleared flag is what every backend reads as stay open").
  Proposal: appkit `guard_close(dirty: fn(&RefAny) -> bool)` showing `MessageBox` (Question: Save / Don't Save /
  Cancel) + an explicit API `CallbackInfo::prevent_window_close()` (module callbacks) instead of the flag trick.
  Effort S-M. Benefits: Sheets, Show, Photo, VideoCut, Writer, Notes.

## Report twins re-checked (known items from the 8 reports)

| Report item | Status at 53c978b33 | Where in this report |
|---|---|---|
| SHEETS: `cell_grid::wheel_steps` vs time_picker `take_scroll_steps` | confirmed, diverged (cap, owner reset) | D17 |
| SHEETS: `cells_to_tsv` vs `model::tsv_of` | confirmed, plus a THIRD copy `ironcalc_engine::tsv_of` | D18, N5 |
| SHEETS: 3 xml/html escapers | confirmed; 7 in total now | D19 |
| SHEETS: `ButtonOnClick {..}` + `reborrow_info` copied from AzWriter | confirmed (also in Show / Notes); `reborrow_info` is unnecessary | D8, D28 |
| DRIVE2: VideoCut `render::fit_within` = `image_scale::fit_within` | confirmed, diverged at zero sizes | D15 |
| DRIVE2: PHOTO `view::thumbnail` is not a twin of `RawImage::thumbnail` | agreed (tile grid input) | D15 |
| DRIVE2: `browse::format_size` vs `tile::format_bytes` | confirmed, decimal rule differs; + Tasks, Photo | D24 |
| DRIVE2: `browse::upload_key` / `crumbs` / `up` unused | confirmed: test-only | N6 |
| DRIVE2: tree drop hook twin removed | verified: one `on_tree_row_drop` in tree_view.rs:1170, navigation_pane wraps it | - |
| SHOW: `themes.rs` stone ramps vs `theme_scope.rs` | confirmed; flora.rs has a third copy (`FloraStone`) | D1 |
| SHOW: every app keeps its own CLI parser | confirmed: 12 parsers | D2 |
| SHOW: AzWriter IR used by two apps via a crate dependency | confirmed (`Cargo.toml:22-24`) | S5 |
| PHOTO: `translate_dirty_rect` twin in `translate_update_image` | verified resolved: one fn, wr_translate2.rs:1555 | - |
| PHOTO: `brush_dab_coverage` single-sourced via `RawImage::paint_dot` | agreed | - |
| VIDEOCUT: `annexb_nals` / `append_avcc_as_annexb` twins | verified resolved: container.rs:45,85 only | - |
| SMALLAPPS: `shells_e2e.py` App driver vs `azlin_e2e.py` | confirmed; 8 more scripts carry the same block | D29 |
| SMALLAPPS: AzWriter `args.rs` could move onto `azul_appkit::args` | confirmed; needs app switches first | D2 |
| MEET2: CoreVideo pixel-format constants twin | verified resolved: `capture_slot.rs:32-36` only | - |
| DIALOGS: each E2E script has its own debug-server client | confirmed | D29 |
| DIALOGS: AzWidgets sections each have `keep` | confirmed (3 copies) | D23 |
| DIALOGS: Button has no `disabled` | confirmed; Calculator and Photo work around it too | A11 |
| DIALOGS: AzShells uses `VirtualKeyCode` without importing it | stale: imported at `azul-shells/src/lib.rs:28` | N9 |

Checked and clean (no duplicate found): AzSetup uses WizardLayout + all 8 wizard pages + ShellSettingsDialog +
MessageBox + AboutDialog + Modal (no hand-rolled copies); AzMaps uses `MapWidget`; AzDrive uses AddressBar,
DetailsPane, Tile, StatusBar, ShellNavigationPane, `RawImage::thumbnail`; AzMeet's tile arrangement (`tiles.rs`) is
app policy over `CallShell::stage` (no grid math twin); AzVideoCut uses `Timeline::format_timecode`; azul-builder
(51 lines) has nothing to share.

## PRIORITIZED NEXT-WAVE TASK LIST (most value per effort first)

1. **Random file ids** (D7, N1, A8, S1) - S. Photo/Show/VideoCut switch to a random mint (appkit `new_uuid` today,
   `Uuid::random` once exported); rename `Uuid::v4` -> `marker`. Stops silent overwrites on the second launch.
2. **S3-readiness checklist for the Office apps** (D3, D4/N2, D21) - S. One data root (`Azlin/`) via
   `azul_appkit::data::data_root`; Show paginates its deck list (`list_folder_all` in azul-storage `ops`); Sheets and
   Show hold one `Arc<dyn Drive>`; `Drive::describe(key)` for user-facing locations.
3. **Export `KeyboardState::primary_down` & friends + `VirtualKeyCodeCombo::matches`** (D14, A5, N4) - S API, then a
   mechanical `ctrl || meta` -> `primary_down()` sweep over 21 files.
4. **Delete the 3 `reborrow_info` copies** (D8) - S, trivial (`*info`).
5. **Export `ShellThemeAccent::colors` + `ShellThemeAccentColors`** (D1, A1, S2); AzShow reads its stones from it;
   theme_scope builds its table from flora's stones (+ `STONE_PLUM`) - S.
6. **StatusBarZoom setters + `create` for callback wrappers** (D28, A2, A3, N3) - S; fixes the 10..190 slider vs
   400 % zoom bug in Show/Sheets and removes `callable: OptionRefAny::None` literals.
7. **Adopt the standard dialogs** (D12, N7): VideoCut About/Export -> AboutDialog/ProgressDialog, Drive confirms ->
   MessageBox, Sheets Find -> FindReplaceDialog, Photo `sheet_frame` -> Modal, appkit About -> AboutDialog - S each.
8. **Small exports that kill private copies**: Ribbon `with_items` (D9, A4), `ColorU::to_hex/parse_hex` (D11, A6),
   `format_bytes` (D24, A10), `Button::with_disabled/with_toggled` (A11), `RawImage::create_rgba8/resized` (D15, A7),
   `Xml::escape_text` (D19, A9) - S each, one autofix batch.
9. **azul-storage `mime::media_type_for`** (Mail's table) for S3 PUTs and `save_bytes` callers (D31) - S.
10. **appkit growth, part 1**: `jobs::spawn` (D6), `history::UndoHistory<T>` (D10), `files::recent_documents` (D5) -
    S-M; adopt in Photo/VideoCut/Show/Sheets as they are touched.
11. **appkit growth, part 2**: app switches in `args` (D2) and the settings page on `ShellSettingsDialog` with persisted
    theme/mode (D13, N13), then move Sheets/Show/Photo/VideoCut/Meet/Drive onto `Kit` one app per commit - M.
12. **Unsaved-changes guard** (N14, A16): `CallbackInfo::prevent_window_close()` + appkit `guard_close` with a
    MessageBox - S-M.
13. **E2E scripts import `azlin_e2e`** (D29) - S-M, best done before the scripts are first debugged.
14. **`ListSelection`** generalised from `MessageListSelection` (D20) - S-M; Show rail, Drive, VideoCut.
15. **Command table** in appkit (R1) feeding Ribbon, Menu, ShellCommandPalette, shortcut help and key dispatch - M.
16. **New widgets for the Office wave**: `ColorPalette` (R7), `PropertyGrid` (R2), `RecentDocuments` (R3),
    `LayerList` (R6), ListView column resize + multi-select (D30), `SuggestionPopup` (N11), `ZoomView` (R4) - M each.
17. **Leftovers** - S each: VideoCut bin -> ThumbnailStrip (D27), wheel helper (D17), one TSV rule (D18, N5),
    word-search helper (D25), `CivilDate` (D26), `now_secs` -> `azul_storage::time::now_unix` (D23), Photo palette ->
    `system:` colours (D16), Drive dead helpers (N6), Meet settings write off the UI thread (N8), VideoCut
    premultiplied flag (N10); decision needed on one Export destination rule (D22).

## Seen but NOT verified (no claim made beyond this list)

- A15 (reading the clipboard outside a paste event): taken from the SHEETS / SMALLAPPS reports; the dll clipboard code
  was not read.
- Photo's `BlendMode` (8 modes, `raster/blend.rs:21`) vs css `StyleMixBlendMode` / cpurender's blend math: names
  overlap, pixel math not compared.
- Photo's raster filters / adjustments (`raster/filter.rs`, `adjust.rs`) vs any cpurender filter (blur) code: not
  compared.
- VideoCut's CPU compositor (`render.rs`, premultiplied over, crossfade) vs Photo's `blend.rs` premultiplied helpers:
  same math family, not diffed.
- AzShow `text.rs` (rich text over AzWriter's IR): left to the EDITORS review.
- AzNotes' search rule: only its test name (`model.rs:939`) was read.
- AzMeet chat panel (`ui.rs` side panel + `chat.rs`) as a future shared `ChatPanel` with an AzChat app: no second
  consumer exists yet.
- SHEETS gap `AccessibilityInfo` without `row_count/column_count` (aria-rowcount): from the report, not checked.
