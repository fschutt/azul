//! The article list, the middle of the window's three columns: a bar with the view's name and
//! All / Unread, the column headers (the unread dot, the star, Title, Source, Date; a click
//! sorts by the column, a second click turns it round; the sorted column is tinted and carries
//! its arrow), and a card per article: the unread dot over the star, the title (bold while
//! unread), the source and the date, the article's opening lines; the selected card in the
//! selection colour. Sorted by date, the cards sit under Outlook's date groups (azul-pim's
//! `DateGroup`: Today, Yesterday, Monday, Last Week ...).
//!
//! The rows are a VirtualView ([`table_view`]): only the rows in view (and a screen either
//! side) are built, however many articles the view has. Every row is [`ROW_PX`] high, a day's
//! header too, so a row's place is its position times the height ([`reveal_selected`] scrolls
//! the open article into view after J / K).

use azul::{
    callbacks::{ButtonOnClickCallbackType, SegmentedOnChangeCallbackType},
    dom::{DomId, DomNodeId, NodeId},
    prelude::*,
    shells::ShellEmptyState,
    str::String as AzString,
    widgets::Segmented,
};
use azul_appkit::{
    l10n::{t, t_args, tr, Arg},
    pieces::{block, button, column, flex_row, primary, strs, text},
};
use azul_pim::dates::{date_group, DateGroup};
use chrono::NaiveDate;

use super::{
    leave_form, local_offset_secs, on_add_open, on_filter, on_layout_change, on_load_card_pictures,
    on_mark_all_no, on_mark_all_yes, on_open_original, select, title_of, view_title, with_app,
    NewsApp, Pictures, FILL,
};
use crate::{
    ids,
    library::{ArticleRef, Library, Sort, SortKey},
};

/// A row's height (px): every row of the list, a day's header too. An article's card holds
/// three lines.
pub const ROW_PX: f32 = 58.0;

/// The bar over the table: the view's name and its counts, All / Unread.
const LIST_BAR: &str = "padding: 4px 8px; gap: 8px; flex-shrink: 0; \
                        border-bottom: 1px solid system:separator;";
const HEADING: &str = "flex-grow: 1; min-width: 0px; font-size: 12px; font-weight: 600; \
                       overflow: hidden; white-space: nowrap; text-overflow: ellipsis;";
/// The column headers: Mail's light bevel.
const HEADER: &str = "display: flex; flex-direction: row; flex-shrink: 0; height: 19px; \
                      font-size: 11px; border-bottom: 1px solid #b4b4b4; \
                      background: linear-gradient(to bottom, #fdfdfd, #e6e6e6); \
                      @media (prefers-color-scheme: dark) { border-bottom: 1px solid #121212; \
                      background: linear-gradient(to bottom, #3b3b3b, #2f2f2f); } \
                   @theme(flora) { border-bottom: 1px solid system:separator; background: #E9E7E2; @media (prefers-color-scheme: dark) { background: #1F1F1F; } }";
const HEAD_CELL: &str = "display: flex; flex-direction: row; align-items: center; height: 19px; \
                         box-sizing: border-box; padding: 0px 2px 0px 6px; overflow: hidden; \
                         white-space: nowrap; border-right: 1px solid rgba(128, 128, 128, 0.35); \
                         cursor: pointer; \
                   @theme(flora) { border-right: 1px solid system:grid; }";
/// The sorted column's header: Mail's blue.
const HEAD_ACTIVE: &str = "background: linear-gradient(to bottom, #cfe1f7, #a9c6ec); \
                           @media (prefers-color-scheme: dark) { \
                           background: linear-gradient(to bottom, #34537d, #27436b); } \
                   @theme(flora) { background: #E0E4EE; @media (prefers-color-scheme: dark) { background: #1E3260; } }";
/// The narrow columns (the unread dot, the star): centred.
const MARK_COL: &str = "width: 20px; flex-shrink: 0; justify-content: center; padding: 0px;";
const TITLE_COL: &str = "flex-grow: 1; flex-basis: 0px; min-width: 0px;";
/// The headers of the source and the date: as wide as their words (the column is narrow, the
/// cards carry both under the title).
const SOURCE_COL: &str = "flex-shrink: 0; padding-right: 8px;";
const DATE_COL: &str = "flex-shrink: 0; padding-right: 8px;";
/// The rows' box: the table's ground under the last row too.
const ROWS: &str = "flex-grow: 1; min-height: 0px; width: 100%; \
                    background: system:control-background;";
const ROW: &str = "display: flex; flex-direction: row; height: 58px; box-sizing: border-box; \
                   padding: 6px 10px 6px 0px; flex-shrink: 0; font-size: 12px; cursor: default; \
                   border-bottom: 1px solid system:separator;";
