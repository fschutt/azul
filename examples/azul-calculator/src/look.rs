//! AzCalculator's look in each app theme and mode - the display, the keys,
//! the programmer panel, the history tape, the graph - one inline style per
//! part, every part carrying all four looks in ONE string ([`themed!`]):
//! flat by day, flat at night (`@media (prefers-color-scheme: dark)`),
//! flora by day and flora at night (`@theme(flora)`).
//!
//! FLAT is Office 2010: white-to-silver key faces in a thin silver-blue rule,
//! the warm yellow under the pointer and the orange while pressed, the
//! accent blue for `=`; Segoe UI digits on a white display.
//!
//! FLORA is the website (`doc/templates/flora.css`): every key is a stone of
//! raised paper (`--fl-rT` -> `--fl-rB` in `--fl-bd2`, the lip and the cast
//! shadow), the operators the desk tone, `=` the app's slate stone
//! (`--fl-gem` cut from the slate ramp); the display a sunken field
//! (`--fl-fld`, the well); digits and labels in EB Garamond, tabular and
//! lining.
//!
//! The rules of `Dom::with_css` (see AzDrive's look): a `:hover { .. }` /
//! `:active { .. }` block also styles every hovered element INSIDE the node,
//! so a state paints SOLID backgrounds and no shadows (a key's label is a
//! bare text node; a solid wash repeated on it is invisible).

/// One part's look: `base` (every theme), then `flat` by day, `flat_dark`,
/// `flora` by day and `flora_dark` - runs of declarations; state blocks
/// (`:hover { .. }`) may nest in each.
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

// ==== The surface and the display ====

/// The calculator's surface: a column that fills the shell's content and
/// holds the keyboard focus (typed characters arrive at the focused node).
pub const SURFACE: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
                           min-width: 0px; padding: 6px 8px 8px 8px;";

/// The display: the expression over the result, right-aligned.
pub const DISPLAY: &str = themed!(
    "display: flex; flex-direction: column; flex-shrink: 0; padding: 6px 12px 6px 12px; \
     margin-bottom: 6px; border-radius: 3px; min-width: 0px; overflow: hidden;",
    "background: linear-gradient(to bottom, #FFFFFF, #F2F6FB); border: 1px solid #B9C4D2; \
     color: #1E1E1E;",
    "background: linear-gradient(to bottom, #2B2B2B, #242424); border: 1px solid #575757; \
     color: #F2F2F2;",
    "background: #FBFAF6; border: 1px solid #C6C3BB; color: #262521; \
     box-shadow: inset 0px 1px 2px rgba(48, 45, 38, 0.10);",
    "background: #1D1D1D; border: 1px solid #3F3F3F; color: #E7E7E7; \
     box-shadow: inset 0px 1px 2px rgba(0, 0, 0, 0.45);",
);

/// The expression line over the result: smaller, in the soft ink.
pub const EXPR_LINE: &str = themed!(
    "text-align: right; font-size: 14px; min-height: 20px; white-space: nowrap; overflow: hidden; \
     font-variant-numeric: tabular-nums lining-nums;",
    "color: #626A76;",
    "color: #9E9E9E;",
    "color: #66645C; font-size: 16px; font-family: EB Garamond, Georgia, Times New Roman, serif;",
    "color: #A8A8A8;",
);

/// The result line: the big figures (the size is the part's own, by length).
pub const RESULT_LINE: &str = themed!(
    "text-align: right; white-space: nowrap; overflow: hidden; \
     font-variant-numeric: tabular-nums lining-nums;",
    "font-weight: 300;",
    "",
    "font-weight: 500; font-family: EB Garamond, Georgia, Times New Roman, serif;",
    "",
);

/// The notice under the result (a hint, a failed paste).
pub const NOTICE: &str = themed!(
    "text-align: right; font-size: 12px; padding-top: 2px; overflow: hidden;",
    "color: #9A5B00;",
    "color: #E8B860;",
    "color: #6E6349; font-size: 14px; font-style: italic; font-family: EB Garamond, Georgia, Times New Roman, serif;",
    "color: #C4B58E;",
);

