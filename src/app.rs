use css::Css;
use app_state::AppState;
use traits::LayoutScreen;
use input::hit_test_ui;
use ui_state::UiState;
use ui_description::UiDescription;

use std::sync::{Arc, Mutex};
use window::{Window, WindowCreateOptions, WindowCreateError, WindowId};
use glium::glutin::Event;
use euclid::TypedScale;

/// Graphical application that maintains some kind of application state
pub struct App<T: LayoutScreen> {
    /// The graphical windows, indexed by ID
    windows: Vec<Window<T>>,
    /// The global application state
    pub app_state: Arc<Mutex<AppState<T>>>,
}

pub(crate) struct FrameEventInfo {
    pub(crate) should_redraw_window: bool,
    pub(crate) should_swap_window: bool,
    pub(crate) should_hittest: bool,
    pub(crate) cur_cursor_pos: (f64, f64),
    pub(crate) new_window_size: Option<(u32, u32)>,
    pub(crate) new_dpi_factor: Option<f32>,
}

impl Default for FrameEventInfo {
    fn default() -> Self {
        Self {
            should_redraw_window: false,
            should_swap_window: false,
            should_hittest: false,
            cur_cursor_pos: (0.0, 0.0),
            new_window_size: None,
            new_dpi_factor: None,
        }
    }
}

impl<T: LayoutScreen> App<T> {

    /// Create a new, empty application (note: doesn't create a window!)
    pub fn new(initial_data: T) -> Self {
        Self {
            windows: Vec::new(),
            app_state: Arc::new(Mutex::new(AppState::new(initial_data))),
        }
    }

    /// Spawn a new window on the screen
    pub fn create_window(&mut self, options: WindowCreateOptions, css: Css) -> Result<(), WindowCreateError> {
        self.windows.push(Window::new(options, css)?);
        Ok(())
    }

    /// Start the rendering loop for the currently open windows
    pub fn start_render_loop(&mut self)
    {
        let mut ui_state_cache = Vec::with_capacity(self.windows.len());
        let mut ui_description_cache = vec![UiDescription::default(); self.windows.len()];

        // first redraw, initialize cache  
        {
            let app_state = self.app_state.lock().unwrap();
            for (idx, _) in self.windows.iter().enumerate() {
                ui_state_cache.push(UiState::from_app_state(&*app_state, WindowId { id: idx }));
            }

            // First repaint, otherwise the window would be black on startup
            for (idx, window) in self.windows.iter_mut().enumerate() {
                ui_description_cache[idx] = UiDescription::from_ui_state(&ui_state_cache[idx], &mut window.css);
                render(window, &WindowId { id: idx, }, &ui_description_cache[idx], true);
                window.display.swap_buffers().unwrap();
            }
        }      

        
        'render_loop: loop {

            let mut closed_windows = Vec::<usize>::new();

            let time_start = ::std::time::Instant::now();
            let mut debug_has_repainted = None;

            // TODO: Use threads on a per-window basis.
            // Currently, events in one window will block all others
            for (idx, ref mut window) in self.windows.iter_mut().enumerate() {

                let current_window_id = WindowId { id: idx };

                let mut frame_event_info = FrameEventInfo::default();

                window.events_loop.poll_events(|event| {
                    let should_close = process_event(event, &mut frame_event_info);
                    if should_close {
                        closed_windows.push(idx);
                    }
                });

                // update the state
                if frame_event_info.should_swap_window {
                    window.display.swap_buffers().unwrap();
                }

                if frame_event_info.should_hittest {

                    use webrender::api::WorldPoint;
                    use dom::UpdateScreen;

                    let point = WorldPoint::new(frame_event_info.cur_cursor_pos.0 as f32, frame_event_info.cur_cursor_pos.1 as f32);
                    let hit_test_results = hit_test_ui(&window.internal.api, window.internal.document_id, Some(window.internal.pipeline_id), point);

                    let mut should_update_screen = UpdateScreen::DontRedraw;

                    for item in hit_test_results.items {
                        if let Some(callback_list) = ui_state_cache[idx].node_ids_to_callbacks_list.get(&item.tag.0) {
                            // TODO: filter by `On` type (On::MouseOver, On::MouseLeave, etc.)
                            // currently, just invoke all actions
                            for callback_id in callback_list.values() {
                                use dom::Callback::*;
                                let update = match ui_state_cache[idx].callback_list[callback_id] {
                                    Sync(callback) => { (callback)(&mut *self.app_state.lock().unwrap()) },
                                    Async(callback) => { (callback)(self.app_state.clone()) },
                                };
                                if update == UpdateScreen::Redraw { 
                                    should_update_screen = UpdateScreen::Redraw;
                                }
                            }
                        }
                    }

                    if should_update_screen == UpdateScreen::Redraw {
                        frame_event_info.should_redraw_window = true;
                    }

                }

                let mut app_state = self.app_state.lock().unwrap();
                ui_state_cache[idx] = UiState::from_app_state(&*app_state, WindowId { id: idx });

                if window.css.is_dirty {
                    frame_event_info.should_redraw_window = true;
                }

                if let Some((w, h)) = frame_event_info.new_window_size {
                    use webrender::api::{DeviceUintSize, DeviceUintPoint, DeviceUintRect, LayoutSize, Transaction};
                    window.internal.layout_size = LayoutSize::new(w as f32, h as f32);
                    window.internal.framebuffer_size = DeviceUintSize::new(w, h);
                    let mut txn = Transaction::new();
                    let bounds = DeviceUintRect::new(DeviceUintPoint::new(0, 0), window.internal.framebuffer_size);
                    txn.set_window_parameters(window.internal.framebuffer_size, bounds, window.internal.hidpi_factor);
                    window.internal.api.send_transaction(window.internal.document_id, txn);
                    render(window, &current_window_id, &ui_description_cache[idx], true);
                    
                    let time_end = ::std::time::Instant::now();
                    debug_has_repainted = Some(time_end - time_start);
                    continue;
                }

                if let Some(dpi) = frame_event_info.new_dpi_factor {
                    use webrender::api::{DeviceUintPoint, DeviceUintRect, Transaction};
                    window.internal.hidpi_factor = dpi;
                    let mut txn = Transaction::new();
                    let bounds = DeviceUintRect::new(DeviceUintPoint::new(0, 0), window.internal.framebuffer_size);
                    txn.set_window_parameters(window.internal.framebuffer_size, bounds, window.internal.hidpi_factor);
                    window.internal.api.send_transaction(window.internal.document_id, txn);
                    render(window, &current_window_id, &ui_description_cache[idx], true);
                    let time_end = ::std::time::Instant::now();
                    debug_has_repainted = Some(time_end - time_start);
                    continue;
                }

                if frame_event_info.should_redraw_window {
                    ui_description_cache[idx] = UiDescription::from_ui_state(&ui_state_cache[idx], &mut window.css);
                    render(window, &current_window_id, &ui_description_cache[idx], frame_event_info.new_window_size.is_some());
                    let time_end = ::std::time::Instant::now();
                    debug_has_repainted = Some(time_end - time_start);
                }
            }

            // close windows if necessary
            for closed_window_id in closed_windows {
                let closed_window_id = closed_window_id;
                ui_state_cache.remove(closed_window_id);
                ui_description_cache.remove(closed_window_id);
                self.windows.remove(closed_window_id);
            }

            if self.windows.is_empty() {
                break;
            } else {
                if let Some(restate_time) = debug_has_repainted {
                    println!("frame time: {:?} ms", restate_time.subsec_nanos() as f32 / 1_000_000.0);
                }
                ::std::thread::sleep(::std::time::Duration::from_millis(16));
            }
        }
    }
}

