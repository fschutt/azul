//! AzDrive's body in each app theme and mode - the source list, the content leaf, the Details
//! list, the icon cells - one inline style per part.
//!
//! FLORA (the website's look, `doc/templates/flora.css`): the content is a LEAF of field paper
//! (`--fl-fld`) filling its pane, over the linen page (`--fl-pg` and its two hairline rasters,
//! `--fl-grain`); the source list rests on a soft vertical gradient from the strip tone down to
//! the desk (`--fl-strip` .. `--fl-desk`);
//! its section titles are flora's capitals (Calmet UI, bold, tracked, `--fl-soft1`);
//! a selected source is the accent stone (`--az-accent`) under paper ink, with a lip and a cast;
//! the Details header is the raised face (`--fl-rT` -> `--fl-rB`), its rows alternate field paper
//! with a quiet wash, a selected item rests on the accent's soft wash (flora's data table and
//! icon grid), and a selected icon cell or tile lifts off the paper.
//!
//! FLAT (Office 2010): the same structure in Office's silver-blue - a white leaf over the silver
//! page, the source list on Outlook's navigation-pane gradient, Office's
//! two-stop blue selection in its hairline, the list header's white-to-silver face.
//!
//! Each part carries all four looks in ONE string ([`themed!`]): flat by day, flat at night
//! (`@media (prefers-color-scheme: dark)`), flora by day and flora at night (`@theme(flora)`).
//! How the engine resolves such a string (`Dom::with_css`: a stylesheet scoped to the node,
//! `core::prop_cache`): a pseudo-state TIER beats the resting one whatever the rank (`:focus` >
//! `:drag-over` > `:hover` > at rest); within a tier the lowest theme rank wins (a
//! `@theme(flora)` declaration over an unthemed one), then the higher specificity, then the
//! later one. And a `:hover { .. }` block is scoped to the node's whole SUBTREE: it also styles
//! every hovered element inside it. Hence the rules this module keeps:
//!
//! - a part's states (selected, focused, a drop over it) are appended AFTER its resting look;
//! - a state that paints a background restates it under `:hover` (solid: the block reaches the
//!   hovered children, where a gradient would restart on each child's box), or the hover wash
//!   would replace it under the pointer (a selected row, a pill);
//! - a hover only ever paints OPAQUE backgrounds (a translucent one would darken twice over a
//!   hovered child) and never a shadow (it would cast one around every hovered child): a cell
//!   lifts when it is SELECTED, a resting state that stays on the node alone.

/// One part's look: `base` (every theme), then `flat` by day, `flat_dark`, `flora` by day and
/// `flora_dark` - runs of declarations; state blocks (`:hover { .. }`) may nest in each.
macro_rules! themed {
    ($base:literal, $flat:literal, $flat_dark:literal, $flora:literal, $flora_dark:literal $(,)?) => {
        concat!(
            $base,
            " ",
            $flat,
            " @media (prefers-color-scheme: dark) { ",
            $flat_dark,
            " } @theme(flora) { ",
            $flora,
            " @media (prefers-color-scheme: dark) { ",
            $flora_dark,
            " } }"
        )
    };
}

/// One line of text, cut with an ellipsis.
pub(crate) const CLIP: &str =
    "flex-grow: 1; min-width: 0px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis;";

// ==== The source list ====

/// The source list's column: a soft vertical gradient - Outlook's navigation pane in flat, the
/// strip tone falling to the desk in flora.
pub(crate) const SIDEBAR: &str = themed!(
    "display: flex; flex-direction: column; flex-grow: 1; width: 100%; height: 100%; \
     min-width: 0px; min-height: 0px; overflow: hidden; font-size: 13px;",
    "background: linear-gradient(to bottom, #EAEFF5, #D6DEE8); color: #1E1E1E;",
    "background: linear-gradient(to bottom, #303338, #25272B); color: #F2F2F2;",
    "background: linear-gradient(to bottom, #EEECE7, #DCD9D2); color: #262521; \
     font-size: 15px;",
    "background: linear-gradient(to bottom, #282828, #1B1B1B); color: #E7E7E7;",
);

/// The rows' scrolling box.
pub(crate) const SIDEBAR_LIST: &str =
    "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow-y: auto; \
     padding: 0px 0px 10px 0px;";

/// A section's title: small capitals behind a disclosure triangle (flora's caps: Calmet UI,
/// 10.5 px, bold, tracked out).
pub(crate) const SECTION_HEAD: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; height: 20px; \
     margin: 10px 6px 3px 6px; padding: 0px 6px 0px 2px; border-radius: 4px; font-size: 11px; \
     font-weight: 700; text-transform: uppercase; cursor: pointer;",
    "color: #6A7788; :hover { color: #27364A; }",
    "color: #9AA6B5; :hover { color: #E3E8EE; }",
    "color: #66645C; font-family: \"Calmet UI\", \"EB Garamond\", \"Hoefler Text\", \
     Georgia, serif; font-size: 10.5px; letter-spacing: 0.08em; :hover { color: #262521; }",
    "color: #A8A8A8; :hover { color: #E7E7E7; }",
);

/// A section's title text: the capitals stated on the text too (whether the engine inherits
/// `text-transform` or not).
pub(crate) const SECTION_TITLE: &str = "flex-grow: 1; min-width: 0px; overflow: hidden; \
                                        white-space: nowrap; text-overflow: ellipsis; \
                                        text-transform: uppercase;";

/// A section's chevron (and a drive's or a folder's, which opens its folders).
pub(crate) const TRIANGLE: &str =
    "font-size: 14px; width: 16px; flex-shrink: 0; margin-right: 1px; opacity: 0.7;";