/// The typeset expression (the graphing view): the math face.
pub const MATH: &str = themed!(
    "display: flex; flex-direction: row; justify-content: flex-end; align-items: center; \
     min-height: 40px; padding: 8px 0px 4px 0px; white-space: nowrap; overflow: hidden; \
     font-size: 26px;",
    "font-family: Cambria Math, Cambria, Times New Roman, serif;",
    "",
    "font-family: EB Garamond, Georgia, Times New Roman, serif;",
    "",
);

/// A fraction bar and a radical's vinculum: the ink, one pixel.
pub const MATH_RULE: &str = themed!(
    "",
    "background: #1E1E1E;",
    "background: #F2F2F2;",
    "background: #262521;",
    "background: #E7E7E7;",
);

/// A hole (an operand still to come) or letters still being typed.
pub const MATH_DIM: &str = themed!(
    "",
    "color: #858C97;",
    "color: #7D7D7D;",
    "color: #827F76;",
    "color: #8C8C8C;",
);

// ==== The keys ====

/// Every key: a box the grid stretches, its label centred.
pub const KEY: &str = "display: flex; flex-direction: row; align-items: center; \
                       justify-content: center; min-width: 0px; min-height: 26px; \
                       border-radius: 3px; cursor: pointer; overflow: hidden; \
                       white-space: nowrap; user-select: none; \
                       font-variant-numeric: tabular-nums lining-nums;";

/// A digit (and the point, +/-, A-F): the brightest face, the biggest figures.
pub const KEY_DIGIT: &str = themed!(
    "font-size: 19px;",
    "background: linear-gradient(to bottom, #FFFFFF, #F1F4F8); border: 1px solid #C2C9D2; \
     color: #1E1E1E; :hover { background: #FDE9A9; border-color: #E5C365; } \
     :active { background: #F9CE6D; border-color: #C2913C; }",
    "background: linear-gradient(to bottom, #4A4A4A, #3E3E3E); border: 1px solid #5E5E5E; \
     color: #F2F2F2; :hover { background: #524526; border-color: #9E7E34; } \
     :active { background: #3A2F12; border-color: #B88C3A; }",
    "background: linear-gradient(to bottom, #FAF9F5, #ECEAE4); border: 1px solid #B4B1A9; \
         color: #262521; font-size: 22px; font-weight: 500; \
         box-shadow: inset 0px 1px 0px rgba(255, 255, 255, 0.7), \
         inset 0px -2px 3px rgba(48, 45, 38, 0.10), 0px 1px 2px rgba(48, 45, 38, 0.14); \
         font-family: EB Garamond, Georgia, Times New Roman, serif; \
         :hover { background: #F8F7F2; border-color: #9C9890; } \
         :active { background: #E7E4DD; border-color: #9C9890; }",
    "background: linear-gradient(to bottom, #333333, #292929); border: 1px solid #4A4A4A; \
     color: #E7E7E7; box-shadow: inset 0px 1px 0px rgba(255, 255, 255, 0.09), \
     inset 0px -2px 3px rgba(0, 0, 0, 0.4), 0px 1px 2px rgba(0, 0, 0, 0.55); \
     :hover { background: #393939; border-color: #616161; } \
     :active { background: #222222; border-color: #616161; }",
);

/// An operator (+ − × ÷): the silver face (flat), the desk tone (flora).
pub const KEY_OP: &str = themed!(
    "font-size: 20px;",
    "background: linear-gradient(to bottom, #F3F6FA, #DDE4EC); border: 1px solid #B6C0CC; \
     color: #1E3A66; :hover { background: #FDE9A9; border-color: #E5C365; color: #1E1E1E; } \
     :active { background: #F9CE6D; border-color: #C2913C; }",
    "background: linear-gradient(to bottom, #3F3F3F, #353535); border: 1px solid #575757; \
     color: #BFD6F5; :hover { background: #524526; border-color: #9E7E34; color: #F2F2F2; } \
     :active { background: #3A2F12; border-color: #B88C3A; }",
    "background: linear-gradient(to bottom, #ECEAE4, #DEDBD4); border: 1px solid #A5A199; \
         color: #354551; font-size: 23px; \
         box-shadow: inset 0px 1px 0px rgba(255, 255, 255, 0.6), \
         inset 0px -2px 3px rgba(48, 45, 38, 0.12), 0px 1px 2px rgba(48, 45, 38, 0.14); \
         font-family: EB Garamond, Georgia, Times New Roman, serif; \
         :hover { background: #F1EFE9; border-color: #9C9890; } \
         :active { background: #DCD9D2; border-color: #9C9890; }",
    "background: linear-gradient(to bottom, #2B2B2B, #222222); border: 1px solid #4A4A4A; \
     color: #B4C6D3; box-shadow: inset 0px 1px 0px rgba(255, 255, 255, 0.07), \
     inset 0px -2px 3px rgba(0, 0, 0, 0.45), 0px 1px 2px rgba(0, 0, 0, 0.55); \
     :hover { background: #333333; border-color: #616161; } \
     :active { background: #1C1C1C; border-color: #616161; }",
);

