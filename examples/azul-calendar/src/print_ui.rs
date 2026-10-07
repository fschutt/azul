//! FILE > Print, as Outlook 2010's print page: on the left the print style (Daily, Weekly
//! Agenda, Monthly), the range (Start, End) and Print; on the right the preview - the
//! printout's first pages.
//!
//! The printout is a PDF that azul's PDF writer makes from a DOM laid out for paper
//! ([`paper`]: A4 sheets, one page of the style a sheet, light paper with the calendars' own
//! colours whatever mode the window is in), saved through the system's save dialog
//! (`FileDialog::save_bytes`). The preview is that same PDF, made on an azul `Thread` and drawn
//! back page by page (PDF -> SVG -> picture, as AzDrive previews a PDF), so it shows what Print
//! saves: Print saves the preview's own bytes when they are of the printout the page shows (and
//! makes them in the callback, with the window's fonts, when they are not yet).
//!
//! The preview follows the page: the main window's write timer (`writes.rs`) and every change
//! of the style or the range call [`pump`], which starts a preview when the printout differs
//! from the one previewed (another style, range, an event saved meanwhile) and none is on its
//! way.
//!
//! On stdout, for scripts: `AZCAL_PRINT_PREVIEW <style> <pages>` when a preview was made (0
//! pages when it could not be), `AZCAL_PRINTED <style> <bytes>` when a printout was saved.

use azul::{
    callbacks::{ButtonOnClickCallbackType, DatePickerOnChangeCallbackType},
    dialog::FileDialog,
    error::ResultParsedSvgSvgParseError,
    font::FontCacheSnapshot,
    image::{ImageCacheSnapshot, ImageRef},
    option::OptionColorU,
    pdf::Pdf,
    prelude::*,
    str::String as AzString,
    svg::{ParsedSvg, SvgFitTo, SvgParseOptions, SvgRenderOptions},
    vec::U8VecRef,
    widgets::{ButtonType, DatePicker, DatePickerState, DatePickerWeekStart},
};
use chrono::{Datelike, NaiveDate};

use crate::{
    args::BackstagePage,
    chrome, ids,
    print::{
        self, content_px, Day, Item, Job, Page, Settings, Style, DAY_ALL_DAY_LINES, DAY_LINE_PX,
        FOOTER_PX, HEADER_GAP_PX, HEADER_PX, MARGIN_PX, MONTH_HEAD_PX, MONTH_LINE_PX,
        MONTH_NAMES_PX, WEEK_GAP_PX, WEEK_HEAD_PX, WEEK_LINE_PX,
    },
    views, week, CalState, ERROR, LABEL, SECONDARY,
};

/// How many pages the preview draws (Print saves them all).
const PREVIEW_PAGES: usize = 6;
/// The long edge of a sheet in the preview, in logical px.
const PREVIEW_LONG_PX: f32 = 640.0;
/// The preview's pictures are drawn at twice the size they are shown at: sharp small print.
const PREVIEW_SCALE: f32 = 2.0;
/// The surface the preview's sheets lie on: grey, so that white paper stands out on it.
pub(crate) const PREVIEW_SURFACE: &str = "background: #d5d8dc; @media (prefers-color-scheme: \
                                          dark) { background: #1c1c1c; }";
/// What Print says when azul made no PDF.
const NO_PDF: &str = "azul made no PDF: this build of azul has no PDF writer (its `pdf` feature).";

// ==== The page ====

/// What the preview holds: the printout it was made of, its PDF and its first pages' pictures.
#[derive(Default)]
pub(crate) struct Preview {
    /// Previews started so far: each one's number, so an answer for an older one is ignored.
    serial: u64,
    /// The preview on its way, by its number.
    pending: Option<u64>,
    /// The printout the preview below is of.
    made: Option<Job>,
    pdf: Vec<u8>,
    pages: Vec<PreviewPage>,
    /// How many pages the printout has (the preview draws `PREVIEW_PAGES` at most).
    page_count: usize,
    /// Why there is no preview (empty when there is one).
    error: String,
}

/// A page of the preview: its picture and the size it is shown at.
struct PreviewPage {
    image: ImageRef,
    width: f32,
    height: f32,
}

/// The printout of what the Print page shows now.
pub(crate) fn job_of(s: &CalState) -> Job {
    print::job(s.print, s.today, &s.events, &s.calendars, |e| s.shows(e))
}

