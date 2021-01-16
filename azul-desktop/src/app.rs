use std::{
    time::Duration,
    collections::BTreeMap,
};
use glutin::{
    window::{
        Window as GlutinWindow,
        WindowId as GlutinWindowId,
    },
    event::{
        WindowEvent as GlutinWindowEvent,
    },
    event_loop::{
        EventLoopProxy as GlutinEventLoopProxy,
        EventLoopWindowTarget as GlutinEventLoopWindowTarget,
        EventLoop as GlutinEventLoop,
    },
    Context, NotCurrent,
};
use webrender::render_api::RenderApi as WrRenderApi;
use webrender::Transaction as WrTransaction;
use crate::{
    display_shader::DisplayShader,
    window::{Window, UserEvent}
};
use azul_core::{
    FastHashMap,
    window::{WindowCreateOptions, FullWindowState},
    gl::GlContextPtr,
    callbacks::{RefAny, UpdateScreen},
    app_resources::{AppConfig, AppResources},
};

#[cfg(test)]
use azul_core::app_resources::FakeRenderApi;

/// Graphical application that maintains some kind of application state
#[derive(Debug)]
pub struct App {
    /// Your data (the global struct which all callbacks will have access to)
    pub data: RefAny,
    /// Application configuration, whether to enable logging, etc.
    pub config: AppConfig,
    /// The window create options (only set at startup), get moved into the `.run_inner()` method
    /// No window is actually shown until the `.run_inner()` method is called.
    pub windows: Vec<WindowCreateOptions>,
    /// Glutin / winit event loop
    pub event_loop: GlutinEventLoop<UserEvent>,
}

impl App {

    #[cfg(not(test))]
    #[allow(unused_variables)]
    /// Creates a new, empty application using a specified callback. This does not open any windows.
    pub fn new(initial_data: RefAny, app_config: AppConfig) -> Self {

        #[cfg(feature = "logging")] {

            const fn translate_log_level(log_level: azul_core::app_resources::AppLogLevel) -> log::LevelFilter {
                match log_level {
                    azul_core::app_resources::AppLogLevel::Off => log::LevelFilter::Off,
                    azul_core::app_resources::AppLogLevel::Error => log::LevelFilter::Error,
                    azul_core::app_resources::AppLogLevel::Warn => log::LevelFilter::Warn,
                    azul_core::app_resources::AppLogLevel::Info => log::LevelFilter::Info,
                    azul_core::app_resources::AppLogLevel::Debug => log::LevelFilter::Debug,
                    azul_core::app_resources::AppLogLevel::Trace => log::LevelFilter::Trace,
                }
            }

            crate::logging::set_up_logging(translate_log_level(app_config.log_level));

            if app_config.enable_logging_on_panic {
                crate::logging::set_up_panic_hooks();
            }

            if app_config.enable_visual_panic_hook {
                use std::sync::atomic::Ordering;
                crate::logging::SHOULD_ENABLE_PANIC_HOOK.store(true, Ordering::SeqCst);
            }
        }

        // NOTE: Usually when the program is started, it's started on the main thread
        // However, if a debugger (such as RenderDoc) is attached, it can happen that the
        // event loop isn't created on the main thread.
        //
        // While it's discouraged to call new_any_thread(), it's necessary to do so here.
        // Do NOT create an application from a non-main thread!
        let event_loop = {

            #[cfg(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd"))] {
                use  glutin::platform::unix::EventLoopExtUnix;
                GlutinEventLoop::<UserEvent>::new_any_thread()
            }

            #[cfg(target_os = "windows")] {
                use glutin::platform::windows::EventLoopExtWindows;

                // Note that any Window created on the new
                // thread will be destroyed when the thread terminates.
                // Attempting to use a Window after its parent
                // thread terminates has unspecified, although explicitly
                // not undefined, behavior.
                GlutinEventLoop::<UserEvent>::new_any_thread()
            }

            #[cfg(not(any(
              target_os = "linux", target_os = "dragonfly", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd",
              target_os = "windows",
            )))] {
                GlutinEventLoop::<UserEvent>::new()
            }
        };

        Self {
            windows: Vec::new(),
            data: initial_data,
            config: app_config,
            event_loop,
        }
    }

    /// Spawn a new window on the screen. Note that this should only be used to
    /// create extra windows, the default window will be the window submitted to
    /// the `.run` method.
    pub fn add_window(&mut self, create_options: WindowCreateOptions) {
        self.windows.push(create_options);
    }

    /// Start the rendering loop for the currently added windows. The run() function
    /// takes one `WindowCreateOptions` as an argument, which is the "root" window, i.e.
    /// the main application window.
    #[cfg(not(test))]
    pub fn run(mut self, root_window: WindowCreateOptions) -> ! {

        #[cfg(target_os = "macos")]
        {
            use core_foundation::{self as cf, base::TCFType};
            let i = cf::bundle::CFBundle::main_bundle().info_dictionary();
            let mut i = unsafe { i.to_mutable() };
            i.set(
                cf::string::CFString::new("NSSupportsAutomaticGraphicsSwitching"),
                cf::boolean::CFBoolean::true_value().into_CFType(),
            );
        }

        self.add_window(root_window);
        run_inner(self)
    }
}