/// A card's marks, the unread dot over the star.
const CARD_MARKS: &str = "display: flex; flex-direction: column; align-items: center; \
                          width: 20px; flex-shrink: 0; padding-top: 2px; gap: 4px;";
/// A card's three lines.
const CARD_TEXT: &str = "display: flex; flex-direction: column; flex-grow: 1; min-width: 0px; \
                         gap: 1px;";
const CARD_TITLE: &str = "font-size: 13px;";
const CARD_LINE: &str = "font-size: 11px; color: system:secondary-text;";
const ROW_EVEN: &str = "background: system:control-background;";
const ROW_SELECTED: &str = "background: system:selection-background; \
                            color: system:selection-text;";
/// A line of text: one line, cut with an ellipsis.
const CLIP: &str = "overflow: hidden; white-space: nowrap; text-overflow: ellipsis; \
                    min-width: 0px;";
/// Mail's unread dot.
const DOT: &str = "font-size: 9px; color: #3478d8; \
                   @media (prefers-color-scheme: dark) { color: #6ea8ff; } \
                   @theme(flora) { color: #2F4A85; @media (prefers-color-scheme: dark) { color: #7A93C6; } }";
const STAR: &str = "font-size: 11px; color: #d99a00; \
                   @theme(flora) { color: #6E6349; @media (prefers-color-scheme: dark) { color: #C4B58E; } }";
const DAY_ROW: &str = "display: flex; flex-direction: row; align-items: flex-end; height: 58px; \
                       box-sizing: border-box; flex-shrink: 0; padding: 0px 0px 6px 8px; \
                       font-size: 11px; font-weight: 700; \
                       color: system:secondary-text; background: system:control-background; \
                       border-bottom: 1px solid system:separator;";

// ==== The rows, as data ====

/// One row of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableRow {
    /// A day's header (the table is sorted by date).
    Day(DateGroup),
    /// An article, and its place among the table's articles (its row's id, its stripe).
    Article(ArticleRef, usize),
}

/// The table's rows: the articles in `order`, each under its day's header while the table is
/// sorted by date (`today` and the dates in the local time `offset_secs` east of UTC).
#[must_use]
pub fn rows(
    lib: &Library,
    order: &[ArticleRef],
    sort: Sort,
    today: NaiveDate,
    offset_secs: i64,
) -> Vec<TableRow> {
    if sort.key != SortKey::Date {
        return order
            .iter()
            .enumerate()
            .map(|(i, r)| TableRow::Article(*r, i))
            .collect();
    }
    let mut out = Vec::with_capacity(order.len() + 8);
    let mut index = 0;
    for (group, members) in lib.sections(order, today, offset_secs) {
        out.push(TableRow::Day(group));
        for r in members {
            out.push(TableRow::Article(r, index));
            index += 1;
        }
    }
    out
}

/// An article's date in the table, as Mail writes it: `Today 10:42`, `Yesterday 09:15`,
/// `Monday 18:02` within the week (the day in the window's language), else `2026-09-12`
/// (`today` and the date in the local time `offset_secs` east of UTC).
#[must_use]
pub fn list_date(date: i64, today: NaiveDate, offset_secs: i64) -> String {
    let Some(local) = chrono::DateTime::<chrono::Utc>::from_timestamp(date + offset_secs, 0)
    else {
        return String::new();
    };
    let local = local.naive_utc();
    let group = date_group(local.date(), today);
    match group {
        DateGroup::Today | DateGroup::Yesterday | DateGroup::Weekday(_) => {
            format!("{} {}", t(&group.message_id()), local.format("%H:%M"))
        }
        _ => local.format("%Y-%m-%d").to_string(),
    }
}

// ==== The table ====

/// The bar over the table, the headers and the rows (or the empty state).
pub(super) fn list_pane(s: &NewsApp, app: &RefAny) -> Dom {
    let list = s.list();
    let unread = list.iter().filter(|r| !s.library.is_read(**r)).count();
    let mut children = vec![list_bar(s, app, list.len(), unread)];
    if !s.loaded {
        children.push(block(
            "padding: 16px; opacity: 0.7;",
            text(tr("aznews-reading-feeds")),
        ));
    } else if s.library.feeds.is_empty() {
        children.push(
            ShellEmptyState::create(t("aznews-no-feeds"))
                .with_icon("rss_feed")
                .with_detail(t("aznews-no-feeds-detail"))
                .with_action_label(t("aznews-add-feed"))
                .with_on_action(app.clone(), on_add_open as ButtonOnClickCallbackType)
                .dom(),
        );
    } else if list.is_empty() {
        children.push(
            ShellEmptyState::create(t("aznews-nothing-here"))
                .with_icon("search")
                .with_detail(if s.query.trim().is_empty() {
                    t("aznews-no-articles-in-view")
                } else {
                    t_args("aznews-no-match", &[("query", Arg::from(s.query.trim()))])
                })
                .dom(),
        );
    } else {
        let mut order = list;
        s.library.sort(&mut order, s.settings.sort);
        let today = chrono::Local::now().date_naive();
        let offset = local_offset_secs();
        let data = TableData {
            app: app.clone(),
            rows: rows(&s.library, &order, s.settings.sort, today, offset),
            selected: s.selected_ref(),
            today,
            offset,
            magazine: s.settings.magazine,
        };
        children.push(header(s, app));
        children.push(
            Dom::create_virtual_view(RefAny::new(data), table_view)
                .with_id(ids::ARTICLE_LIST)
                .with_accessibility_name(t("aznews-articles"))
                .with_css(ROWS),
        );
    }
    column(FILL, children)
}