/// The Print page (`ids::backstage_page("print")`): the style, the range, Print, the preview.
pub(crate) fn print_page(s: &CalState, app: &RefAny) -> Dom {
    let settings = s.print;
    let styles: [(Style, ButtonOnClickCallbackType); 3] = [
        (Style::Daily, on_style_daily),
        (Style::Weekly, on_style_weekly),
        (Style::Monthly, on_style_monthly),
    ];
    let mut left = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; width: 280px; flex-shrink: 0; margin-right: \
             24px; overflow-y: auto;",
        )
        .with_child(Dom::create_span_with_text("Print style").with_css(LABEL));
    for (style, cb) in styles {
        left.add_child(
            Button::create(style.label())
                .with_icon(style.icon())
                .with_toggled(settings.style == style)
                .with_on_click(app.clone(), cb)
                .dom()
                .with_id(ids::print_style(style.name()))
                .with_accessibility_name(style.label())
                .with_css("margin-top: 6px;"),
        );
    }
    left.add_child(Dom::create_span_with_text("Print range").with_css(LABEL));
    left.add_child(date_line("Start", settings.from, ids::PRINT_START, app, on_start));
    left.add_child(date_line("End", settings.to, ids::PRINT_END, app, on_end));
    left.add_child(
        Dom::create_span_with_text(settings.describe())
            .with_id(ids::PRINT_PAGES)
            .with_css(format!("font-size: 12px; margin-top: 10px; {SECONDARY}")),
    );
    left.add_child(
        Button::create("Print")
            .with_icon("print")
            .with_button_type(ButtonType::Primary)
            .with_on_click(app.clone(), on_print)
            .dom()
            .with_id(ids::PRINT_RUN)
            .with_css("margin-top: 16px;"),
    );
    left.add_child(
        Dom::create_span_with_text(
            "Print saves the printout as a PDF file, to print from there or to keep.",
        )
        .with_css(format!("font-size: 12px; margin-top: 6px; {SECONDARY}")),
    );
    if !s.print_message.is_empty() {
        let css = if s.print_failed {
            ERROR.to_string()
        } else {
            format!("font-size: 13px; margin-top: 12px; {SECONDARY}")
        };
        left.add_child(
            Dom::create_span_with_text(s.print_message.as_str())
                .with_id(ids::PRINT_MESSAGE)
                .with_css(css),
        );
    }
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; min-height: 0; padding: 20px \
             28px; color: system:text;",
        )
        .with_child(chrome::page_title("Print"))
        .with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; flex-grow: 1; min-height: 0; \
                     margin-top: 8px;",
                )
                .with_child(left)
                .with_child(preview(s)),
        )
}

/// "Start" / "End" and the day, a date picker.
fn date_line(
    label: &str,
    date: NaiveDate,
    id: AzString,
    app: &RefAny,
    cb: DatePickerOnChangeCallbackType,
) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; margin-top: 8px;")
        .with_child(
            Dom::create_span_with_text(label)
                .with_css(format!("width: 48px; flex-shrink: 0; {SECONDARY}")),
        )
        .with_child(
            DatePicker::create(date.year().max(1) as u32, date.month(), date.day())
                // The calendar's weeks run Monday to Sunday: so do its date pickers' rows.
                .with_week_start(DatePickerWeekStart::Monday)
                .with_accessibility_name(format!("{label} of the printout"))
                .with_on_change(app.clone(), cb)
                .dom()
                .with_id(id),
        )
}