/// A row: a place, a drive, a folder, "Add S3 drive".
pub(crate) const ROW: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; height: 24px; \
     margin: 0px 6px 1px 6px; padding-right: 4px; border-radius: 5px; cursor: default;",
    ":hover { background: #F2F5F9; }",
    ":hover { background: #3A3D43; }",
    ":hover { background: #F6F5F1; }",
    ":hover { background: #323232; }",
);

/// The row of the place the window shows: Finder's rounded highlight - Office's two-stop blue
/// in its hairline (flat), the accent stone under paper ink with a lip and a cast (flora).
pub(crate) const ROW_SELECTED: &str = themed!(
    "font-weight: 600;",
    "color: #1E1E1E; background: linear-gradient(to bottom, #DDEBFD, #C1DCFC); \
     box-shadow: inset 0px 0px 0px 1px #84ACDD; \
     :hover { background: #CFE3FC; }",
    "color: #FFFFFF; background: linear-gradient(to bottom, #37577F, #2E4A6E); \
     box-shadow: inset 0px 0px 0px 1px #4A78B0; :hover { background: #335277; }",
    "color: var(--az-on-accent, #F4F2EA); background: var(--az-accent, #2F4A85); \
     box-shadow: inset 0px 1px 0px rgba(255, 255, 255, 0.16), 0px 1px 2px rgba(48, 45, 38, 0.30); \
     :hover { background: var(--az-accent, #2F4A85); }",
    "box-shadow: inset 0px 1px 0px rgba(255, 255, 255, 0.10), 0px 1px 3px rgba(0, 0, 0, 0.60);",
);

/// The ring of the row the keyboard is on (after the selection, so it shows on it too).
pub(crate) const ROW_FOCUS: &str = themed!(
    "",
    ":focus-visible { box-shadow: inset 0px 0px 0px 2px #4286F4; }",
    ":focus-visible { box-shadow: inset 0px 0px 0px 2px #4683D6; }",
    ":focus-visible { box-shadow: inset 0px 0px 0px 2px #7A93C6; }",
    "",
);

/// A row that takes dropped items, while they are dragged over it.
pub(crate) const ROW_DROP: &str = themed!(
    "",
    ":drag-over { background: #C1DCFC; box-shadow: inset 0px 0px 0px 2px #2A63B8; }",
    ":drag-over { background: #2E4A6E; box-shadow: inset 0px 0px 0px 2px #4683D6; }",
    ":drag-over { background: #E0E4EE; box-shadow: inset 0px 0px 0px 2px #2F4A85; }",
    ":drag-over { background: #1E3260; box-shadow: inset 0px 0px 0px 2px #7A93C6; }",
);

/// A row's icon (a selected row's takes the row's ink).
pub(crate) const ROW_ICON: &str =
    "font-size: 16px; width: 18px; flex-shrink: 0; margin-right: 6px;";

/// The tint of a monochrome icon at rest: Office's icon blue, flora's small-icon ink
/// (`system:icon`: `--fl-icon`, #56544C by day, #BEBEBE at night). A multicolour Haiku icon
/// keeps its own colours under any tint.
pub(crate) const ICON_TINT: &str = themed!(
    "",
    "color: #3D6AA8;",
    "color: #7AA7E0;",
    "color: system:icon;",
    "color: system:icon;",
);

/// The room a row without a triangle keeps, so every icon of a level lines up.
pub(crate) const TRIANGLE_ROOM: &str = "width: 16px; flex-shrink: 0; margin-right: 1px;";

/// A count (transfers on a cloud drive): a pill.
pub(crate) const PILL: &str = themed!(
    "flex-shrink: 0; margin-left: 4px; padding: 0px 6px; border-radius: 8px; font-size: 10px; \
     font-weight: 700; line-height: 15px;",
    "color: #FFFFFF; background: #8A9BB3; :hover { background: #8A9BB3; }",
    "background: #56657A; :hover { background: #56657A; }",
    "color: #FBFAF6; background: #9C9890; :hover { background: #9C9890; }",
    "color: #E7E7E7; background: #4A4A4A; :hover { background: #4A4A4A; }",
);

/// The pill on a selected row: inverted.
pub(crate) const PILL_SELECTED: &str = themed!(
    "",
    "color: #2A63B8; background: #FFFFFF; :hover { background: #FFFFFF; }",
    "color: #2E4A6E; background: #DDEBFD; :hover { background: #DDEBFD; }",
    "color: var(--az-accent, #2F4A85); background: #F4F2EA; :hover { background: #F4F2EA; }",
    "",
);

/// A cloud drive's state glyph (syncing, locked, connected).
pub(crate) const STATE: &str = "font-size: 13px; flex-shrink: 0; margin-left: 4px; opacity: 0.8;";

/// A drive's eject button (Finder's): forgets a drive the user added.
pub(crate) const EJECT: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; justify-content: center; \
     width: 20px; height: 18px; flex-shrink: 0; margin-left: 2px; border-radius: 4px; \
     font-size: 14px; cursor: pointer;",
    "color: #626A76; :hover { color: #1E1E1E; background: #D5DCE4; }",
    "color: #9E9E9E; :hover { color: #F2F2F2; background: #4A4D53; }",
    "color: #66645C; :hover { color: #262521; background: #DDD9D1; }",
    "color: #A8A8A8; :hover { color: #E7E7E7; background: #3D3D3D; }",
);

/// The eject button on a selected row: in the row's ink.
pub(crate) const EJECT_SELECTED: &str = themed!(
    "",
    "",
    "color: #FFFFFF;",
    "color: var(--az-on-accent, #F4F2EA); :hover { color: #FFFFFF; background: #4A64A0; }",
    "",
);

