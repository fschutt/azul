use azul_core::{
    callbacks::{RefAny, Update},
    events::{Events, NodesToCheck, ProcessEventResult},
    resources::{AppConfig, ImageCache},
    styled_dom::DomId,
    window::{RawWindowHandle, WindowId},
};
use azul_layout::{
    callbacks::CallCallbacksResult,
    solver3::LayoutResult,
    window_state::{FullWindowState, WindowCreateOptions},
    // TODO: CallbacksOfHitTest, StyleAndLayoutChanges need to be ported from
    // REFACTORING/portedfromcore.rs
};
use webrender::Transaction as WrTransaction;

#[cfg(target_os = "macos")]
use crate::desktop::shell::appkit::Window;
#[cfg(target_os = "windows")]
use crate::desktop::shell::win32::Window;
use crate::desktop::{app::LazyFcCache, wr_translate::wr_synchronize_updated_images};

// Assuming that current_window_state and the previous_window_state of the window
// are set correctly and the hit-test has been performed, will call the callbacks
// and return what the application should do next
#[must_use]
pub(crate) fn process_event(
    window_handle: &RawWindowHandle,
    window: &mut Window,
    fc_cache: &mut LazyFcCache,
    image_cache: &mut ImageCache,
    config: &AppConfig,
    new_windows: &mut Vec<WindowCreateOptions>,
    destroyed_windows: &mut Vec<WindowId>,
) -> ProcessEventResult {
    // TODO:
    // window.internal.current_window_state.monitor =
    // win32_translate_monitor(MonitorFromWindow(window.hwnd, MONITOR_DEFAULTTONEAREST));

    // Get events
    let events = Events::new(
        &window.internal.current_window_state,
        &window.internal.previous_window_state,
    );

    // Get nodes for events
    let nodes_to_check =
        NodesToCheck::new(&window.internal.current_window_state.last_hit_test, &events);

    // TODO: CallbacksOfHitTest no longer exists - need to reimplement with new API
    // Invoke callbacks on nodes
    // let callback_result = fc_cache.apply_closure(|fc_cache| {
    //     // Get callbacks for nodes
    //     let mut callbacks =
    //         CallbacksOfHitTest::new(&nodes_to_check, &events, &window.internal.layout_results);
    //
    //     let current_scroll_states = window.internal.get_current_scroll_states();
    //
    //     // Invoke user-defined callbacks in the UI
    //     callbacks.call(
    //         &window.internal.previous_window_state,
    //         &window.internal.current_window_state,
    //         &window_handle,
    //         &current_scroll_states,
    //         &window.gl_context_ptr,
    //         &mut window.internal.layout_results,
    //         &mut window.internal.scroll_states,
    //         image_cache,
    //         fc_cache,
    //         &config.system_callbacks,
    //         &window.internal.renderer_resources,
    //     )
    // });

    // Temporary: return empty callback result until reimplemented
    let callback_result = CallCallbacksResult::default();

    return process_callback_results(
        callback_result,
        window,
        &nodes_to_check,
        image_cache,
        fc_cache,
        new_windows,
        destroyed_windows,
    );
}

#[must_use]
pub(crate) fn process_timer(
    timer_id: usize,
    window_handle: &RawWindowHandle,
    window: &mut Window,
    fc_cache: &mut LazyFcCache,
    image_cache: &mut ImageCache,
    config: &AppConfig,
    new_windows: &mut Vec<WindowCreateOptions>,
    destroyed_windows: &mut Vec<WindowId>,
) -> ProcessEventResult {
    use azul_core::window::{RawWindowHandle, WindowsHandle};

    let callback_result = fc_cache.apply_closure(|fc_cache| {
        let frame_start = (config.system_callbacks.get_system_time_fn.cb)();
        window.internal.run_single_timer(
            timer_id,
            frame_start,
            &window_handle,
            &window.gl_context_ptr,
            image_cache,
            fc_cache,
            &config.system_callbacks,
        )
    });

    return process_callback_results(
        callback_result,
        window,
        &NodesToCheck::empty(
            window
                .internal
                .current_window_state
                .mouse_state
                .mouse_down(),
            window.internal.current_window_state.focused_node,
        ),
        image_cache,
        fc_cache,
        new_windows,
        destroyed_windows,
    );
}

