use azul::{
    callbacks::{ButtonOnClickCallbackType, RefAny},
    css::{EventFilter, HoverEventFilter, SystemStyle},
    dom::Dom,
    str::String as AzString,
    widgets::{Backstage, Button, QuickAccessAction, QuickAccessBar, QuickAccessStyle},
};

use crate::{fonts, palette::Palette, AppState};

const OFFICE_2013_NAV_LABELS: &[&str] = &[
    "Info", "New", "Open", "Save", "Save As", "Print", "Share", "Export", "Close",
];

fn export_pane(state: &AppState, data: &RefAny, pal: &Palette) -> Dom {
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
    pane_frame(pal)
        .with_child(pane_title("Export", pal))
        .with_child(
            Dom::create_div()
                .with_css("flex-grow: 0; margin-top: 18px; display: flex; flex-direction: column;")
                .with_child(fonts::text("Create PDF/XPS Document", 16, pal.brand_text))
                .with_child(
                    Dom::create_div()
                        .with_css("flex-grow: 0; margin-top: 6px;")
                        .with_child(fonts::text(&desc, 12, pal.text_gray)),
                )
                .with_child(
                    Dom::create_div()
                        .with_css("flex-grow: 0; margin-top: 16px; width: 160px;")
                        .with_child(button.dom()),
                ),
        )
}

fn pane_title(text: &str, pal: &Palette) -> Dom {
    Dom::create_div()
        .with_css("flex-grow: 0; display: flex; flex-direction: row;")
        .with_child(fonts::text(text, 38, pal.title_gray))
}

fn pane_frame(pal: &Palette) -> Dom {
    Dom::create_div().with_css(format!(
        "display: flex; flex-direction: column; flex-grow: 1; padding-left: 46px; padding-top: \
         20px; padding-right: 30px; background: {};",
        Palette::hex(pal.chrome)
    ))
}

fn doc_icon(pal: &Palette) -> Dom {
    Dom::create_div()
        .with_css(format!(
            "display: flex; align-items: center; justify-content: center; width: 22px; height: \
             22px; background: {}; flex-grow: 0; margin-right: 10px;",
            Palette::hex(pal.brand)
        ))
        .with_child(fonts::text("W", 12, pal.on_brand))
}

fn boxed(css: &str, child: Dom) -> Dom {
    Dom::create_div()
        .with_css(format!("display: flex; flex-direction: row; flex-grow: 0; {css}").as_str())
        .with_child(child)
}

fn info_action(
    icon: &str,
    button_label: &str,
    heading: &str,
    description: &str,
    pal: &Palette,
) -> Dom {
    let button = Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; align-items: center; justify-content: center; \
             width: 78px; height: 66px; flex-grow: 0; border: 1px solid {border}; background: \
             {bg}; margin-right: 18px; :hover {{ background: {hover}; }}",
            border = Palette::hex(pal.control_border),
            bg = Palette::hex(pal.control_bg),
            hover = Palette::hex(pal.hover_bg),
        ))
        .with_child(Dom::create_icon(icon).with_css(format!(
            "font-size: 24px; color: {};",
            Palette::hex(pal.brand_text)
        )))
        .with_child(boxed(
            "margin-top: 3px;",
            fonts::text(button_label, 10, pal.text),
        ));
    let text_col = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1;")
        .with_child(boxed("", fonts::text(heading, 16, pal.text)))
        .with_child(boxed(
            "margin-top: 4px; max-width: 430px;",
            fonts::text(description, 12, pal.text_gray),
        ));
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; align-items: center; flex-grow: 0; \
             margin-bottom: 26px;",
        )
        .with_child(button)
        .with_child(text_col)
}

fn property_row(label: &str, value: &str, pal: &Palette) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; flex-grow: 0; margin-bottom: 9px;")
        .with_child(
            Dom::create_div()
                .with_css("width: 130px; flex-grow: 0;")
                .with_child(fonts::text(label, 12, pal.text_gray)),
        )
        .with_child(fonts::text(value, 12, pal.text))
}

fn info_pane(state: &AppState, pal: &Palette) -> Dom {
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
            pal,
        ))
        .with_child(info_action(
            "find_in_page",
            "Check for Issues",
            "Inspect Document",
            "Before publishing this file, be aware that it contains document properties and the \
             author's name.",
            pal,
        ))
        .with_child(info_action(
            "history",
            "Manage Versions",
            "Versions",
            "There are no previous versions of this file.",
            pal,
        ));

    let right = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 0; width: 280px; margin-top: 30px;",
        )
        .with_child(boxed(
            "margin-bottom: 14px;",
            fonts::text("Properties", 15, pal.text),
        ))
        .with_child(property_row("Size", "\u{2014}", pal))
        .with_child(property_row("Pages", "1", pal))
        .with_child(property_row("Words", &words, pal))
        .with_child(property_row("Total Editing Time", "0 Minutes", pal))
        .with_child(boxed(
            "margin-top: 22px; margin-bottom: 14px;",
            fonts::text("Related Dates", 15, pal.text),
        ))
        .with_child(property_row("Last Modified", "Today", pal))
        .with_child(property_row("Created", "Today", pal));

    pane_frame(pal)
        .with_child(pane_title("Info", pal))
        .with_child(boxed(
            "margin-top: 16px;",
            fonts::text(&name, 20, pal.brand_text),
        ))
        .with_child(boxed("", fonts::text(location, 12, pal.text_faint)))
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; flex-grow: 1;")
                .with_child(left)
                .with_child(right),
        )
}