/// A function or clearing key (%, CE, C, ⌫, 1/x, sin ...): the standard
/// face, a smaller label.
pub const KEY_FUNC: &str = themed!(
    "font-size: 15px;",
    "background: linear-gradient(to bottom, #F7F9FB, #E6EBF1); border: 1px solid #C2C9D2; \
     color: #1E1E1E; :hover { background: #FDE9A9; border-color: #E5C365; } \
     :active { background: #F9CE6D; border-color: #C2913C; }",
    "background: linear-gradient(to bottom, #424242, #383838); border: 1px solid #575757; \
     color: #E6E6E6; :hover { background: #524526; border-color: #9E7E34; } \
     :active { background: #3A2F12; border-color: #B88C3A; }",
    "background: linear-gradient(to bottom, #F4F3EE, #E6E4DE); border: 1px solid #B4B1A9; \
         color: #2E2C26; font-size: 17px; \
         box-shadow: inset 0px 1px 0px rgba(255, 255, 255, 0.65), \
         inset 0px -2px 3px rgba(48, 45, 38, 0.10), 0px 1px 2px rgba(48, 45, 38, 0.12); \
         font-family: EB Garamond, Georgia, Times New Roman, serif; \
         :hover { background: #F6F5F0; border-color: #9C9890; } \
         :active { background: #E2DFD8; border-color: #9C9890; }",
    "background: linear-gradient(to bottom, #2F2F2F, #272727); border: 1px solid #454545; \
     color: #DCDCDC; box-shadow: inset 0px 1px 0px rgba(255, 255, 255, 0.07), \
     inset 0px -2px 3px rgba(0, 0, 0, 0.4), 0px 1px 2px rgba(0, 0, 0, 0.5); \
     :hover { background: #363636; border-color: #5A5A5A; } \
     :active { background: #1F1F1F; border-color: #5A5A5A; }",
);

/// `=`: the accent - Office's blue face, flora's slate stone.
pub const KEY_EQUALS: &str = themed!(
    "font-size: 22px;",
    "background: linear-gradient(to bottom, #4A86D8, #2A63B8); border: 1px solid #1E4F99; \
     color: #FFFFFF; :hover { background: #3D7BD0; border-color: #1E4F99; } \
     :active { background: #2459A6; border-color: #173F7A; }",
    "background: linear-gradient(to bottom, #5A93DE, #3A70C0); border: 1px solid #2B5AA0; \
     color: #FFFFFF; :hover { background: #4C86D4; border-color: #2B5AA0; } \
     :active { background: #3468B8; border-color: #224C88; }",
    "background: radial-gradient(ellipse farthest-corner at 30% 12%, #8AA0B0 0%, \
         #4A5C6B 48%, #354551 100%); border: 1px solid #354551; color: #F4F2EA; \
         font-size: 25px; box-shadow: inset 0px 1px 0px rgba(255, 255, 255, 0.3), \
         inset 0px -2px 4px rgba(0, 0, 0, 0.3), 0px 1px 2px rgba(48, 45, 38, 0.14); \
         font-family: EB Garamond, Georgia, Times New Roman, serif; \
         :hover { background: #55697A; border-color: #354551; } \
         :active { background: #3E4E5B; border-color: #2A3640; }",
    "border: 1px solid #22303A; box-shadow: inset 0px 1px 0px rgba(255, 255, 255, 0.18), \
     inset 0px -2px 4px rgba(0, 0, 0, 0.45), 0px 1px 2px rgba(0, 0, 0, 0.55);",
);