#[cfg(not(test))]
#[allow(unused_variables)]
fn run_inner(app: App) -> ! {

    use std::time::Instant;

    let App {
        mut data,
        event_loop,
        config,
        windows
    } = app;

    let mut timers = BTreeMap::new();
    let mut threads = BTreeMap::new();
    let mut resources = AppResources::default();
    let mut active_windows = BTreeMap::new();

    let window_created_instant = Instant::now();

    let proxy = event_loop.create_proxy();

    // Create the windows (makes them actually show up on the screen)
    for window_create_options in windows {
        let create_callback = window_create_options.create_callback.clone();

        let id = create_window(
            &data,
            window_create_options,
            &event_loop,
            &proxy,
            &mut active_windows,
            &mut resources,
        );

        if let Some(init_callback) = create_callback.as_ref() {
            if let Some(window_id) = id.as_ref() {

                use azul_core::callbacks::DomNodeId;
                use azul_core::callbacks::CallbackInfo;
                use azul_core::window::WindowState;

                let window = match active_windows.get_mut(&window_id) {
                    Some(s) => s,
                    None => continue,
                };

                let mut window_state: WindowState = window.internal.current_window_state.clone().into();
                let mut new_windows = Vec::new();
                let window_handle = window.get_raw_window_handle();
                let mut stop_propagation = false;
                let mut focus_target = None; // TODO: useful to implement autofocus
                let scroll_states = window.internal.get_current_scroll_states();
                let mut css_properties_changed = BTreeMap::new();
                let mut nodes_scrolled_in_callback = BTreeMap::new();

                let mut new_timers = FastHashMap::new();
                let mut new_threads = FastHashMap::new();

                let callback_info = CallbackInfo::new(
                    &window.internal.current_window_state,
                    &mut window_state,
                    &window.get_gl_context_ptr(),
                    &mut resources,
                    &mut new_timers,
                    &mut new_threads,
                    &mut new_windows,
                    &window_handle,
                    &window.internal.layout_results,
                    &mut stop_propagation,
                    &mut focus_target,
                    &scroll_states,
                    &mut css_properties_changed,
                    &mut nodes_scrolled_in_callback,
                    DomNodeId::ROOT,
                    None.into(),
                    None.into(),
                );

                let _ = (init_callback.cb)(&mut data, callback_info);

                timers.entry(*window_id).or_insert_with(|| FastHashMap::new()).extend(new_timers.drain());
                threads.entry(*window_id).or_insert_with(|| FastHashMap::new()).extend(new_threads.drain());
            }
        }
    };

    event_loop.run(move |event, event_loop_target, control_flow| {

        use glutin::event::{Event, StartCause};
        use glutin::event_loop::ControlFlow;
        use std::collections::HashSet;
        use azul_core::task::{run_all_timers, clean_up_finished_threads};
        use azul_core::window_state::StyleAndLayoutChanges;
        use azul_core::window_state::{Events, NodesToCheck};
        use azul_core::window::{FullHitTest, CursorTypeHitTest};

        let frame_start = Instant::now();

        let mut windows_that_need_to_redraw = HashSet::new();
        let mut windows_created = Vec::<WindowCreateOptions>::new();

        match event {
            Event::DeviceEvent { .. } => {
                // ignore high-frequency events
                *control_flow = ControlFlow::Wait;
                return;
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) |
            Event::NewEvents(StartCause::Poll) => {
                // run timers / tasks only every 60ms, not on every window event

                let mut update_screen_timers_tasks = UpdateScreen::DoNothing;

                // run timers
                let mut all_new_current_timers = BTreeMap::new();
                for (window_id, mut timer_map) in timers.iter_mut() {

                    // for timers it makes sense to call them on the window,
                    // since that's mostly what they're for (animations, etc.)
                    //
                    // for threads this model doesn't make that much sense
                    let window = match active_windows.get_mut(&window_id) {
                        Some(s) => s,
                        None => continue,
                    };

                    let mut css_properties_changed_in_timers = BTreeMap::new();
                    let mut nodes_scrolled_in_timers = BTreeMap::new();
                    let mut new_focus_node = None;
                    let mut new_timers = FastHashMap::new();
                    let mut modifiable_window_state = window.internal.current_window_state.clone().into();
                    let mut cur_threads = threads.get_mut(window_id).unwrap();

                    let raw_window_handle = window.get_raw_window_handle();
                    let update_screen_timers = run_all_timers(
                        &mut data,
                        &mut timer_map,
                        frame_start,

                        &window.internal.current_window_state,
                        &mut modifiable_window_state,
                        &window.get_gl_context_ptr(),
                        &mut resources,
                        &mut new_timers,
                        &mut cur_threads,
                        &mut windows_created,
                        &raw_window_handle,
                        &window.internal.layout_results,
                        &mut false, // stop_propagation - can't be set in timer
                        &mut new_focus_node,
                        &window.internal.get_current_scroll_states(),
                        &mut css_properties_changed_in_timers,
                        &mut nodes_scrolled_in_timers,
                    );

                    match update_screen_timers {
                        UpdateScreen::DoNothing => {
                            let new_focus_node = new_focus_node.and_then(|ft| ft.resolve(&window.internal.layout_results).ok());
                            let window_size = window.internal.get_layout_size();

                            // re-layouts and re-styles the window.internal.layout_results
                            let changes = StyleAndLayoutChanges::new(
                                &NodesToCheck::empty(window.internal.current_window_state.mouse_state.mouse_down()),
                                &mut window.internal.layout_results,
                                &mut resources,
                                window_size,
                                window.internal.pipeline_id,
                                &css_properties_changed_in_timers,
                                &new_focus_node,
                                azul_layout::do_the_relayout,
                            );

                            let changes_need_regenerate_dl = changes.need_regenerate_display_list();

                            let mut transaction = WrTransaction::new();

                            if changes_need_regenerate_dl {
                                let resource_updates = Vec::new(); // when re-generating the display list, no resource updates necessary
                                window.rebuild_display_list(&mut transaction, &resources, resource_updates);
                                windows_that_need_to_redraw.insert(*window_id);
                            }

                            if changes_need_regenerate_dl || changes.need_redraw() {
                                window.render_async(transaction, changes_need_regenerate_dl);
                                windows_that_need_to_redraw.insert(*window_id);
                            }
                        },
                        UpdateScreen::RegenerateStyledDomForCurrentWindow => {
                            let mut resource_updates = Vec::new();
                            let mut transaction = WrTransaction::new();
                            window.regenerate_styled_dom(&data, &mut resources, &mut resource_updates);
                            window.rebuild_display_list(&mut transaction, &resources, resource_updates);
                            window.render_async(transaction, /* display list was rebuilt */ true);
                            windows_that_need_to_redraw.insert(*window_id);
                            window.internal.current_window_state.focused_node = None; // unset the focus
                        },
                        UpdateScreen::RegenerateStyledDomForAllWindows => {
                            if update_screen_timers_tasks == UpdateScreen::DoNothing ||
                               update_screen_timers_tasks == UpdateScreen::RegenerateStyledDomForCurrentWindow {
                                update_screen_timers_tasks = update_screen_timers;
                            }
                        }
                    }

                    if !new_timers.is_empty() {
                        all_new_current_timers.insert(window_id, new_timers);
                    }

                    let current_window_save_state = window.internal.current_window_state.clone();
                    let window_state_changed_in_callbacks = window.synchronize_window_state_with_os(modifiable_window_state);
                    window.internal.previous_window_state = Some(current_window_save_state);
                }

                // -- doesn't work somehow???
                // for (window_id, mut nct) in all_new_current_timers.into_iter() {
                //     timers.entry(*window_id).or_insert_with(|| FastHashMap::default()).extend(nct.drain());
                // }

                // run threads
                // TODO: threads should not depend on the window being active (?)
                let mut all_new_current_threads = BTreeMap::new();
                for (window_id, mut thread_map) in threads.iter_mut() {
                    let window = match active_windows.get_mut(&window_id) {
                        Some(s) => s,
                        None => continue,
                    };

                    let mut css_properties_changed_in_threads = BTreeMap::new();
                    let mut nodes_scrolled_in_threads = BTreeMap::new();
                    let mut new_focus_node = None;
                    let mut modifiable_window_state = window.internal.current_window_state.clone().into();
                    let mut cur_timers = timers.get_mut(window_id).unwrap();
                    let mut new_threads = FastHashMap::new();

                    let raw_window_handle = window.get_raw_window_handle();
                    let update_screen_threads = clean_up_finished_threads(
                        &mut thread_map,

                        &window.internal.current_window_state,
                        &mut modifiable_window_state,
                        &window.get_gl_context_ptr(),
                        &mut resources,
                        &mut cur_timers,
                        &mut new_threads,
                        &mut windows_created,
                        &raw_window_handle,
                        &window.internal.layout_results,
                        &mut false, // stop_propagation - can't be set in timer
                        &mut new_focus_node,
                        &window.internal.get_current_scroll_states(),
                        &mut css_properties_changed_in_threads,
                        &mut nodes_scrolled_in_threads,
                    );

                    match update_screen_threads {
                        UpdateScreen::DoNothing => {
                            let new_focus_node = new_focus_node.and_then(|ft| ft.resolve(&window.internal.layout_results).ok());
                            let window_size = window.internal.get_layout_size();

                            // re-layouts and re-styles the window.internal.layout_results
                            let changes = StyleAndLayoutChanges::new(
                                &NodesToCheck::empty(window.internal.current_window_state.mouse_state.mouse_down()),
                                &mut window.internal.layout_results,
                                &mut resources,
                                window_size,
                                window.internal.pipeline_id,
                                &css_properties_changed_in_threads,
                                &new_focus_node,
                                azul_layout::do_the_relayout,
                            );

                                let changes_need_regenerate_dl = changes.need_regenerate_display_list();
                                let mut transaction = WrTransaction::new();

                                if changes_need_regenerate_dl {
                                    let resource_updates = Vec::new(); // when re-generating the display list, no resource updates necessary
                                    window.rebuild_display_list(&mut transaction, &resources, resource_updates);
                                    windows_that_need_to_redraw.insert(*window_id);
                                }

                                if changes_need_regenerate_dl || changes.need_redraw() {
                                    window.render_async(transaction, changes_need_regenerate_dl);
                                    windows_that_need_to_redraw.insert(*window_id);
                                }
                            },
                            UpdateScreen::RegenerateStyledDomForCurrentWindow => {
                                let mut resource_updates = Vec::new();
                                let mut transaction = WrTransaction::new();
                                window.regenerate_styled_dom(&data, &mut resources, &mut resource_updates);
                                window.rebuild_display_list(&mut transaction, &resources, resource_updates);
                                window.render_async(transaction, /* display list was rebuilt */ true);
                                windows_that_need_to_redraw.insert(*window_id);
                                window.internal.current_window_state.focused_node = None; // unset the focus
                            },
                        UpdateScreen::RegenerateStyledDomForAllWindows => {
                            if update_screen_timers_tasks == UpdateScreen::DoNothing ||
                               update_screen_timers_tasks == UpdateScreen::RegenerateStyledDomForCurrentWindow {
                                update_screen_timers_tasks = update_screen_threads;
                            }
                        }
                    }

                    if !new_threads.is_empty() {
                        all_new_current_threads.entry(*window_id).or_insert_with(|| FastHashMap::new()).extend(new_threads.drain());
                    }

                    let current_window_save_state = window.internal.current_window_state.clone();
                    let window_state_changed_in_callbacks = window.synchronize_window_state_with_os(modifiable_window_state);
                    window.internal.previous_window_state = Some(current_window_save_state);
                }

                for (window_id, mut new_current_threads) in all_new_current_threads {
                    threads.entry(window_id).or_insert_with(|| FastHashMap::default()).extend(new_current_threads.drain());
                }

                if update_screen_timers_tasks == UpdateScreen::RegenerateStyledDomForAllWindows {
                    for (window_id, window) in active_windows.iter_mut() {
                        let mut resource_updates = Vec::new();
                        let mut transaction = WrTransaction::new();

                        window.regenerate_styled_dom(&data, &mut resources, &mut resource_updates);
                        window.rebuild_display_list(&mut transaction, &resources, resource_updates);
                        window.render_async(transaction, /* display list was rebuilt */ true);
                        windows_that_need_to_redraw.insert(*window_id);
                        window.internal.current_window_state.focused_node = None; // unset the focus
                    }
                }
            },
            Event::RedrawRequested(window_id) => {

                // Ignore this event
                //
                // If we redraw here, the screen will flicker because the
                // screen may not be finished painting

                let mut window = match active_windows.get_mut(&window_id) {
                    Some(s) => s,
                    None => {return; },
                };

                window.display.window().set_visible(window.internal.current_window_state.flags.is_visible);
            },
            Event::WindowEvent { event, window_id } => {

                let mut window = match active_windows.get_mut(&window_id) {
                    Some(s) => s,
                    None => {return; },
                };

                let window_event_start = Instant::now();

                // ONLY update the window_state of the window, don't do anything else
                // everything is then
                process_window_event(&mut window, &event_loop_target, &event);

                let mut need_regenerate_display_list = false;
                let mut should_scroll_render = false;
                let mut should_callback_render = false;

                let mut updated_resources = Vec::new();

                loop {
                    let events = Events::new(&window.internal.current_window_state, &window.internal.previous_window_state);
                    let is_first_frame = window.internal.previous_window_state.is_none();
                    let layout_callback_changed = window.internal.current_window_state.layout_callback_changed(&window.internal.previous_window_state);
                    let hit_test = if !events.needs_hit_test() { FullHitTest::empty() } else {
                        let ht = FullHitTest::new(&window.internal.layout_results, &window.internal.current_window_state.mouse_state.cursor_position, &window.internal.scroll_states);
                        window.internal.current_window_state.hovered_nodes = ht.hovered_nodes.clone();
                        ht
                    };

                    // previous_window_state = current_window_state, nothing to do
                    if (events.is_empty() && !is_first_frame) || layout_callback_changed { break; }

                    let scroll_event = window.internal.current_window_state.get_scroll_amount();
                    let nodes_to_check = NodesToCheck::new(&hit_test, &events);
                    let mut callback_results = window.call_callbacks(&nodes_to_check, &events, &window.get_gl_context_ptr(), &mut resources);

                    let cur_should_callback_render = callback_results.should_scroll_render;
                    if cur_should_callback_render { should_callback_render = true; }
                    let cur_should_scroll_render = window.internal.current_window_state.get_scroll_amount().as_ref().map(|se| window.internal.scroll_states.should_scroll_render(se, &hit_test)).unwrap_or(false);
                    if cur_should_scroll_render { should_scroll_render = true; }
                    window.internal.current_window_state.mouse_state.reset_scroll_to_zero();

                    if layout_callback_changed {
                        window.regenerate_styled_dom(&data, &mut resources, &mut updated_resources);
                        need_regenerate_display_list = true;
                        callback_results.update_focused_node = Some(None); // unset the focus
                    } else {
                        match callback_results.callbacks_update_screen {
                            UpdateScreen::RegenerateStyledDomForCurrentWindow => {
                                window.regenerate_styled_dom(&data, &mut resources, &mut updated_resources);
                                need_regenerate_display_list = true;
                                callback_results.update_focused_node = Some(None); // unset the focus
                            },
                            UpdateScreen::RegenerateStyledDomForAllWindows => {
                                /* for window in active_windows { window.regenerate_styled_dom(); } */
                            },
                            UpdateScreen::DoNothing => {

                                let window_size = window.internal.get_layout_size();

                                // re-layouts and re-styles the window.internal.layout_results
                                let changes = StyleAndLayoutChanges::new(
                                    &nodes_to_check,
                                    &mut window.internal.layout_results,
                                    &mut resources,
                                    window_size,
                                    window.internal.pipeline_id,
                                    &callback_results.css_properties_changed,
                                    &callback_results.update_focused_node,
                                    azul_layout::do_the_relayout,
                                );

                                if changes.need_regenerate_display_list() || events.contains_resize_event() {
                                    // this can be false in case that only opacity: / transform: properties changed!
                                    need_regenerate_display_list = true;
                                }

                                if changes.need_redraw() {
                                    should_callback_render = true;
                                }
                            }
                        }
                    }

                    windows_created.extend(callback_results.windows_created.drain(..));

                    timers.entry(window_id).or_insert_with(|| FastHashMap::new()).extend(callback_results.timers.drain());
                    threads.entry(window_id).or_insert_with(|| FastHashMap::new()).extend(callback_results.threads.drain());

                    // see if the callbacks modified the WindowState - if yes, re-determine the events
                    let current_window_save_state = window.internal.current_window_state.clone();
                    let callbacks_changed_cursor = callback_results.cursor_changed();
                    if !callbacks_changed_cursor {
                        let ht = FullHitTest::new(&window.internal.layout_results, &window.internal.current_window_state.mouse_state.cursor_position, &window.internal.scroll_states);
                        let cht = CursorTypeHitTest::new(&ht, &window.internal.layout_results);
                        callback_results.modified_window_state.mouse_state.mouse_cursor_type = Some(cht.cursor_icon).into();
                    }
                    if let Some(callback_new_focus) = callback_results.update_focused_node.as_ref() {
                        window.internal.current_window_state.focused_node = *callback_new_focus;
                    }

                    let window_state_changed_in_callbacks = window.synchronize_window_state_with_os(callback_results.modified_window_state);
                    window.internal.previous_window_state = Some(current_window_save_state);
                    if !window_state_changed_in_callbacks {
                        break;
                    } else {
                        continue;
                    }
                }

                if need_regenerate_display_list {
                    let mut transaction = WrTransaction::new();
                    window.rebuild_display_list(&mut transaction, &resources, updated_resources);
                    window.render_async(transaction, need_regenerate_display_list);
                    windows_that_need_to_redraw.insert(window_id);
                } else if should_scroll_render || should_callback_render {
                    let mut transaction = WrTransaction::new();
                    window.render_async(transaction, need_regenerate_display_list);
                    windows_that_need_to_redraw.insert(window_id);
                }
            },
            Event::UserEvent(UserEvent { window_id, composite_needed: _ }) => {

                let window = match active_windows.get_mut(&window_id) {
                    Some(s) => s,
                    None => {return; },
                };

                // transaction has finished, now render
                window.render_block_and_swap();
            }
            _ => { },
        }

        // close windows
        let windows_to_remove = active_windows.iter()
        .filter(|(id, window)| window.internal.current_window_state.flags.is_about_to_close)
        .map(|(id, window)| id.clone())
        .collect::<Vec<_>>();

        for window_id in windows_to_remove {

            let mut window_should_close = true;

            {
                let window = match active_windows.get_mut(&window_id) {
                    Some(s) => s,
                    None => continue,
                };
                let close_callback = window.internal.current_window_state.close_callback.clone();
                if let Some(close_callback) = close_callback.as_ref() {

                    use azul_core::callbacks::DomNodeId;
                    use azul_core::callbacks::CallbackInfo;
                    use azul_core::window::WindowState;

                    let mut window_state: WindowState = window.internal.current_window_state.clone().into();
                    let mut new_windows = Vec::new();
                    let window_handle = window.get_raw_window_handle();
                    let mut stop_propagation = false;
                    let mut focus_target = None; // TODO: useful to implement autofocus
                    let scroll_states = window.internal.get_current_scroll_states();
                    let mut css_properties_changed = BTreeMap::new();
                    let mut nodes_scrolled_in_callback = BTreeMap::new();

                    let mut new_timers = FastHashMap::new();
                    let mut new_threads = FastHashMap::new();
                    let gl_context_ptr = window.get_gl_context_ptr();

                    let callback_info = CallbackInfo::new(
                        &window.internal.current_window_state,
                        &mut window_state,
                        &gl_context_ptr,
                        &mut resources,
                        &mut new_timers,
                        &mut new_threads,
                        &mut new_windows,
                        &window_handle,
                        &window.internal.layout_results,
                        &mut stop_propagation,
                        &mut focus_target,
                        &scroll_states,
                        &mut css_properties_changed,
                        &mut nodes_scrolled_in_callback,
                        DomNodeId::ROOT,
                        None.into(),
                        None.into(),
                    );

                    let result = (close_callback.cb)(&mut data, callback_info);

                    timers.entry(window_id).or_insert_with(|| FastHashMap::new()).extend(new_timers.drain());
                    threads.entry(window_id).or_insert_with(|| FastHashMap::new()).extend(new_threads.drain());
                    if result == UpdateScreen::DoNothing {
                        window_should_close = false;
                    }
                }
            }

            if window_should_close {

                let window = match active_windows.remove(&window_id) {
                    Some(w) => w,
                    None => continue,
                };

                close_window(window, &mut resources);
            }
        }

        // open windows
        for window_create_options in windows_created.into_iter() {

            let create_callback = window_create_options.create_callback.clone();

            let id = create_window(
                &data,
                window_create_options,
                &event_loop_target,
                &proxy,
                &mut active_windows,
                &mut resources,
            );

            if let Some(init_callback) = create_callback.as_ref() {
                if let Some(window_id) = id.as_ref() {

                    use azul_core::callbacks::DomNodeId;
                    use azul_core::callbacks::CallbackInfo;
                    use azul_core::window::WindowState;

                    let window = match active_windows.get_mut(&window_id) {
                        Some(s) => s,
                        None => continue,
                    };

                    let mut window_state: WindowState = window.internal.current_window_state.clone().into();
                    let mut new_windows = Vec::new();
                    let window_handle = window.get_raw_window_handle();
                    let mut stop_propagation = false;
                    let mut focus_target = None; // TODO: useful to implement autofocus
                    let scroll_states = window.internal.get_current_scroll_states();
                    let mut css_properties_changed = BTreeMap::new();
                    let mut nodes_scrolled_in_callback = BTreeMap::new();

                    let mut new_timers = FastHashMap::new();
                    let mut new_threads = FastHashMap::new();

                    let gl_context_ptr = window.get_gl_context_ptr();
                    let callback_info = CallbackInfo::new(
                        &window.internal.current_window_state,
                        &mut window_state,
                        &gl_context_ptr,
                        &mut resources,
                        &mut new_timers,
                        &mut new_threads,
                        &mut new_windows,
                        &window_handle,
                        &window.internal.layout_results,
                        &mut stop_propagation,
                        &mut focus_target,
                        &scroll_states,
                        &mut css_properties_changed,
                        &mut nodes_scrolled_in_callback,
                        DomNodeId::ROOT,
                        None.into(),
                        None.into(),
                    );

                    let _ = (init_callback.cb)(&mut data, callback_info);

                    timers.entry(*window_id).or_insert_with(|| FastHashMap::new()).extend(new_timers.drain());
                    threads.entry(*window_id).or_insert_with(|| FastHashMap::new()).extend(new_threads.drain());
                }
            }
        }
/*
        for window_id in windows_that_need_to_redraw.into_iter() {
            let window = match active_windows.get_mut(&window_id) {
                Some(s) => s,
                None => continue,
            };
            window.display.window().request_redraw();
        }
*/
        // end: handle control flow and app shutdown
        *control_flow = if !active_windows.is_empty() {
            // If no timers / threads are running, wait until next user event
            if timers.is_empty() && threads.is_empty() {
                 ControlFlow::Wait
            } else {
                if timers.is_empty() {
                    // minimum time to re-poll for threads = 16ms
                    ControlFlow::WaitUntil(frame_start + Duration::from_millis(16))
                } else if timers.values().any(|timer_map| timer_map.values().any(|t| t.interval.as_ref().is_none())) {
                    ControlFlow::Poll
                } else {
                    // timers are not empty, select the minimum time that the next timer needs to run
                    // ex. if one timer is set to run every 2 seconds, then we only need
                    // to poll in 2 seconds, not every 16ms
                    let mut min_time = Duration::from_secs(1000); // really long time

                    for timer_map in timers.values() {
                        for timer in timer_map.values() {
                            if let Some(new_min) = timer.instant_of_next_run().checked_duration_since(frame_start) {
                                min_time = min_time.min(new_min);
                            }
                        }
                    }

                    ControlFlow::WaitUntil(frame_start + min_time)
                }
            }
        } else {
            timers = BTreeMap::new();
            threads = BTreeMap::new();
            // Application shutdown
            ControlFlow::Exit
        };
    })
}