/// The preview (`ids::PRINT_PREVIEW`): the printout's first pages as pictures, each with its
/// number; a line while one is being made.
fn preview(s: &CalState) -> Dom {
    let p = &s.print_preview;
    let mut pane = Dom::create_div().with_id(ids::PRINT_PREVIEW).with_css(format!(
        "display: flex; flex-direction: column; align-items: center; flex-grow: 1; min-width: \
         0; min-height: 0; overflow: auto; padding: 16px; {PREVIEW_SURFACE}"
    ));
    let caption = |text: String| {
        Dom::create_span_with_text(text).with_css(format!(
            "font-size: 12px; margin-top: 6px; margin-bottom: 14px; flex-shrink: 0; {SECONDARY}"
        ))
    };
    if p.made.is_none() || (p.pending.is_some() && p.pages.is_empty()) {
        pane.add_child(caption(String::from("Making the preview\u{2026}")));
        return pane;
    }
    if p.pending.is_some() {
        pane.add_child(caption(String::from("Updating the preview\u{2026}")));
    }
    if !p.error.is_empty() {
        pane.add_child(caption(p.error.clone()));
        return pane;
    }
    for (index, page) in p.pages.iter().enumerate() {
        let name = format!("Page {} of {}", index + 1, p.page_count);
        pane.add_child(
            Dom::create_image(page.image.clone())
                .with_id(ids::print_sheet(index))
                .with_accessibility_name(name.as_str())
                .with_css(format!(
                    "width: {:.0}px; height: {:.0}px; flex-shrink: 0; background: #ffffff; \
                     border: 1px solid #9aa0a6;",
                    page.width, page.height
                )),
        );
        pane.add_child(caption(name));
    }
    if p.page_count > p.pages.len() {
        pane.add_child(caption(format!(
            "The preview shows the first {} pages; Print saves all {}.",
            p.pages.len(),
            p.page_count
        )));
    }
    pane
}

// ==== The preview, on a Thread ====

/// A preview to make.
struct PreviewInit {
    serial: u64,
    job: Option<Job>,
    /// The window's fonts: the PDF's text is in the fonts the window shows.
    fonts: FontCacheSnapshot,
}

/// A preview made (or why not), for the UI thread.
struct PreviewMade {
    serial: u64,
    job: Job,
    result: Result<Rendered, String>,
}

struct Rendered {
    pdf: Vec<u8>,
    pages: Vec<PreviewPage>,
    count: usize,
}

/// The thread's answer, taken out once by the write-back.
struct PreviewDone {
    made: Option<PreviewMade>,
}

/// Starts a preview of the Print page's printout when the page is shown, the printout is not
/// the one previewed, and no preview is on its way (its answer calls this again).
pub(crate) fn pump(s: &mut CalState, info: &mut CallbackInfo, app: &RefAny) {
    if s.backstage != Some(BackstagePage::Print) || s.print_preview.pending.is_some() {
        return;
    }
    let job = job_of(s);
    if s.print_preview.made.as_ref() == Some(&job) {
        return;
    }
    s.print_preview.serial += 1;
    let serial = s.print_preview.serial;
    s.print_preview.pending = Some(serial);
    let init = RefAny::new(PreviewInit {
        serial,
        job: Some(job),
        fonts: info.get_font_cache_clone(),
    });
    info.add_thread(
        ThreadId::unique(),
        Thread::create(init, app.clone(), preview_thread),
    );
}

/// Runs on a worker thread: the printout's PDF, and its first pages drawn.
extern "C" fn preview_thread(
    mut init: RefAny,
    mut sender: ThreadSender,
    _receiver: ThreadReceiver,
) {
    let Some((serial, job, fonts)) = init
        .downcast_mut::<PreviewInit>()
        .and_then(|mut i| Some((i.serial, i.job.take()?, i.fonts.clone())))
    else {
        return;
    };
    let result = render(&job, fonts);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_preview_done,
        RefAny::new(PreviewDone {
            made: Some(PreviewMade {
                serial,
                job,
                result,
            }),
        }),
    )));
}

/// The printout's PDF (laid out with `fonts`), and pictures of its first pages: each page
/// becomes SVG (azul's PDF reader), and azul's SVG renderer draws it on white.
fn render(job: &Job, fonts: FontCacheSnapshot) -> Result<Rendered, String> {
    let style = job.settings.style;
    let (w, h) = style.page_px();
    let styled = StyledDom::create_from_dom(paper(job));
    let pdf = Pdf::create()
        .from_styled_dom_with_resources(styled, w, h, fonts, ImageCacheSnapshot::empty())
        .as_ref()
        .to_vec();
    if pdf.is_empty() {
        return Err(String::from(NO_PDF));
    }
    let svgs = Pdf::create().to_svg_pages(U8VecRef::from(&pdf[..]));
    let count = svgs.as_slice().len();
    let width = (preview_px(style).0 * PREVIEW_SCALE).round() as u32;
    let pages = svgs
        .as_slice()
        .iter()
        .take(PREVIEW_PAGES)
        .filter_map(|svg| picture(svg.as_str(), width))
        .collect();
    Ok(Rendered { pdf, pages, count })
}

/// A sheet of `style` in the preview, width and height: its long edge `PREVIEW_LONG_PX`.
fn preview_px(style: Style) -> (f32, f32) {
    let (w, h) = style.page_px();
    let scale = PREVIEW_LONG_PX / w.max(h);
    (w * scale, h * scale)
}

