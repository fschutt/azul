//! The history in the whiteboard's panel: a button that asks WHY the board is as it is now (a sheet with the changes since
//! the last snapshot worked out for the person, a text area for the reason and a field for tags), the list of snapshots
//! (their revision, their date, the first line of why, what changed), a snapshot looked at read-only with its reason in a
//! banner, "compare with the previous" (what was added is outlined green, what was taken off is a red ghost, what moved
//! has an arrow from where it was) and "restore" (the old state becomes a new snapshot: the history is only added to).
//! What the clicks ask is [`Panel::history_action`]; a change of the history is kept by the host like any change of the
//! board ([`crate::whiteboard_ui::Host::on_changed`]). A dictation host hands what was heard to [`Panel::dictated`].

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, TextAreaOnFocusLostCallbackType, TextAreaOnTextInputCallbackType, TextInputOnFocusLostCallbackType,
        TextInputOnTextInputCallbackType,
    },
    dom::AttributeType,
    prelude::*,
    str::String as AzString,
    widgets::{Button, OnTextInputReturn, TextArea, TextAreaState, TextInput, TextInputState, TextInputValid},
};
use azul_appkit::l10n::{label, t, t_args, Arg};

use crate::{
    whiteboard_app::HEADER_PX,
    whiteboard_history::{date_utc, short_rev, Snapshot, Summary},
    whiteboard_jump::FileView,
    whiteboard_relocate::RefStatus,
    whiteboard_ui::{say, Host, Panel},
};

/// How many lines of what was done since the last snapshot start the reason, and how long each may be (a host that has
/// hundreds of them gets the first ones and a line that says how many are left out).
pub const MAX_MATERIAL_LINES: usize = 40;
pub const MAX_MATERIAL_CHARS: usize = 200;

/// What the sheet that takes a snapshot is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SheetKind {
    /// A snapshot of the board as it is.
    Snapshot,
    /// The board becomes what this snapshot was, and is a snapshot of its own.
    Restore(String),
}

/// The sheet that asks why: what has been typed in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sheet {
    pub kind: SheetKind,
    pub why: String,
    pub tags: String,
}

/// The state of the history's part of the panel.
#[derive(Debug, Clone, Default)]
pub struct HistoryUi {
    /// The list of snapshots is open.
    pub list: bool,
    pub sheet: Option<Sheet>,
    /// The revision the person said "not now" to when asked whether to take a snapshot of the code that moved: asked once
    /// for each revision, once in a session.
    pub head_dismissed: Option<String>,
    /// What the app knows of what happened since the last snapshot (the conversations that were resolved, the patches that were
    /// applied), one line each: the start of the reason when the code moved.
    pub material: Vec<String>,
}

/// What a click on the history asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryAction {
    /// The list of snapshots opens or closes.
    Toggle,
    /// The sheet that takes a snapshot.
    OpenSheet,
    /// The sheet that restores snapshot `id`.
    OpenRestore(String),
    SetWhy(String),
    SetTags(String),
    /// The sheet's snapshot is taken (or the restore made).
    Save,
    Cancel,
    /// Snapshot `id` is looked at.
    View(String),
    /// The board is shown again.
    Back,
    /// The snapshot that is looked at is set against its parent, or no more.
    Compare(bool),
    /// "Not now" to the question whether to take a snapshot of the code that moved.
    DismissHead,
}

/// What a [`HistoryAction`] came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryOutcome {
    /// The history changed: the host keeps it.
    Changed,
    /// Only what is shown changed.
    Refresh,
    /// Refused: what was said is in [`Panel::hint`].
    Told,
}

impl Panel {
    /// Does what a click on the history asks for, or says why not.
    pub fn history_action(&mut self, action: HistoryAction) -> HistoryOutcome {
        self.history_action_files(action, None)
    }

