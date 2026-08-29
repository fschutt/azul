//! Backstage pane content (the Office-2013-era look "FILE" screen): the Info and Open
//! panes per the reference screenshots; every other nav entry shows an
//! empty pane with its title. The chrome (nav column, back ring, Esc
//! behavior) is the `azul::widgets::Backstage` widget.
//!
//! Every label goes through `fonts::text` (programmatic styling — see
//! ENGINE-ISSUES.md #4 for why inline `with_css` strings on text nodes are
//! avoided); wrapper DIVs own the margins and layout.

use azul::callbacks::{BackstageOnNavSelectCallbackType, ButtonOnClickCallbackType, RefAny};
use azul::css::{EventFilter, HoverEventFilter};
use azul::dom::{BackstageOnNavSelectCallback, Dom};
use azul::option::OptionRefAny;
use azul::str::String as AzString;
use azul::widgets::{Backstage, Button, QuickAccessAction, QuickAccessBar};

use crate::fonts::{self, OFFICE_BLUE, TEXT, TEXT_FAINT, TEXT_GRAY, TITLE_GRAY, WHITE};
use crate::AppState;

/// The nav labels `Backstage::office_2013()` builds its column from (the
/// widget-crate const is not part of the public api.json surface, so the
/// list lives here; indices must match the widget's nav order).
const OFFICE_2013_NAV_LABELS: &[&str] = &[
    "Info", "New", "Open", "Save", "Save As", "Print", "Share", "Export", "Close",
];

/// Export pane: "Create PDF/XPS Document" (the Office-2013-era wording), wired to
/// the engine's DOM->PDF path.
fn export_pane(state: &AppState, data: &RefAny) -> Dom {
    let pages =
        crate::document::paginate_cached(&state.document.content, state.document.generation);
    let desc = format!(
        "Preserves layout, formatting and fonts. {} page{} at A4.",
        pages.len(),
        if pages.len() == 1 { "" } else { "s" }
    );
    let button = Button::create(AzString::from("Create PDF/XPS")).with_on_click(
        data.clone(),
        crate::on_export_pdf as ButtonOnClickCallbackType,
    );
    pane_frame().with_child(pane_title("Export")).with_child(
        Dom::create_div()
            .with_css("flex-grow: 0; margin-top: 18px; display: flex; flex-direction: column;")
            .with_child(fonts::text("Create PDF/XPS Document", 16, OFFICE_BLUE))
            .with_child(
                Dom::create_div()
                    .with_css("flex-grow: 0; margin-top: 6px;")
                    .with_child(fonts::text(&desc, 12, TEXT_GRAY)),
            )
            .with_child(
                Dom::create_div()
                    .with_css("flex-grow: 0; margin-top: 16px; width: 160px;")
                    .with_child(button.dom()),
            ),
    )
}

/// Big light pane title ("Info", "Open", …).
fn pane_title(text: &str) -> Dom {
    Dom::create_div()
        .with_css("flex-grow: 0; display: flex; flex-direction: row;")
        .with_child(fonts::text(text, 38, TITLE_GRAY))
}

fn pane_frame() -> Dom {
    Dom::create_div().with_css(
        "display: flex; flex-direction: column; flex-grow: 1; padding-left: 46px; \
         padding-top: 20px; padding-right: 30px; background: white;",
    )
}

/// The small blue "W" document icon in the recent-documents list.
fn doc_icon() -> Dom {
    Dom::create_div()
        .with_css(
            "display: flex; align-items: center; justify-content: center; width: 22px; \
             height: 22px; background: #2b579a; flex-grow: 0; margin-right: 10px;",
        )
        .with_child(fonts::text("W", 12, WHITE))
}

/// A margin-owning wrapper around a text node.
fn boxed(css: &str, child: Dom) -> Dom {
    Dom::create_div()
        .with_css(
            format!("display: flex; flex-direction: row; flex-grow: 0; {css}").as_str(),
        )
        .with_child(child)
}

// ---------------------------------------------------------------------------
// Info pane
// ---------------------------------------------------------------------------

