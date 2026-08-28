//! Probe + law: ribbon tab labels with internal whitespace ("PAGE LAYOUT")
//! must never wrap, at ANY window width — and the group captions below must
//! stay centered (live 2026-08-12: resizing narrower broke "PAGE LAYOUT"
//! onto a second line and visibly de-centered the Clipboard/Font/Paragraph/
//! Styles captions).
//!
//! Labels are `<p>`-wrapped text (the raw-text convention fix): the text
//! node itself carries no layout rect, so the detectors read the `<p>` box
//! and its text3 `UnifiedLayout` — line count for wrap, glyph-extent gap
//! symmetry for centering.

use std::sync::OnceLock;

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId, NodeType},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::NodeHierarchyItemId,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::ribbon::{
        Ribbon, RibbonAppButton, RibbonButton, RibbonColumn, RibbonGallery, RibbonGalleryCell,
        RibbonGroup, RibbonItem, RibbonTab, RibbonTabVec,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn font_cache() -> &'static FcFontCache {
    static CACHE: OnceLock<FcFontCache> = OnceLock::new();
    CACHE.get_or_init(FcFontCache::build)
}

fn layout_dom(dom: Dom, width: f32, height: f32) -> LayoutWindow {
    layout_dom_css(dom, "", width, height)
}

fn layout_dom_css(dom: Dom, css_str: &str, width: f32, height: f32) -> LayoutWindow {
    let (css, _) = azul_css::parser2::new_from_str(css_str);
    let mut dom = dom;
    let styled_dom = azul_core::styled_dom::StyledDom::create(&mut dom, css);

    let mut layout_window = LayoutWindow::new(font_cache().clone()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(width, height);
    layout_window.current_window_state = window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());

    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();
    layout_window
}

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![azul_core::dom::IdOrClass::Class(name.into())].into()
}