    /// The same, with the workspace's files: the sheet that takes a snapshot, and the snapshot, look for the code the cutouts
    /// point at again first ([`crate::whiteboard_relocate`]), so that what moved, changed or vanished is said.
    pub fn history_action_files(&mut self, action: HistoryAction, files: Option<&dyn FileView>) -> HistoryOutcome {
        let asks_for_code = match &action {
            HistoryAction::OpenSheet => true,
            HistoryAction::Save => self.history_ui.sheet.as_ref().is_some_and(|s| s.kind == SheetKind::Snapshot),
            _ => false,
        };
        if let (true, Some(files)) = (asks_for_code, files) {
            self.wb.relocate_all(files);
        }
        let now = self.unix_ms();
        match action {
            HistoryAction::Toggle => {
                self.history_ui.list = !self.history_ui.list;
                if self.history_ui.list {
                    self.switcher.open = false;
                    self.switcher.export = None;
                    self.history_ui.sheet = None;
                }
                HistoryOutcome::Refresh
            }
            HistoryAction::OpenSheet => {
                // When the code moved on, what the app knows of what happened since is where the reason starts.
                let why = if self.wb.head_moved().is_some() && !self.history_ui.material.is_empty() {
                    let mut text = t("azreview-whiteboard-material-intro");
                    for line in self.history_ui.material.iter().take(MAX_MATERIAL_LINES) {
                        text.push_str("\n- ");
                        text.extend(line.chars().filter(|c| !c.is_control()).take(MAX_MATERIAL_CHARS));
                    }
                    let more = self.history_ui.material.len().saturating_sub(MAX_MATERIAL_LINES);
                    if more > 0 {
                        text.push_str("\n- ");
                        text.push_str(&t_args("azreview-whiteboard-material-more", &[("n", Arg::from(more))]));
                    }
                    text.push_str("\n\n");
                    text
                } else {
                    String::new()
                };
                self.history_ui.sheet = Some(Sheet { kind: SheetKind::Snapshot, why, tags: String::new() });
                self.history_ui.list = false;
                self.switcher.open = false;
                self.switcher.export = None;
                self.hint = None;
                HistoryOutcome::Refresh
            }
            HistoryAction::OpenRestore(id) => {
                if self.wb.history().get(&id).is_none() {
                    self.hint = Some(say(&crate::whiteboard::Notice::NoSuchSnapshot));
                    return HistoryOutcome::Told;
                }
                let why = t_args("azreview-whiteboard-restore-why", &[("id", Arg::from(id.clone()))]);
                self.history_ui.sheet = Some(Sheet { kind: SheetKind::Restore(id), why, tags: String::new() });
                self.history_ui.list = false;
                self.hint = None;
                HistoryOutcome::Refresh
            }
            HistoryAction::SetWhy(text) => {
                if let Some(sheet) = self.history_ui.sheet.as_mut() {
                    sheet.why = text;
                }
                HistoryOutcome::Refresh
            }
            HistoryAction::SetTags(text) => {
                if let Some(sheet) = self.history_ui.sheet.as_mut() {
                    sheet.tags = text;
                }
                HistoryOutcome::Refresh
            }
            HistoryAction::Save => {
                let Some(sheet) = self.history_ui.sheet.clone() else {
                    return HistoryOutcome::Refresh;
                };
                let done = match &sheet.kind {
                    SheetKind::Snapshot => self.wb.take_snapshot(&sheet.why, &sheet.tags, now),
                    SheetKind::Restore(id) => self.wb.restore_snapshot(id, &sheet.why, &sheet.tags, now),
                };
                self.collect_notices();
                match done {
                    Ok(_) => {
                        self.history_ui.sheet = None;
                        self.hint = None;
                        self.editing = None;
                        HistoryOutcome::Changed
                    }
                    Err(n) => {
                        self.hint = Some(say(&n));
                        HistoryOutcome::Told
                    }
                }
            }
            HistoryAction::Cancel => {
                self.history_ui.sheet = None;
                HistoryOutcome::Refresh
            }
            HistoryAction::View(id) => match self.wb.view_snapshot(&id) {
                Ok(()) => {
                    self.history_ui.list = false;
                    self.editing = None;
                    self.hint = None;
                    HistoryOutcome::Refresh
                }
                Err(n) => {
                    self.hint = Some(say(&n));
                    HistoryOutcome::Told
                }
            },
            HistoryAction::Back => {
                self.wb.back_to_live();
                HistoryOutcome::Refresh
            }
            HistoryAction::Compare(on) => {
                self.wb.set_compare(on);
                HistoryOutcome::Refresh
            }
            HistoryAction::DismissHead => {
                self.history_ui.head_dismissed = self.wb.head_moved().map(|(_, now)| now);
                HistoryOutcome::Refresh
            }
        }
    }