fn process_event(event: Event, frame_event_info: &mut FrameEventInfo) -> bool {
    use glium::glutin::WindowEvent;
    match event {
        Event::WindowEvent {
            window_id,
            event
        } => {
            match event {
                WindowEvent::CursorMoved {
                    device_id,
                    position,
                    modifiers,
                } => {
                    frame_event_info.should_hittest = true;
                    frame_event_info.cur_cursor_pos = position;

                    let _ = window_id;
                    let _ = device_id;
                    let _ = modifiers;
                },
                WindowEvent::Resized(w, h) => {
                    frame_event_info.new_window_size = Some((w, h));
                },
                WindowEvent::Refresh => {
                    frame_event_info.should_redraw_window = true;
                },
                WindowEvent::HiDPIFactorChanged(dpi) => {
                    frame_event_info.new_dpi_factor = Some(dpi);
                },
                WindowEvent::Closed => {
                    return true;
                }
                _ => { },
            }
        },
        Event::Awakened => {
            frame_event_info.should_swap_window = true;
        },
        _ => { },
    }

    false
}

fn render<T: LayoutScreen>(window: &mut Window<T>, _window_id: &WindowId, ui_description: &UiDescription<T>, has_window_size_changed: bool) 
{
    use webrender::api::*;
    use display_list::DisplayList;
    
    let display_list = DisplayList::new_from_ui_description(ui_description);
    let builder = display_list.into_display_list_builder(window.internal.pipeline_id, &mut window.solver, &mut window.css, has_window_size_changed);
    
    if let Some(new_builder) = builder {
        // only finalize the list if we actually need to. Otherwise just redraw the last display list
        window.internal.last_display_list_builder = new_builder.finalize().2;
    }

    let resources = ResourceUpdates::new();
    let mut txn = Transaction::new();
    
    // TODO: something is wrong, the redraw times increase, even if the same display list is redrawn
    txn.set_display_list(
        window.internal.epoch,
        None,
        window.internal.layout_size,
        (window.internal.pipeline_id, window.solver.window_dimensions.layout_size, window.internal.last_display_list_builder.clone()),
        true,
    );

    txn.update_resources(resources);
    txn.set_root_pipeline(window.internal.pipeline_id);
    txn.generate_frame();
    window.internal.api.send_transaction(window.internal.document_id, txn);

    window.renderer.as_mut().unwrap().update();
    window.renderer.as_mut().unwrap().render(window.internal.framebuffer_size).unwrap();
}