/// A memory key (MC MR M+ M− MS): quiet - no face until the pointer comes.
pub const KEY_MEM: &str = themed!(
    "font-size: 12px; border: 1px solid transparent; background: transparent;",
    "color: #2A4E85; :hover { background: #FDE9A9; border-color: #E5C365; } \
     :active { background: #F9CE6D; border-color: #C2913C; }",
    "color: #9DC0EC; :hover { background: #524526; border-color: #9E7E34; } \
     :active { background: #3A2F12; border-color: #B88C3A; }",
    "color: #6E6349; font-size: 13px; font-weight: 700; letter-spacing: 1px; \
         font-family: EB Garamond, Georgia, Times New Roman, serif; \
         :hover { background: #F6F5F1; border-color: #D8D5CE; color: #4F4633; } \
         :active { background: #E7E4DD; }",
    "color: #C4B58E; :hover { background: #2E2E2E; border-color: #383838; color: #DED3B4; } \
     :active { background: #1F1F1F; }",
);

/// A toggle that is on (2nd, F-E, the angle unit's key): the accent's soft wash.
pub const KEY_ON: &str = themed!(
    "",
    "background: linear-gradient(to bottom, #DDEBFD, #C1DCFC); border-color: #84ACDD; \
     color: #1E4F99; :hover { background: #CFE3FC; }",
    "background: linear-gradient(to bottom, #37577F, #2E4A6E); border-color: #4A78B0; \
     color: #FFFFFF; :hover { background: #335277; }",
    "background: #DEE3E7; border-color: #4A5C6B; color: #354551; :hover { background: #E4E8EB; }",
    "background: #354551; border-color: #8AA0B0; color: #F4F2EA; :hover { background: #3B4C59; }",
);

/// A key that does nothing in this state (a digit the base does not take).
pub const KEY_OFF: &str = themed!(
    "cursor: default; opacity: 0.38;",
    ":hover { background: #F1F4F8; border-color: #C2C9D2; }",
    ":hover { background: #3E3E3E; border-color: #5E5E5E; }",
    ":hover { background: #ECEAE4; border-color: #B4B1A9; }",
    ":hover { background: #292929; border-color: #4A4A4A; }",
);

// ==== Panels ====

/// A leaf laid beside the keys: the programmer panel, the history tape,
/// the graph's function list.
pub const PANEL: &str = themed!(
    "display: flex; flex-direction: column; min-width: 0px; min-height: 0px; \
     border-radius: 3px; overflow: hidden;",
    "background: #F7F9FB; border: 1px solid #D3D9E0;",
    "background: #262626; border: 1px solid #3D3D3D;",
    "background: #F2F1ED; border: 1px solid #D8D5CE; \
     box-shadow: 0px 1px 2px rgba(48, 45, 38, 0.10);",
    "background: #232323; border: 1px solid #383838; \
     box-shadow: 0px 1px 2px rgba(0, 0, 0, 0.5);",
);

/// A panel's title: flat's small bold label, flora's tracked capitals.
pub const PANEL_TITLE: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; \
     padding: 6px 10px 4px 10px; font-size: 11px; font-weight: 700; \
     text-transform: uppercase; letter-spacing: 0.5px;",
    "color: #626A76;",
    "color: #9E9E9E;",
    "color: #66645C; font-size: 12px; letter-spacing: 1.5px; font-family: EB Garamond, Georgia, Times New Roman, serif;",
    "color: #A8A8A8;",
);

/// HEX / DEC / OCT / BIN: one row each, the input base marked.
pub const BASE_ROW: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; padding: 3px 10px 3px 7px; \
     border-left: 3px solid transparent; cursor: pointer; font-size: 13px; \
     font-variant-numeric: tabular-nums lining-nums; white-space: nowrap; overflow: hidden;",
    "color: #3A3F47; :hover { background: #EAF0F7; }",
    "color: #CFCFCF; :hover { background: #303030; }",
    "color: #4E4C45; font-size: 15px; font-family: EB Garamond, Georgia, Times New Roman, serif; \
     :hover { background: #F6F5F1; }",
    "color: #BCBCBC; :hover { background: #2B2B2B; }",
);

/// The input base's row.
pub const BASE_ROW_SELECTED: &str = themed!(
    "font-weight: 700;",
    "border-left-color: #2A63B8; background: #E3EDFA; color: #1E1E1E; \
     :hover { background: #E3EDFA; }",
    "border-left-color: #4683D6; background: #2E3A4A; color: #FFFFFF; \
     :hover { background: #2E3A4A; }",
    "border-left-color: #4A5C6B; background: #E6E9EB; color: #262521; \
     :hover { background: #E6E9EB; }",
    "border-left-color: #8AA0B0; background: #2C343A; color: #E7E7E7; \
     :hover { background: #2C343A; }",
);