    /// The question to put to the person: the code moved from the revision the newest snapshot was taken at to another, once
    /// for each revision (`None` when there is nothing to ask, or "not now" was said to this revision, or something else is open).
    #[must_use]
    pub fn head_question(&self) -> Option<(String, String)> {
        if self.history_ui.sheet.is_some() || self.history_ui.list || self.switcher.open || self.wb.viewing().is_some() {
            return None;
        }
        self.wb.head_moved().filter(|(_, now)| self.history_ui.head_dismissed.as_ref() != Some(now))
    }

    /// What was heard (by the dictation) is added to the reason that is being typed; whether there was a sheet to take it.
    pub fn dictated(&mut self, heard: &str) -> bool {
        match self.history_ui.sheet.as_mut() {
            Some(sheet) => {
                sheet.why = crate::dictation::join_text(&sheet.why, heard);
                true
            }
            None => false,
        }
    }
}

// ---- the DOM

/// What a button of the history carries: the panel it is on, and the snapshot or the choice it is about.
struct Tap {
    host: Host,
    id: String,
    flag: bool,
}

fn tap(host: &Host, id: &str, flag: bool) -> RefAny {
    RefAny::new(Tap { host: host.clone(), id: id.to_string(), flag })
}

fn button(text: &str, name: &str, tag: RefAny, on_click: ButtonOnClickCallbackType, id: &str, css: &str) -> Dom {
    Button::create(text).with_on_click(tag, on_click).dom().with_id(id).with_accessibility_name(AzString::from(name)).with_css(css)
}

const SMALL: &str = "font-size: 10px; padding: 1px 6px; flex-shrink: 0;";

/// The buttons at the foot of the sheet, the banner and the question: in a row that wraps, so that a bar of the narrowest
/// width does not push a button out of itself (a button that cannot be reached cannot be pressed).
const BUTTON_ROW: &str = "display: flex; flex-direction: row; flex-wrap: wrap; justify-content: flex-end; gap: 4px;";
const BUTTON_ROW_BANNER: &str = "display: flex; flex-direction: row; flex-wrap: wrap; justify-content: flex-end; gap: 4px; margin-top: 3px;";

/// The two buttons the header gets: take a snapshot, and the list of them.
pub(crate) fn header_buttons(panel: &Panel, host: &Host) -> Vec<Dom> {
    let n = panel.wb.history().snapshots.len();
    vec![
        button(
            &t("azreview-whiteboard-snapshot"),
            &t("azreview-whiteboard-snapshot-name"),
            tap(host, "", false),
            on_open_sheet,
            "__azreview_whiteboard-snapshot",
            "font-size: 11px; padding: 1px 6px; margin-left: 4px; flex-shrink: 0;",
        ),
        button(
            &t_args("azreview-whiteboard-history", &[("n", Arg::from(n))]),
            &t("azreview-whiteboard-history-name"),
            tap(host, "", false),
            on_toggle_list,
            "__azreview_whiteboard-history",
            "font-size: 11px; padding: 1px 6px; margin-left: 4px; flex-shrink: 0; white-space: nowrap;",
        ),
    ]
}

/// What changed since the last snapshot, as lines of words: how many of each, then the first of them by name.
fn summary_lines(s: &Summary, parent: Option<&str>) -> Vec<String> {
    let mut lines = Vec::new();
    let (a, r, m, c) = s.counts();
    lines.push(match parent {
        None => t("azreview-whiteboard-summary-first"),
        Some(p) if s.is_empty() => t_args("azreview-whiteboard-summary-none", &[("parent", Arg::from(p))]),
        Some(p) => t_args(
            "azreview-whiteboard-summary-counts",
            &[("parent", Arg::from(p)), ("added", Arg::from(a)), ("removed", Arg::from(r)), ("moved", Arg::from(m)), ("changed", Arg::from(c))],
        ),
    });
    for b in s.added.iter().take(6) {
        lines.push(format!("+ {}", b.name));
    }
    for b in s.removed.iter().take(6) {
        lines.push(format!("- {}", b.name));
    }
    for m in s.moved.iter().take(4) {
        lines.push(format!("~ #{} {} -> {}", m.id, point(m.from), point(m.to)));
    }
    for c in s.changed.iter().take(4) {
        lines.push(format!("~ #{} ({})", c.id, c.what.join(", ")));
    }
    lines
}