/// A page's SVG drawn `width` px wide on white, shown at `1 / PREVIEW_SCALE` of that.
fn picture(svg: &str, width: u32) -> Option<PreviewPage> {
    let parsed = match ParsedSvg::from_string(svg.to_string(), SvgParseOptions::create_default()) {
        ResultParsedSvgSvgParseError::Ok(parsed) => parsed,
        ResultParsedSvgSvgParseError::Err(_) => return None,
    };
    let mut options = SvgRenderOptions::create_default();
    options.fit = SvgFitTo::Width(width);
    options.background_color = OptionColorU::Some(ColorU {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    });
    let image = parsed.render(options).into_option()?;
    let (w, h) = (image.width as f32, image.height as f32);
    let image = ImageRef::create_rawimage(image).into_option()?;
    Some(PreviewPage {
        image,
        width: w / PREVIEW_SCALE,
        height: h / PREVIEW_SCALE,
    })
}

/// A preview landed: it is shown (when it is the one asked for last), and the next one starts
/// when the page changed meanwhile.
extern "C" fn on_preview_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(made) = msg
        .downcast_mut::<PreviewDone>()
        .and_then(|mut done| done.made.take())
    else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    if s.print_preview.pending != Some(made.serial) {
        return Update::DoNothing;
    }
    let style = made.job.settings.style;
    let p = &mut s.print_preview;
    p.pending = None;
    match made.result {
        Ok(rendered) => {
            println!("AZCAL_PRINT_PREVIEW {} {}", style.name(), rendered.count);
            p.pdf = rendered.pdf;
            p.pages = rendered.pages;
            p.page_count = rendered.count;
            p.error.clear();
        }
        Err(why) => {
            eprintln!("[azcalendar] no print preview: {why}");
            println!("AZCAL_PRINT_PREVIEW {} 0", style.name());
            p.pdf.clear();
            p.pages.clear();
            p.page_count = 0;
            p.error = why;
        }
    }
    p.made = Some(made.job);
    pump(s, &mut info, &handle);
    if s.backstage == Some(BackstagePage::Print) {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

// ==== Callbacks ====

/// Changes what the page prints (the preview follows).
fn set_print(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    change: impl FnOnce(Settings) -> Settings,
) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    s.print = change(s.print);
    s.print_message.clear();
    s.print_failed = false;
    pump(s, info, &app);
    Update::RefreshDom
}

/// A style: one page of it at the range's start (the style shown stays as it is).
fn choose(style: Style) -> impl FnOnce(Settings) -> Settings {
    move |p| {
        if p.style == style {
            p
        } else {
            p.with_style(style)
        }
    }
}

extern "C" fn on_style_daily(mut data: RefAny, mut info: CallbackInfo) -> Update {
    set_print(&mut data, &mut info, choose(Style::Daily))
}

extern "C" fn on_style_weekly(mut data: RefAny, mut info: CallbackInfo) -> Update {
    set_print(&mut data, &mut info, choose(Style::Weekly))
}

extern "C" fn on_style_monthly(mut data: RefAny, mut info: CallbackInfo) -> Update {
    set_print(&mut data, &mut info, choose(Style::Monthly))
}

extern "C" fn on_start(mut data: RefAny, mut info: CallbackInfo, state: DatePickerState) -> Update {
    let Some(day) = crate::picked(state) else {
        return Update::DoNothing;
    };
    set_print(&mut data, &mut info, |p| p.with_start(day))
}

extern "C" fn on_end(mut data: RefAny, mut info: CallbackInfo, state: DatePickerState) -> Update {
    let Some(day) = crate::picked(state) else {
        return Update::DoNothing;
    };
    set_print(&mut data, &mut info, |p| p.with_end(day))
}