/// The view's name and counts, All / Unread; "Mark all as read?" under it while it is asked.
fn list_bar(s: &NewsApp, app: &RefAny, count: usize, unread: usize) -> Dom {
    let heading = block(
        HEADING,
        text(format!(
            "{} \u{2014} {}",
            view_title(s),
            t_args(
                "aznews-count-unread",
                &[("count", Arg::from(count)), ("unread", Arg::from(unread))],
            )
        )),
    )
    .with_id(ids::LIST_HEADING);
    let filter = Segmented::create(strs(&["aznews-all", "aznews-unread"]))
        .with_selected_index(usize::from(s.unread_only))
        .with_on_change(app.clone(), on_filter as SegmentedOnChangeCallbackType)
        .dom()
        .with_id(ids::LIST_FILTER);
    let layout = Segmented::create(strs(&["aznews-layout-list", "aznews-layout-magazine"]))
        .with_selected_index(usize::from(s.settings.magazine))
        .with_on_change(app.clone(), on_layout_change as SegmentedOnChangeCallbackType)
        .dom()
        .with_id(ids::LIST_LAYOUT);
    let mut rows = vec![flex_row(LIST_BAR, vec![heading, filter, layout])];
    if s.settings.magazine
        && s.settings.pictures == Pictures::OnClick
        && !s.card_pictures_allowed
    {
        rows.push(flex_row(
            "padding: 4px 8px; gap: 6px; font-size: 12px; flex-shrink: 0; align-items: center; \
             border-bottom: 1px solid system:separator;",
            vec![
                block(
                    "flex-grow: 1; color: system:secondary-text;",
                    text(t("aznews-magazine-pictures-off")),
                ),
                button(
                    "aznews-load-pictures",
                    ids::LOAD_CARD_PICTURES,
                    app,
                    on_load_card_pictures,
                ),
            ],
        ));
    }
    if s.confirm_mark_all {
        rows.push(
            flex_row(
                "padding: 6px 8px; gap: 6px; font-size: 12px; flex-shrink: 0; \
                 border-bottom: 1px solid system:separator;",
                vec![
                    block(
                        "flex-grow: 1;",
                        text(t_args(
                            "aznews-mark-all-ask",
                            &[("count", Arg::from(count))],
                        )),
                    ),
                    primary(
                        "aznews-mark-as-read",
                        ids::MARK_ALL_YES,
                        app,
                        on_mark_all_yes,
                    ),
                    button("kit-button-cancel", ids::MARK_ALL_NO, app, on_mark_all_no),
                ],
            )
            .with_id(ids::MARK_ALL_CONFIRM),
        );
    }
    column("flex-shrink: 0;", rows)
}

/// What a column header carries: the app and the column.
struct SortRef {
    app: RefAny,
    key: SortKey,
}

/// The column headers: a click sorts by the column, again turns it round. A header's `label`
/// is a glyph or a key, its `name` (what a click does) a key.
fn header(s: &NewsApp, app: &RefAny) -> Dom {
    let sort = s.settings.sort;
    let cell = |key: SortKey, id: AzString, label: &str, name: &str, width: &str| -> Dom {
        let active = sort.key == key;
        let wide = key != SortKey::Unread && key != SortKey::Starred;
        let mut out = Dom::create_div()
            .with_id(id)
            .with_css(format!(
                "{HEAD_CELL} {width} {}",
                if active { HEAD_ACTIVE } else { "" }
            ))
            .with_accessibility_name(t(name))
            .with_child(block(
                &format!("{CLIP} flex-grow: 1;"),
                text(azul_appkit::l10n::label(label)),
            ));
        if active && wide {
            out.add_child(
                Dom::create_icon(if sort.descending {
                    "arrow_drop_down"
                } else {
                    "arrow_drop_up"
                })
                .with_css("font-size: 16px; flex-shrink: 0;"),
            );
        }
        out.with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            RefAny::new(SortRef {
                app: app.clone(),
                key,
            }),
            on_sort,
        )
    };
    Dom::create_div()
        .with_id(ids::TABLE_HEADER)
        .with_css(HEADER)
        .with_child(cell(
            SortKey::Unread,
            ids::SORT_UNREAD,
            "\u{25cf}",
            "aznews-sort-unread",
            MARK_COL,
        ))
        .with_child(cell(
            SortKey::Starred,
            ids::SORT_STARRED,
            "\u{2605}",
            "aznews-sort-starred",
            MARK_COL,
        ))
        .with_child(cell(
            SortKey::Title,
            ids::SORT_TITLE,
            "aznews-col-title",
            "aznews-sort-title",
            TITLE_COL,
        ))
        .with_child(cell(
            SortKey::Source,
            ids::SORT_SOURCE,
            "aznews-col-source",
            "aznews-sort-source",
            SOURCE_COL,
        ))
        .with_child(cell(
            SortKey::Date,
            ids::SORT_DATE,
            "aznews-col-date",
            "aznews-sort-date",
            DATE_COL,
        ))
}