/// What looking for the code again found: how many references moved, changed or are gone, then each by name.
fn reference_lines(refs: &[crate::whiteboard_history::RefChange]) -> Vec<String> {
    if refs.is_empty() {
        return Vec::new();
    }
    let count = |s: RefStatus| refs.iter().filter(|r| r.status == s).count();
    let mut lines = vec![t_args(
        "azreview-whiteboard-summary-references",
        &[("moved", Arg::from(count(RefStatus::Moved))), ("changed", Arg::from(count(RefStatus::Changed))), ("vanished", Arg::from(count(RefStatus::Vanished)))],
    )];
    for r in refs.iter().take(8) {
        lines.push(match r.status {
            RefStatus::Vanished => format!("! #{} {} ({}-{})", r.id, r.status.name(), r.from.0, r.from.1),
            _ => format!("! #{} {} ({}-{} -> {}-{})", r.id, r.status.name(), r.from.0, r.from.1, r.to.0, r.to.1),
        });
    }
    lines
}

fn point(p: crate::whiteboard_geom::Pt) -> String {
    format!("({:.0}, {:.0})", p.x, p.y)
}

/// The sheet that asks why, over the board.
pub(crate) fn sheet_dom(panel: &Panel, host: &Host, sheet: &Sheet) -> Dom {
    let draft = panel.wb.snapshot_draft();
    let paragraph = |text: String| Dom::create_p_with_text(text.as_str()).with_css("margin: 0px 0px 6px 0px; font-family: system:ui; font-size: 11px; color: system:text;");
    let title = match &sheet.kind {
        SheetKind::Snapshot => t("azreview-whiteboard-sheet-title"),
        SheetKind::Restore(id) => t_args("azreview-whiteboard-sheet-restore-title", &[("id", Arg::from(id.clone()))]),
    };
    let code = match (&draft.code.rev, &draft.code.branch) {
        (Some(rev), branch) => t_args(
            "azreview-whiteboard-sheet-code",
            &[
                ("rev", Arg::from(short_rev(Some(rev.as_str())))),
                ("branch", Arg::from(branch.clone().unwrap_or_else(|| "-".to_string()))),
            ],
        ),
        (None, _) => t("azreview-whiteboard-sheet-code-none"),
    };
    let mut root = Dom::create_div()
        .with_id("__azreview_whiteboard-sheet")
        .with_css(
            format!(
                "position: absolute; left: 6px; right: 6px; top: {}px; max-height: 88%; overflow-y: auto; display: flex; \
                 flex-direction: column; padding: 8px; background: system:window-background; border: 1px solid system:separator; \
                 box-shadow: 0px 3px 10px rgba(0,0,0,0.25);",
                HEADER_PX + 2.0
            )
            .as_str(),
        )
        .with_child(Dom::create_p_with_text(title.as_str()).with_css("margin: 0px 0px 6px 0px; font-family: system:ui; font-size: 12px; font-weight: bold; color: system:text;"))
        .with_child(paragraph(code));
    if draft.code.dirty == Some(true) {
        root.add_child(paragraph(t("azreview-whiteboard-sheet-dirty")));
    }
    if matches!(sheet.kind, SheetKind::Snapshot) {
        for line in summary_lines(&draft.summary, draft.parent.as_deref()).into_iter().chain(reference_lines(&draft.references)) {
            root.add_child(Dom::create_p_with_text(line.as_str()).with_css("margin: 0px 0px 2px 0px; font-family: system:ui; font-size: 11px; color: system:secondary-text;"));
        }
    }
    if draft.pending > 0 {
        root.add_child(paragraph(t_args("azreview-whiteboard-sheet-pending", &[("n", Arg::from(draft.pending))])));
    }
    let why_tag = RefAny::new(Tap { host: host.clone(), id: String::new(), flag: false });
    let why = TextArea::create()
        .with_text(sheet.why.as_str())
        .with_placeholder(label("azreview-whiteboard-sheet-why-placeholder"))
        .with_accessibility_name(t("azreview-whiteboard-sheet-why-name"))
        .with_on_text_input(why_tag.clone(), on_why_text as TextAreaOnTextInputCallbackType)
        .with_on_focus_lost(why_tag, on_why_blur as TextAreaOnFocusLostCallbackType)
        .dom()
        .with_id("__azreview_whiteboard-why")
        .with_attribute(AttributeType::Autofocus)
        .with_css("min-height: 90px; margin: 6px 0px;");
    let tags_tag = RefAny::new(Tap { host: host.clone(), id: String::new(), flag: false });
    let tags = TextInput::create()
        .with_text(sheet.tags.as_str())
        .with_accessibility_name(t("azreview-whiteboard-sheet-tags-name"))
        .with_on_text_input(tags_tag.clone(), on_tags_text as TextInputOnTextInputCallbackType)
        .with_on_focus_lost(tags_tag, on_tags_blur as TextInputOnFocusLostCallbackType)
        .dom()
        .with_id("__azreview_whiteboard-tags")
        .with_css("margin-bottom: 6px;");
    root.with_child(paragraph(t("azreview-whiteboard-sheet-why")))
        .with_child(why)
        .with_child(paragraph(t("azreview-whiteboard-sheet-tags")))
        .with_child(tags)
        .with_child(
            Dom::create_div()
                .with_css(BUTTON_ROW)
                .with_child(button(
                    &t("azreview-whiteboard-sheet-save"),
                    &t("azreview-whiteboard-sheet-save"),
                    tap(host, "", false),
                    on_save,
                    "__azreview_whiteboard-sheet-save",
                    "font-size: 11px; padding: 2px 10px; margin-right: 6px;",
                ))
                .with_child(button(
                    &t("azreview-whiteboard-sheet-cancel"),
                    &t("azreview-whiteboard-sheet-cancel"),
                    tap(host, "", false),
                    on_cancel,
                    "__azreview_whiteboard-sheet-cancel",
                    "font-size: 11px; padding: 2px 10px;",
                )),
        )
}