fn info_action(icon: &str, button_label: &str, heading: &str, description: &str) -> Dom {
    let button = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; align-items: center; \
             justify-content: center; width: 78px; height: 66px; flex-grow: 0; \
             border: 1px solid #c5c5c5; background: white; margin-right: 18px; \
             :hover { background: #f2f7fc; }",
        )
        .with_child(Dom::create_icon(icon).with_css("font-size: 24px; color: #2b579a;"))
        .with_child(boxed(
            "margin-top: 3px;",
            fonts::text(button_label, 10, TEXT),
        ));
    let text_col = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1;")
        .with_child(boxed("", fonts::text(heading, 16, TEXT)))
        .with_child(boxed(
            "margin-top: 4px; max-width: 430px;",
            fonts::text(description, 12, TEXT_GRAY),
        ));
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; align-items: center; flex-grow: 0; \
             margin-bottom: 26px;",
        )
        .with_child(button)
        .with_child(text_col)
}

fn property_row(label: &str, value: &str) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; flex-grow: 0; margin-bottom: 9px;")
        .with_child(
            Dom::create_div()
                .with_css("width: 130px; flex-grow: 0;")
                .with_child(fonts::text(label, 12, TEXT_GRAY)),
        )
        .with_child(fonts::text(value, 12, TEXT))
}

fn info_pane(state: &AppState) -> Dom {
    let name = state.document.display_name();
    let location = if state.document.path.is_some() {
        "Documents"
    } else {
        "Desktop"
    };
    let words = state.document.word_count().to_string();

    let left = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; margin-top: 30px;")
        .with_child(info_action(
            "security",
            "Protect",
            "Protect Document",
            "Control what types of changes people can make to this document.",
        ))
        .with_child(info_action(
            "find_in_page",
            "Check for Issues",
            "Inspect Document",
            "Before publishing this file, be aware that it contains document \
             properties and the author's name.",
        ))
        .with_child(info_action(
            "history",
            "Manage Versions",
            "Versions",
            "There are no previous versions of this file.",
        ));

    let right = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 0; width: 280px; \
             margin-top: 30px;",
        )
        .with_child(boxed(
            "margin-bottom: 14px;",
            fonts::text("Properties", 15, TEXT),
        ))
        .with_child(property_row("Size", "\u{2014}"))
        .with_child(property_row("Pages", "1"))
        .with_child(property_row("Words", &words))
        .with_child(property_row("Total Editing Time", "0 Minutes"))
        .with_child(boxed(
            "margin-top: 22px; margin-bottom: 14px;",
            fonts::text("Related Dates", 15, TEXT),
        ))
        .with_child(property_row("Last Modified", "Today"))
        .with_child(property_row("Created", "Today"));

    pane_frame()
        .with_child(pane_title("Info"))
        .with_child(boxed(
            "margin-top: 16px;",
            fonts::text(&name, 20, OFFICE_BLUE),
        ))
        .with_child(boxed("", fonts::text(location, 12, TEXT_FAINT)))
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; flex-grow: 1;")
                .with_child(left)
                .with_child(right),
        )
}

// ---------------------------------------------------------------------------
// Open pane
// ---------------------------------------------------------------------------

fn place_row(icon: &str, label: &str, active: bool) -> Dom {
    let bg = if active {
        "background: #d5e1f2;"
    } else {
        "background: transparent; :hover { background: #e8eff9; }"
    };
    Dom::create_div()
        .with_css(
            format!(
                "display: flex; flex-direction: row; align-items: center; flex-grow: 0; \
                 height: 46px; padding-left: 14px; cursor: pointer; {bg}",
            )
            .as_str(),
        )
        .with_child(
            Dom::create_icon(icon).with_css("font-size: 20px; color: #2b579a; margin-right: 12px;"),
        )
        .with_child(fonts::text(label, 13, TEXT))
}