/// The activity area over the buttons: the transfer that runs, those that wait or failed.
pub(crate) const ACTIVITY: &str = themed!(
    "display: flex; flex-direction: column; flex-shrink: 0; padding: 5px 0px 7px 0px; \
     font-size: 11px; cursor: pointer;",
    "border-top: 1px solid #C2C9D2;",
    "border-top: 1px solid #575757;",
    "border-top: 1px solid #C6C3BB;",
    "border-top: 1px solid #3F3F3F;",
);

/// The cash orders' area title (under the source list).
pub(crate) const ACTIVITY_HEAD: &str = themed!(
    "padding: 0px 10px 3px 10px; font-size: 10px; font-weight: 700; text-transform: uppercase;",
    "color: #6A7788;",
    "color: #9AA6B5;",
    "color: #66645C; font-family: \"Calmet UI\", \"EB Garamond\", \"Hoefler Text\", \
     Georgia, serif; letter-spacing: 0.08em;",
    "color: #A8A8A8;",
);

/// A line of the activity area.
pub(crate) const ACTIVITY_LINE: &str =
    "padding: 1px 10px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis;";

/// A failed transfer's line.
pub(crate) const ACTIVITY_ERROR: &str = themed!(
    "padding: 1px 10px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis;",
    "color: #B3261E;",
    "color: #F1707B;",
    "color: #A33A2E;",
    "color: #E8847A;",
);

/// The bar under the list: + and the actions.
pub(crate) const SIDEBAR_FOOT: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; height: 26px;",
    "border-top: 1px solid #C2C9D2; background: linear-gradient(to bottom, #F7F9FB, #E4E9EF);",
    "border-top: 1px solid #575757; background: linear-gradient(to bottom, #3C3C3C, #313131);",
    "border-top: 1px solid #C6C3BB; background: linear-gradient(to bottom, #FAF9F5, #ECEAE4);",
    "border-top: 1px solid #3F3F3F; background: linear-gradient(to bottom, #333333, #292929);",
);

/// One of the small buttons in that bar.
pub(crate) const SMALL_BUTTON: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; justify-content: center; \
     width: 32px; height: 26px; font-size: 15px; cursor: pointer;",
    "border-right: 1px solid #D3D9E0; color: #3D5574; :hover { background: #DDE4EC; } \
     :focus-visible { box-shadow: inset 0px 0px 0px 2px #4286F4; }",
    "border-right: 1px solid #4D4D4D; color: #DADADA; :hover { background: #474747; } \
     :focus-visible { box-shadow: inset 0px 0px 0px 2px #4683D6; }",
    "border-right: 1px solid #D8D5CE; color: #56544C; :hover { background: #E3E0D9; } \
     :focus-visible { box-shadow: inset 0px 0px 0px 2px #7A93C6; }",
    "border-right: 1px solid #383838; color: #BEBEBE; :hover { background: #3A3A3A; }",
);

/// The two pies at the right of the foot.
pub(crate) const PIES: &str =
    "display: flex; flex-direction: row; align-items: center; margin-left: auto; \
     padding-right: 4px; height: 26px;";

/// The sync popover's content: 330 px of field paper.
pub(crate) const SYNC_POPOVER: &str = themed!(
    "display: flex; flex-direction: column; width: 330px; box-sizing: border-box; \
     padding: 12px 14px;",
    "color: #1E1E1E;",
    "color: #F2F2F2;",
    "color: #262521;",
    "color: #E7E7E7;",
);

/// The popover's first line: the sync's state.
pub(crate) const POPOVER_HEAD: &str = "font-weight: 600; margin-bottom: 4px;";

/// A sentence of the popover.
pub(crate) const POPOVER_TEXT: &str = "margin: 2px 0px;";

/// A small line of the popover (the transfers, this computer's health).
pub(crate) const POPOVER_SMALL: &str = themed!(
    "font-size: 12px; margin: 4px 0px 2px 0px;",
    "color: #626A76;",
    "color: #9E9E9E;",
    "color: #66645C;",
    "color: #A8A8A8;",
);

/// The drive's usage line.
pub(crate) const POPOVER_DRIVE: &str = "margin-top: 10px;";

/// A warning of the popover (near full, failed transfers).
pub(crate) const POPOVER_WARNING: &str = themed!(
    "font-size: 12.5px; margin: 4px 0px;",
    "color: #B04032;",
    "color: #E08070;",
    "color: #B04032;",
    "color: #E08070;",
);

/// The popover's buttons, at its foot on the right.
pub(crate) const POPOVER_ACTIONS: &str =
    "display: flex; flex-direction: row; justify-content: flex-end; margin-top: 12px;";

// ==== The content: a leaf filling its pane ====

/// What a pane's leaf lies on (the content's, the right pane's): Office's silver in flat, the
/// linen in flora - the page colour and flora's two hairline rasters over it (a renderer that
/// cannot draw the rasters keeps the colour, the declaration before). The leaf fills it edge to
/// edge, so it shows only where the leaf leaves room.
pub(crate) const PAGE: &str = themed!(
    "display: flex; flex-direction: column; flex-grow: 1; width: 100%; height: 100%; \
     min-width: 0px; min-height: 0px; box-sizing: border-box;",
    "background: #DCE2E9;",
    "background: #191919;",
    "background: #E6E4DF; background: #E6E4DF, \
     repeating-linear-gradient(0deg, rgba(90, 86, 74, 0.031) 0px, rgba(90, 86, 74, 0.031) 1px, \
     rgba(90, 86, 74, 0) 1px, rgba(90, 86, 74, 0) 3px), \
     repeating-linear-gradient(90deg, rgba(90, 86, 74, 0.022) 0px, rgba(90, 86, 74, 0.022) 1px, \
     rgba(90, 86, 74, 0) 1px, rgba(90, 86, 74, 0) 3px);",
    "background: #151515; background: #151515, \
     repeating-linear-gradient(0deg, rgba(0, 0, 0, 0.20) 0px, rgba(0, 0, 0, 0.20) 1px, \
     rgba(0, 0, 0, 0) 1px, rgba(0, 0, 0, 0) 3px), \
     repeating-linear-gradient(90deg, rgba(0, 0, 0, 0.14) 0px, rgba(0, 0, 0, 0.14) 1px, \
     rgba(0, 0, 0, 0) 1px, rgba(0, 0, 0, 0) 3px);",
);