#[must_use]
pub(crate) fn process_threads(
    window_handle: &RawWindowHandle,
    data: &mut RefAny,
    window: &mut Window,
    fc_cache: &mut LazyFcCache,
    image_cache: &mut ImageCache,
    config: &AppConfig,
    new_windows: &mut Vec<WindowCreateOptions>,
    destroyed_windows: &mut Vec<WindowId>,
) -> ProcessEventResult {
    #[cfg(feature = "std")]
    {
        use azul_core::window::{RawWindowHandle, WindowsHandle};

        let callback_result = fc_cache.apply_closure(|fc_cache| {
            let frame_start = (config.system_callbacks.get_system_time_fn.cb)();
            window.internal.run_all_threads(
                data,
                &window_handle,
                &window.gl_context_ptr,
                image_cache,
                fc_cache,
                &config.system_callbacks,
            )
        });

        process_callback_results(
            callback_result,
            window,
            &NodesToCheck::empty(
                window
                    .internal
                    .current_window_state
                    .mouse_state
                    .mouse_down(),
                window.internal.current_window_state.focused_node,
            ),
            image_cache,
            fc_cache,
            new_windows,
            destroyed_windows,
        )
    }

    #[cfg(not(feature = "std"))]
    {
        ProcessEventResult::DoNothing
    }
}

