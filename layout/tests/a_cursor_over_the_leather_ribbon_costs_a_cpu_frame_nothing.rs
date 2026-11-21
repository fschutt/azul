//! The CPU frame of a cursor moving over `AzDrive`'s leather ribbon, in its
//! window at 2x: it draws no material and damages nothing - the leather was
//! drawn once, and the sheen is a layer only a window WebRender draws has -
//! and it fits a time budget in a release build.
//!
//! Before, every cursor move eased the sheen into the material's own frame:
//! the CPU shader ran over the whole ribbon (2400 x 268 device px) per move
//! and the frame damaged all of it.

use std::time::Duration;

use azul_core::{
    dom::{Dom, DomId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
    window::CursorPosition,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{compute_display_list_damage, ScrollOffsetMap},
    solver3::display_list::DisplayListItem,
    texture::materials::renders_on_this_thread,
    widgets::{
        ribbon::{
            Ribbon, RibbonAppButton, RibbonButton, RibbonGroup, RibbonItem, RibbonTab,
            RibbonTabVec,
        },
        tabs::TabStyle,
        themes::UiTheme,
        titlebar::TabsInTitlebar,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The budget of one such frame on the UI thread, in the CPU time of the
/// thread that runs it, release builds only: the window walks its display
/// list for the background layers and finds nothing to draw.
const FRAME_BUDGET: Duration = Duration::from_millis(4);

/// `AzDrive`'s window: 1200 x 800 at 2x, the ribbon of tab buttons on the
/// leather in its title bar, its content open, and the file list under it.
fn azdrive_window() -> LayoutWindow {
    let group = |name: &str, commands: &[(&str, &str)]| {
        RibbonGroup::new(name.into()).with_items(
            commands
                .iter()
                .map(|(icon, label)| {
                    RibbonItem::LargeButton(RibbonButton::new((*icon).into(), (*label).into()))
                })
                .collect::<Vec<_>>()
                .into(),
        )
    };
    let home = RibbonTab::new("HOME".into())
        .with_group(group("Clipboard", &[("content_copy", "Copy"), ("content_paste", "Paste")]))
        .with_group(group("Organize", &[("drive_file_move", "Move to"), ("delete", "Delete")]))
        .with_group(group("New", &[("create_new_folder", "New folder")]));
    let ribbon = Ribbon::new(RibbonTabVec::from_vec(vec![
        home,
        RibbonTab::new("SHARE".into()),
        RibbonTab::new("VIEW".into()),
    ]))
    .with_app_button(RibbonAppButton::new("FILE".into()))
    .with_theme(UiTheme::Flora)
    .with_tab_style(TabStyle::Pills)
    .with_tabs_in_titlebar(TabsInTitlebar::create(0.0, 74.0, 0.0))
    .with_available_width(1200.0);
    let body = Dom::create_body()
        .with_css("margin: 0px; display: flex; flex-direction: column; height: 100%;")
        .with_child(ribbon.dom_desktop())
        .with_child(Dom::create_div().with_css("flex-grow: 1; background: white;"));

    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(1200.0, 800.0);
    ws.size.dpi = 192;
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(Dom::create_html().with_child(body)),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the window lays out");
    let _ = lw.prepare_frame_cpu();
    lw
}

/// The leather's frame in the display list: its rect.
fn leather(lw: &LayoutWindow) -> Option<LogicalRect> {
    lw.get_layout_result(&DomId::ROOT_ID)?
        .display_list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::Image { bounds, image, .. }
                if !image.is_callback() && bounds.inner().size.width > 1000.0 =>
            {
                Some(*bounds.inner())
            }
            _ => None,
        })
}

/// One CPU frame of the cursor at `(x, y)`: the animation step, the frame's
/// preparation and the damage against the frame before. Its cost on this
/// thread, and the damage.
fn cursor_frame(lw: &mut LayoutWindow, x: f32, y: f32) -> (Duration, Option<Vec<LogicalRect>>) {
    let before = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .display_list
        .clone();
    lw.current_window_state.mouse_state.cursor_position =
        CursorPosition::InWindow(LogicalPosition::new(x, y));
    let started = crate::thread_cpu_clock::CpuInstant::now();
    let _ = lw.step_material_sheen(0.016);
    let _ = lw.prepare_frame_cpu();
    let after = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .display_list
        .clone();
    let offsets = ScrollOffsetMap::new();
    let damage = compute_display_list_damage(&before, &after, &offsets, &offsets);
    (started.elapsed(), damage)
}

#[test]
fn a_cursor_over_the_leather_draws_nothing_and_damages_nothing_on_the_cpu() {
    let mut lw = azdrive_window();
    let rect = leather(&lw).expect("harness: the ribbon's leather is drawn");
    let renders = renders_on_this_thread();
    for i in 0..24 {
        #[allow(clippy::cast_precision_loss)] // a small step count
        let x = rect.origin.x + 40.0 + 45.0 * i as f32;
        let (_, damage) = cursor_frame(&mut lw, x, rect.origin.y + 60.0);
        assert_eq!(
            damage,
            Some(Vec::new()),
            "a cursor move over the leather at x {x} damaged the frame"
        );
    }
    assert_eq!(
        renders_on_this_thread() - renders,
        0,
        "the cursor drew the leather again"
    );
}

/// The worst of the cursor's frames fits the budget, in a release build.
#[test]
fn a_cursor_frame_over_the_leather_fits_its_time_budget_at_2x() {
    if cfg!(debug_assertions) {
        eprintln!("[leather_cursor_frame] debug build: the time budget is for release builds");
        return;
    }
    let mut lw = azdrive_window();
    let rect = leather(&lw).expect("harness: the ribbon's leather is drawn");
    let worst = crate::thread_cpu_clock::on_a_performance_core(|| {
        (0..24)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)] // a small step count
                let x = rect.origin.x + 40.0 + 45.0 * i as f32;
                cursor_frame(&mut lw, x, rect.origin.y + 60.0).0
            })
            .max()
            .expect("24 frames")
    });
    eprintln!("[leather_cursor_frame] the worst cursor frame over the leather at 2x: {worst:?}");
    // A budget in time needs the machine to itself: checked where the test
    // runs alone, `AZ_TIMING=1 cargo test --release -p azul-layout --test all
    // -- --test-threads=1 a_cursor_frame_over_the_leather`.
    if std::env::var_os("AZ_TIMING").is_none() {
        eprintln!("[leather_cursor_frame] AZ_TIMING unset: the budget is checked in the serial run");
        return;
    }
    assert!(
        worst <= FRAME_BUDGET,
        "a cursor frame over the leather took {worst:?} on the UI thread, over the \
         {FRAME_BUDGET:?} budget"
    );
}