/// A click on a column header: sorted by it (again: turned round), the order kept in the
/// settings.
extern "C" fn on_sort(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, key)) = data
        .downcast_ref::<SortRef>()
        .map(|r| (r.app.clone(), r.key))
    else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, info, _| {
        let sort = s.settings.sort.clicked(key);
        s.settings.sort = sort;
        azul_appkit::ui::set_value(&s.kit, info, "sort", &super::sort_value(sort));
        println!(
            "AZNEWS_SORTED {} {}",
            sort.key.key(),
            if sort.descending { "desc" } else { "asc" }
        );
    })
}

// ==== The rows: a VirtualView ====

/// What the table's VirtualView hands its callback: the app and the rows of this layout.
struct TableData {
    app: RefAny,
    rows: Vec<TableRow>,
    selected: Option<ArticleRef>,
    today: NaiveDate,
    offset: i64,
    /// The rows as the magazine's cards.
    magazine: bool,
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect::create(LogicalPosition::create(x, y), LogicalSize::create(w, h))
}

/// The rows in view and a screen either side, one under the other from the first one's place.
extern "C" fn table_view(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let Some(table) = data.downcast_ref::<TableData>() else {
        return VirtualViewReturn::default();
    };
    let mut app = table.app.clone();
    let Some(guard) = app.downcast_ref::<NewsApp>() else {
        return VirtualViewReturn::default();
    };
    let s = &*guard;
    let size = info.bounds.get_logical_size();
    let width = size.width.max(1.0);
    if table.magazine {
        return magazine_view(s, &table, width, size.height.max(CARD_ROW_PX), info.scroll_offset.y);
    }
    let height = size.height.max(ROW_PX);
    let n = table.rows.len();
    if n == 0 {
        return VirtualViewReturn::with_dom(
            Dom::create_div(),
            rect(0.0, 0.0, width, 1.0),
            rect(0.0, 0.0, width, 1.0),
        );
    }
    let screen = (height / ROW_PX).ceil() as usize + 1;
    // The offset kept from a longer list (another view, a search) may lie past this one's
    // end: what shows is clamped to the last screen of the rows there are.
    let last_top = (n as f32 * ROW_PX - height).max(0.0);
    let first_visible = (info.scroll_offset.y.clamp(0.0, last_top) / ROW_PX) as usize;
    let first = first_visible.saturating_sub(screen).min(n - 1);
    let end = (first_visible + 2 * screen).min(n).max(first + 1);
    let mut root = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: column; width: {width}px;"
    ));
    for row in &table.rows[first..end] {
        root.add_child(match *row {
            TableRow::Day(group) => Dom::create_div()
                .with_class(ids::DAY_HEADER_CLASS)
                .with_css(DAY_ROW)
                .with_child(text(t(&group.message_id()))),
            TableRow::Article(r, index) => article_row(s, &table, r, index),
        });
    }
    VirtualViewReturn::with_dom(
        root,
        rect(0.0, first as f32 * ROW_PX, width, (end - first) as f32 * ROW_PX),
        rect(0.0, 0.0, width, n as f32 * ROW_PX),
    )
}

// ==== The magazine ====
//
// The articles as cards with their pictures, as many a row as the width holds, under their
// days' headers while sorted by date: a VirtualView too, its rows of two heights (a day's
// header, a row of cards) found by their tops.

/// The narrowest a card gets, and the room around and between the cards (px).
pub const CARD_MIN_PX: f32 = 240.0;
pub const CARD_GAP_PX: f32 = 12.0;
/// A day's header and a row of cards (px).
pub const DAY_PX: f32 = 36.0;
pub const CARD_ROW_PX: f32 = 262.0;