#[must_use]
pub(crate) fn process_callback_results(
    mut callback_results: CallCallbacksResult,
    window: &mut Window,
    nodes_to_check: &NodesToCheck,
    image_cache: &mut ImageCache,
    fc_cache: &mut LazyFcCache,
    new_windows: &mut Vec<WindowCreateOptions>,
    destroyed_windows: &mut Vec<WindowId>,
) -> ProcessEventResult {
    use azul_core::callbacks::Update;

    // TODO: StyleAndLayoutChanges no longer exists - need to reimplement with new API
    // use azul_core::window_state::{NodesToCheck, StyleAndLayoutChanges};
    use crate::desktop::wr_translate::wr_translate_document_id;

    let mut result = ProcessEventResult::DoNothing;

    if callback_results.images_changed.is_some() || callback_results.image_masks_changed.is_some() {
        let updated_images = window.internal.renderer_resources.update_image_resources(
            &window.internal.layout_results,
            callback_results.images_changed.unwrap_or_default(),
            callback_results.image_masks_changed.unwrap_or_default(),
            &crate::desktop::app::CALLBACKS,
            &*image_cache,
            &mut window.internal.gl_texture_cache,
            window.internal.document_id,
            window.internal.epoch,
        );

        if !updated_images.is_empty() {
            let mut txn = WrTransaction::new();
            let did = wr_translate_document_id(window.internal.document_id);
            wr_synchronize_updated_images(updated_images, &mut txn);
            window.render_api.send_transaction(did, txn);
            result = result.max_self(ProcessEventResult::ShouldReRenderCurrentWindow);
        }
    }

    window.start_stop_timers(
        callback_results.timers.unwrap_or_default(),
        callback_results.timers_removed.unwrap_or_default(),
    );
    window.start_stop_threads(
        callback_results.threads.unwrap_or_default(),
        callback_results.threads_removed.unwrap_or_default(),
    );

    for w in callback_results.windows_created {
        new_windows.push(w);
    }

    let scroll = window
        .internal
        .current_window_state
        .process_system_scroll(&window.internal.scroll_states);
    let need_scroll_render = scroll.is_some();

    if let Some(modified) = callback_results.modified_window_state.as_ref() {
        if modified.flags.is_about_to_close {
            destroyed_windows.push(window.get_id());
        }
        window.internal.current_window_state = FullWindowState::from_window_state(
            modified,
            window.internal.current_window_state.dropped_file.clone(),
            window.internal.current_window_state.hovered_file.clone(),
            window.internal.current_window_state.focused_node.clone(),
            window.internal.current_window_state.last_hit_test.clone(),
        );
        if modified.size.get_layout_size()
            != window.internal.current_window_state.size.get_layout_size()
        {
            result = result.max_self(ProcessEventResult::UpdateHitTesterAndProcessAgain);
        } else if !need_scroll_render {
            result = result.max_self(ProcessEventResult::ShouldReRenderCurrentWindow);
        }
    }

    #[cfg(target_os = "macos")]
    crate::desktop::shell::appkit::synchronize_window_state_with_os(&window);
    #[cfg(target_os = "windows")]
    crate::desktop::shell::win32::synchronize_window_state_with_os(&window);

    let layout_callback_changed = window
        .internal
        .current_window_state
        .layout_callback_changed(&window.internal.previous_window_state);

    if layout_callback_changed {
        return ProcessEventResult::ShouldRegenerateDomCurrentWindow;
    } else {
        match callback_results.callbacks_update_screen {
            Update::RefreshDom => {
                return ProcessEventResult::ShouldRegenerateDomCurrentWindow;
            }
            Update::RefreshDomAllWindows => {
                return ProcessEventResult::ShouldRegenerateDomAllWindows;
            }
            Update::DoNothing => {}
        }
    }

    // TODO: StyleAndLayoutChanges no longer exists - need to reimplement with new API
    // Re-layout and re-style the window.internal.layout_results
    // let mut style_layout_changes = StyleAndLayoutChanges::new(
    //     &nodes_to_check,
    //     &mut window.internal.layout_results,
    //     &image_cache,
    //     &mut window.internal.renderer_resources,
    //     window.internal.current_window_state.size.get_layout_size(),
    //     &window.internal.document_id,
    //     callback_results.css_properties_changed.as_ref(),
    //     callback_results.words_changed.as_ref(),
    //     &callback_results.update_focused_node,
    //     azul_layout::solver2::do_the_relayout,
    // );

    // Temporary: skip resize logic until reimplemented
    /*
    if let Some(rsn) = style_layout_changes.nodes_that_changed_size.as_ref() {
        let updated_images = fc_cache.apply_closure(|fc_cache| {
            LayoutResult::resize_images(
                window.internal.id_namespace,
                window.internal.document_id,
                window.internal.epoch,
                DomId::ROOT_ID,
                &image_cache,
                &window.gl_context_ptr,
                &mut window.internal.layout_results,
                &mut window.internal.gl_texture_cache,
                &mut window.internal.renderer_resources,
                &crate::desktop::app::CALLBACKS,
                azul_layout::solver2::do_the_relayout,
                &*fc_cache,
                &window.internal.current_window_state.size,
                window.internal.current_window_state.theme,
                &rsn,
            )
        });

        if !updated_images.is_empty() {
            let mut txn = WrTransaction::new();
            wr_synchronize_updated_images(updated_images, &mut txn);
            window
                .render_api
                .send_transaction(wr_translate_document_id(window.internal.document_id), txn);
        }
    }
    */

    // TODO: FOCUS CHANGE HAPPENS HERE! - need to reimplement with new API
    // if let Some(focus_change) = style_layout_changes.focus_change.clone() {
    //     window.internal.current_window_state.focused_node = focus_change.new;
    // }

    // Perform a system or user scroll event: only
    // scroll nodes that were not scrolled in the current frame
    //
    // Update the scroll states of the nodes, returning what nodes were actually scrolled this frame
    if let Some(scroll) = scroll {
        // Does a system scroll and re-invokes the IFrame
        // callbacks if scrolled out of view
        window.do_system_scroll(scroll);
        window
            .internal
            .current_window_state
            .mouse_state
            .reset_scroll_to_zero();
    }

    if style_layout_changes.did_resize_nodes() {
        // at least update the hit-tester
        result.max_self(ProcessEventResult::UpdateHitTesterAndProcessAgain)
    } else if style_layout_changes.need_regenerate_display_list() {
        result.max_self(ProcessEventResult::ShouldUpdateDisplayListCurrentWindow)
    } else if need_scroll_render || style_layout_changes.need_redraw() {
        result.max_self(ProcessEventResult::ShouldReRenderCurrentWindow)
    } else {
        result
    }
}