/// One line of the list: the snapshot's number, revision and date, and what changed in it.
fn row_text(s: &Snapshot) -> String {
    let (a, r, m, c) = s.summary.counts();
    let rev = short_rev(s.code.rev.as_deref());
    format!("{}  {}  {}   +{a} -{r} ~{}", s.id, if rev.is_empty() { "-" } else { rev.as_str() }, date_utc(s.taken_ms), m + c)
}

/// The list of snapshots under the header, the newest first.
pub(crate) fn list_dom(panel: &Panel, host: &Host) -> Dom {
    let mut list = Dom::create_div().with_id("__azreview_whiteboard-history-list").with_css(
        format!(
            "position: absolute; left: 6px; right: 6px; top: {}px; max-height: 360px; overflow-y: auto; display: flex; \
             flex-direction: column; padding: 4px; background: system:window-background; border: 1px solid system:separator; \
             box-shadow: 0px 3px 10px rgba(0,0,0,0.25);",
            HEADER_PX + 2.0
        )
        .as_str(),
    );
    let snapshots = &panel.wb.history().snapshots;
    if snapshots.is_empty() {
        list.add_child(Dom::create_p_with_text(t("azreview-whiteboard-history-empty").as_str()).with_css("margin: 4px; font-family: system:ui; font-size: 11px; color: system:secondary-text;"));
    }
    for s in snapshots.iter().rev() {
        let mut row = Dom::create_div().with_css("display: flex; flex-direction: column; padding: 2px; margin-bottom: 3px;");
        row.add_child(button(
            &row_text(s),
            &t_args("azreview-whiteboard-snapshot-open-name", &[("id", Arg::from(s.id.clone()))]),
            tap(host, &s.id, false),
            on_pick,
            &format!("__azreview_whiteboard-snap-{}", s.id),
            "min-width: 0px; text-align: left; font-size: 11px; overflow: hidden; white-space: nowrap;",
        ));
        let headline = s.headline();
        let (said, tone) = if headline.is_empty() {
            (t("azreview-whiteboard-snapshot-no-why"), "#b36b00")
        } else {
            (headline, "system:secondary-text")
        };
        row.add_child(
            Dom::create_p_with_text(said.as_str()).with_css(format!("margin: 2px 4px 0px 4px; font-family: system:ui; font-size: 11px; color: {tone}; overflow: hidden; white-space: nowrap;").as_str()),
        );
        if !s.verified {
            row.add_child(
                Dom::create_p_with_text(t("azreview-whiteboard-snapshot-unverified").as_str())
                    .with_id(format!("__azreview_whiteboard-snap-{}-unverified", s.id).as_str())
                    .with_css("margin: 2px 4px 0px 4px; font-family: system:ui; font-size: 10px; font-weight: bold; color: #c8322d;"),
            );
        }
        list.add_child(row);
    }
    list
}