const CARD: &str = "display: flex; flex-direction: column; flex-grow: 1; flex-basis: 0px; \
                    min-width: 0px; height: 250px; box-sizing: border-box; overflow: hidden; \
                    border-radius: 6px; border: 1px solid system:separator; \
                    background: system:window-background; cursor: default;";
const CARD_SELECTED: &str = "border: 2px solid system:accent;";
const CARD_PICTURE: &str = "height: 130px; flex-shrink: 0; background-size: cover; \
                            background-position: center; background-color: system:control-background;";
/// A card without its picture: the source's first letter on a quiet ground.
const CARD_PLACEHOLDER: &str = "height: 130px; flex-shrink: 0; display: flex; \
                                align-items: center; justify-content: center; font-size: 40px; \
                                font-weight: 700; color: system:secondary-text; \
                                background: system:control-background;";
const CARD_BODY: &str = "display: flex; flex-direction: column; gap: 3px; padding: 8px 10px; \
                         min-height: 0px; overflow: hidden;";
const CARD_HEADLINE: &str = "font-size: 14px; font-weight: 600; max-height: 38px; overflow: hidden;";
const CARD_EXCERPT: &str = "font-size: 12px; color: system:secondary-text; max-height: 46px; \
                            overflow: hidden;";
const MAGAZINE_DAY: &str = "display: flex; flex-direction: row; align-items: flex-end; \
                            height: 36px; box-sizing: border-box; padding: 0px 12px 6px 12px; \
                            font-size: 12px; font-weight: 700; color: system:secondary-text;";

/// One row of the magazine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MagazineRow {
    /// A day's header (the list is sorted by date).
    Day(DateGroup),
    /// A row of cards: the articles and their places among the list's.
    Cards(Vec<(ArticleRef, usize)>),
}

/// How many cards a row of `width` holds (at least one).
#[must_use]
pub fn cards_per_row(width: f32) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = ((width - CARD_GAP_PX) / (CARD_MIN_PX + CARD_GAP_PX)).floor().max(0.0) as usize;
    n.max(1)
}

/// The table's rows as the magazine's: each day's articles in rows of `per_row` cards (at
/// least one), a day's last row never filled with the next day's.
#[must_use]
pub fn magazine_rows(rows: &[TableRow], per_row: usize) -> Vec<MagazineRow> {
    let per_row = per_row.max(1);
    let mut out = Vec::new();
    let mut cards = Vec::new();
    for row in rows {
        match *row {
            TableRow::Day(group) => {
                if !cards.is_empty() {
                    out.push(MagazineRow::Cards(core::mem::take(&mut cards)));
                }
                out.push(MagazineRow::Day(group));
            }
            TableRow::Article(r, index) => {
                cards.push((r, index));
                if cards.len() == per_row {
                    out.push(MagazineRow::Cards(core::mem::take(&mut cards)));
                }
            }
        }
    }
    if !cards.is_empty() {
        out.push(MagazineRow::Cards(cards));
    }
    out
}

/// Where each row starts, and the end of the last: one more entry than `rows`.
#[must_use]
pub fn magazine_tops(rows: &[MagazineRow]) -> Vec<f32> {
    let mut tops = Vec::with_capacity(rows.len() + 1);
    let mut y = 0.0;
    tops.push(y);
    for row in rows {
        y += match row {
            MagazineRow::Day(_) => DAY_PX,
            MagazineRow::Cards(_) => CARD_ROW_PX,
        };
        tops.push(y);
    }
    tops
}

/// The row at `y` (the last one past the end, the first before the start).
#[must_use]
pub fn magazine_row_at(tops: &[f32], y: f32) -> usize {
    let rows = tops.len().saturating_sub(1);
    tops.partition_point(|top| *top <= y)
        .saturating_sub(1)
        .min(rows.saturating_sub(1))
}

