//! The "Mail" section: the widgets an Outlook-2010-style mail window is
//! built from - the message list, the reading pane with its info bar, the
//! To-Do bar, the module switcher (the navigation pane's), the status bar with its sync indicator
//! and the account wizard's layout.
//!
//! Every value the cards show is the APP's (`MailDemo`): each widget is built
//! from it and reports every action back into it, so a rebuild - which any
//! callback on the page may ask for - never loses a selection, a flag, a
//! typed search or a checked task. The list is virtualised in principle
//! (a window of `first_row..` of `total_rows`); the demo renders all of its
//! few rows and reports the window a settled scroll asks for in the status
//! line.

use azul::{
    callbacks::ShellNavigationPaneOnEventCallbackType,
    prelude::*,
    shells::{
        ShellNavigationModule, ShellNavigationPane, ShellNavigationPaneEvent,
        ShellNavigationPaneEventKind,
    },
    str::String as AzString,
    widgets::*,
};

use crate::{captioned, section, strs, Showcase};

/// The sample inbox: `(id, from, subject, preview, date, unread, attachment)`.
const INBOX: &[(u64, &str, &str, &str, &str, bool, bool)] = &[
    (
        11,
        "Google Mail-Team",
        "Willkommen bei Gmail",
        "Vielen Dank, dass Sie sich fuer Gmail entschieden haben",
        "21:12",
        true,
        false,
    ),
    (
        12,
        "Alice Weber",
        "Rechnung September",
        "Anbei die Rechnung fuer September",
        "18:03",
        false,
        true,
    ),
    (13, "Bob Fischer", "Mittagessen?", "Hast du morgen Zeit", "Mo", false, false),
    (
        14,
        "Newsletter",
        "Neues aus der Werkstatt",
        "Diese Woche: drei neue Werkzeuge",
        "Fr",
        false,
        false,
    ),
];

/// Where the inbox's groups start: row 0 is "Heute", row 4 "Gestern".
const GROUPS: &[(usize, &str)] = &[(0, "Heute"), (3, "Gestern")];

/// The status line's prefix.
const NOTE_CSS: &str = "font-size: 12px; color: system:secondary-text; margin: 0px;";
/// A pane of fixed height so the card shows the widget scrolling.
const PANE_CSS: &str =
    "display: flex; flex-direction: column; height: 280px; border: 1px solid system:separator;";
/// The three panes of the mail window side by side.
const WINDOW_CSS: &str = "display: flex; flex-direction: row; gap: 12px; align-items: stretch;";

/// Every value the mail cards show.
#[derive(Clone)]
pub(crate) struct MailDemo {
    /// The list's selection (indices into the flat list, group rows
    /// included).
    selection: ListSelection,
    /// Which messages are flagged, per `INBOX` entry.
    flagged: Vec<bool>,
    /// Which messages are read, per `INBOX` entry.
    read: Vec<bool>,
    /// The search box's text.
    search: AzString,
    /// The active scope (0 all, 1 unread).
    scope: usize,
    /// Newest on top.
    descending: bool,
    /// The last thing a widget reported.
    status: AzString,
    /// The reading pane's pictures were loaded (the info bar goes).
    images_loaded: bool,
    /// The To-Do bar's calendar.
    date: DatePickerState,
    /// The tasks, with their done flags.
    tasks: Vec<(AzString, bool)>,
    /// The active module.
    module: usize,
    /// The module switcher is collapsed.
    collapsed: bool,
    /// The wizard's step.
    wizard_step: usize,
    /// The sync indicator shows an error.
    sync_error: bool,
}

impl MailDemo {
    pub(crate) fn create() -> Self {
        Self {
            selection: ListSelection::create().apply(1, false, false),
            flagged: vec![true, false, false, false],
            read: vec![false, true, true, false],
            search: "".into(),
            scope: 0,
            descending: true,
            status: "Noch nichts gemeldet.".into(),
            images_loaded: false,
            date: DatePickerState {
                year: 2026,
                month: 9,
                day: 30,
            },
            tasks: vec![("Alice antworten".into(), false), ("Fluege buchen".into(), true)],
            module: 0,
            collapsed: false,
            wizard_step: 1,
            sync_error: true,
        }
    }