/// The banner over the board while a snapshot is looked at: which it is, why it was taken, and what can be done with it.
pub(crate) fn banner_dom(panel: &Panel, host: &Host) -> Option<Dom> {
    let v = panel.wb.viewing()?;
    let s = panel.wb.history().get(&v.id)?;
    let head = t_args(
        "azreview-whiteboard-viewing",
        &[
            ("id", Arg::from(s.id.clone())),
            ("rev", Arg::from(if s.code.rev.is_some() { short_rev(s.code.rev.as_deref()) } else { "-".to_string() })),
            ("date", Arg::from(date_utc(s.taken_ms))),
        ],
    );
    let white = "margin: 0px 0px 3px 0px; font-family: system:ui; font-size: 11px; color: #ffffff; overflow-wrap: anywhere;";
    let mut root = Dom::create_div()
        .with_id("__azreview_whiteboard-viewing")
        .with_css(
            format!(
                "position: absolute; left: 0px; right: 0px; top: {HEADER_PX}px; padding: 6px 8px; background: rgba(32, 92, 214, 0.94); \
                 display: flex; flex-direction: column; max-height: 45%; overflow-y: auto;"
            )
            .as_str(),
        )
        .with_child(Dom::create_p_with_text(head.as_str()).with_css("margin: 0px 0px 3px 0px; font-family: system:ui; font-size: 12px; font-weight: bold; color: #ffffff;"));
    if !s.why.is_empty() {
        root.add_child(Dom::create_p_with_text(s.why.as_str()).with_id("__azreview_whiteboard-viewing-why").with_css(format!("{white} white-space: pre-wrap; overflow-wrap: anywhere;").as_str()));
    }
    if !s.tags.is_empty() {
        root.add_child(Dom::create_p_with_text(s.tags.join("  ").as_str()).with_css("margin: 0px 0px 3px 0px; font-family: system:ui; font-size: 10px; color: #dbe7ff; overflow-wrap: anywhere;"));
    }
    if !s.verified {
        root.add_child(Dom::create_p_with_text(t("azreview-whiteboard-snapshot-unverified").as_str()).with_css("margin: 0px 0px 3px 0px; font-family: system:ui; font-size: 11px; font-weight: bold; color: #ffd0cc;"));
    }
    let comparing = v.compare.is_some();
    if let Some(c) = &v.compare {
        let (a, r, m, ch) = c.summary.counts();
        let line = match &c.against {
            Some(p) => t_args(
                "azreview-whiteboard-compare-legend",
                &[("parent", Arg::from(p.clone())), ("added", Arg::from(a)), ("removed", Arg::from(r)), ("moved", Arg::from(m)), ("changed", Arg::from(ch))],
            ),
            None => t("azreview-whiteboard-compare-first"),
        };
        root.add_child(Dom::create_p_with_text(line.as_str()).with_id("__azreview_whiteboard-compare-legend").with_css(white));
    }
    let id = s.id.clone();
    Some(
        root.with_child(
            Dom::create_div()
                .with_css(BUTTON_ROW_BANNER)
                .with_child(button(
                    &t(if comparing { "azreview-whiteboard-compare-off" } else { "azreview-whiteboard-compare-on" }),
                    &t("azreview-whiteboard-compare-name"),
                    tap(host, "", !comparing),
                    on_compare,
                    "__azreview_whiteboard-compare",
                    SMALL,
                ))
                .with_child(button(
                    &t("azreview-whiteboard-restore"),
                    &t("azreview-whiteboard-restore"),
                    tap(host, &id, false),
                    on_restore,
                    "__azreview_whiteboard-restore",
                    SMALL,
                ))
                .with_child(button(
                    &t("azreview-whiteboard-back"),
                    &t("azreview-whiteboard-back-name"),
                    tap(host, "", false),
                    on_back,
                    "__azreview_whiteboard-back",
                    SMALL,
                )),
        ),
    )
}