/// The magazine's rows in view and a screen either side.
fn magazine_view(
    s: &NewsApp,
    table: &TableData,
    width: f32,
    height: f32,
    scroll_y: f32,
) -> VirtualViewReturn {
    let rows = magazine_rows(&table.rows, cards_per_row(width));
    let tops = magazine_tops(&rows);
    let total = tops.last().copied().unwrap_or(0.0).max(1.0);
    if rows.is_empty() {
        return VirtualViewReturn::with_dom(
            Dom::create_div(),
            rect(0.0, 0.0, width, 1.0),
            rect(0.0, 0.0, width, 1.0),
        );
    }
    let y = scroll_y.clamp(0.0, (total - height).max(0.0));
    let first = magazine_row_at(&tops, y - height);
    let end = (magazine_row_at(&tops, y + 2.0 * height) + 1).min(rows.len());
    let mut root = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: column; width: {width}px;"
    ));
    for row in &rows[first..end] {
        root.add_child(match row {
            MagazineRow::Day(group) => Dom::create_div()
                .with_class(ids::DAY_HEADER_CLASS)
                .with_css(MAGAZINE_DAY)
                .with_child(text(t(&group.message_id()))),
            MagazineRow::Cards(cards) => {
                let mut line = Dom::create_div().with_css(format!(
                    "display: flex; flex-direction: row; gap: {CARD_GAP_PX}px; height: {CARD_ROW_PX}px; \
                     box-sizing: border-box; padding: 0px {CARD_GAP_PX}px {CARD_GAP_PX}px {CARD_GAP_PX}px;"
                ));
                for (r, index) in cards {
                    line.add_child(card(s, table, *r, *index));
                }
                // A short last row: its cards as wide as a full row's.
                for _ in cards.len()..cards_per_row(width) {
                    line.add_child(Dom::create_div().with_css("flex-grow: 1; flex-basis: 0px;"));
                }
                line
            }
        });
    }
    VirtualViewReturn::with_dom(
        root,
        rect(0.0, tops[first], width, tops[end] - tops[first]),
        rect(0.0, 0.0, width, total),
    )
}

/// Whether `url` can stand in a CSS `url("...")` as it is.
fn css_safe(url: &str) -> bool {
    !url.is_empty() && !url.chars().any(|c| c == '"' || c == '\\' || c.is_whitespace() || c.is_control())
}

/// One article's card in the magazine: its picture (or its source's letter), its title, its
/// source and date, its opening lines; a click opens it, a double click its page.
fn card(s: &NewsApp, table: &TableData, r: ArticleRef, index: usize) -> Dom {
    let lib = &s.library;
    let (Some(feed), Some(item)) = (lib.feeds.get(r.feed), lib.article(r)) else {
        return Dom::create_div().with_css(CARD);
    };
    let selected = table.selected == Some(r);
    let unread = !lib.is_read(r);
    let title = title_of(item);
    let picture = item.image.trim();
    let top = if s.pictures.contains(picture) && css_safe(picture) {
        Dom::create_div().with_css(format!("{CARD_PICTURE} background-image: url(\"{picture}\");"))
    } else {
        let letter = feed
            .name()
            .chars()
            .find(|c| c.is_alphanumeric())
            .map_or_else(String::new, |c| c.to_uppercase().collect());
        Dom::create_div().with_css(CARD_PLACEHOLDER).with_child(text(letter))
    };
    let source_and_date = format!(
        "{}{} \u{b7} {}",
        if unread { "\u{25cf} " } else { "" },
        feed.name(),
        list_date(item.date(), table.today, table.offset)
    );
    let reference = || RowRef {
        app: table.app.clone(),
        feed: feed.sub.id.clone(),
        item: item.id.clone(),
    };
    Dom::create_div()
        .with_id(ids::article(index))
        .with_class(ids::MAGAZINE_CARD_CLASS)
        .with_css(format!("{CARD} {}", if selected { CARD_SELECTED } else { "" }))
        .with_accessibility_name(title.as_str())
        .with_child(top)
        .with_child(
            Dom::create_div()
                .with_css(CARD_BODY)
                .with_child(block(
                    &format!("{CARD_LINE} {CLIP}"),
                    text(source_and_date),
                ))
                .with_child(block(
                    &format!("{CARD_HEADLINE} {}", if unread { "" } else { "font-weight: 500;" }),
                    text(title),
                ))
                .with_child(block(CARD_EXCERPT, text(item.excerpt.clone()))),
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            RefAny::new(reference()),
            on_row,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::DoubleClick),
            RefAny::new(reference()),
            on_row_open,
        )
}

/// What an article's row carries: the app and the article (by ids: the row outlives a
/// refresh that renumbers the articles).
struct RowRef {
    app: RefAny,
    feed: String,
    item: String,
}