/// Print: the printout as a PDF, through the system's save dialog. The preview's own PDF when
/// it is of the printout the page shows, else one made now with the window's fonts; the app's
/// state is let go before the dialog.
extern "C" fn on_print(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((settings, bytes)) = data.downcast_ref::<CalState>().map(|guard| {
        let s = &*guard;
        let job = job_of(s);
        let p = &s.print_preview;
        let bytes = if p.made.as_ref() == Some(&job) && !p.pdf.is_empty() {
            p.pdf.clone()
        } else {
            let (w, h) = job.settings.style.page_px();
            Pdf::create()
                .from_dom_in_callback(info, paper(&job), w, h)
                .as_ref()
                .to_vec()
        };
        (job.settings, bytes)
    }) else {
        return Update::DoNothing;
    };
    let (message, failed) = if bytes.is_empty() {
        eprintln!("[azcalendar] {NO_PDF}");
        (String::from(NO_PDF), true)
    } else {
        let len = bytes.len();
        let name = settings.file_name();
        if FileDialog::save_bytes(name.as_str(), "application/pdf", bytes) {
            println!("AZCAL_PRINTED {} {len}", settings.style.name());
            (format!("Saved {name} ({}).", settings.describe()), false)
        } else {
            (String::from("The printout was not saved."), false)
        }
    };
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    s.print_message = message;
    s.print_failed = failed;
    Update::RefreshDom
}

// ==== Paper ====

// Paper is white in either mode: fixed light colours, no `system:` colour and no dark twin.
const PAPER: &str = "#ffffff";
const INK: &str = "#202124";
const MUTED: &str = "#5f6368";
const RULE: &str = "#9aa0a6";
/// The heading of a Weekly day, the days of other months on a Monthly page, a printed day in a
/// small month.
const SHADE: &str = "#f1f3f4";
/// One line of text, cut with an ellipsis at its box's edge.
const CLIP: &str = "white-space: nowrap; overflow: hidden; text-overflow: ellipsis;";

/// A run of text on paper.
fn text(content: &str, css: &str) -> Dom {
    Dom::create_span_with_text(content).with_css(css)
}

/// The printout on paper: its pages one under the other, each exactly a sheet tall, so the
/// PDF writer (which cuts the document into sheets at the sheet's height) puts each on a sheet
/// of its own.
pub(crate) fn paper(job: &Job) -> Dom {
    let (w, _) = job.settings.style.page_px();
    let mut body = Dom::create_body().with_css(format!(
        "margin: 0px; padding: 0px; width: {w}px; background: {PAPER}; color: {INK}; \
         font-family: sans-serif;"
    ));
    for page in &job.pages {
        body.add_child(sheet(job, page));
    }
    body
}

/// One sheet: the margin, the header, the style's page, the footer.
fn sheet(job: &Job, page: &Page) -> Dom {
    let style = job.settings.style;
    let (w, h) = style.page_px();
    let content = match style {
        Style::Daily => day_page(page),
        Style::Weekly => week_page(page),
        Style::Monthly => month_page(page),
    };
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; width: {w}px; height: {h}px; padding: \
             {MARGIN_PX}px; box-sizing: border-box; overflow: hidden; background: {PAPER}; \
             color: {INK};"
        ))
        .with_child(header(style, page))
        .with_child(content)
        .with_child(footer(job))
}

/// The page's title (and week) on the left, two small months on the right - this month and the
/// next (a Monthly page: the months before and after it) - over a rule.
fn header(style: Style, page: &Page) -> Dom {
    let mut titles = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-width: 0;")
        .with_child(text(
            &page.title,
            &format!("font-size: 26px; font-weight: bold; {CLIP}"),
        ));
    if !page.subtitle.is_empty() {
        titles.add_child(text(
            &page.subtitle,
            &format!("font-size: 13px; margin-top: 4px; color: {MUTED};"),
        ));
    }
    let month = views::month_start(page.first);
    let next = views::month_end(page.first).succ_opt().unwrap_or(month);
    let (left, right, marked) = match style {
        Style::Monthly => (
            month
                .pred_opt()
                .map_or(month, views::month_start),
            next,
            None,
        ),
        Style::Daily | Style::Weekly => (month, next, Some((page.first, page.last))),
    };
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: flex-start; height: {HEADER_PX}px; \
             flex-shrink: 0; padding-bottom: 8px; box-sizing: border-box; border-bottom: 2px \
             solid {INK}; margin-bottom: {HEADER_GAP_PX}px;"
        ))
        .with_child(titles)
        .with_child(small_month(left, marked))
        .with_child(small_month(right, marked))
}