/// The question at the foot of the panel when the code moved on since the last snapshot: take a new one?
pub(crate) fn head_dom(panel: &Panel, host: &Host) -> Option<Dom> {
    let (from, to) = panel.head_question()?;
    let said = t_args("azreview-whiteboard-head-moved", &[("from", Arg::from(short_rev(Some(from.as_str())))), ("to", Arg::from(short_rev(Some(to.as_str()))))]);
    let mut bar = Dom::create_div()
        .with_id("__azreview_whiteboard-head")
        .with_css(
            "position: absolute; left: 8px; right: 8px; bottom: 8px; display: flex; flex-direction: column; padding: 6px 8px; \
             background: #2d3340; border-radius: 4px; box-shadow: 0px 2px 8px rgba(0,0,0,0.3);",
        )
        .with_child(Dom::create_p_with_text(said.as_str()).with_css("margin: 0px 0px 4px 0px; font-family: system:ui; font-size: 11px; color: #ffffff;"));
    let n = panel.history_ui.material.len();
    if n > 0 {
        bar.add_child(
            Dom::create_p_with_text(t_args("azreview-whiteboard-head-material", &[("n", Arg::from(n))]).as_str())
                .with_css("margin: 0px 0px 4px 0px; font-family: system:ui; font-size: 10px; color: #dbe7ff;"),
        );
    }
    Some(
        bar.with_child(
            Dom::create_div()
                .with_css(BUTTON_ROW)
                .with_child(button(
                    &t("azreview-whiteboard-head-take"),
                    &t("azreview-whiteboard-head-take"),
                    tap(host, "", false),
                    on_open_sheet_head,
                    "__azreview_whiteboard-head-take",
                    SMALL,
                ))
                .with_child(button(
                    &t("azreview-whiteboard-head-dismiss"),
                    &t("azreview-whiteboard-head-dismiss"),
                    tap(host, "", false),
                    on_dismiss_head,
                    "__azreview_whiteboard-head-dismiss",
                    SMALL,
                )),
        ),
    )
}

/// The flag a cutout carries when the code it points at is not where it was: moved (blue), changed (orange) or gone (red).
/// While a snapshot is looked at it is the one that snapshot recorded.
pub(crate) fn flag_dom(panel: &Panel, id: u64) -> Option<Dom> {
    let change = match panel.wb.viewing() {
        Some(v) => panel.wb.history().get(&v.id)?.summary.references.iter().find(|r| r.id == id)?.clone(),
        None => panel.wb.flags().get(&id)?.clone(),
    };
    let (text, color, name) = match change.status {
        RefStatus::Moved => (
            t("azreview-whiteboard-flag-moved"),
            "#205cd6",
            t_args("azreview-whiteboard-flag-moved-name", &[("first", Arg::from(change.to.0 as usize)), ("last", Arg::from(change.to.1 as usize))]),
        ),
        RefStatus::Changed => (
            t("azreview-whiteboard-flag-changed"),
            "#d97a00",
            t_args("azreview-whiteboard-flag-changed-name", &[("first", Arg::from(change.to.0 as usize)), ("last", Arg::from(change.to.1 as usize))]),
        ),
        RefStatus::Vanished => (t("azreview-whiteboard-flag-vanished"), "#c8322d", t("azreview-whiteboard-flag-vanished-name")),
        RefStatus::Unchanged => return None,
    };
    Some(
        Dom::create_span_with_text(text.as_str())
            .with_id(format!("__azreview_whiteboard-flag-{id}").as_str())
            .with_accessibility_name(AzString::from(name.as_str()))
            .with_css(
                format!(
                    "position: absolute; right: 2px; top: 2px; padding: 0px 4px; font-family: system:ui; font-size: 9px; font-weight: bold; color: #ffffff; \
                     background: {color}; border-radius: 3px;"
                )
                .as_str(),
            ),
    )
}