/// A pane's leaf: the paper the view (or the preview, the details) is drawn on. The splitters
/// between the panes are its edges.
pub(crate) const LEAF: &str = themed!(
    "display: flex; flex-direction: column; flex-grow: 1; min-width: 0px; min-height: 0px; \
     overflow: hidden;",
    "background: #FFFFFF;",
    "background: #262626;",
    "background: #FBFAF6;",
    "background: #1D1D1D;",
);

/// The details pane (azul's DetailsPane) inside its leaf: it fills the leaf. (A widget's own
/// declarations are its node's inline style, which outranks a `with_css` sheet: only what the
/// widget leaves unset can be added.)
pub(crate) const DETAILS_FILL: &str = "flex-grow: 1;";

// ==== The Details list ====

/// The column headers: a raised face in a hairline.
pub(crate) const DETAILS_HEADER: &str = themed!(
    "display: flex; flex-direction: row; flex-shrink: 0; height: 24px; font-size: 12px;",
    "background: linear-gradient(to bottom, #FFFFFF, #EDF1F5); border-bottom: 1px solid #C2C9D2; \
     color: #4C607A;",
    "background: linear-gradient(to bottom, #474747, #3A3A3A); border-bottom: 1px solid #1E1E1E; \
     color: #D0D0D0;",
    "background: linear-gradient(to bottom, #FAF9F5, #ECEAE4); border-bottom: 1px solid #C6C3BB; \
     color: #4E4C45;",
    "background: linear-gradient(to bottom, #333333, #292929); border-bottom: 1px solid #101010; \
     color: #BCBCBC;",
);

/// A column header (a click sorts by it).
pub(crate) const COLUMN: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; \
     padding: 0px 0px 0px 8px; cursor: default;",
    ":hover { background: #E3F1FB; }",
    ":hover { background: #4C4C4C; }",
    ":hover { background: #F7F5F0; }",
    ":hover { background: #3A3A3A; }",
);

/// The sorted column's header: tinted, its label in the accent.
pub(crate) const COLUMN_SORTED: &str = themed!(
    "font-weight: 600;",
    "background: linear-gradient(to bottom, #F2F8FE, #DCEBFB); color: #1E3287;",
    "background: linear-gradient(to bottom, #3B4F68, #324459); color: #FFFFFF;",
    "background: linear-gradient(to bottom, #EEF0F5, #DFE3EC); color: var(--az-accent, #2F4A85);",
    "background: linear-gradient(to bottom, #2C3A55, #24304A); color: #DCE3F2;",
);

/// A column's draggable edge: a short separator.
pub(crate) const COLUMN_EDGE: &str = themed!(
    "width: 6px; align-self: stretch; flex-shrink: 0; margin: 4px 0px 4px 0px; \
     cursor: col-resize;",
    "border-right: 1px solid #D5DBE3;",
    "border-right: 1px solid #555555;",
    "border-right: 1px solid #D2CFC8;",
    "border-right: 1px solid #3A3A3A;",
);

/// A Details row on an odd line: a quiet stripe over the leaf's paper.
pub(crate) const STRIPE: &str = themed!(
    "",
    "background: #F5F8FB;",
    "background: #2C2C2C;",
    "background: #F4F2ED;",
    "background: #232323;",
);

/// The text a search matched in a result's line (the Match column): a marker's wash.
pub(crate) const FIND_MARK: &str = themed!(
    "border-radius: 2px;",
    "background: #FFE48A; color: #1F2A36;",
    "background: #6B5A1E; color: #FFF6D8;",
    "background: #F3E2A6; color: #3A3426;",
    "background: #5E5222; color: #F4ECD2;",
);

/// The note over a cloud drive's search results (searched by name, slower).
pub(crate) const FIND_NOTE: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; \
     padding: 4px 8px; font-size: 12px;",
    "background: #FFF8E1; border-bottom: 1px solid #E6D9A8; color: #5C4B12;",
    "background: #3A3423; border-bottom: 1px solid #1E1E1E; color: #E8DDB5;",
    "background: #F6F0DD; border-bottom: 1px solid #DCD3B8; color: #5A4E2C;",
    "background: #302B1E; border-bottom: 1px solid #101010; color: #DCD2B0;",
);

/// The search's bar over its view: the chips, the count, Save search.
pub(crate) const FIND_BAR: &str = themed!(
    "display: flex; flex-direction: row; flex-wrap: wrap; align-items: center; flex-shrink: 0; \
     padding: 7px 10px; column-gap: 6px; row-gap: 6px;",
    "background: #F3F6F9; border-bottom: 1px solid #D3D9E0;",
    "background: #2B2B2B; border-bottom: 1px solid #1E1E1E;",
    "background: #EFEDE7; border-bottom: 1px solid #D8D5CE;",
    "background: #222222; border-bottom: 1px solid #383838;",
);