/// Updates the `FullWindowState` with the new event
fn process_window_event(window: &mut Window, event_loop: &GlutinEventLoopWindowTarget<UserEvent>, event: &GlutinWindowEvent) {

    use glutin::event::{KeyboardInput, Touch};
    use azul_core::window::{CursorPosition, WindowPosition, LogicalPosition};
    use crate::wr_translate::winit_translate::{
        winit_translate_physical_size, winit_translate_physical_position,
    };

    let mut current_window_state = &mut window.internal.current_window_state;

    match event {
        GlutinWindowEvent::ModifiersChanged(modifier_state) => {
            current_window_state.keyboard_state.shift_down = modifier_state.shift();
            current_window_state.keyboard_state.ctrl_down = modifier_state.ctrl();
            current_window_state.keyboard_state.alt_down = modifier_state.alt();
            current_window_state.keyboard_state.super_down = modifier_state.logo();
        },
        GlutinWindowEvent::Resized(physical_size) => {
            // window.display.make_current();
            // window.display.windowed_context().unwrap().resize(*physical_size);
            current_window_state.size.dimensions = winit_translate_physical_size(*physical_size).to_logical(current_window_state.size.system_hidpi_factor as f32);
        },
        GlutinWindowEvent::ScaleFactorChanged { scale_factor, new_inner_size } => {
            use crate::window::get_hidpi_factor;
            let (hidpi_factor, _) = get_hidpi_factor(&window.display.window(), event_loop);
            current_window_state.size.system_hidpi_factor = *scale_factor as f32;
            current_window_state.size.hidpi_factor = hidpi_factor;
            current_window_state.size.dimensions = winit_translate_physical_size(**new_inner_size).to_logical(current_window_state.size.system_hidpi_factor as f32);
        },
        GlutinWindowEvent::Moved(new_window_position) => {
            current_window_state.position = WindowPosition::Initialized(winit_translate_physical_position(*new_window_position));
        },
        GlutinWindowEvent::CursorMoved { position, .. } => {
            let world_pos_x = position.x as f32 / current_window_state.size.hidpi_factor * current_window_state.size.system_hidpi_factor;
            let world_pos_y = position.y as f32 / current_window_state.size.hidpi_factor * current_window_state.size.system_hidpi_factor;
            current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(LogicalPosition::new(world_pos_x, world_pos_y));
        },
        GlutinWindowEvent::CursorLeft { .. } => {
            current_window_state.mouse_state.cursor_position = CursorPosition::OutOfWindow;
        },
        GlutinWindowEvent::CursorEntered { .. } => {
            current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(LogicalPosition::new(0.0, 0.0));
        },
        GlutinWindowEvent::KeyboardInput { input: KeyboardInput { state, virtual_keycode, scancode, .. }, .. } => {
            use crate::wr_translate::winit_translate::translate_virtual_keycode;
            use glutin::event::ElementState;
            match state {
                ElementState::Pressed => {
                    if let Some(vk) = virtual_keycode.map(translate_virtual_keycode) {
                        current_window_state.keyboard_state.pressed_virtual_keycodes.insert_hm_item(vk);
                        current_window_state.keyboard_state.current_virtual_keycode = Some(vk).into();
                    }
                    current_window_state.keyboard_state.pressed_scancodes.insert_hm_item(*scancode);
                    current_window_state.keyboard_state.current_char = None.into();
                },
                ElementState::Released => {
                    if let Some(vk) = virtual_keycode.map(translate_virtual_keycode) {
                        current_window_state.keyboard_state.pressed_virtual_keycodes.remove_hm_item(&vk);
                        current_window_state.keyboard_state.current_virtual_keycode = None.into();
                    }
                    current_window_state.keyboard_state.pressed_scancodes.remove_hm_item(scancode);
                    current_window_state.keyboard_state.current_char = None.into();
                }
            }
        },
        // The char event is sliced inbetween a keydown and a keyup event, so the keyup
        // has to clear the character again
        GlutinWindowEvent::ReceivedCharacter(c) => {
            current_window_state.keyboard_state.current_char = Some((*c) as u32).into();
        },
        GlutinWindowEvent::MouseInput { state, button, .. } => {
            use glutin::event::{ElementState::*, MouseButton::*};
            match state {
                Pressed => {
                    match button {
                        Left => current_window_state.mouse_state.left_down = true,
                        Right => current_window_state.mouse_state.right_down = true,
                        Middle => current_window_state.mouse_state.middle_down = true,
                        _ => current_window_state.mouse_state.left_down = true,
                    }
                },
                Released => {
                    match button {
                        Left => current_window_state.mouse_state.left_down = false,
                        Right => current_window_state.mouse_state.right_down = false,
                        Middle => current_window_state.mouse_state.middle_down = false,
                        _ => current_window_state.mouse_state.left_down = false,
                    }
                },
            }
        },
        GlutinWindowEvent::MouseWheel { delta, .. } => {

            const LINE_DELTA: f32 = 38.0;

            use glutin::event::MouseScrollDelta;

            let (scroll_x_px, scroll_y_px) = match delta {
                MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                MouseScrollDelta::LineDelta(x, y) => (x * LINE_DELTA, y * LINE_DELTA),
            };

            // TODO: "natural scrolling" + configurable LINE_DELTA?
            current_window_state.mouse_state.scroll_x = Some(-scroll_x_px).into();
            current_window_state.mouse_state.scroll_y = Some(-scroll_y_px).into();
        },
        GlutinWindowEvent::HoveredFile(file_path) => {
            current_window_state.hovered_file = Some(file_path.clone());
            current_window_state.dropped_file = None;
        },
        GlutinWindowEvent::HoveredFileCancelled => {
            current_window_state.hovered_file = None;
            current_window_state.dropped_file = None;
        },
        GlutinWindowEvent::DroppedFile(file_path) => {
            current_window_state.hovered_file = None;
            current_window_state.dropped_file = Some(file_path.clone());
        },
        GlutinWindowEvent::Focused(f) => {
            current_window_state.flags.has_focus = *f;
        },
        GlutinWindowEvent::CloseRequested => {
            current_window_state.flags.is_about_to_close = true;
        },
        GlutinWindowEvent::Touch(Touch { location, .. }) => {
            // TODO: use current_window_state.touch_state instead, this is wrong
            // TODO: multitouch
            let world_pos_x = location.x as f32 / current_window_state.size.hidpi_factor * current_window_state.size.system_hidpi_factor;
            let world_pos_y = location.y as f32 / current_window_state.size.hidpi_factor * current_window_state.size.system_hidpi_factor;
            current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(LogicalPosition::new(world_pos_x, world_pos_y));
        },
        GlutinWindowEvent::TouchpadPressure { .. } => {
            // At the moment, only supported on Apple forcetouch-capable macbooks.
            // The parameters are: pressure level (value between 0 and 1 representing how hard the touchpad is being pressed) and stage
            // (integer representing the click level).

            // TODO!
        },
        GlutinWindowEvent::ThemeChanged(new_theme) => {
            use crate::wr_translate::winit_translate::translate_winit_theme;
            current_window_state.theme = translate_winit_theme(*new_theme);
        },
        GlutinWindowEvent::AxisMotion { .. } => {
            // Motion on some analog axis. May report data redundant to other, more specific events.

            // TODO!
        },
        GlutinWindowEvent::Destroyed => { },
    }
}