// ---- the clicks

fn of(data: &mut RefAny) -> Option<(Host, String, bool)> {
    data.downcast_ref::<Tap>().map(|t| (t.host.clone(), t.id.clone(), t.flag))
}

/// Asks for `action`; a change of the history is kept (the host's save).
fn act(host: &Host, action: HistoryAction, info: &mut CallbackInfo) -> Update {
    if host.run_with_files(|p, files| p.history_action_files(action.clone(), Some(files))) == Some(HistoryOutcome::Changed) {
        (host.on_changed)(&mut host.app.clone(), info);
    }
    Update::RefreshDom
}

extern "C" fn on_open_sheet(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, _, _)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, HistoryAction::OpenSheet, &mut info)
}

extern "C" fn on_open_sheet_head(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, _, _)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, HistoryAction::OpenSheet, &mut info)
}

extern "C" fn on_dismiss_head(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, _, _)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, HistoryAction::DismissHead, &mut info)
}

extern "C" fn on_toggle_list(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, _, _)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, HistoryAction::Toggle, &mut info)
}

extern "C" fn on_pick(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, id, _)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, HistoryAction::View(id), &mut info)
}

extern "C" fn on_save(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, _, _)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, HistoryAction::Save, &mut info)
}

extern "C" fn on_cancel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, _, _)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, HistoryAction::Cancel, &mut info)
}

extern "C" fn on_back(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, _, _)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, HistoryAction::Back, &mut info)
}

extern "C" fn on_compare(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, _, on)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, HistoryAction::Compare(on), &mut info)
}

extern "C" fn on_restore(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((host, id, _)) = of(&mut data) else {
        return Update::DoNothing;
    };
    act(&host, HistoryAction::OpenRestore(id), &mut info)
}

/// What is typed in the reason's area is kept at once (the button that saves can be pressed with no other event between).
extern "C" fn on_why_text(mut data: RefAny, _info: CallbackInfo, state: TextAreaState) -> OnTextInputReturn {
    if let Some((host, _, _)) = of(&mut data) {
        let text = state.get_text().as_str().to_string();
        host.run(|p| p.history_action(HistoryAction::SetWhy(text.clone())));
    }
    OnTextInputReturn { update: Update::DoNothing, valid: TextInputValid::Yes }
}

extern "C" fn on_why_blur(mut data: RefAny, _info: CallbackInfo, state: TextAreaState) -> Update {
    let Some((host, _, _)) = of(&mut data) else {
        return Update::DoNothing;
    };
    let text = state.get_text().as_str().to_string();
    host.run(|p| p.history_action(HistoryAction::SetWhy(text.clone())));
    Update::DoNothing
}

extern "C" fn on_tags_text(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    if let Some((host, _, _)) = of(&mut data) {
        let text = state.get_text().as_str().to_string();
        host.run(|p| p.history_action(HistoryAction::SetTags(text.clone())));
    }
    OnTextInputReturn { update: Update::DoNothing, valid: TextInputValid::Yes }
}

extern "C" fn on_tags_blur(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> Update {
    let Some((host, _, _)) = of(&mut data) else {
        return Update::DoNothing;
    };
    let text = state.get_text().as_str().to_string();
    host.run(|p| p.history_action(HistoryAction::SetTags(text.clone())));
    Update::DoNothing
}

#[cfg(test)]
#[path = "whiteboard_history_ui_test.rs"]
mod whiteboard_history_ui_test;