    /// The flat list the message list renders: group headers and messages.
    fn rows(&self) -> Vec<MessageRow> {
        let mut rows = Vec::with_capacity(INBOX.len() + GROUPS.len());
        for (i, (id, from, subject, preview, date, _, attachment)) in INBOX.iter().enumerate() {
            if let Some((_, title)) = GROUPS.iter().find(|(at, _)| *at == i) {
                rows.push(MessageRow::create_group(AzString::from(*title)));
            }
            let unread = !self.read[i];
            if self.scope == 1 && !unread {
                continue;
            }
            let index = rows.len() as u64;
            rows.push(
                MessageRow::create(*id, AzString::from(*from), AzString::from(*subject))
                    .with_preview(AzString::from(*preview))
                    .with_date(AzString::from(*date))
                    .with_icon(AzString::from(if unread { "mail" } else { "drafts" }))
                    .with_unread(unread)
                    .with_flagged(self.flagged[i])
                    .with_attachment(*attachment)
                    .with_selected(self.selection.contains(index)),
            );
        }
        rows
    }
}

/// The `INBOX` entry with id `id`.
fn entry(id: u64) -> Option<usize> {
    INBOX.iter().position(|e| e.0 == id)
}

/// Keep what a widget reported and rebuild.
fn keep(data: &mut RefAny, put: impl FnOnce(&mut MailDemo)) -> Update {
    match data.downcast_mut::<Showcase>() {
        Some(mut s) => {
            put(&mut s.mail);
            s.interactions += 1;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

fn note(text: &str) -> Dom {
    Dom::create_p_with_text(text).with_css(NOTE_CSS)
}

/// The "Mail" section.
pub(crate) fn mail_section(data: &RefAny, m: &MailDemo, theme: UiTheme) -> Dom {
    let list = MessageList::create(m.rows())
        .with_scopes(strs(&["Alle", "Ungelesen"]), m.scope)
        .with_search(m.search.clone())
        .with_search_placeholder("Posteingang durchsuchen (Strg+E)")
        .with_sort("Anordnen nach:", "Datum", m.descending)
        .with_sort_direction_label(
            if m.descending {
                "Neu nach alt"
            } else {
                "Alt nach neu"
            },
        )
        .with_on_select(data.clone(), on_list)
        .with_on_open(data.clone(), on_list)
        .with_on_flag(data.clone(), on_list)
        .with_on_delete(data.clone(), on_list)
        .with_on_sort(data.clone(), on_list)
        .with_on_search(data.clone(), on_list)
        .with_on_scope(data.clone(), on_list)
        .with_on_scroll(data.clone(), on_list)
        .with_theme(theme)
        .dom();

    let open = m
        .selection
        .keys
        .as_ref()
        .first()
        .and_then(|row| m.rows().get(*row as usize).map(|r| r.id))
        .and_then(entry)
        .unwrap_or(0);
    let (_, from, subject, preview, date, _, attachment) = INBOX[open];
    let mut pane = ReadingPane::create(
        AzString::from(subject),
        AzString::from(format!("{from} <{}@example.com>", from.to_lowercase().replace(' ', "."))),
    )
    .with_date(AzString::from(format!("Mi 30.09.2026 {date}")))
    .with_field("Gesendet", AzString::from(format!("Mi 30.09.2026 {date}")))
    .with_field("An", "felix@example.com")
    .with_body(
        Dom::create_div()
            .with_child(Dom::create_p_with_text(preview))
            .with_child(Dom::create_p_with_text("Mit freundlichen Gruessen")),
    )
    .with_people(
        strs(&[&from.split(' ').filter_map(|w| w.chars().next()).collect::<String>()]),
        AzString::from(format!("Weitere Informationen ueber {from}")),
    )
    .with_on_link(data.clone(), on_pane)
    .with_on_load_images(data.clone(), on_pane)
    .with_on_attachment(data.clone(), on_pane)
    .with_theme(theme);
    if attachment {
        pane = pane.with_attachments(strs(&["rechnung.pdf"]));
    }
    if !m.images_loaded {
        pane = pane.with_info_bar(
            InfoBar::create(
                "Klicken Sie hier, um Bilder herunterzuladen. Zum Schutz Ihrer Privatsphaere \
                 wurden einige Bilder nicht automatisch heruntergeladen.",
            )
            .with_icon("info")
            .with_action("Bilder herunterladen"),
        );
    }
    let pane = pane.dom();

    let todo = ToDoBar::create(m.date.year, m.date.month, m.date.day)
        .with_today(2026, 9, 30)
        .with_appointments_empty("Keine anstehenden Termine.")
        .with_task_line("Neue Aufgabe eingeben", "")
        .with_tasks(
            m.tasks
                .iter()
                .enumerate()
                .map(|(i, (title, done))| {
                    ToDoTask::create(i as u64 + 1, title.clone()).with_done(*done)
                })
                .collect::<Vec<_>>(),
        )
        .with_accessibility_name("Aufgabenleiste")
        .with_on_pick(data.clone(), on_todo)
        .with_on_task(data.clone(), on_todo)
        .with_on_appointment(data.clone(), on_todo)
        .with_theme(theme)
        .dom();

    // The module switcher is the navigation pane's (no groups here): the
    // big module buttons, the badge, the collapse chevron.
    let switcher = ShellNavigationPane::create()
        .with_label("Module")
        .with_module(ShellNavigationModule::create("E-Mail", "mail").with_badge("2"))
        .with_module(ShellNavigationModule::create("Kalender", "calendar_month"))
        .with_module(ShellNavigationModule::create("Kontakte", "contacts"))
        .with_module(ShellNavigationModule::create("Aufgaben", "task"))
        .with_active_module(m.module)
        .with_collapsed(m.collapsed)
        .with_on_event(data.clone(), on_navigation as ShellNavigationPaneOnEventCallbackType)
        .with_theme(theme)
        .dom();

    let status = StatusBar::create(vec![StatusBarSegment::create("Filter angewendet")])
        .with_sync(
            StatusBarSync::create(
                if m.sync_error {
                    "Uebermittlungsfehler"
                } else {
                    "Verbunden"
                },
                if m.sync_error {
                    StatusBarSyncKind::Error
                } else {
                    StatusBarSyncKind::Connected
                },
            )
            .with_on_click(data.clone(), on_sync),
        )
        .with_views(StatusBarViewSwitcher::office_2013())
        .with_zoom(StatusBarZoom::office_2013())
        .with_theme(theme)
        .dom();

    let wizard = WizardLayout::create(
        "Konto hinzufuegen",
        strs(&["Konto", "Server", "Fertig"]),
    )
    .with_current_step(m.wizard_step)
    .with_labels(
        "Zurueck",
        "Weiter",
        "Fertig stellen",
        "Abbrechen",
    )
    .with_page(Dom::create_p_with_text(match m.wizard_step {
        0 => "Ihre E-Mail-Adresse",
        1 => "Posteingangs- und Postausgangsserver",
        _ => "Das Konto ist eingerichtet.",
    }))
    .with_on_event(data.clone(), on_wizard)
    .with_theme(theme)
    .dom();

    section(
        "Mail",
        vec![
            captioned(
                "MessageList, ReadingPane + InfoBar, ToDoBar",
                Dom::create_div()
                    .with_css(WINDOW_CSS)
                    .with_child(
                        Dom::create_div()
                            .with_css(format!("{PANE_CSS} width: 300px;"))
                            .with_child(list),
                    )
                    .with_child(
                        Dom::create_div()
                            .with_css(format!("{PANE_CSS} flex-grow: 1; min-width: 0px;"))
                            .with_child(pane),
                    )
                    .with_child(
                        Dom::create_div()
                            .with_css(format!("{PANE_CSS} width: 240px;"))
                            .with_child(todo),
                    ),
            ),
            captioned(
                "ShellNavigationPane (module switcher)",
                Dom::create_div()
                    .with_css("width: 220px;")
                    .with_child(switcher),
            ),
            captioned("StatusBar with a sync indicator", status),
            captioned(
                "WizardLayout",
                Dom::create_div()
                    .with_css("height: 260px; display: flex; flex-direction: column; border: 1px \
                               solid system:separator;")
                    .with_child(wizard),
            ),
            note(&format!("Zuletzt gemeldet: {}", m.status.as_str())),
        ],
    )
}

extern "C" fn on_list(mut data: RefAny, _: CallbackInfo, event: MessageListEvent) -> Update {
    keep(&mut data, |m| {
        let text = event.text.as_str().to_string();
        m.status = match event.kind {
            MessageListEventKind::Select => {
                m.selection = m.selection.clone().apply(event.index as u64, event.shift, event.ctrl);
                format!("Zeile {} ausgewaehlt (id {})", event.index, event.id)
            }
            MessageListEventKind::Open => format!("Nachricht {} geoeffnet", event.id),
            MessageListEventKind::Flag => {
                if let Some(i) = entry(event.id) {
                    m.flagged[i] = !m.flagged[i];
                }
                format!("Nachricht {} markiert", event.id)
            }
            MessageListEventKind::Delete => format!("Nachricht {} geloescht", event.id),
            MessageListEventKind::Sort => format!("Sortieren nach {text}"),
            MessageListEventKind::SortDirection => {
                m.descending = !m.descending;
                "Sortierrichtung umgekehrt".to_string()
            }
            MessageListEventKind::Search => {
                m.search = text.as_str().into();
                format!("Suche: {text}")
            }
            MessageListEventKind::Scope => {
                m.scope = event.index;
                format!("Bereich {}", event.index)
            }
            MessageListEventKind::Scroll => {
                format!("Zeilen {}..{} sichtbar", event.index, event.end)
            }
        }
        .as_str()
        .into();
    })
}

extern "C" fn on_pane(mut data: RefAny, _: CallbackInfo, event: ReadingPaneEvent) -> Update {
    keep(&mut data, |m| {
        let text = event.text.as_str().to_string();
        m.status = match event.kind {
            ReadingPaneEventKind::Sender => format!("Absender: {text}"),
            ReadingPaneEventKind::LoadImages => {
                m.images_loaded = true;
                "Bilder heruntergeladen".to_string()
            }
            ReadingPaneEventKind::Attachment => format!("Anlage {} ({text})", event.index),
            ReadingPaneEventKind::People => format!("Personen: {text}"),
        }
        .as_str()
        .into();
    })
}

extern "C" fn on_todo(mut data: RefAny, _: CallbackInfo, event: ToDoBarEvent) -> Update {
    keep(&mut data, |m| {
        let text = event.text.as_str().to_string();
        m.status = match event.kind {
            ToDoBarEventKind::DatePicked => {
                m.date = event.date;
                format!(
                    "Datum {:02}.{:02}.{}",
                    event.date.day, event.date.month, event.date.year
                )
            }
            ToDoBarEventKind::TaskAdded => {
                if !text.is_empty() {
                    m.tasks.push((text.as_str().into(), false));
                }
                format!("Aufgabe hinzugefuegt: {text}")
            }
            ToDoBarEventKind::TaskToggled => {
                if let Some(task) = m.tasks.get_mut(event.index) {
                    task.1 = !task.1;
                }
                format!("Aufgabe {} umgeschaltet", event.index)
            }
            ToDoBarEventKind::TaskOpened => format!("Aufgabe {} geoeffnet", event.index),
            ToDoBarEventKind::AppointmentOpened => format!("Termin: {text}"),
        }
        .as_str()
        .into();
    })
}

extern "C" fn on_navigation(mut data: RefAny, _: CallbackInfo, event: ShellNavigationPaneEvent) -> Update {
    keep(&mut data, |m| match event.kind {
        ShellNavigationPaneEventKind::ModuleSelected => {
            m.module = event.index;
            m.status = format!("Modul {}", event.index).as_str().into();
        }
        ShellNavigationPaneEventKind::CollapseToggled => {
            m.collapsed = !event.expand;
            m.status = if m.collapsed {
                "Navigationsbereich minimiert"
            } else {
                "Navigationsbereich erweitert"
            }
            .into();
        }
        _ => {}
    })
}

extern "C" fn on_sync(mut data: RefAny, _: CallbackInfo) -> Update {
    keep(&mut data, |m| {
        m.sync_error = !m.sync_error;
        m.status = "Synchronisierungsstatus geklickt".into();
    })
}

extern "C" fn on_wizard(mut data: RefAny, _: CallbackInfo, event: WizardEvent) -> Update {
    keep(&mut data, |m| {
        m.status = match event.kind {
            WizardEventKind::Back => {
                m.wizard_step = m.wizard_step.saturating_sub(1);
                "Zurueck"
            }
            WizardEventKind::Next => {
                m.wizard_step = (m.wizard_step + 1).min(2);
                "Weiter"
            }
            WizardEventKind::Finish => {
                m.wizard_step = 0;
                "Fertig gestellt"
            }
            WizardEventKind::Cancel => {
                m.wizard_step = 0;
                "Abgebrochen"
            }
            WizardEventKind::Step => {
                m.wizard_step = event.step.min(2);
                "Schritt gewaehlt"
            }
        }
        .into();
    })
}