fn create_window(
    data: &RefAny,
    window_create_options: WindowCreateOptions,
    events_loop: &GlutinEventLoopWindowTarget<UserEvent>,
    proxy: &GlutinEventLoopProxy<UserEvent>,
    active_windows: &mut BTreeMap<GlutinWindowId, Window>,
    app_resources: &mut AppResources,
) -> Option<GlutinWindowId> {

    let window = Window::new(
         &data,
         window_create_options,
         events_loop,
         proxy,
         app_resources,
    );

    let window = match window {
        Ok(o) => o,
        Err(e) => {
            #[cfg(feature = "logging")] {
                error!("Error initializing window: {}", e);
            }
            return None;
        }
    };

    let glutin_window_id = window.display.window().id();
    active_windows.insert(glutin_window_id, window);
    Some(glutin_window_id)
}

fn close_window(mut window: Window, app_resources: &mut AppResources) {
    use azul_core::gl::gl_textures_remove_active_pipeline;
    use crate::wr_translate::wr_translate_document_id;
    use crate::wr_translate::wr_translate_resource_update;

    gl_textures_remove_active_pipeline(&window.internal.pipeline_id);
    let mut resources_to_delete = Vec::new();
    app_resources.delete_pipeline(&window.internal.pipeline_id, &mut resources_to_delete);
    let mut txn = WrTransaction::new();
    txn.skip_scene_builder();
    txn.update_resources(resources_to_delete.into_iter().map(wr_translate_resource_update).collect());
    window.render_api.send_transaction(wr_translate_document_id(window.internal.document_id), txn);
    window.render_api.delete_document(wr_translate_document_id(window.internal.document_id));
}