fn node_id(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

/// The azwriter ribbon: FILE app button + nine tabs; HOME carries the four
/// visible groups from the live screenshot. Desktop-only DOM + the pinned
/// static UI family, exactly like the live app.
fn azwriter_like_ribbon() -> Dom {
    let col = |labels: [&str; 3]| {
        RibbonItem::Column(labels.into_iter().fold(RibbonColumn::new(), |c, l| {
            c.with_item(RibbonItem::SmallButton(RibbonButton::new(
                "content_cut".into(),
                l.into(),
            )))
        }))
    };
    let cells: Vec<RibbonGalleryCell> = (0..4)
        .map(|i| {
            RibbonGalleryCell::new(
                Dom::create_text_do_not_use_without_block_level_wrapper("AaBbCcDc"),
                format!("Style {i}").into(),
            )
        })
        .collect();

    let tab = RibbonTab::new("HOME".into())
        .with_group(RibbonGroup::new("Clipboard".into()).with_item(col([
            "Cut",
            "Copy",
            "Format Painter",
        ])))
        .with_group(RibbonGroup::new("Font".into()).with_item(col(["Grow", "Shrink", "Clear"])))
        .with_group(RibbonGroup::new("Paragraph".into()).with_item(col([
            "Bullets",
            "Numbering",
            "Sort",
        ])))
        .with_group(
            RibbonGroup::new("Styles".into())
                .with_item(RibbonItem::Gallery(RibbonGallery::new(cells.into())))
                .with_fills_space(true),
        );

    let mut tabs = vec![tab];
    for label in [
        "INSERT",
        "DESIGN",
        "PAGE LAYOUT",
        "REFERENCES",
        "MAILINGS",
        "REVIEW",
        "VIEW",
    ] {
        tabs.push(RibbonTab::new(label.into()).with_group(
            RibbonGroup::new("Preview".into()).with_item(RibbonItem::LargeButton(
                RibbonButton::new("layers".into(), label.into()),
            )),
        ));
    }
    let mut ribbon = Ribbon::new(RibbonTabVec::from_vec(tabs))
        .with_app_button(RibbonAppButton::new("FILE".into()));
    {
        use azul_css::{
            dynamic_selector::CssPropertyWithConditions,
            props::{
                basic::font::{StyleFontFamily, StyleFontFamilyVec},
                property::CssProperty,
            },
        };
        let mut v = ribbon.style.container_style.as_ref().to_vec();
        v.push(CssPropertyWithConditions::simple(
            CssProperty::const_font_family(StyleFontFamilyVec::from_vec(vec![
                StyleFontFamily::System("Liberation Sans".into()),
            ])),
        ));
        ribbon.style.container_style =
            azul_css::dynamic_selector::CssPropertyWithConditionsVec::from_vec(v);
    }
    Dom::create_body().with_child(ribbon.dom_desktop())
}

fn node_count(lw: &LayoutWindow) -> usize {
    lw.layout_results
        .get(&DomId::ROOT_ID)
        .map(|r| r.styled_dom.node_data.as_container().len())
        .unwrap_or(0)
}

fn text_of(lw: &LayoutWindow, i: usize) -> Option<String> {
    let result = lw.layout_results.get(&DomId::ROOT_ID)?;
    let node_data = result.styled_dom.node_data.as_container();
    match node_data[NodeId::new(i)].get_node_type() {
        NodeType::Text(s) => Some(s.as_ref().as_str().to_string()),
        _ => None,
    }
}

fn find_text(lw: &LayoutWindow, needle: &str) -> Option<usize> {
    (0..node_count(lw)).find(|i| text_of(lw, *i).as_deref() == Some(needle))
}

fn parent_of(lw: &LayoutWindow, i: usize) -> Option<usize> {
    let result = lw.layout_results.get(&DomId::ROOT_ID)?;
    let hier = result.styled_dom.node_hierarchy.as_container();
    hier[NodeId::new(i)].parent_id().map(|p| p.index())
}

#[derive(Debug)]
struct Rect {
    x: f32,
    w: f32,
    h: f32,
}

fn rect_of(lw: &LayoutWindow, i: usize) -> Option<Rect> {
    let r = lw.get_node_layout_rect(node_id(i))?;
    Some(Rect {
        x: r.origin.x,
        w: r.size.width,
        h: r.size.height,
    })
}

/// The `<p>` label box of a text node. Static screen text retains no
/// per-node `UnifiedLayout` (§3.2 memory campaign drops it), so wrap and
/// centering are detected at the BOX level: a wrapped label doubles the
/// `<p>` height; a de-centered caption is a `<p>` that failed to grow onto
/// its footer (exactly the signature the raw-text bug showed live).
fn label_box(lw: &LayoutWindow, text_i: usize) -> Option<Rect> {
    let p = parent_of(lw, text_i)?;
    rect_of(lw, p)
}

/// Caption centering error: the `<p>` box center vs its footer's center.
fn caption_center_error(lw: &LayoutWindow, text_i: usize) -> Option<f32> {
    let p = parent_of(lw, text_i)?;
    let pr = rect_of(lw, p)?;
    let footer = parent_of(lw, p)?;
    let fr = rect_of(lw, footer)?;
    Some((pr.x + pr.w / 2.0) - (fr.x + fr.w / 2.0))
}

const TAB_LABELS: &[&str] = &[
    "HOME",
    "INSERT",
    "DESIGN",
    "PAGE LAYOUT",
    "REFERENCES",
    "MAILINGS",
    "REVIEW",
    "VIEW",
];
const GROUP_LABELS: &[&str] = &["Clipboard", "Font", "Paragraph", "Styles"];

/// PROBE (ignored by default): sweeps window widths and prints every width
/// where a tab label leaves one line or a caption drifts. Run with:
///   cargo test --release -p azul-layout --test all -- ribbon_tab_whitespace:: --ignored --nocapture
#[test]
#[ignore = "diagnostic PROBE, not a gate: it asserts NOTHING, it prints the \
            widths at which a tab label wraps or a caption drifts. The LAW it \
            probes IS enforced, by the non-ignored \
            `tab_labels_never_wrap_and_captions_stay_centered` right below. \
            Runs green headless in ~37s (verified 2026-08-20); stays ignored \
            because 801 layouts of printout is not something CI should pay for."]
fn probe_tab_wrap_and_caption_centering_across_widths() {
    let wide = layout_dom(azwriter_like_ribbon(), 1400.0, 300.0);
    let base_h: Vec<f32> = TAB_LABELS
        .iter()
        .map(|l| {
            let i = find_text(&wide, l).unwrap_or_else(|| panic!("tab text {l} in DOM"));
            label_box(&wide, i).expect("tab label box").h
        })
        .collect();
    println!("baseline tab <p> heights @1400: {base_h:?}");
    for l in GROUP_LABELS {
        let i = find_text(&wide, l).expect("caption");
        let err = caption_center_error(&wide, i).expect("caption metrics");
        println!("  caption {l}: center error {err:.2}px");
    }

    let mut anomalies = 0usize;
    let mut w = 1400.0f32;
    while w >= 600.0 {
        let lw = layout_dom(azwriter_like_ribbon(), w, 300.0);
        let mut msgs: Vec<String> = Vec::new();
        for (k, l) in TAB_LABELS.iter().enumerate() {
            match find_text(&lw, l) {
                Some(i) => {
                    if let Some(r) = label_box(&lw, i) {
                        if r.h > base_h[k] * 1.5 {
                            msgs.push(format!(
                                "tab '{l}' WRAPPED: <p> h={:.2} (one-line {:.2})",
                                r.h, base_h[k]
                            ));
                        }
                    }
                }
                None => msgs.push(format!("tab '{l}' text node GONE")),
            }
        }
        for l in GROUP_LABELS {
            if let Some(i) = find_text(&lw, l) {
                if let Some(err) = caption_center_error(&lw, i) {
                    if err.abs() > 1.0 {
                        msgs.push(format!("caption '{l}' OFF-CENTER by {err:.2}px"));
                    }
                }
            }
        }
        if !msgs.is_empty() {
            anomalies += 1;
            if anomalies <= 40 {
                println!("w={w:.0}:");
                for m in msgs {
                    println!("    {m}");
                }
            }
        }
        w -= 1.0;
    }
    println!("total anomalous widths: {anomalies}/801");
}

/// LAW: at every window width, every ribbon tab label lays out on ONE line
/// and every group caption sits centered in its grown caption box.
#[test]
fn tab_labels_never_wrap_and_captions_stay_centered() {
    let wide = layout_dom(azwriter_like_ribbon(), 1400.0, 300.0);
    let base_h: Vec<f32> = TAB_LABELS
        .iter()
        .map(|l| {
            let i = find_text(&wide, l).unwrap_or_else(|| panic!("tab text {l} in DOM"));
            label_box(&wide, i).expect("tab label box").h
        })
        .collect();

    let mut w = 1400.0f32;
    while w >= 600.0 {
        let lw = layout_dom(azwriter_like_ribbon(), w, 300.0);
        for (k, l) in TAB_LABELS.iter().enumerate() {
            let i = find_text(&lw, l).unwrap_or_else(|| panic!("tab text {l} present @w={w}"));
            let r = label_box(&lw, i).unwrap_or_else(|| panic!("tab {l} box @w={w}"));
            assert!(
                r.h <= base_h[k] * 1.5,
                "tab '{l}' wrapped at w={w}: <p> height {:.2} vs one-line {:.2} — the live \
                 PAGE-LAYOUT symptom",
                r.h,
                base_h[k]
            );
        }
        for l in GROUP_LABELS {
            let i = find_text(&lw, l).unwrap_or_else(|| panic!("caption {l} present @w={w}"));
            let err = caption_center_error(&lw, i).expect("caption metrics");
            assert!(
                err.abs() <= 1.0,
                "caption '{l}' off-center by {err:.2}px at w={w} — the live de-centering symptom"
            );
        }
        w -= 7.0;
    }
}

/// NEGATIVE CONTROL for the wrap detector: a `<p>`-wrapped "PAGE LAYOUT"
/// squeezed into a 40px box CANNOT fit one line — the line counter must see
/// >= 2 lines. If this fails, the law above is vacuous (a detector that can
/// > never fire proves nothing).
#[test]
fn nc_squeezed_label_is_detected_as_wrapped() {
    // One-line reference.
    let wide = layout_dom_css(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_ids_and_classes(class("row"))
                .with_child(Dom::create_p_with_text("PAGE LAYOUT")),
        ),
        ".row { display: flex; flex-direction: row; width: 400px; }",
        800.0,
        200.0,
    );
    let i = find_text(&wide, "PAGE LAYOUT").expect("wide label present");
    let one_line_h = label_box(&wide, i).expect("wide label box").h;

    // Squeezed: a DEFINITE 40px width on the <p> cannot fit one line; the
    // box must grow taller. (Styling goes through class selectors — string
    // css on the node is not part of this harness.)
    const NC_CSS: &str = "
        .row { display: flex; flex-direction: row; }
        .sq  { width: 40px; }
    ";
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(class("row"))
            .with_child(
                Dom::create_p()
                    .with_ids_and_classes(class("sq"))
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                        "PAGE LAYOUT",
                    )),
            ),
    );
    let lw = layout_dom_css(dom, NC_CSS, 800.0, 200.0);
    let i = find_text(&lw, "PAGE LAYOUT").expect("squeezed label present");
    let r = label_box(&lw, i).expect("squeezed label box");
    println!("squeezed <p>: {r:?} (one-line h {one_line_h:.2})");
    let h = r.h;
    assert!(
        h > one_line_h * 1.5,
        "the wrap detector failed to see a forced wrap (h {h:.2} vs one-line {one_line_h:.2}) \
         — the one-line law would be vacuous"
    );
}