fn place_row(icon: &str, label: &str, active: bool, pal: &Palette) -> Dom {
    let bg = if active {
        format!("background: {};", Palette::hex(pal.selected_bg))
    } else {
        format!(
            "background: transparent; :hover {{ background: {}; }}",
            Palette::hex(pal.hover_bg)
        )
    };
    Dom::create_div()
        .with_css(
            format!(
                "display: flex; flex-direction: row; align-items: center; flex-grow: 0; height: \
                 46px; padding-left: 14px; cursor: pointer; {bg}",
            )
            .as_str(),
        )
        .with_child(Dom::create_icon(icon).with_css(format!(
            "font-size: 20px; color: {}; margin-right: 12px;",
            Palette::hex(pal.brand_text)
        )))
        .with_child(fonts::text(label, 13, pal.text))
}

fn recent_row(name: &str, place: &str, pal: &Palette) -> Dom {
    let text_col = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1;")
        .with_child(fonts::text(name, 13, pal.text))
        .with_child(fonts::text(place, 11, pal.text_faint));
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; flex-grow: 0; height: 42px; \
             padding-left: 8px; padding-right: 8px; cursor: pointer; :hover {{ background: {}; }}",
            Palette::hex(pal.hover_bg)
        ))
        .with_child(doc_icon(pal))
        .with_child(text_col)
}

fn browse_button(data: &RefAny, pal: &Palette) -> Dom {
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; flex-grow: 0; border: 1px \
             solid {border}; background: {bg}; padding: 7px 16px 7px 12px; margin-top: 18px; \
             margin-left: 14px; cursor: pointer; :hover {{ background: {hover}; }}",
            border = Palette::hex(pal.control_border),
            bg = Palette::hex(pal.control_bg),
            hover = Palette::hex(pal.hover_bg),
        ))
        .with_child(Dom::create_icon("folder_open").with_css(format!(
            "font-size: 18px; color: {}; margin-right: 8px;",
            Palette::hex(pal.brand_text)
        )))
        .with_child(fonts::text("Browse", 13, pal.text))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            data.clone(),
            crate::on_browse_clicked,
        )
}

fn open_pane(data: &RefAny, pal: &Palette) -> Dom {
    let places = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 0; width: 270px; margin-top: 26px;",
        )
        .with_child(place_row("schedule", "Recent Documents", true, pal))
        .with_child(place_row("computer", "Computer", false, pal))
        .with_child(place_row("add", "Add a Place", false, pal))
        .with_child(browse_button(data, pal));

    let recent = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; margin-top: 26px; margin-left: \
             30px;",
        )
        .with_child(boxed(
            "margin-bottom: 12px;",
            fonts::text("Recent Documents", 16, pal.brand_text),
        ))
        .with_child(recent_row("Welcome", "Desktop", pal))
        .with_child(recent_row("Project Notes", "Documents", pal))
        .with_child(recent_row("recipes", "Desktop \u{00bb} Personal", pal))
        .with_child(recent_row(
            "Meeting Minutes 2026",
            "Documents \u{00bb} Work",
            pal,
        ))
        .with_child(recent_row("Velvet Market Segmentation", "Downloads", pal));

    pane_frame(pal)
        .with_child(pane_title("Open", pal))
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; flex-grow: 1;")
                .with_child(places)
                .with_child(recent),
        )
}

pub fn backstage_screen(state: &AppState, data: &RefAny, pal: &Palette, sys: &SystemStyle) -> Dom {
    let title = format!("{} - AzWriter", state.document.display_name());

    let mut strip = QuickAccessBar::create(AzString::from(title));
    strip.trailing_actions = vec![QuickAccessAction::create(AzString::from("help_outline"))].into();
    strip.style = QuickAccessStyle::from_system(SystemStyle::clone(sys));
    let strip_bar = strip.style.resolved_bar_style();
    crate::fonts::push_ui_font(&mut strip.style.bar_style, strip_bar);
    let title_strip = strip.dom();

    let content = match state.backstage_pane {
        0 => info_pane(state, pal),
        2 => open_pane(data, pal),
        7 => export_pane(state, data, pal),
        i => {
            let label = OFFICE_2013_NAV_LABELS.get(i).copied().unwrap_or(if i == 9 {
                "Account"
            } else {
                "Options"
            });
            pane_frame(pal).with_child(pane_title(label, pal))
        }
    };

    let mut backstage = Backstage::office_2013()
        .with_active_item(state.backstage_pane)
        .with_on_nav_select(
            data.clone(),
            crate::on_backstage_nav,
        )
        .with_on_back(
            data.clone(),
            crate::on_backstage_back as ButtonOnClickCallbackType,
        )
        .with_title_strip(title_strip)
        .with_content(content);
    backstage.style = crate::palette::widgets::backstage(pal, sys);
    let root = backstage.style.resolved_root_style();
    crate::fonts::push_ui_font(&mut backstage.style.root_style, root);
    backstage.dom()
}
