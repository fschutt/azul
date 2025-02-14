use crate::app;
///! This module encapsulates the different "event actions" that were formerly
///! triggered by Windows messages such as `AZ_REGENERATE_DOM`, `AZ_REDO_HIT_TEST`,
///! and so on.
///
///! Instead of sending `PostMessageW(...)`, we call these event functions directly.
///! This way, the same logic is reusable for both Win32 and macOS.

use crate::wr_translate::{
    generate_frame, rebuild_display_list, wr_synchronize_updated_images, wr_translate_document_id
};
use azul_core::app_resources::ResourceUpdate;
use azul_core::callbacks::LayoutCallbackInfo;
use azul_core::window_state::NodesToCheck;
use azul_core::window::{RawWindowHandle, WindowId};
use super::appkit::GlContextGuard;
use super::{AZ_TICK_REGENERATE_DOM, AZ_THREAD_TICK};

#[cfg(target_os = "macos")]
use crate::shell::appkit::Window;
#[cfg(target_os = "macos")]
use crate::shell::appkit::AppData;

#[cfg(target_os = "windows")]
use crate::shell::win32::Window;
#[cfg(target_os = "windows")]
use crate::shell::win32::AppData;

/// Regenerate the entire DOM (style, layout, etc.).
/// On Win32, this was triggered by `AZ_REGENERATE_DOM`.
pub fn regenerate_dom(window: &mut Window, appdata: &mut AppData, _guard: &GlContextGuard) {

    // 2) Re-build the styled DOM (layout callback).
    //    This used to happen in the `WM_TIMER` or `AZ_REGENERATE_DOM` handler in Win32.
    let mut resource_updates = Vec::new();
    let mut ud = &mut appdata.userdata;
    let fc_cache = &mut ud.fc_cache;
    let dat = &mut ud.data;
    let image_cache = &ud.image_cache;

    {
        let hit_tester = window.render_api
            .request_hit_tester(wr_translate_document_id(window.internal.document_id))
            .resolve();

        let hit_tester_ref = &*hit_tester;
        let did = window.internal.document_id;

        fc_cache.apply_closure(|fc_cache| {
            window.internal.regenerate_styled_dom(
                dat,
                image_cache,
                &window.gl_context_ptr,
                &mut resource_updates,
                window.internal.get_dpi_scale_factor(),
                &crate::app::CALLBACKS, // your user callbacks
                fc_cache,
                azul_layout::do_the_relayout,
                // new hit-tester creation:
                |window_state, _, layout_results| {
                    crate::wr_translate::fullhittest_new_webrender(
                        &*hit_tester_ref,
                        did,
                        window_state.focused_node,
                        layout_results,
                        &window_state.mouse_state.cursor_position,
                        window_state.size.get_hidpi_factor(),
                    )
                },
            );
        });
    }

    // 3) Stop any timers associated with now-removed NodeIds.
    window.stop_timers_with_node_ids();

    // 4) Possibly update the menu bar (if your framework allows).
    window.update_menus();

    // 5) Rebuild display list after DOM update.
    rebuild_display_list(&mut window.internal, &mut window.render_api, image_cache, resource_updates);

    // 6) The new display list will produce a new hit-tester, schedule that.
    window.request_new_hit_tester();

    // 7) Since we've updated the entire DOM, we then want a new GPU render:
    generate_frame(&mut window.internal, &mut window.render_api, true);
    
    window.request_redraw(); // (if needed)

    // 8) Done. The caller might also queue a second message to "AZ_REDO_HIT_TEST",
    //    but in this design, we can do the hit test here or just call
    //    event::redo_hit_test() as a separate function if you prefer.
}

/// Rebuild the display-list for the *existing* DOM (no layout reflow).
/// On Win32, triggered by `AZ_REGENERATE_DISPLAY_LIST`.
pub fn rebuild_display_list_only(window: &mut Window, appdata: &mut AppData, _guard: &GlContextGuard) {

    let image_cache = &appdata.userdata.image_cache;

    // No new resources, we only re-send the existing display-list
    rebuild_display_list(&mut window.internal, &mut window.render_api, image_cache, vec![]);
    window.request_new_hit_tester(); // refresh the hit-tester
    generate_frame(&mut window.internal, &mut window.render_api, true);
    window.request_redraw();
}

/// Re-run the hit-test logic after the display list is up-to-date,
/// previously triggered by `AZ_REDO_HIT_TEST` on Win32.
pub fn redo_hit_test(window: &mut Window, appdata: &mut AppData, _guard: &GlContextGuard) {

    let new_hit_tester = window.render_api.request_hit_tester(
        crate::wr_translate::wr_translate_document_id(window.internal.document_id),
    );

    window.hit_tester = crate::wr_translate::AsyncHitTester::Requested(new_hit_tester);

    let hit = crate::wr_translate::fullhittest_new_webrender(
        &*window.hit_tester.resolve(),
        window.internal.document_id,
        window.internal.current_window_state.focused_node,
        &window.internal.layout_results,
        &window.internal.current_window_state.mouse_state.cursor_position,
        window.internal.current_window_state.size.get_hidpi_factor(),
    );
    window.internal.current_window_state.last_hit_test = hit;

    // Possibly re-render or check callbacks that depend on the new hittest
    window.request_redraw();
}

/// Rerun the "GPU scroll render" step, i.e. do any final re-draw calls needed.
/// On Win32, triggered by `AZ_GPU_SCROLL_RENDER`.
pub fn gpu_scroll_render(window: &mut Window, _appdata: &mut AppData, _guard: &GlContextGuard) {
    generate_frame(&mut window.internal, &mut window.render_api, false);
    window.request_redraw();
}

/// Called when the OS says "size changed" or "resized to new physical size",
/// merges logic from `WM_SIZE + the partial re-layout`.
pub fn do_resize(window: &mut Window, appdata: &mut AppData, new_width: u32, new_height: u32, _guard: &GlContextGuard) {

    let new_physical_size = azul_core::window::PhysicalSize {
        width: new_width,
        height: new_height,
    };

    // TODO: check if size is above / below a certain bounds to trigger a DOM_REFRESH event
    // (switching from desktop to mobile view)

    let glc = window.gl_context_ptr.clone();

    let resize = window.do_resize_impl(
        new_physical_size,
        &appdata.userdata.image_cache,
        &mut appdata.userdata.fc_cache,
        &glc,
    );

    if !resize.updated_images.is_empty() {
        let mut txn = webrender::Transaction::new();
        let did = wr_translate_document_id(window.internal.document_id);
        wr_synchronize_updated_images(resize.updated_images, &mut txn);
        window.render_api.send_transaction(did, txn);
    }

    // Rebuild display-list after resizing
    rebuild_display_list(&mut window.internal, &mut window.render_api, &appdata.userdata.image_cache, vec![]);
    window.request_new_hit_tester(); // Must re-request after size changed
    
    generate_frame(&mut window.internal, &mut window.render_api, true);
    window.request_redraw();
}

/// Called from your OS timer or thread events, merges logic from `WM_TIMER` + `AZ_THREAD_TICK`.
pub fn handle_timer_event(window: &mut Window, appdata: &mut AppData, timer_id: usize, guard: &GlContextGuard) {
    // 1) Possibly dispatch user timers
    // 2) Possibly handle "regenerate DOM" if it's the hot-reload timer
    // ...
    match timer_id {
        AZ_TICK_REGENERATE_DOM => {
            regenerate_dom(window, appdata, guard);
        },
        AZ_THREAD_TICK => {
            // process threads
        },
        // or custom user TimerId => do callback, etc.
        _ => { /* user timer logic */ }
    }
}