/// The rule between the scope chips and the refine chips.
pub(crate) const FIND_BAR_SEPARATOR: &str = themed!(
    "width: 1px; height: 18px; margin: 0px 4px; flex-shrink: 0;",
    "background: #C2C9D2;",
    "background: #4D4D4D;",
    "background: #C6C3BB;",
    "background: #3F3F3F;",
);

/// A chip: a small rounded button of the search's bar and its empty states.
pub(crate) const CHIP: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; height: 24px; \
     box-sizing: border-box; padding: 0px 10px; border-radius: 12px; font-size: 12.5px; \
     white-space: nowrap; cursor: default;",
    "border: 1px solid #C2C9D2; background: linear-gradient(to bottom, #FFFFFF, #EDF1F5); \
     color: #3D4A5C; :hover { border-color: #8E9BAD; color: #1E1E1E; }",
    "border: 1px solid #4D4D4D; background: linear-gradient(to bottom, #3C3C3C, #313131); \
     color: #D0D0D0; :hover { border-color: #6A6A6A; color: #F2F2F2; }",
    "border: 1px solid #C6C3BB; background: linear-gradient(to bottom, #FAF9F5, #ECEAE4); \
     color: #4E4C45; :hover { border-color: #9C9890; color: #262521; }",
    "border: 1px solid #3F3F3F; background: linear-gradient(to bottom, #333333, #292929); \
     color: #BCBCBC; :hover { border-color: #616161; color: #E7E7E7; }",
);

/// A chosen scope chip: the accent.
pub(crate) const CHIP_ON: &str = themed!(
    "",
    "background: #2B579A; border-color: #1E3F73; color: #FFFFFF; \
     :hover { background: #2B579A; color: #FFFFFF; }",
    "background: #2B579A; border-color: #1E3F73; color: #FFFFFF; \
     :hover { background: #2B579A; color: #FFFFFF; }",
    "background: #2F4A85; border-color: #1E3260; color: #F4F2EA; \
     box-shadow: inset 0px 1px 0px rgba(255, 255, 255, 0.16); \
     :hover { background: #2F4A85; color: #F4F2EA; }",
    "background: #2F4A85; border-color: #1E3260; color: #F4F2EA; \
     :hover { background: #2F4A85; color: #F4F2EA; }",
);

/// A filter chip that narrows the search: outlined in the accent.
pub(crate) const CHIP_SET: &str = themed!(
    "",
    "border-color: #2B579A; color: #2B579A; background: #E3ECF7; \
     :hover { background: #E3ECF7; }",
    "border-color: #4683D6; color: #DCE6F2; background: #2E3E55; \
     :hover { background: #2E3E55; }",
    "border-color: #2F4A85; color: #2F4A85; background: #E0E4EE; \
     :hover { background: #E0E4EE; }",
    "border-color: #4A65A0; color: #DCE3F2; background: #24304A; \
     :hover { background: #24304A; }",
);

/// A row of chips under an empty state, centred.
pub(crate) const CHIP_ROW: &str =
    "display: flex; flex-direction: row; flex-wrap: wrap; justify-content: center; \
     column-gap: 8px; row-gap: 8px; margin-top: 10px;";

/// The search bar's count.
pub(crate) const FIND_STATUS: &str = themed!(
    "font-size: 12.5px; font-variant-numeric: tabular-nums; margin-right: 6px;",
    "color: #626A76;",
    "color: #9E9E9E;",
    "color: #66645C;",
    "color: #A8A8A8;",
);

/// A quiet text button (Save search): its words in the accent, a face only under the pointer.
pub(crate) const QUIET_BUTTON: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; height: 24px; padding: 0px 8px; \
     border-radius: 3px; font-size: 12.5px; cursor: default;",
    "color: #2B579A; :hover { background: #DDE4EC; }",
    "color: #7AA7E0; :hover { background: #3A3A3A; }",
    "color: #2F4A85; :hover { background: #E4E1DA; }",
    "color: #7A93C6; :hover { background: #333333; }",
);

/// An empty state of the search view: centred, its icon, its title, its detail, its chips.
pub(crate) const FIND_EMPTY_STATE: &str = themed!(
    "display: flex; flex-direction: column; align-items: center; justify-content: center; \
     flex-grow: 1; padding: 40px; text-align: center;",
    "color: #626A76;",
    "color: #9E9E9E;",
    "color: #66645C;",
    "color: #A8A8A8;",
);

/// The empty state's big icon.
pub(crate) const FIND_EMPTY_ICON: &str = themed!(
    "font-size: 46px; margin-bottom: 6px;",
    "color: #B4BDC9;",
    "color: #5A5A5A;",
    "color: #B4B1A9;",
    "color: #4A4A4A;",
);

/// The empty state's title.
pub(crate) const FIND_EMPTY_TITLE: &str = themed!(
    "font-size: 18px; margin-bottom: 6px;",
    "color: #1E1E1E;",
    "color: #F2F2F2;",
    "color: #262521;",
    "color: #E7E7E7;",
);

/// The empty state's sentence under its title.
pub(crate) const FIND_EMPTY_DETAIL: &str = "font-size: 13.5px; max-width: 420px;";

/// The header of a folder's results: the folder's path in the accent, a rule to the edge.
pub(crate) const FIND_GROUP: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; box-sizing: border-box; \
     padding: 0px 10px; flex-shrink: 0; font-size: 13.5px; position: sticky; top: 0px;",
    "color: #2B579A; background: #FFFFFF; border-bottom: 1px solid #E2E6EB;",
    "color: #7AA7E0; background: #1E1E1E; border-bottom: 1px solid #333333;",
    "color: #2F4A85; background: #FBFAF6; border-bottom: 1px solid #E0DDD7;",
    "color: #7A93C6; background: #1D1D1D; border-bottom: 1px solid #2E2E2E;",
);

