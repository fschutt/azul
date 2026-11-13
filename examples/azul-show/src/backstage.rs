//! File: the backstage. Info, New (the theme picker with live previews,
//! colour variant, fonts, slide size), Open (the decks under `show/`, the
//! sample deck), Save, Export (PDF, the slide as a picture, into
//! `show/exports/`), Close, Options (azul-appkit's settings page over the
//! whole window: the app theme and the mode, remembered; the data folder;
//! the shortcuts table) and About (the standard About box).

use std::collections::HashMap;

use azul::{
    callbacks::{
        BackstageOnNavSelectCallbackType, ButtonOnClickCallbackType, CallbackInfo, RefAny,
        SegmentedOnChangeCallbackType, StandardDialogOnEventCallbackType, Update,
    },
    dom::Dom,
    shells::ShellEmptyState,
    vec::StringVec,
    widgets::{
        AboutDialog, Backstage, BackstageNavItem, Button, ButtonType, Segmented, SegmentedState,
        StandardDialogEvent,
    },
};

use azul_appkit::l10n::{t_args, Arg};

use crate::{
    app::{command, AppState, BackstagePage, Command},
    l10n::word,
    s,
    commands::on_command,
    model::{Deck, FontScheme, PlaceholderRole, SlideSize},
    render::{self, RenderOptions},
    themes,
};

fn strings(items: &[&str]) -> StringVec {
    azul_appkit::l10n::labels(items)
}

fn button(app: &RefAny, label: &str, cmd: Command) -> Dom {
    Button::create(s(label))
        .with_on_click(command(app, cmd), on_command as ButtonOnClickCallbackType)
        .dom()
}

fn primary(app: &RefAny, label: &str, cmd: Command) -> Dom {
    Button::create(s(label))
        .with_button_type(ButtonType::Primary)
        .with_on_click(command(app, cmd), on_command as ButtonOnClickCallbackType)
        .dom()
}

fn heading(text: &str) -> Dom {
    Dom::create_h2_with_text(s(text)).with_css("margin: 0px 0px 16px 0px; font-size: 26px; font-weight: normal;")
}

fn line(text: &str) -> Dom {
    Dom::create_p_with_text(s(text)).with_css("margin: 0px 0px 8px 0px; font-size: 13px;")
}

fn pane(children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; padding: 24px 36px; overflow-y: auto;")
        .with_children(children)
}

/// What a segmented control in the backstage sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Segment {
    Variant,
    Fonts,
    Size,
}

struct SegmentData {
    app: RefAny,
    segment: Segment,
}

extern "C" fn on_segment(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let (mut app, segment) = match data.downcast_ref::<SegmentData>() {
        Some(d) => (d.app.clone(), d.segment),
        None => return Update::DoNothing,
    };
    let i = state.selected_index;
    let cmd = match segment {
        Segment::Variant => Command::NewVariant(i),
        Segment::Fonts => Command::NewFonts(i),
        Segment::Size => Command::NewSize(if i == 1 { SlideSize::Standard } else { SlideSize::Wide }),
    };
    crate::commands::run(&mut app, cmd, &mut info)
}

fn segmented(app: &RefAny, segment: Segment, labels: &[&str], selected: usize) -> Dom {
    Segmented::create(strings(labels))
        .with_selected_index(selected)
        .with_on_change(
            RefAny::new(SegmentData {
                app: app.clone(),
                segment,
            }),
            on_segment as SegmentedOnChangeCallbackType,
        )
        .dom()
}

extern "C" fn on_nav(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let page = BackstagePage::NAV.get(index).copied().unwrap_or(BackstagePage::Info);
    let cmd = match page {
        BackstagePage::Close => Command::CloseDeck,
        page => Command::OpenBackstage(page),
    };
    crate::commands::run(&mut data, cmd, &mut info)
}

/// The About box's OK: back from the backstage.
extern "C" fn on_about_event(mut data: RefAny, mut info: CallbackInfo, _event: StandardDialogEvent) -> Update {
    crate::commands::run(&mut data, Command::CloseBackstage, &mut info)
}