/// A small month for the header: its name, the weekdays' letters and its days, the days of
/// `marked` (the page's) shaded.
fn small_month(first: NaiveDate, marked: Option<(NaiveDate, NaiveDate)>) -> Dom {
    const CELL: &str = "width: 16px; height: 10px; flex-shrink: 0; text-align: center;";
    let mut month = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; width: 112px; flex-shrink: 0; margin-left: \
             18px; font-size: 8px;",
        )
        .with_child(text(
            &first.format("%B %Y").to_string(),
            "height: 12px; flex-shrink: 0; font-size: 9px; font-weight: bold; text-align: center;",
        ));
    let mut letters = Dom::create_div()
        .with_css(format!("display: flex; flex-direction: row; color: {MUTED};"));
    for letter in ["M", "T", "W", "T", "F", "S", "S"] {
        letters.add_child(text(letter, CELL));
    }
    month.add_child(letters);
    for days in print::month_days(first).chunks(7) {
        let mut row = Dom::create_div().with_css("display: flex; flex-direction: row;");
        for day in days {
            if day.month() != first.month() {
                row.add_child(Dom::create_div().with_css(CELL));
                continue;
            }
            let shaded = marked.is_some_and(|(from, to)| from <= *day && *day <= to);
            let css = if shaded {
                format!("{CELL} font-weight: bold; background: {SHADE};")
            } else {
                CELL.to_string()
            };
            row.add_child(text(&day.day().to_string(), &css));
        }
        month.add_child(row);
    }
    month
}

/// When it was printed, and the calendars printed with their colours.
fn footer(job: &Job) -> Dom {
    let mut row = Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; height: {FOOTER_PX}px; \
             flex-shrink: 0; padding-top: 6px; box-sizing: border-box; font-size: 9px; color: \
             {MUTED};"
        ))
        .with_child(text(
            &format!("Printed {} - AzCalendar", job.printed.format("%-d %B %Y")),
            "flex-grow: 1; min-width: 0;",
        ));
    for (name, colour) in &job.legend {
        row.add_child(Dom::create_div().with_css(format!(
            "width: 8px; height: 8px; margin-left: 10px; margin-right: 4px; flex-shrink: 0; \
             background: {};",
            colour.paint().light_edge
        )));
        row.add_child(text(name, "flex-shrink: 0;"));
    }
    row
}

/// An event's line on paper in its calendar's colour, as on screen: the tint, edged on the
/// left (an all-day event's all round). `full`: its times, title and place (Daily, Weekly);
/// else its start and title (a Monthly day's).
fn item_line(item: &Item, line_px: f32, font_px: f32, full: bool) -> Dom {
    let paint = item.colour.paint();
    let edge = if item.all_day {
        format!("border: 1px solid {};", paint.light_edge)
    } else {
        format!("border-left: 3px solid {};", paint.light_edge)
    };
    let mut line = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: row; align-items: center; height: {:.2}px; margin-top: \
         1px; padding: 0px 4px; box-sizing: border-box; overflow: hidden; flex-shrink: 0; \
         font-size: {font_px}px; color: {INK}; background: {}; {edge}",
        line_px - 1.0,
        paint.light_fill
    ));
    if full {
        line.add_child(text(
            &item.times(),
            &format!(
                "width: {:.0}px; flex-shrink: 0; color: {MUTED}; {CLIP}",
                font_px * 7.5
            ),
        ));
        line.add_child(text(
            &item.title,
            &format!("font-weight: bold; flex-shrink: 1; min-width: 0; {CLIP}"),
        ));
        if !item.location.is_empty() {
            line.add_child(text(
                &item.location,
                &format!(
                    "margin-left: 8px; flex-shrink: 1; min-width: 0; color: {MUTED}; {CLIP}"
                ),
            ));
        }
    } else {
        line.add_child(text(
            &item.short_line(),
            &format!("flex-grow: 1; min-width: 0; {CLIP}"),
        ));
    }
    line
}

/// The lines of `items` in a box with room for `rows` lines: all of them when they fit, else
/// one line fewer and "+N more" (as a month view's day).
fn item_lines(items: &[&Item], rows: usize, line_px: f32, font_px: f32, full: bool) -> Vec<Dom> {
    let (shown, more) = views::month_cell(items.len(), rows);
    let mut lines: Vec<Dom> = items
        .iter()
        .take(shown)
        .map(|item| item_line(item, line_px, font_px, full))
        .collect();
    if more > 0 {
        lines.push(text(
            &views::more_label(more),
            &format!(
                "height: {line_px}px; flex-shrink: 0; padding-left: 4px; font-size: {font_px}px; \
                 color: {MUTED};"
            ),
        ));
    }
    lines
}

