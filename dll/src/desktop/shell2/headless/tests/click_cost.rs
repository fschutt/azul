//! A click that rebuilds the page lays it out once.
//!
//! The click is three pointer events (move, press, release). Only the release's
//! callback changes anything: it asks for a rebuild, and that rebuild is the one
//! layout pass the click may cost. A pointer event that changed nothing paints the
//! layout as it stands; laying the window out for each of them made every click
//! three or four whole-window layouts (AzWidgets: ~17 ms each, beside the rebuild).

use core::sync::atomic::{AtomicUsize, Ordering};

use super::*;

/// How often the button was clicked; the label shows it. The app state and the
/// button's callback data share the counter.
#[derive(Debug, Clone, Default)]
struct ClickPage {
    clicks: Arc<AtomicUsize>,
}

extern "C" fn count_click(
    mut data: RefAny,
    _info: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    if let Some(page) = data.downcast_ref::<ClickPage>() {
        page.clicks.fetch_add(1, Ordering::SeqCst);
    }
    azul_core::callbacks::Update::RefreshDom
}

/// `body > [button (MouseUp: count, RefreshDom), label, 40 paragraphs]`, the
/// button 200 x 40 at the top left.
extern "C" fn click_page_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        events::{EventFilter, HoverEventFilter},
    };

    let page = data
        .downcast_ref::<ClickPage>()
        .map(|p| (*p).clone())
        .unwrap_or_default();
    let clicks = page.clicks.load(Ordering::SeqCst);
    let mut body = Dom::create_body()
        .with_css("margin: 0;")
        .with_child(
            Dom::create_div()
                .with_css("width: 200px; height: 40px;")
                .with_callbacks(
                    vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseUp),
                        callback: CoreCallback {
                            cb: count_click as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: RefAny::new(page),
                    }]
                    .into(),
                ),
        )
        .with_child(Dom::create_p_with_text(
            format!("Clicked {clicks} times").as_str(),
        ));
    for i in 0..40 {
        body = body.with_child(Dom::create_p_with_text(format!("Paragraph {i}").as_str()));
    }
    body
}

#[test]
fn a_click_that_rebuilds_the_page_lays_it_out_once() {
    let state = Arc::new(RefCell::new(RefAny::new(ClickPage::default())));
    let mut window = make_window_sized(&state, click_page_layout, 400.0, 600.0);
    window.regenerate_layout().expect("initial layout");
    window.regenerate_layout().expect("settle");
    let _ = window.common.take_regeneration();
    if let Some(lw) = window.common.layout_window.as_mut() {
        lw.frame_report.reset_counters();
    }

    let _ = click_at(&mut window, 100.0, 20.0);

    let clicks = {
        let mut g = state.borrow_mut();
        let r: &mut RefAny = &mut g;
        r.downcast_ref::<ClickPage>()
            .map_or(0, |p| p.clicks.load(Ordering::SeqCst))
    };
    assert_eq!(
        clicks, 1,
        "harness: the release ran the button's callback once"
    );
    let report = window
        .common
        .layout_window
        .as_ref()
        .expect("layout window")
        .frame_report
        .clone();
    assert_eq!(
        report.dom_regenerations, 1,
        "the click rebuilt the page once"
    );
    assert_eq!(
        report.layout_passes, 1,
        "the click laid the page out once: the rebuild's pass, no pass for the pointer \
         events around it"
    );
}