/// A theme's card on File > New: a live title slide in the theme, its name.
fn theme_card(app: &RefAny, st: &AppState, index: usize, name: &str) -> Dom {
    let theme = themes::theme(index, st.new_variant, st.new_fonts);
    let mut deck = Deck::new("preview", name, theme, st.new_size);
    if let Some(body) = deck.slides[0]
        .elements
        .iter_mut()
        .find(|e| e.placeholder == Some(PlaceholderRole::Title))
        .and_then(|e| e.body_mut())
    {
        body.set_text(name);
    }
    let media = HashMap::new();
    let scale = 0.1;
    let picture = render::slide_dom(&deck, &deck.slides[0], &RenderOptions::still(scale, &media));
    // The theme's accents under its title slide (as PowerPoint's New shows
    // them): the grounds of the paper variants alone look alike.
    let colors = &deck.theme.colors;
    let mut accents = render::content(Dom::create_div().with_css("display: flex; flex-direction: row; height: 6px;"));
    for c in [colors.accent, colors.accent2, colors.accent3, colors.title] {
        accents.add_child(Dom::create_div().with_css(format!("flex-grow: 1; background: {};", render::css_color(c))));
    }
    // The chosen theme's ring: PowerPoint's orange; under flora the stone (AzShow's red spin).
    let ring = if st.new_theme == index {
        "border: 3px solid #d24726; @theme(flora) { border: 3px solid system:accent; }"
    } else {
        "border: 3px solid transparent;"
    };
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; align-items: stretch; margin: 0px 16px 16px 0px; {ring}"
        ))
        .with_child(picture)
        .with_child(accents)
        .with_child(button(app, name, Command::NewTheme(index)))
}

fn new_page(app: &RefAny, st: &AppState) -> Dom {
    let mut cards = Dom::create_div()
        .with_css("display: flex; flex-direction: row; flex-wrap: wrap; align-content: flex-start;");
    for (i, name) in themes::names().iter().enumerate() {
        cards.add_child(theme_card(app, st, i, name));
    }
    let fonts: Vec<String> = FontScheme::all().into_iter().map(|f| f.name).collect();
    let font_labels: Vec<&str> = fonts.iter().map(String::as_str).collect();
    let row = |label: &str, control: Dom| {
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center; margin: 0px 0px 10px 0px;")
            .with_child(Dom::create_p_with_text(s(label)).with_css("margin: 0px 12px 0px 0px; width: 90px; font-size: 13px;"))
            .with_child(control)
    };
    pane(vec![
        heading("azshow-backstage-new"),
        cards,
        row("azshow-variant", segmented(app, Segment::Variant, &themes::VARIANTS, st.new_variant)),
        row("azshow-fonts", segmented(app, Segment::Fonts, &font_labels, st.new_fonts)),
        row(
            "azshow-slide-size",
            segmented(
                app,
                Segment::Size,
                &["azshow-size-widescreen-16-9", "azshow-size-standard-4-3"],
                usize::from(st.new_size == SlideSize::Standard),
            ),
        ),
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; margin: 8px 0px 0px 0px;")
            .with_child(primary(app, "azshow-create", Command::NewDeck))
            .with_child(Dom::create_div().with_css("width: 12px;"))
            .with_child(button(app, "azshow-open-sample", Command::OpenSample)),
    ])
}

fn open_page(app: &RefAny, st: &AppState) -> Dom {
    let mut list = Dom::create_div().with_css("display: flex; flex-direction: column;");
    if st.decks.is_empty() {
        list.add_child(
            ShellEmptyState::create(s("azshow-no-decks"))
                .with_icon(s("slideshow"))
                .with_detail(s(&t_args(
                    "azshow-decks-saved-under",
                    &[("folder", Arg::from(st.data_root.join("show").display().to_string()))],
                )))
                .with_action_label(s("azshow-open-sample"))
                .with_on_action(command(app, Command::OpenSample), on_command as ButtonOnClickCallbackType)
                .dom(),
        );
    }
    for deck in &st.decks {
        list.add_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; padding: 6px 0px; \
                     border-bottom: 1px solid #c8c8c8; @theme(flora) { border-bottom: 1px solid \
                     system:separator; }",
                )
                .with_child(
                    Dom::create_div()
                        .with_css("display: flex; flex-direction: column; flex-grow: 1;")
                        .with_child(Dom::create_p_with_text(deck.title.as_str()).with_css("margin: 0px; font-size: 15px;"))
                        .with_child(
                            Dom::create_p_with_text(t_args(
                                "azshow-deck-row",
                                &[("count", Arg::from(deck.slides)), ("path", Arg::from(format!("show/{}", deck.id)))],
                            ))
                                .with_css("margin: 0px; font-size: 11px;"),
                        ),
                )
                .with_child(button(app, "azshow-open", Command::OpenDeck(deck.id.clone()))),
        );
    }
    pane(vec![
        heading("azshow-backstage-open"),
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; margin: 0px 0px 12px 0px;")
            .with_child(button(app, "azshow-refresh", Command::RefreshDecks))
            .with_child(Dom::create_div().with_css("width: 12px;"))
            .with_child(button(app, "azshow-sample-deck", Command::OpenSample)),
        list,
    ])
}