/// A column of `lines`.
fn lines_column(lines: Vec<Dom>, css: &str) -> Dom {
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; min-width: 0; {css}"
        ))
        .with_children(DomVec::from_vec(lines))
}

/// Daily: the all-day events over the hours, each hour a row with the events that start in it.
fn day_page(page: &Page) -> Dom {
    let (_, h) = content_px(Style::Daily);
    let mut column = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: column; height: {h:.2}px; flex-shrink: 0;"
    ));
    let Some(day) = page.days.first() else {
        return column;
    };
    let gutter = "width: 56px; flex-shrink: 0; padding-top: 2px; font-size: 12px; font-weight: \
                  bold;";
    let all_day: Vec<&Item> = day.items.iter().filter(|i| i.all_day).collect();
    let mut used = 0.0;
    if !all_day.is_empty() {
        let strip = all_day.len().min(DAY_ALL_DAY_LINES) as f32 * DAY_LINE_PX + 8.0;
        used = strip;
        column.add_child(
            Dom::create_div()
                .with_css(format!(
                    "display: flex; flex-direction: row; height: {strip:.2}px; flex-shrink: 0; \
                     padding: 3px 0px; box-sizing: border-box; border-bottom: 1px solid {RULE};"
                ))
                .with_child(text("All day", gutter))
                .with_child(lines_column(
                    item_lines(&all_day, DAY_ALL_DAY_LINES, DAY_LINE_PX, 11.0, true),
                    "flex-grow: 1;",
                )),
        );
    }
    let (first, end) = print::day_hours(&day.items);
    let hours = end.saturating_sub(first).max(1);
    let hour_px = (h - used) / hours as f32;
    let rows = print::lines_in(hour_px, 0.0, DAY_LINE_PX);
    for hour in first..end {
        let items = print::in_hour(&day.items, hour);
        column.add_child(
            Dom::create_div()
                .with_css(format!(
                    "display: flex; flex-direction: row; height: {hour_px:.2}px; flex-shrink: 0; \
                     padding-top: 1px; box-sizing: border-box; overflow: hidden; border-bottom: \
                     1px solid {RULE};"
                ))
                .with_child(text(&week::hour_label(hour), gutter))
                .with_child(lines_column(
                    item_lines(&items, rows, DAY_LINE_PX, 11.0, true),
                    "flex-grow: 1;",
                )),
        );
    }
    column
}

/// Weekly: Monday to Thursday down the left, Friday to Sunday and a box for notes down the
/// right, each day with its events and their times.
fn week_page(page: &Page) -> Dom {
    let (_, h) = content_px(Style::Weekly);
    let box_px = (h - 3.0 * WEEK_GAP_PX) / 4.0;
    // The box's border takes 2 px of it.
    let rows = print::lines_in(box_px - 2.0, WEEK_HEAD_PX, WEEK_LINE_PX);
    let days: Vec<Option<&Day>> = page.days.iter().map(Some).chain([None]).collect();
    let side = |boxes: &[Option<&Day>], css: &str| {
        let mut column = Dom::create_div().with_css(format!(
            "display: flex; flex-direction: column; flex-grow: 1; flex-basis: 0px; min-width: 0; \
             {css}"
        ));
        for (index, day) in boxes.iter().enumerate() {
            let gap = if index == 0 { 0.0 } else { WEEK_GAP_PX };
            column.add_child(week_box(*day, box_px, gap, rows));
        }
        column
    };
    let split = days.len().min(4);
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; height: {h:.2}px; flex-shrink: 0;"
        ))
        .with_child(side(&days[..split], "margin-right: 12px;"))
        .with_child(side(&days[split..], ""))
}