/// "· 2 items" after a results header's path.
pub(crate) const FIND_GROUP_COUNT: &str = themed!(
    "font-size: 12px; margin: 0px 8px 0px 4px; font-variant-numeric: tabular-nums;",
    "color: #7A8594;",
    "color: #8C8C8C;",
    "color: #827F76;",
    "color: #8C8C8C;",
);

/// The rule after a results header's count.
pub(crate) const FIND_GROUP_RULE: &str = themed!(
    "flex-grow: 1; height: 1px;",
    "background: #E2E6EB;",
    "background: #333333;",
    "background: #D8D5CE;",
    "background: #383838;",
);

/// A result's two lines of the text its contents matched on.
pub(crate) const SNIPPET: &str = themed!(
    "display: block; display: -webkit-box; -webkit-box-orient: vertical; \
     -webkit-line-clamp: 2; line-clamp: 2; overflow: hidden; white-space: normal; \
     max-height: 33px; font-size: 12.5px; line-height: 1.3; padding-right: 8px;",
    "color: #626A76;",
    "color: #9E9E9E;",
    "color: #66645C;",
    "color: #A8A8A8;",
);

/// What a result found by its name is, in the snippet's place.
pub(crate) const SNIPPET_KIND: &str = themed!(
    "font-size: 12px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis;",
    "color: #7A8594;",
    "color: #8C8C8C;",
    "color: #827F76;",
    "color: #8C8C8C;",
);

// ==== The preview pane ====

/// The preview's stage: the picture, the page, the text - on a soft gradient over the leaf.
pub(crate) const PREVIEW_STAGE: &str = themed!(
    "display: flex; flex-direction: column; align-items: center; justify-content: center; \
     flex-shrink: 0; height: 236px; padding: 16px; box-sizing: border-box; overflow: hidden;",
    "border-bottom: 1px solid #E2E6EB;",
    "border-bottom: 1px solid #333333;",
    "background: linear-gradient(to bottom, #EFEDE7, #FBFAF6); border-bottom: 1px solid #E0DDD7;",
    "background: linear-gradient(to bottom, #242424, #1D1D1D); border-bottom: 1px solid #2E2E2E;",
);

/// The item's facts under the preview.
pub(crate) const PREVIEW_INFO: &str =
    "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; padding: 14px 16px;";

/// The item's name: 17 px, semibold.
pub(crate) const PREVIEW_NAME: &str =
    "font-size: 17px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; \
     white-space: nowrap;";

/// The item's kind under its name.
pub(crate) const PREVIEW_KIND: &str = themed!(
    "font-size: 12.5px; margin-bottom: 10px;",
    "color: #626A76;",
    "color: #9E9E9E;",
    "color: #66645C;",
    "color: #A8A8A8;",
);

/// A fact: its name on the left, its value.
pub(crate) const PREVIEW_ROW: &str =
    "display: flex; flex-direction: row; padding: 2px 0px; font-size: 12.5px;";

pub(crate) const PREVIEW_ROW_KEY: &str = themed!(
    "width: 84px; flex-shrink: 0;",
    "color: #7A8594;",
    "color: #8C8C8C;",
    "color: #827F76;",
    "color: #8C8C8C;",
);

pub(crate) const PREVIEW_ROW_VALUE: &str =
    "flex-grow: 1; min-width: 0px; overflow: hidden; text-overflow: ellipsis; \
     white-space: nowrap; font-variant-numeric: tabular-nums;";

/// Open and Copy link at the foot.
pub(crate) const PREVIEW_ACTIONS: &str =
    "display: flex; flex-direction: row; justify-content: flex-start; column-gap: 6px; \
     margin-top: auto; padding-top: 12px;";

// ==== Over the window: Quick Look, the command palette ====

/// The grey scrim over the window, what lies on it centred.
pub(crate) const SCRIM: &str = themed!(
    "position: absolute; top: 0px; left: 0px; width: 100%; height: 100%; z-index: 50; \
     display: flex; flex-direction: column; align-items: center; justify-content: center;",
    "background: rgba(40, 38, 34, 0.32);",
    "background: rgba(0, 0, 0, 0.5);",
    "background: rgba(40, 38, 34, 0.32);",
    "background: rgba(0, 0, 0, 0.5);",
);

/// The palette's scrim holds it near the top of the window.
pub(crate) const SCRIM_TOP: &str = "justify-content: flex-start; padding-top: 80px; \
                                    box-sizing: border-box;";

/// Quick Look's panel: most of the window, the preview filling it.
pub(crate) const QUICK_LOOK: &str = themed!(
    "display: flex; flex-direction: column; width: 72%; height: 80%; border-radius: 8px; \
     overflow: hidden;",
    "background: #FFFFFF; box-shadow: 0px 2px 5px rgba(48, 45, 38, 0.16), \
     0px 9px 32px rgba(48, 45, 38, 0.18);",
    "background: #1E1E1E; box-shadow: 0px 2px 6px rgba(0, 0, 0, 0.5), \
     0px 10px 36px rgba(0, 0, 0, 0.55);",
    "background: #FBFAF6; box-shadow: 0px 2px 5px rgba(48, 45, 38, 0.16), \
     0px 9px 32px rgba(48, 45, 38, 0.18);",
    "background: #1D1D1D; box-shadow: 0px 2px 6px rgba(0, 0, 0, 0.5), \
     0px 10px 36px rgba(0, 0, 0, 0.55);",
);