fn recent_row(name: &str, place: &str) -> Dom {
    let text_col = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1;")
        .with_child(fonts::text(name, 13, TEXT))
        .with_child(fonts::text(place, 11, TEXT_FAINT));
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; align-items: center; flex-grow: 0; \
             height: 42px; padding-left: 8px; padding-right: 8px; cursor: pointer; \
             :hover { background: #d6e2f3; }",
        )
        .with_child(doc_icon())
        .with_child(text_col)
}

/// The bordered "Browse" button — opens the native *.md file dialog.
fn browse_button(data: &RefAny) -> Dom {
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; align-items: center; flex-grow: 0; \
             border: 1px solid #ababab; background: #fdfdfd; padding: 7px 16px 7px 12px; \
             margin-top: 18px; margin-left: 14px; cursor: pointer; \
             :hover { background: #e8f0fa; }",
        )
        .with_child(
            Dom::create_icon("folder_open")
                .with_css("font-size: 18px; color: #2b579a; margin-right: 8px;"),
        )
        .with_child(fonts::text("Browse", 13, TEXT))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            data.clone(),
            crate::on_browse_clicked,
        )
}

fn open_pane(data: &RefAny) -> Dom {
    let places = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 0; width: 270px; \
             margin-top: 26px;",
        )
        .with_child(place_row("schedule", "Recent Documents", true))
        .with_child(place_row("computer", "Computer", false))
        .with_child(place_row("add", "Add a Place", false))
        .with_child(browse_button(data));

    let recent = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; margin-top: 26px; \
             margin-left: 30px;",
        )
        .with_child(boxed(
            "margin-bottom: 12px;",
            fonts::text("Recent Documents", 16, OFFICE_BLUE),
        ))
        .with_child(recent_row("Welcome", "Desktop"))
        .with_child(recent_row("Project Notes", "Documents"))
        .with_child(recent_row("recipes", "Desktop \u{00bb} Personal"))
        .with_child(recent_row(
            "Meeting Minutes 2026",
            "Documents \u{00bb} Work",
        ))
        .with_child(recent_row("Velvet Market Segmentation", "Downloads"));

    pane_frame().with_child(pane_title("Open")).with_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; flex-grow: 1;")
            .with_child(places)
            .with_child(recent),
    )
}

// ---------------------------------------------------------------------------
// The backstage screen
// ---------------------------------------------------------------------------

/// Full-window backstage takeover: widget chrome + the active pane.
pub fn backstage_screen(state: &AppState, data: &RefAny) -> Dom {
    let title = format!("{} - AzWriter", state.document.display_name());

    // The white strip right of the nav column: centered title, help and the
    // window buttons — no quick-access actions (per the the Office-2013-era look
    // screenshots, which show "? − ⧉ ✕" top right in the backstage).
    let mut strip = QuickAccessBar::new(AzString::from(title));
    strip.trailing_actions =
        vec![QuickAccessAction::new(AzString::from("help_outline"))].into();
    crate::fonts::push_ui_font(&mut strip.style.bar_style);
    let title_strip = strip.dom();

    let content = match state.backstage_pane {
        0 => info_pane(state),
        2 => open_pane(data),
        7 => export_pane(state, data),
        i => {
            let label = OFFICE_2013_NAV_LABELS.get(i).copied().unwrap_or(if i == 9 {
                "Account"
            } else {
                "Options"
            });
            pane_frame().with_child(pane_title(label))
        }
    };

    let mut backstage = Backstage::office_2013()
        .with_active_item(state.backstage_pane)
        .with_on_nav_select(
            data.clone(),
            BackstageOnNavSelectCallback {
                cb: crate::on_backstage_nav as BackstageOnNavSelectCallbackType,
                callable: OptionRefAny::None,
            },
        )
        .with_on_back(
            data.clone(),
            crate::on_backstage_back as ButtonOnClickCallbackType,
        )
        .with_title_strip(title_strip)
        .with_content(content);
    // WORKAROUND(engine): pin the static UI font (inherits into the panes).
    crate::fonts::push_ui_font(&mut backstage.style.root_style);
    backstage.dom()
}