fn info_page(app: &RefAny, st: &AppState) -> Dom {
    let Some(ed) = st.editor.as_ref() else {
        return pane(vec![
            heading("azshow-backstage-info"),
            line("azshow-no-deck-open"),
            button(app, "azshow-backstage-new", Command::OpenBackstage(BackstagePage::New)),
        ]);
    };
    let path = st.data_root.join("show").join(&ed.deck.id).join("deck.json");
    pane(vec![
        heading(&ed.deck.title),
        line(&t_args(
            "azshow-info-slides",
            &[("count", Arg::from(ed.deck.slides.len())), ("size", Arg::from(word("size", ed.deck.size.label())))],
        )),
        line(&t_args(
            "azshow-info-theme",
            &[("theme", Arg::from(&ed.deck.theme.name)), ("fonts", Arg::from(&ed.deck.theme.fonts.name))],
        )),
        line(&t_args("azshow-info-file", &[("file", Arg::from(path.display().to_string()))])),
        line(if ed.dirty { "azshow-info-changed" } else { "azshow-info-saved" }),
        Dom::create_div().with_css("height: 8px;"),
        primary(app, "azshow-backstage-save", Command::Save),
    ])
}

fn export_page(app: &RefAny) -> Dom {
    pane(vec![
        heading("azshow-backstage-export"),
        line("azshow-export-pdf-about"),
        primary(app, "azshow-create-pdf", Command::ExportPdf),
        Dom::create_div().with_css("height: 20px;"),
        line("azshow-export-picture-about"),
        button(app, "azshow-save-picture", Command::ExportImages),
    ])
}

/// Options without a kit (with one, File > Options opens azul-appkit's settings page over the
/// whole window instead: commands.rs, `OpenBackstage(Options)`).
fn options_page() -> Dom {
    pane(vec![heading("azshow-backstage-options"), line("azshow-no-settings")])
}

/// About: the standard About box (DEDUP_OFFICE D12); OK goes back.
fn about_page(app: &RefAny) -> Dom {
    pane(vec![AboutDialog::create(s(crate::ABOUT.name), s(crate::ABOUT.version))
        .with_icon(s("slideshow"))
        .with_description(s(&azul_appkit::l10n::app_word(crate::ABOUT.name, "about-summary", crate::ABOUT.summary)))
        .with_copyright(s("azshow-copyright"))
        .with_credit(s("azul"), s("MIT"))
        .with_on_event(app.clone(), on_about_event as StandardDialogOnEventCallbackType)
        .dom()])
}

/// The backstage for the app's state.
#[must_use]
pub fn backstage(app: &RefAny, st: &AppState) -> Dom {
    let content = match st.page {
        BackstagePage::Info | BackstagePage::Save => info_page(app, st),
        BackstagePage::New => new_page(app, st),
        BackstagePage::Open => open_page(app, st),
        BackstagePage::Export => export_page(app),
        BackstagePage::Close => info_page(app, st),
        BackstagePage::Options => options_page(),
        BackstagePage::About => about_page(app),
    };
    let nav: Vec<BackstageNavItem> = BackstagePage::NAV
        .iter()
        .map(|p| {
            let item = BackstageNavItem::create(s(&word("backstage", p.label())));
            if *p == BackstagePage::Options {
                item.with_gap_before()
            } else {
                item
            }
        })
        .collect();
    Backstage::create(nav)
        .with_active_item(st.page.index())
        .with_on_nav_select(app.clone(), on_nav as BackstageOnNavSelectCallbackType)
        .with_on_back(command(app, Command::CloseBackstage), on_command as ButtonOnClickCallbackType)
        .with_content(content)
        .dom()
}