/// A bit of the bit field.
pub const BIT: &str = themed!(
    "width: 13px; text-align: center; cursor: pointer; font-size: 12px; \
     font-variant-numeric: tabular-nums lining-nums; border-radius: 2px;",
    "color: #858C97; :hover { background: #FDE9A9; color: #1E1E1E; }",
    "color: #7D7D7D; :hover { background: #524526; color: #F2F2F2; }",
    "color: #827F76; :hover { background: #E4E1DA; color: #262521; }",
    "color: #8C8C8C; :hover { background: #333333; color: #E7E7E7; }",
);

/// A set bit.
pub const BIT_ON: &str = themed!(
    "font-weight: 700;",
    "color: #1E4F99;",
    "color: #9DC0EC;",
    "color: #354551;",
    "color: #B4C6D3;",
);

/// A bit beyond the word size.
pub const BIT_OUT: &str = "opacity: 0.3; cursor: default;";

/// The bit field's position labels (63, 47, ...).
pub const BIT_LABEL: &str = themed!(
    "width: 22px; font-size: 10px; flex-shrink: 0;",
    "color: #A0A7B2;",
    "color: #6E6E6E;",
    "color: #9C9890;",
    "color: #616161;",
);

/// One calculation on the history tape.
pub const TAPE_ENTRY: &str = themed!(
    "display: flex; flex-direction: column; padding: 6px 10px; cursor: pointer; \
     border-bottom: 1px solid transparent;",
    "border-bottom-color: #E4E9EF; :hover { background: #FDF3CE; }",
    "border-bottom-color: #353535; :hover { background: #3A3424; }",
    "border-bottom-color: #E0DDD7; :hover { background: #F8F7F3; }",
    "border-bottom-color: #2E2E2E; :hover { background: #2B2B2B; }",
);

/// A tape entry's expression.
pub const TAPE_EXPR: &str = themed!(
    "text-align: right; font-size: 12px; overflow-wrap: anywhere;",
    "color: #626A76;",
    "color: #9E9E9E;",
    "color: #66645C; font-size: 14px; font-family: EB Garamond, Georgia, Times New Roman, serif;",
    "color: #A8A8A8;",
);

/// A tape entry's result.
pub const TAPE_RESULT: &str = themed!(
    "text-align: right; font-size: 18px; font-weight: 600; overflow-wrap: anywhere; \
     font-variant-numeric: tabular-nums lining-nums;",
    "color: #1E1E1E;",
    "color: #F2F2F2;",
    "color: #262521; font-size: 21px; font-weight: 500; font-family: EB Garamond, Georgia, Times New Roman, serif;",
    "color: #E7E7E7;",
);

/// The quiet text of an empty list.
pub const EMPTY_NOTE: &str = themed!(
    "padding: 12px; font-size: 13px;",
    "color: #858C97;",
    "color: #7D7D7D;",
    "color: #827F76; font-size: 15px; font-style: italic; font-family: EB Garamond, Georgia, Times New Roman, serif;",
    "color: #8C8C8C;",
);

// ==== The graph ====

/// The plot: paper under the grid (the pointer pans and zooms it).
pub const PLOT: &str = themed!(
    "position: relative; flex-grow: 1; min-width: 0px; min-height: 0px; overflow: hidden; \
     border-radius: 3px; cursor: move;",
    "background: #FFFFFF; border: 1px solid #C2C9D2;",
    "background: #1B1B1B; border: 1px solid #4D4D4D;",
    "background: #FBFAF6; border: 1px solid #C6C3BB; \
     box-shadow: inset 0px 1px 2px rgba(48, 45, 38, 0.08);",
    "background: #1D1D1D; border: 1px solid #3F3F3F;",
);

/// A minor grid line.
pub const GRID_MINOR: &str = themed!(
    "position: absolute;",
    "background: #F0F3F7;",
    "background: #262626;",
    "background: #EFEDE7;",
    "background: #252525;",
);

/// A major grid line (at a labelled tick).
pub const GRID_MAJOR: &str = themed!(
    "position: absolute;",
    "background: #DCE2EA;",
    "background: #343434;",
    "background: #DCD9D2;",
    "background: #323232;",
);