/// Quick Look's title: the item's name.
pub(crate) const QUICK_LOOK_HEAD: &str = themed!(
    "flex-shrink: 0; padding: 8px 14px; font-weight: 600;",
    "border-bottom: 1px solid #E2E6EB;",
    "border-bottom: 1px solid #333333;",
    "border-bottom: 1px solid #E0DDD7;",
    "border-bottom: 1px solid #2E2E2E;",
);

/// The palette: 560 px of field paper.
pub(crate) const PALETTE_PANEL: &str = themed!(
    "display: flex; flex-direction: column; width: 560px; max-height: 70%; border-radius: 8px; \
     overflow: hidden;",
    "background: #FFFFFF; box-shadow: 0px 2px 5px rgba(48, 45, 38, 0.16), \
     0px 9px 32px rgba(48, 45, 38, 0.18);",
    "background: #252525; box-shadow: 0px 2px 6px rgba(0, 0, 0, 0.5), \
     0px 10px 36px rgba(0, 0, 0, 0.55);",
    "background: #FBFAF6; box-shadow: 0px 2px 5px rgba(48, 45, 38, 0.16), \
     0px 9px 32px rgba(48, 45, 38, 0.18);",
    "background: #1D1D1D; box-shadow: 0px 2px 6px rgba(0, 0, 0, 0.5), \
     0px 10px 36px rgba(0, 0, 0, 0.55);",
);

/// The palette's box row.
pub(crate) const PALETTE_INPUT_ROW: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; \
     padding: 10px 12px;",
    "border-bottom: 1px solid #E2E6EB;",
    "border-bottom: 1px solid #333333;",
    "border-bottom: 1px solid #E0DDD7;",
    "border-bottom: 1px solid #2E2E2E;",
);

/// The palette's rows.
pub(crate) const PALETTE_LIST: &str =
    "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow-y: auto; \
     padding: 4px 0px;";

/// A group's title in the palette.
pub(crate) const PALETTE_SECTION: &str = themed!(
    "padding: 6px 14px 2px 14px; font-size: 11px; font-weight: 700; text-transform: uppercase;",
    "color: #7A8594;",
    "color: #8C8C8C;",
    "color: #827F76; letter-spacing: 0.08em;",
    "color: #8C8C8C;",
);

/// A row of the palette.
pub(crate) const PALETTE_ROW: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; height: 30px; \
     padding: 0px 14px; cursor: default;",
    ":hover { background: #EEF3F9; }",
    ":hover { background: #333333; }",
    ":hover { background: #EDEFF4; }",
    ":hover { background: #2B2F37; }",
);

/// The hot row (the arrows', Return's).
pub(crate) const PALETTE_ROW_HOT: &str = themed!(
    "",
    "background: #DDE8F5; :hover { background: #DDE8F5; }",
    "background: #2E3E55; :hover { background: #2E3E55; }",
    "background: #E0E4EE; :hover { background: #E0E4EE; }",
    "background: #2F4A85; :hover { background: #2F4A85; }",
);

/// Where a palette row is (a place's folders, a command's keys).
pub(crate) const PALETTE_DETAIL: &str = themed!(
    "flex-shrink: 0; margin-left: 10px; font-size: 12px; white-space: nowrap;",
    "color: #7A8594;",
    "color: #8C8C8C;",
    "color: #827F76;",
    "color: #8C8C8C;",
);

/// The keys at the palette's foot.
pub(crate) const PALETTE_FOOT: &str = themed!(
    "flex-shrink: 0; padding: 6px 14px; font-size: 12px;",
    "color: #7A8594; border-top: 1px solid #E2E6EB;",
    "color: #8C8C8C; border-top: 1px solid #333333;",
    "color: #827F76; border-top: 1px solid #E0DDD7;",
    "color: #8C8C8C; border-top: 1px solid #2E2E2E;",
);

// ==== Items (every layout) ====

/// An item at rest: a hairline kept free for the focus, a wash under the pointer.
pub(crate) const ITEM: &str = themed!(
    "border: 1px solid transparent; cursor: default;",
    ":hover { background: #E8F2FD; }",
    ":hover { background: #323A46; }",
    ":hover { background: #EDEFF4; }",
    ":hover { background: #2B2F37; }",
);

/// A selected item: Office's blue (flat), the accent's soft wash - its stone at night (flora,
/// as flora's data table and icon grid).
pub(crate) const ITEM_SELECTED: &str = themed!(
    "",
    "background: linear-gradient(to bottom, #DDEBFD, #C1DCFC); border: 1px solid #84ACDD; \
     :hover { background: #C6DDFB; }",
    "background: linear-gradient(to bottom, #37577F, #2E4A6E); border: 1px solid #4A78B0; \
     :hover { background: #39597F; }",
    "background: var(--az-accent-soft, #E0E4EE); border: 1px solid #C3CCE0; \
     :hover { background: var(--az-accent-soft, #E0E4EE); }",
    "background: var(--az-accent, #2F4A85); border: 1px solid #4A65A0; color: #F4F2EA; \
     :hover { background: var(--az-accent, #2F4A85); }",
);

/// The item the keyboard is on: its hairline in the accent.
pub(crate) const ITEM_FOCUS: &str = themed!(
    "",
    "border: 1px solid #5B8FD6;",
    "border: 1px solid #6F9AD8;",
    "border: 1px solid #2F4A85;",
    "border: 1px solid #7A93C6;",
);

/// An icon cell: rounded (an item's wash under the pointer; it lifts when selected).
pub(crate) const CELL: &str = "border-radius: 6px;";