/// One article's card: the unread dot over the star; its title, its feed and date, its
/// opening lines.
fn article_row(s: &NewsApp, table: &TableData, r: ArticleRef, index: usize) -> Dom {
    let lib = &s.library;
    let (Some(feed), Some(item)) = (lib.feeds.get(r.feed), lib.article(r)) else {
        // The library changed under this layout's rows: an empty line until the rebuild.
        return Dom::create_div().with_css(ROW);
    };
    let selected = table.selected == Some(r);
    let unread = !lib.is_read(r);
    let title = title_of(item);
    let stripe = if selected { ROW_SELECTED } else { ROW_EVEN };
    let mark = |glyph: &str, css: &str| {
        Dom::create_div()
            .with_css(if selected { "" } else { css })
            .with_child(text(glyph))
    };
    let line = |content: String, extra: &str| block(&format!("{CLIP} {extra}"), text(content));
    let source_and_date = format!(
        "{} \u{b7} {}",
        feed.name(),
        list_date(item.date(), table.today, table.offset)
    );
    let secondary = if selected { "font-size: 11px;" } else { CARD_LINE };
    let reference = || RowRef {
        app: table.app.clone(),
        feed: feed.sub.id.clone(),
        item: item.id.clone(),
    };
    Dom::create_div()
        .with_id(ids::article(index))
        .with_class(ids::ARTICLE_ROW_CLASS)
        .with_css(format!("{ROW} {stripe}"))
        .with_accessibility_name(title.as_str())
        .with_child(
            Dom::create_div()
                .with_css(CARD_MARKS)
                .with_child(mark(if unread { "\u{25cf}" } else { "" }, DOT))
                .with_child(mark(if lib.is_starred(r) { "\u{2605}" } else { "" }, STAR)),
        )
        .with_child(
            Dom::create_div()
                .with_css(CARD_TEXT)
                .with_child(line(
                    title,
                    &format!("{CARD_TITLE} {}", if unread { "font-weight: 700;" } else { "" }),
                ))
                .with_child(line(source_and_date, secondary))
                .with_child(line(item.excerpt.clone(), secondary)),
        )
        // The release, as AzPdf's thumbnails in their VirtualView (a press and its release
        // land on the row's DOM, the VirtualView's own).
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            RefAny::new(reference()),
            on_row,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::DoubleClick),
            RefAny::new(reference()),
            on_row_open,
        )
}

/// The article a row names, in the library as it is now.
fn row_article(s: &NewsApp, feed: &str, item: &str) -> Option<ArticleRef> {
    let f = s.library.feed_index(feed)?;
    let i = s.library.feeds[f].items.iter().position(|x| x.id == item)?;
    Some(ArticleRef { feed: f, item: i })
}

fn row_parts(data: &mut RefAny) -> Option<(RefAny, String, String)> {
    data.downcast_ref::<RowRef>()
        .map(|r| (r.app.clone(), r.feed.clone(), r.item.clone()))
}

/// A click on a row: the article opens in the reading pane.
extern "C" fn on_row(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, feed, item)) = row_parts(&mut data) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, info, handle| {
        let Some(r) = row_article(s, &feed, &item) else {
            return;
        };
        leave_form(s, info, handle);
        select(s, info, handle, r);
    })
}

/// A double click on a row: the original opens in the browser (Mail opened the message in a
/// window of its own).
extern "C" fn on_row_open(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((mut app, feed, item)) = row_parts(&mut data) else {
        return Update::DoNothing;
    };
    let open = {
        let Some(s) = app.downcast_ref::<NewsApp>() else {
            return Update::DoNothing;
        };
        let r = row_article(&s, &feed, &item);
        r.is_some() && r == s.selected_ref()
    };
    if !open {
        return Update::DoNothing;
    }
    on_open_original(app, info)
}

