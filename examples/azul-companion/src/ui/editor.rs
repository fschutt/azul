//! The buttons' editor (screen D): the four quick buttons, each a short label and the instruction it stands for, kept in
//! AzReview's settings (the phone keeps nothing). The fields cut what is typed past their caps and say so.

use azul::{
    prelude::*,
    widgets::{TextArea, TextInput},
};
use azul_review_link::limits::{MAX_BUTTON_TEXT_CHARS, MAX_LABEL_CHARS};

use super::glue::{Field, FieldCtx, LABEL_CALLBACK, TEXT_CALLBACK};
use super::kit::{card, column, div, hairline, icon, px, row, tap, text, Cx};
use super::settings::header as nav_header;
use super::Action;
use crate::theme as th;

/// The count of a field in words (`4 / 12`), in the colour of a field that is full.
fn counter(cx: &Cx, have: usize, max: usize) -> Dom {
    let colour = if have >= max { th::WARN } else { th::TEXT_DIM };
    text(&cx.tf("editor-count", &[("n", &have.to_string()), ("max", &max.to_string())]), &format!("font-size: 12px; color: {colour}; white-space: nowrap;"))
}

/// What a field looks like: as wide as its card, a rounded box a little lighter than the card.
fn field_css(height: f32) -> String {
    format!(
        "box-sizing: border-box; width: 100%; height: {}; padding: 8px 12px; border-radius: 10px; border: 1px solid {}; background: {}; \
         color: {}; font-size: 16px;",
        px(height),
        th::SEPARATOR,
        th::SURFACE_HI,
        th::TEXT
    )
}

fn slot_card(cx: &Cx, i: usize, width: f32) -> Dom {
    let ui = &cx.root.ui;
    let (label, instruction) = &ui.drafts[i];
    let n = (i + 1).to_string();
    let focus = ui.editor_focus == Some(i);
    let mut head = row("gap: 10px; width: 100%;")
        .with_child(
            div(&format!(
                "width: 28px; height: 28px; border-radius: 14px; background: {}; display: flex; align-items: center; justify-content: center; flex-shrink: 0;",
                th::button_colour(i)
            ))
            .with_child(text(&n, "font-size: 14px; font-weight: 700; color: #ffffff;")),
        )
        .with_child(text(&cx.tf("editor-slot", &[("n", &n)]), &format!("font-size: 13px; font-weight: 600; color: {}; flex-grow: 1;", th::TEXT_DIM)));
    head.add_child(counter(cx, label.chars().count(), MAX_LABEL_CHARS));
    let label_field = TextInput::create()
        .with_text(label.as_str())
        .with_placeholder(cx.t("editor-label").as_str())
        .with_accessibility_name(cx.tf("editor-label-name", &[("n", &n)]).as_str())
        .with_on_text_input(RefAny::new(FieldCtx { root: cx.data.clone(), slot: i, field: Field::Label }), LABEL_CALLBACK)
        .dom()
        .with_css(field_css(44.0).as_str());
    let text_field = TextArea::create()
        .with_text(instruction.as_str())
        .with_placeholder(cx.t("editor-text").as_str())
        .with_accessibility_name(cx.tf("editor-text-name", &[("n", &n)]).as_str())
        .with_on_text_input(RefAny::new(FieldCtx { root: cx.data.clone(), slot: i, field: Field::Instruction }), TEXT_CALLBACK)
        .dom()
        .with_css(field_css(96.0).as_str());
    let mut foot = row("gap: 8px; width: 100%; justify-content: flex-end; min-height: 16px;");
    if ui.draft_cut[i].0.is_some() {
        foot.add_child(text(&cx.tf("editor-capped", &[("max", &MAX_LABEL_CHARS.to_string())]), &format!("font-size: 12px; color: {}; flex-grow: 1;", th::WARN)));
    }
    if ui.draft_cut[i].1.is_some() {
        foot.add_child(text(&cx.tf("editor-capped", &[("max", &MAX_BUTTON_TEXT_CHARS.to_string())]), &format!("font-size: 12px; color: {}; flex-grow: 1;", th::WARN)));
    }
    foot.add_child(counter(cx, instruction.chars().count(), MAX_BUTTON_TEXT_CHARS));
    card(&format!("width: {}; padding: 12px; gap: 10px; border: 1px solid {}; flex-shrink: 0;", px(width), if focus { th::ACCENT } else { th::SEPARATOR }))
        .with_child(head)
        .with_child(div("width: 100%; height: 44px;").with_child(label_field))
        .with_child(div("width: 100%; height: 96px;").with_child(text_field))
        .with_child(foot)
}

/// The editor screen.
#[must_use]
pub fn dom(cx: &Cx) -> Dom {
    let z = &cx.z;
    let landscape = cx.env.landscape();
    let head_h = if landscape { 48.0 } else { z.header.h };
    let gutter = z.gutter;
    let mut header = nav_header(cx, &cx.t("editor-title"), Action::Back, &cx.t("nav-close"));
    let save_bg = if cx.root.ui.editor_dirty { th::ACCENT } else { th::SURFACE_HI };
    header.add_child(
        tap(&cx.t("editor-save"), &format!("height: 44px; min-width: 44px; padding: 0px 18px; border-radius: 22px; background: {save_bg};"), cx, Action::EditorSave)
            .with_child(text(&cx.t("editor-save"), &format!("font-size: 15px; font-weight: 600; color: {};", th::ON_ACCENT))),
    );
    let inner_w = z.safe.w - 2.0 * gutter;
    let card_w = if landscape { (inner_w - 12.0) / 2.0 } else { inner_w };
    let mut cards = row("flex-wrap: wrap; align-items: flex-start; gap: 12px; width: 100%;");
    for i in 0..4 {
        cards.add_child(slot_card(cx, i, card_w));
    }
    let footer = column("gap: 12px; width: 100%; padding-top: 4px;")
        .with_child(
            tap(&cx.t("editor-reset"), "height: 48px; padding: 0px 14px; flex-direction: row; gap: 8px; align-self: flex-start; border-radius: 12px;", cx, Action::EditorReset)
                .with_child(icon("restart_alt", 22.0, th::DANGER))
                .with_child(text(&cx.t("editor-reset"), &format!("font-size: 16px; font-weight: 600; color: {};", th::DANGER))),
        )
        .with_child(text(&cx.t("editor-hint"), &format!("font-size: 13px; color: {}; line-height: 1.35;", th::TEXT_DIM)));
    let _ = hairline;
    column(&format!("width: {}; height: {};", px(z.safe.w), px(z.safe.h)))
        .with_child(header)
        .with_child(
            column(&format!(
                "width: {}; height: {}; overflow-y: auto; padding: 14px {} 24px {}; gap: 12px;",
                px(z.safe.w),
                px((z.safe.h - head_h).max(0.0)),
                px(gutter),
                px(gutter)
            ))
            .with_class("az-scroll")
            .with_child(cards)
            .with_child(footer),
        )
}