/// A selected icon cell lifts off the paper, in the accent's hairline (flora).
pub(crate) const CELL_SELECTED: &str = themed!(
    "",
    "box-shadow: 0px 2px 6px rgba(30, 60, 110, 0.25);",
    "box-shadow: 0px 2px 6px rgba(0, 0, 0, 0.60);",
    "border: 1px solid #2F4A85; \
     box-shadow: 0px 2px 5px rgba(48, 45, 38, 0.16), 0px 8px 20px rgba(48, 45, 38, 0.14);",
    "border: 1px solid #7A93C6; \
     box-shadow: 0px 2px 5px rgba(0, 0, 0, 0.50), 0px 8px 20px rgba(0, 0, 0, 0.45);",
);

/// A selected Tile (This PC's drive, a Quick access folder, an item of the Tiles layout) lifts
/// off the paper, on the tile's own selection wash (azul's Tile: under the pointer it washes as
/// its look has it).
pub(crate) const TILE_SELECTED: &str = themed!(
    "",
    "box-shadow: 0px 2px 6px rgba(30, 60, 110, 0.25);",
    "box-shadow: 0px 2px 6px rgba(0, 0, 0, 0.60);",
    "box-shadow: 0px 1px 2px rgba(48, 45, 38, 0.14), 0px 6px 16px rgba(48, 45, 38, 0.16);",
    "box-shadow: 0px 1px 2px rgba(0, 0, 0, 0.55), 0px 6px 16px rgba(0, 0, 0, 0.45);",
);

// ==== Buy storage's payment popover ====

/// The verified-origin chip at the popover's head: native, outside the web view, so no page
/// can draw over it - a lock, the host, the provider's legal name.
pub(crate) const PAY_CHIP: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; padding: 6px 10px; \
     margin-bottom: 10px; border-radius: 6px; font-size: 12px;",
    "background: #EEF3EA; color: #2D5016; border: 1px solid #C9DCBC;",
    "background: #24331B; color: #CFE6BF; border: 1px solid #3C5530;",
    "background: #EDEFE6; color: #34452A; border: 1px solid #D3D8C6;",
    "background: #232A1F; color: #C9D6BD; border: 1px solid #39432F;",
);

/// The card artwork: the brand the fields page reports, the masked number, the cardholder's
/// name - drawn by the app, never the provider's page.
pub(crate) const PAY_CARD: &str = themed!(
    "width: 300px; height: 172px; box-sizing: border-box; padding: 18px 20px; \
     margin: 0px auto 12px auto; border-radius: 12px; display: flex; flex-direction: column; \
     justify-content: space-between; color: #FFFFFF; font-size: 13px; letter-spacing: 1px;",
    "background: linear-gradient(135deg, #2A63B8, #6C9BDB); \
     box-shadow: 0px 3px 10px rgba(30, 60, 110, 0.35);",
    "background: linear-gradient(135deg, #1E3E70, #3C6AAE); \
     box-shadow: 0px 3px 10px rgba(0, 0, 0, 0.60);",
    "background: linear-gradient(135deg, var(--az-accent, #2F4A85), #6B5B95); \
     box-shadow: 0px 1px 2px rgba(48, 45, 38, 0.20), 0px 8px 20px rgba(48, 45, 38, 0.22);",
    "background: linear-gradient(135deg, #1E2E55, #44396A); \
     box-shadow: 0px 1px 2px rgba(0, 0, 0, 0.55), 0px 8px 20px rgba(0, 0, 0, 0.45);",
);

/// The frame around the provider's page in the popover.
pub(crate) const PAY_SLOT: &str = themed!(
    "width: 100%; box-sizing: border-box; border-radius: 6px; overflow: hidden;",
    "border: 1px solid #C7CED8;",
    "border: 1px solid #44484F;",
    "border: 1px solid #D3D0C8;",
    "border: 1px solid #3A3A3A;",
);

#[cfg(test)]
mod tests {
    use super::*;

    /// Every look is ONE inline style holding all four variants, its braces balanced (an
    /// unbalanced brace would swallow the rest of the string), the flora block last.
    #[test]
    fn every_look_holds_four_variants_in_balanced_blocks() {
        let themed_looks = [
            SIDEBAR, SECTION_HEAD, ROW, ROW_SELECTED, ROW_FOCUS, ROW_DROP, ICON_TINT, PILL,
            PILL_SELECTED, EJECT, EJECT_SELECTED, ACTIVITY, ACTIVITY_HEAD, ACTIVITY_ERROR,
            SIDEBAR_FOOT, SMALL_BUTTON, PAGE, LEAF, DETAILS_HEADER, COLUMN, COLUMN_SORTED,
            COLUMN_EDGE, STRIPE, ITEM, ITEM_SELECTED, ITEM_FOCUS, CELL_SELECTED, TILE_SELECTED,
        ];
        for look in themed_looks {
            let mut depth = 0i32;
            for c in look.chars() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        assert!(depth >= 0, "a closing brace too many: {look}");
                    }
                    _ => {}
                }
            }
            assert_eq!(depth, 0, "unbalanced: {look}");
            assert_eq!(look.matches("@theme(flora)").count(), 1, "{look}");
            assert_eq!(look.matches("@media (prefers-color-scheme: dark)").count(), 2, "{look}");
            assert!(look.trim_end().ends_with("} }"), "the flora block closes the look: {look}");
        }
    }

    /// Flora's page is the linen: its colour first, so a renderer without the rasters keeps it.
    #[test]
    fn floras_page_is_the_linen_and_its_colour_holds_alone() {
        let flora = &PAGE[PAGE.find("@theme(flora)").unwrap()..];
        let first = flora.find("background: #E6E4DF;").expect("the page colour alone");
        let linen = flora.find("repeating-linear-gradient(0deg").expect("the raster across");
        assert!(first < linen);
        assert!(flora.contains("repeating-linear-gradient(90deg"), "and the raster down");
    }
}