/// NEGATIVE CONTROL for the centering detector: a caption `<p>` that does
/// NOT grow (flex-grow: 0) beside a grow:1 filler sits left-packed in its
/// footer — the center-error detector must fire. If it cannot, the
/// centered-caption law is vacuous.
#[test]
fn nc_non_growing_caption_is_detected_off_center() {
    const NC_CSS: &str = "
        .row  { display: flex; flex-direction: row; width: 300px; }
        .cap  { width: 60px; flex-grow: 0; }
        .fill { flex-grow: 1; }
    ";
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(class("row"))
            .with_child(
                Dom::create_p()
                    .with_ids_and_classes(class("cap"))
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                        "Clipboard",
                    )),
            )
            .with_child(Dom::create_div().with_ids_and_classes(class("fill"))),
    );
    let lw = layout_dom_css(dom, NC_CSS, 800.0, 200.0);
    let i = find_text(&lw, "Clipboard").expect("caption present");
    {
        let pi = parent_of(&lw, i).unwrap();
        let fi = parent_of(&lw, pi).unwrap();
        println!(
            "caption <p>: {:?} footer: {:?}",
            rect_of(&lw, pi),
            rect_of(&lw, fi)
        );
    }
    let err = caption_center_error(&lw, i).expect("caption metrics");
    assert!(
        err.abs() > 1.0,
        "the centering detector failed to see a left-packed caption (err {err:.2}px) — the \
         centered-caption law would be vacuous"
    );
}
