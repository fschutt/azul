//! AzDrive's body in each app theme and mode - the source list, the content leaf, the Details
//! list, the icon cells, the path bar and the status line - one inline style per part.
//!
//! FLORA (the website's look, `doc/templates/flora.css`): the content is a LEAF of field paper
//! (`--fl-fld`) laid on the linen page (`--fl-pg` and its two hairline rasters, `--fl-grain`) in
//! a thin rule (`--fl-bd`) with a soft cast shadow (`--fl-shadow-1` / `-2`); the source list rests
//! on a soft vertical gradient from the strip tone down to the desk (`--fl-strip` .. `--fl-desk`);
//! its section titles are flora's capitals (`.fl-label`: EB Garamond, bold, tracked, `--fl-soft1`);
//! a selected source is the accent stone (`--az-accent`) under paper ink, with a lip and a cast;
//! the Details header is the raised face (`--fl-rT` -> `--fl-rB`), its rows alternate field paper
//! with a quiet wash, a selected item rests on the accent's soft wash (flora's data table and
//! icon grid), and a selected icon cell or tile lifts off the paper.
//!
//! FLAT (Office 2010): the same structure in Office's silver-blue - a white leaf in a thin silver
//! rule on the silver page, the source list on Outlook's navigation-pane gradient, Office's
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
    "background: linear-gradient(to bottom, #EEECE7, #DCD9D2); color: #262521;",
    "background: linear-gradient(to bottom, #282828, #1B1B1B); color: #E7E7E7;",
);

/// The rows' scrolling box.
pub(crate) const SIDEBAR_LIST: &str =
    "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow-y: auto; \
     padding: 0px 0px 10px 0px;";

/// A section's title: small capitals behind a disclosure triangle (flora's `.fl-label`: EB
/// Garamond, bold, tracked out).
pub(crate) const SECTION_HEAD: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; height: 20px; \
     margin: 10px 6px 3px 6px; padding: 0px 6px 0px 2px; border-radius: 4px; font-size: 11px; \
     font-weight: 700; text-transform: uppercase; cursor: pointer;",
    "color: #6A7788; :hover { color: #27364A; }",
    "color: #9AA6B5; :hover { color: #E3E8EE; }",
    "color: #66645C; font-family: EB Garamond, Georgia, Times New Roman, serif; \
     letter-spacing: 1px; :hover { color: #262521; }",
    "color: #A8A8A8; :hover { color: #E7E7E7; }",
);

/// A section's title text: the capitals stated on the text too (whether the engine inherits
/// `text-transform` or not).
pub(crate) const SECTION_TITLE: &str = "flex-grow: 1; min-width: 0px; overflow: hidden; \
                                        white-space: nowrap; text-overflow: ellipsis; \
                                        text-transform: uppercase;";

/// A section's triangle (and a drive's, which opens its folders).
pub(crate) const TRIANGLE: &str =
    "font-size: 16px; width: 16px; flex-shrink: 0; margin-right: 1px; opacity: 0.8;";

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
    ":focus { box-shadow: inset 0px 0px 0px 2px #4286F4; }",
    ":focus { box-shadow: inset 0px 0px 0px 2px #4683D6; }",
    ":focus { box-shadow: inset 0px 0px 0px 2px #7A93C6; }",
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

/// The tint of an icon at rest: Office's icon blue, flora's accent.
pub(crate) const ICON_TINT: &str = themed!(
    "",
    "color: #3D6AA8;",
    "color: #7AA7E0;",
    "color: var(--az-accent, #2F4A85);",
    "color: var(--az-accent-glow, #7A93C6);",
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

/// The activity area's title.
pub(crate) const ACTIVITY_HEAD: &str = themed!(
    "padding: 0px 10px 3px 10px; font-size: 10px; font-weight: 700; text-transform: uppercase;",
    "color: #6A7788;",
    "color: #9AA6B5;",
    "color: #66645C; font-family: EB Garamond, Georgia, Times New Roman, serif; \
     letter-spacing: 1px;",
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
    "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; height: 24px;",
    "border-top: 1px solid #C2C9D2; background: linear-gradient(to bottom, #F7F9FB, #E4E9EF);",
    "border-top: 1px solid #575757; background: linear-gradient(to bottom, #3C3C3C, #313131);",
    "border-top: 1px solid #C6C3BB; background: linear-gradient(to bottom, #FAF9F5, #ECEAE4);",
    "border-top: 1px solid #3F3F3F; background: linear-gradient(to bottom, #333333, #292929);",
);

/// One of the small buttons in that bar.
pub(crate) const SMALL_BUTTON: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; justify-content: center; \
     width: 30px; height: 24px; font-size: 15px; cursor: pointer;",
    "border-right: 1px solid #D3D9E0; color: #3D5574; :hover { background: #DDE4EC; } \
     :focus { box-shadow: inset 0px 0px 0px 2px #4286F4; }",
    "border-right: 1px solid #4D4D4D; color: #DADADA; :hover { background: #474747; } \
     :focus { box-shadow: inset 0px 0px 0px 2px #4683D6; }",
    "border-right: 1px solid #D8D5CE; color: #56544C; :hover { background: #E3E0D9; } \
     :focus { box-shadow: inset 0px 0px 0px 2px #7A93C6; }",
    "border-right: 1px solid #383838; color: #BEBEBE; :hover { background: #3A3A3A; }",
);