/// A day of the week (or, `None`, the notes box): its name and date, its events.
fn week_box(day: Option<&Day>, box_px: f32, gap: f32, rows: usize) -> Dom {
    let (name, date) = match day {
        Some(day) => (
            day.date.format("%A").to_string(),
            day.date.format("%-d %B").to_string(),
        ),
        None => (String::from("Notes"), String::new()),
    };
    let mut b = Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; height: {box_px:.2}px; flex-shrink: 0; \
             margin-top: {gap}px; box-sizing: border-box; border: 1px solid {RULE}; overflow: \
             hidden;"
        ))
        .with_child(
            Dom::create_div()
                .with_css(format!(
                    "display: flex; flex-direction: row; align-items: center; height: \
                     {WEEK_HEAD_PX}px; flex-shrink: 0; padding: 0px 6px; box-sizing: border-box; \
                     background: {SHADE}; border-bottom: 1px solid {RULE};"
                ))
                .with_child(text(&name, "font-size: 11px; font-weight: bold; flex-shrink: 0;"))
                .with_child(text(
                    &date,
                    &format!("font-size: 11px; margin-left: 6px; color: {MUTED}; {CLIP}"),
                )),
        );
    if let Some(day) = day {
        let items: Vec<&Item> = day.items.iter().collect();
        b.add_child(lines_column(
            item_lines(&items, rows, WEEK_LINE_PX, 10.0, true),
            "padding: 1px 4px;",
        ));
    }
    b
}

/// Monthly: the weekdays' names over the month's weeks, each day with its number and its
/// events' starts and titles; the other months' days shaded.
fn month_page(page: &Page) -> Dom {
    let (_, h) = content_px(Style::Monthly);
    // The grid's top border takes a px of it.
    let inner = h - 1.0;
    let weeks = (page.days.len() / 7).max(1);
    let row_px = (inner - MONTH_NAMES_PX) / weeks as f32;
    let rows = print::lines_in(row_px, MONTH_HEAD_PX, MONTH_LINE_PX);
    let mut grid = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: column; height: {h:.2}px; flex-shrink: 0; box-sizing: \
         border-box; border-top: 1px solid {RULE}; border-left: 1px solid {RULE};"
    ));
    let mut names = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: row; height: {MONTH_NAMES_PX}px; flex-shrink: 0;"
    ));
    for day in page.days.iter().take(7) {
        names.add_child(text(
            &day.date.format("%A").to_string(),
            &format!(
                "flex-grow: 1; flex-basis: 0px; min-width: 0; padding: 3px 4px 0px 4px; \
                 box-sizing: border-box; font-size: 10px; font-weight: bold; color: {MUTED}; \
                 border-right: 1px solid {RULE}; border-bottom: 1px solid {RULE}; {CLIP}"
            ),
        ));
    }
    grid.add_child(names);
    for week in page.days.chunks(7) {
        let mut row = Dom::create_div().with_css(format!(
            "display: flex; flex-direction: row; height: {row_px:.2}px; flex-shrink: 0;"
        ));
        for day in week {
            let (ground, ink) = if day.in_month {
                (PAPER, INK)
            } else {
                (SHADE, MUTED)
            };
            let label = if day.date.day() == 1 {
                day.date.format("%-d %b").to_string()
            } else {
                day.date.day().to_string()
            };
            let items: Vec<&Item> = day.items.iter().collect();
            let mut cell = Dom::create_div()
                .with_css(format!(
                    "display: flex; flex-direction: column; flex-grow: 1; flex-basis: 0px; \
                     min-width: 0; padding: 2px 3px; box-sizing: border-box; overflow: hidden; \
                     background: {ground}; border-right: 1px solid {RULE}; border-bottom: 1px \
                     solid {RULE};"
                ))
                .with_child(text(
                    &label,
                    &format!(
                        "height: {MONTH_HEAD_PX}px; flex-shrink: 0; font-size: 10px; \
                         font-weight: bold; color: {ink};"
                    ),
                ));
            for line in item_lines(&items, rows, MONTH_LINE_PX, 9.0, false) {
                cell.add_child(line);
            }
            row.add_child(cell);
        }
        grid.add_child(row);
    }
    grid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_previewed_sheet_keeps_the_sheets_shape() {
        let (w, h) = preview_px(Style::Monthly);
        assert!((w - PREVIEW_LONG_PX).abs() < 0.01);
        assert!((w / h - print::A4_LONG_PX / print::A4_SHORT_PX).abs() < 0.001);
        let (w, h) = preview_px(Style::Daily);
        assert!((h - PREVIEW_LONG_PX).abs() < 0.01 && w < h);
    }

    /// Paper is light whatever mode the window is in: no colour of the desktop's, no dark twin.
    #[test]
    fn paper_is_light_in_either_mode() {
        for css in [PAPER, INK, MUTED, RULE, SHADE] {
            assert!(css.starts_with('#') && !css.contains("system:"), "{css}");
        }
        assert_eq!(PAPER, "#ffffff");
        assert!(PREVIEW_SURFACE.contains("@media (prefers-color-scheme: dark) {"));
    }
}