/// The axes through the origin.
pub const AXIS: &str = themed!(
    "position: absolute;",
    "background: #6F7A88;",
    "background: #8A8A8A;",
    "background: #74716A;",
    "background: #8C8C8C;",
);

/// A tick's number beside an axis.
pub const TICK: &str = themed!(
    "position: absolute; font-size: 11px; white-space: nowrap; \
     font-variant-numeric: tabular-nums lining-nums;",
    "color: #626A76;",
    "color: #9E9E9E;",
    "color: #66645C; font-size: 13px; font-family: EB Garamond, Georgia, Times New Roman, serif;",
    "color: #A8A8A8;",
);

/// A function in the graph's list.
pub const FUNCTION_ROW: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; padding: 5px 6px 5px 10px; \
     min-height: 30px; border-bottom: 1px solid transparent;",
    "border-bottom-color: #E4E9EF;",
    "border-bottom-color: #353535;",
    "border-bottom-color: #E0DDD7;",
    "border-bottom-color: #2E2E2E;",
);

/// A function's name (`y₁ =`).
pub const FUNCTION_NAME: &str = themed!(
    "font-size: 13px; margin-right: 6px; flex-shrink: 0; font-style: italic;",
    "color: #3A3F47;",
    "color: #CFCFCF;",
    "color: #4E4C45; font-size: 16px; font-family: EB Garamond, Georgia, Times New Roman, serif;",
    "color: #BCBCBC;",
);

/// The small remove button of a function row.
pub const FUNCTION_REMOVE: &str = themed!(
    "width: 22px; height: 22px; flex-shrink: 0; display: flex; align-items: center; \
     justify-content: center; border-radius: 3px; cursor: pointer; font-size: 14px; \
     border: 1px solid transparent;",
    "color: #858C97; :hover { background: #FDE9A9; border-color: #E5C365; color: #1E1E1E; }",
    "color: #7D7D7D; :hover { background: #524526; border-color: #9E7E34; color: #F2F2F2; }",
    "color: #827F76; :hover { background: #F8F7F2; border-color: #9C9890; color: #262521; }",
    "color: #8C8C8C; :hover { background: #393939; border-color: #616161; color: #E7E7E7; }",
);

/// A small command in a panel's title row (Reset view, Clear).
pub const SMALL_BUTTON: &str = themed!(
    "display: flex; align-items: center; justify-content: center; padding: 2px 8px; \
     margin-left: 4px; border-radius: 3px; cursor: pointer; font-size: 11px; font-weight: 400; \
     text-transform: none; letter-spacing: 0px; white-space: nowrap; user-select: none;",
    "background: linear-gradient(to bottom, #FFFFFF, #E9EDF2); border: 1px solid #C2C9D2; \
     color: #1E1E1E; :hover { background: #FDE9A9; border-color: #E5C365; } \
     :active { background: #F9CE6D; border-color: #C2913C; }",
    "background: linear-gradient(to bottom, #454545, #3A3A3A); border: 1px solid #575757; \
     color: #F2F2F2; :hover { background: #524526; border-color: #9E7E34; } \
     :active { background: #3A2F12; border-color: #B88C3A; }",
    "background: linear-gradient(to bottom, #FAF9F5, #ECEAE4); border: 1px solid #B4B1A9; \
     color: #262521; font-size: 12px; letter-spacing: 0.5px; \
     box-shadow: 0px 1px 1px rgba(48, 45, 38, 0.10); \
     font-family: EB Garamond, Georgia, Times New Roman, serif; \
     :hover { background: #F8F7F2; border-color: #9C9890; } \
     :active { background: #E7E4DD; border-color: #9C9890; }",
    "background: linear-gradient(to bottom, #333333, #292929); border: 1px solid #4A4A4A; \
     color: #E7E7E7; box-shadow: 0px 1px 1px rgba(0, 0, 0, 0.5); \
     :hover { background: #393939; border-color: #616161; } \
     :active { background: #222222; border-color: #616161; }",
);

/// The quiet name of the view the window's size picked (beside the View menu).
pub const VIEW_BADGE: &str = themed!(
    "font-size: 11px; margin-left: 8px; white-space: nowrap; text-transform: uppercase; \
     letter-spacing: 0.5px;",
    "color: #858C97;",
    "color: #7D7D7D;",
    "color: #827F76; font-size: 12px; letter-spacing: 1.5px; \
     font-family: EB Garamond, Georgia, Times New Roman, serif;",
    "color: #8C8C8C;",
);