// ==== The content: a leaf on the page ====

/// The page the leaves lie on (the content's, the right pane's): Office's silver in flat, the
/// linen in flora - the page colour and flora's two hairline rasters over it (azul paints the
/// layers in their order; a renderer that cannot draw the rasters keeps the page colour, the
/// declaration before).
pub(crate) const PAGE: &str = themed!(
    "display: flex; flex-direction: column; flex-grow: 1; width: 100%; height: 100%; \
     min-width: 0px; min-height: 0px; box-sizing: border-box; padding: 8px;",
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

/// The leaf: paper in a thin rule with a soft cast shadow (the page's padding holds it).
pub(crate) const LEAF: &str = themed!(
    "display: flex; flex-direction: column; flex-grow: 1; min-width: 0px; min-height: 0px; \
     overflow: hidden; border-radius: 3px;",
    "background: #FFFFFF; border: 1px solid #B8C2CE; \
     box-shadow: 0px 1px 3px rgba(40, 60, 90, 0.14);",
    "background: #262626; border: 1px solid #4A4A4A; box-shadow: 0px 1px 3px rgba(0, 0, 0, 0.50);",
    "background: #FBFAF6; border: 1px solid #C6C3BB; border-radius: 5px; \
     box-shadow: 0px 1px 2px rgba(48, 45, 38, 0.14), 0px 4px 12px rgba(48, 45, 38, 0.12);",
    "background: #1D1D1D; border: 1px solid #3F3F3F; \
     box-shadow: 0px 1px 2px rgba(0, 0, 0, 0.55), 0px 4px 12px rgba(0, 0, 0, 0.45);",
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

// ==== The path bar and the status line, at the foot of the leaf ====

/// Finder's path bar: the open place's trail, each step an icon and its name.
pub(crate) const PATH_BAR: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; height: 24px; \
     padding: 0px 6px; font-size: 12px; overflow: hidden;",
    "border-top: 1px solid #D3D9E0; background: linear-gradient(to bottom, #F8FAFC, #EDF1F5);",
    "border-top: 1px solid #3D3D3D; background: linear-gradient(to bottom, #303030, #292929);",
    "border-top: 1px solid #D8D5CE; background: linear-gradient(to bottom, #F7F6F2, #EEECE7);",
    "border-top: 1px solid #383838; background: linear-gradient(to bottom, #272727, #212121);",
);

/// A step of the path bar (a click goes there; items dropped on it move there).
pub(crate) const CRUMB: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; flex-shrink: 1; min-width: 0px; \
     height: 20px; padding: 0px 5px; border-radius: 4px; cursor: pointer;",
    ":hover { background: #E2EBF6; } :drag-over { background: #C1DCFC; }",
    ":hover { background: #3A3A3A; } :drag-over { background: #2E4A6E; }",
    ":hover { background: #E6E8EE; } :drag-over { background: #E0E4EE; }",
    ":hover { background: #2E2E2E; } :drag-over { background: #1E3260; }",
);

/// The open place's own step: in full, bold.
pub(crate) const CRUMB_LAST: &str = "flex-shrink: 0; font-weight: 600;";

/// A step's icon.
pub(crate) const CRUMB_ICON: &str = themed!(
    "font-size: 14px; margin-right: 4px; flex-shrink: 0;",
    "color: #3D6AA8;",
    "color: #7AA7E0;",
    "color: var(--az-accent, #2F4A85);",
    "color: var(--az-accent-glow, #7A93C6);",
);

/// The chevron between two steps.
pub(crate) const CRUMB_SEPARATOR: &str =
    "font-size: 14px; flex-shrink: 0; margin: 0px 1px; opacity: 0.45;";

/// Finder's status line: how many items, how many selected, the space left.
pub(crate) const STATUS_LINE: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; justify-content: center; \
     flex-shrink: 0; height: 22px; padding: 0px 10px; font-size: 11px; overflow: hidden; \
     border-radius: 0px 0px 2px 2px;",
    "border-top: 1px solid #E2E6EB; background: #EDF1F5; color: #626A76;",
    "border-top: 1px solid #333333; background: #292929; color: #9E9E9E;",
    "border-top: 1px solid #E0DDD7; background: #EEECE7; color: #66645C; \
     border-radius: 0px 0px 4px 4px;",
    "border-top: 1px solid #2E2E2E; background: #212121; color: #A8A8A8;",
);

/// A transfer (or a failure) in the status line, while the source list is hidden: a chip that
/// opens the transfers.
pub(crate) const STATUS_CHIP: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; height: 16px; \
     margin-left: 10px; padding: 0px 7px; border-radius: 8px; cursor: pointer;",
    "background: #DCE6F2; color: #1E4F99;",
    "background: #3A4556; color: #DCE6F2;",
    "background: #E0E4EE; color: #1E3260;",
    "background: #1E3260; color: #DCE3F2;",
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
            PATH_BAR, CRUMB, CRUMB_ICON, STATUS_LINE, STATUS_CHIP,
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