/// Scrolls the table so that the open article's row is in view (after J / K).
pub(super) fn reveal_selected(s: &NewsApp, info: &mut CallbackInfo) {
    let Some(selected) = s.selected_ref() else {
        return;
    };
    let table_rows = s.table_rows();
    let dom = DomId { inner: 0 };
    let node = info.get_node_id_by_id_attribute(dom, ids::ARTICLE_LIST);
    // 0 is "no node"; a node's raw id is its index + 1.
    let Some(index) = node.into_raw().checked_sub(1) else {
        return;
    };
    let Some(view) = info.get_node_rect(DomNodeId { dom, node }).into_option() else {
        return;
    };
    let scroll_y = info
        .get_scroll_offset_for_node(dom, NodeId::create(index))
        .into_option()
        .map_or(0.0, |offset| offset.y);
    // Where the open article's row lies: a row of the list, or the row of cards holding it.
    let (top, bottom) = if s.settings.magazine {
        let rows = magazine_rows(&table_rows, cards_per_row(view.size.width));
        let Some(position) = rows.iter().position(|row| {
            matches!(row, MagazineRow::Cards(cards) if cards.iter().any(|(r, _)| *r == selected))
        }) else {
            return;
        };
        let tops = magazine_tops(&rows);
        (tops[position], tops[position + 1])
    } else {
        let Some(position) = table_rows
            .iter()
            .position(|row| matches!(row, TableRow::Article(r, _) if *r == selected))
        else {
            return;
        };
        let top = position as f32 * ROW_PX;
        (top, top + ROW_PX)
    };
    let height = view.size.height.max(ROW_PX);
    let y = if top < scroll_y {
        top
    } else if bottom > scroll_y + height {
        bottom - height
    } else {
        return;
    };
    info.scroll_to(dom, node, LogicalPosition::create(0.0, y.max(0.0)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{feed::Item, library::FeedData, opml::Subscription};

    /// Thursday, 1 October 2026, 12:00 UTC.
    const NOW: i64 = 1_790_856_000;
    const DAY: i64 = 86_400;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 1).expect("a date")
    }

    #[test]
    fn the_date_column_reads_like_mails() {
        crate::l10n::in_english();
        assert_eq!(list_date(NOW - 3_600, today(), 0), "Today 11:00");
        assert_eq!(list_date(NOW - DAY, today(), 0), "Yesterday 12:00");
        assert_eq!(
            list_date(NOW - DAY, today(), 2 * 3_600),
            "Yesterday 14:00",
            "in local time"
        );
        assert_eq!(list_date(NOW - 3 * DAY, today(), 0), "Monday 12:00");
        assert_eq!(list_date(NOW - 20 * DAY, today(), 0), "2026-09-11");
    }

    fn library() -> Library {
        let item = |id: &str, date: i64| Item {
            id: id.to_string(),
            title: id.to_string(),
            published: Some(date),
            ..Item::default()
        };
        Library {
            feeds: vec![FeedData {
                sub: Subscription {
                    id: "f".into(),
                    title: "Feed".into(),
                    url: "https://f.example.org/feed".into(),
                    ..Subscription::default()
                },
                items: vec![
                    item("a", NOW - 60),
                    item("b", NOW - 120),
                    item("c", NOW - DAY),
                ],
                ..FeedData::default()
            }],
        }
    }

    #[test]
    fn sorted_by_date_the_rows_sit_under_their_days() {
        let lib = library();
        let order = lib.list(&crate::library::View::All, "");
        let r = |item| ArticleRef { feed: 0, item };
        assert_eq!(
            rows(&lib, &order, Sort::default(), today(), 0),
            vec![
                TableRow::Day(DateGroup::Today),
                TableRow::Article(r(0), 0),
                TableRow::Article(r(1), 1),
                TableRow::Day(DateGroup::Yesterday),
                TableRow::Article(r(2), 2),
            ]
        );
        let by_title = Sort::default().clicked(SortKey::Title);
        assert_eq!(
            rows(&lib, &order, by_title, today(), 0),
            vec![
                TableRow::Article(r(0), 0),
                TableRow::Article(r(1), 1),
                TableRow::Article(r(2), 2),
            ],
            "no day headers for another column"
        );
    }

    fn article(i: usize) -> TableRow {
        TableRow::Article(ArticleRef { feed: 0, item: i }, i)
    }

    #[test]
    fn the_magazine_packs_a_days_articles_into_rows_of_cards() {
        let rows = vec![
            TableRow::Day(DateGroup::Today),
            article(0),
            article(1),
            article(2),
            TableRow::Day(DateGroup::Yesterday),
            article(3),
        ];
        let magazine = magazine_rows(&rows, 2);
        assert_eq!(
            magazine,
            vec![
                MagazineRow::Day(DateGroup::Today),
                MagazineRow::Cards(vec![(ArticleRef { feed: 0, item: 0 }, 0), (ArticleRef { feed: 0, item: 1 }, 1)]),
                MagazineRow::Cards(vec![(ArticleRef { feed: 0, item: 2 }, 2)]),
                MagazineRow::Day(DateGroup::Yesterday),
                MagazineRow::Cards(vec![(ArticleRef { feed: 0, item: 3 }, 3)]),
            ],
            "a day's last row is not filled with the next day's articles"
        );
        // Not sorted by date: no days, the articles one row after the other.
        let flat: Vec<TableRow> = (0..5).map(article).collect();
        assert_eq!(magazine_rows(&flat, 3).len(), 2);
        assert_eq!(magazine_rows(&flat, 0).len(), 5, "at least one card a row");
    }

    #[test]
    fn the_magazines_rows_lie_one_under_the_other_by_their_heights() {
        let rows = vec![
            MagazineRow::Day(DateGroup::Today),
            MagazineRow::Cards(vec![(ArticleRef { feed: 0, item: 0 }, 0)]),
            MagazineRow::Cards(vec![(ArticleRef { feed: 0, item: 1 }, 1)]),
        ];
        let tops = magazine_tops(&rows);
        assert_eq!(tops, vec![0.0, DAY_PX, DAY_PX + CARD_ROW_PX, DAY_PX + 2.0 * CARD_ROW_PX]);
        assert_eq!(magazine_row_at(&tops, 0.0), 0);
        assert_eq!(magazine_row_at(&tops, DAY_PX + 1.0), 1);
        assert_eq!(magazine_row_at(&tops, 1e9), 2, "past the end: the last row");
        assert_eq!(cards_per_row(100.0), 1);
        assert_eq!(cards_per_row(3.0 * CARD_MIN_PX + 2.0 * CARD_GAP_PX + 2.0 * CARD_GAP_PX), 3);
    }
}