/// The graph's footnote (radians, how to zoom).
pub const GRAPH_NOTE: &str = themed!(
    "font-size: 11px; padding: 6px 10px 8px 10px;",
    "color: #858C97;",
    "color: #7D7D7D;",
    "color: #827F76; font-size: 13px; font-style: italic; font-family: EB Garamond, Georgia, Times New Roman, serif;",
    "color: #8C8C8C;",
);

/// The panels' entrances when the window grows into a wider layout.
pub const KEYFRAMES: &str = "\
@keyframes azc-slide-in { from { opacity: 0; transform: translateX(-14px); } to { opacity: 1; transform: translateX(0px); } }
@keyframes azc-rise-in { from { opacity: 0; transform: translateY(18px); } to { opacity: 1; transform: translateY(0px); } }
@keyframes azc-fade-in { from { opacity: 0; } to { opacity: 1; } }
";

/// A side panel slides in from the keypad's side.
pub const ENTER_SIDE: &str = "-azul-animation-in: azc-slide-in 180ms ease-out;";
/// The graph rises in from the bottom.
pub const ENTER_GRAPH: &str = "-azul-animation-in: azc-rise-in 220ms ease-out;";
/// A panel that swaps in place (the history over the keys) fades in.
pub const ENTER_FADE: &str = "-azul-animation-in: azc-fade-in 140ms ease-out;";

// ==== Colours a style string cannot carry (the SVG's strokes) ====

/// The theme and mode the window is built for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    pub flora: bool,
    pub dark: bool,
}

/// The curves' colours, in order: told apart by hue and by lightness, on
/// paper and at night.
const CURVES_LIGHT: [&str; 8] = [
    "#2A63B8", "#C8531E", "#2E8B57", "#8E44AD", "#C0392B", "#138D90", "#B7950B", "#6D4C41",
];
const CURVES_DARK: [&str; 8] = [
    "#7FB0EA", "#F0955A", "#6FCF97", "#C39BD3", "#F1948A", "#48C9B0", "#F4D03F", "#BCAAA4",
];

impl Look {
    /// The colour of the graph's function `i` (y₁ is 0).
    #[must_use]
    pub fn curve(self, i: usize) -> &'static str {
        let set = if self.dark { &CURVES_DARK } else { &CURVES_LIGHT };
        set[i % set.len()]
    }

    /// The colour of the entry being typed, drawn before it is committed.
    #[must_use]
    pub fn draft(self) -> &'static str {
        match (self.flora, self.dark) {
            (false, false) => "#626A76",
            (false, true) => "#B0B0B0",
            (true, false) => "#66645C",
            (true, true) => "#A8A8A8",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_part_carries_four_looks() {
        for part in [DISPLAY, KEY_DIGIT, KEY_OP, KEY_FUNC, KEY_EQUALS, PANEL, PLOT] {
            assert!(part.contains("@media (prefers-color-scheme: dark)"), "{part}");
            assert!(part.contains("@theme(flora)"), "{part}");
        }
        assert!(KEY_DIGIT.contains("EB Garamond"), "flora's figures are Garamond");
        assert!(RESULT_LINE.contains("tabular-nums"));
    }

    #[test]
    fn states_paint_solid_backgrounds() {
        // A `:hover` block reaches the hovered children too: a gradient there
        // would restart on the label's box (see the module's doc).
        for part in [KEY_DIGIT, KEY_OP, KEY_FUNC, KEY_EQUALS, KEY_MEM] {
            for block in part.split(":hover {").skip(1).chain(part.split(":active {").skip(1)) {
                let state = &block[..block.find('}').unwrap()];
                assert!(!state.contains("gradient"), "{state}");
                assert!(!state.contains("box-shadow"), "{state}");
            }
        }
    }

    #[test]
    fn curves_take_their_colours_in_order() {
        let day = Look { flora: false, dark: false };
        assert_eq!(day.curve(0), "#2A63B8");
        assert_eq!(day.curve(8), day.curve(0));
        let night = Look { flora: true, dark: true };
        assert_ne!(night.curve(0), day.curve(0));
    }
}
