use css::Css;
use app_state::AppState;
use traits::LayoutScreen;
use input::hit_test_ui;
use ui_state::UiState;
use ui_description::UiDescription;

use std::sync::{Arc, Mutex};
use std::collections::BTreeMap;
use window::{Window, WindowCreateOptions, WindowCreateError, WindowId};

/// Graphical application that maintains some kind of application state
pub struct App<T: LayoutScreen> {
    /// The graphical windows, indexed by ID
    windows: BTreeMap<WindowId, Window>,
    /// The global application state
    pub app_state: Arc<Mutex<AppState<T>>>,
}

impl<T: LayoutScreen> App<T> {

    /// Create a new, empty application (note: doesn't create a window!)
    pub fn new(initial_data: T) -> Self {
        Self {
            windows: BTreeMap::new(),
            app_state: Arc::new(Mutex::new(AppState::new(initial_data))),
        }
    }

    /// Spawn a new window on the screen
    pub fn create_window(&mut self, options: WindowCreateOptions) -> Result<WindowId, WindowCreateError> {
        let window = Window::new(options)?;
        if self.windows.len() == 0 {
            self.windows.insert(WindowId::new(0), window);
            Ok(WindowId::new(0))
        } else {
            let highest_id = *self.windows.iter().next_back().unwrap().0;
            let new_id = highest_id.id.saturating_add(1);
            self.windows.insert(WindowId::new(new_id), window);
            Ok(WindowId::new(new_id))
        }
    }

    /// Start the rendering loop for the currently open windows
    pub fn start_render_loop(&mut self)
    {
        let mut ui_state_cache = Vec::with_capacity(self.windows.len());
        let mut ui_description_cache = vec![UiDescription::default(); self.windows.len()];
        let mut css_cache = vec![Css::new(); self.windows.len()];

        // first redraw, initialize cache  
        {
            let mut app_state = self.app_state.lock().unwrap();
            for (window_id, _) in self.windows.iter_mut() {
                ui_state_cache.push(UiState::from_app_state(&*app_state, *window_id));
            }

            for (window_id, _) in self.windows.iter_mut() {
                let new_css = app_state.data.get_css(*window_id);
                css_cache[window_id.id] = new_css.clone();
                ui_description_cache[window_id.id] = UiDescription::from_ui_state(&ui_state_cache[window_id.id], &mut css_cache[window_id.id]);
            }
        }      

        
        'render_loop: loop {

            use glium::glutin::WindowEvent;
            use glium::glutin::Event;

            // TODO: Use threads on a per-window basis.
            // Currently, events in one window will block all others
            for (window_id, window) in self.windows.iter_mut() {

                let mut should_redraw_window = false;
                let mut should_swap_window = false;
                let mut should_hittest = false;
                let mut cur_cursor_pos = (0.0, 0.0);
                let mut new_window_size = None;
                let mut new_dpi_factor = None;

                window.events_loop.poll_events(|event| {
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
                                    should_hittest = true;
                                    cur_cursor_pos = position;

                                    let _ = window_id;
                                    let _ = device_id;
                                    let _ = modifiers;
                                },
                                WindowEvent::Resized(w, h) => {
                                    new_window_size = Some((w, h));
                                },
                                WindowEvent::Refresh => {
                                    should_redraw_window = true;
                                },
                                WindowEvent::HiDPIFactorChanged(dpi) => {
                                    new_dpi_factor = Some(dpi);
                                }
                                _ => { },
                            }
                        },
                        Event::Awakened => {
                            should_swap_window = true;
                        },
                        _ => { },
                    }
                });

                // update the state
                if should_swap_window {
                    window.display.swap_buffers().unwrap();
                }

                if should_hittest {
                    use webrender::api::WorldPoint;
                    let point = WorldPoint::new(cur_cursor_pos.0 as f32, cur_cursor_pos.1 as f32);
                    let hit_test_results = hit_test_ui(&window.internal.api, window.internal.document_id, Some(window.internal.pipeline_id), point);
                    
                    if !hit_test_results.items.is_empty() { 
                        // note: we only need to redraw if the state or the CSS was modified / invalidated
                        should_redraw_window = true;
                    }

                    for item in hit_test_results.items {
                        if let Some(callback_list) = ui_state_cache[window_id.id].node_ids_to_callbacks_list.get(&item.tag.0) {
                            // TODO: filter by `On` type (On::MouseOver, On::MouseLeave, etc.)
                            // currently, just invoke all actions
                            for callback_id in callback_list.values() {
                                let callback_fn = ui_state_cache[window_id.id].callback_list[callback_id];
                                use dom::Callback::*;
                                match callback_fn {
                                    Sync(callback) => { (callback)(&mut *self.app_state.lock().unwrap()); },
                                    Async(callback) => { (callback)(self.app_state.clone()) },
                                }
                            }
                        }
                    }
                }

                if let Some((w, h)) = new_window_size {
                    use webrender::api::{DeviceUintSize, DeviceUintPoint, DeviceUintRect, LayoutSize, Transaction};
                    window.internal.layout_size = LayoutSize::new(w as f32, h as f32);
                    window.internal.framebuffer_size = DeviceUintSize::new(w, h);
                    let mut txn = Transaction::new();
                    let bounds = DeviceUintRect::new(DeviceUintPoint::new(0, 0), window.internal.framebuffer_size);
                    txn.set_window_parameters(window.internal.framebuffer_size, bounds, window.internal.hidpi_factor);
                    window.internal.api.send_transaction(window.internal.document_id, txn);
                    render(window, window_id, &ui_description_cache[window_id.id]);
                }

                if let Some(dpi) = new_dpi_factor {
                    use webrender::api::{DeviceUintPoint, DeviceUintRect, Transaction};
                    window.internal.hidpi_factor = dpi;
                    let mut txn = Transaction::new();
                    let bounds = DeviceUintRect::new(DeviceUintPoint::new(0, 0), window.internal.framebuffer_size);
                    txn.set_window_parameters(window.internal.framebuffer_size, bounds, window.internal.hidpi_factor);                    window.internal.api.send_transaction(window.internal.document_id, txn);
                    render(window, window_id, &ui_description_cache[window_id.id]);
                }

                let mut app_state = self.app_state.lock().unwrap();
                let new_css = app_state.data.get_css(*window_id);
            
                // Note: this comparison might be expensive, but it is more expensive to re-parse the CSS
                if css_cache[window_id.id].rules != new_css.rules {
                    // Re-styles (NOT re-layouts!) the UI. Possibly very performance-heavy.
                    css_cache[window_id.id] = new_css.clone();
                    ui_description_cache[window_id.id] = UiDescription::from_ui_state(&ui_state_cache[window_id.id], &mut css_cache[window_id.id]);
                }

                // Re-layouts the UI.
                if should_redraw_window {
                    render(window, window_id, &ui_description_cache[window_id.id]);
                }
            }

            ::std::thread::sleep(::std::time::Duration::from_millis(16));
        }
    }
}

fn render<T: LayoutScreen>(window: &mut Window, _window_id: &WindowId, ui_description: &UiDescription<T>) 
{
    use webrender::api::*;
    use display_list::DisplayList;

    let display_list = DisplayList::new_from_ui_description(ui_description);
    
    let builder = display_list.into_display_list_builder(
        window.internal.pipeline_id,
        window.internal.layout_size,
        &mut window.solver.solver);

    let resources = ResourceUpdates::new();

    let mut txn = Transaction::new();
    
    txn.set_display_list(
        window.internal.epoch,
        None,
        window.internal.layout_size,
        builder.finalize(),
        true,
    );

    txn.update_resources(resources);
    txn.set_root_pipeline(window.internal.pipeline_id);
    txn.generate_frame();
    window.internal.api.send_transaction(window.internal.document_id, txn);

    window.renderer.as_mut().unwrap().update();
    window.renderer.as_mut().unwrap().render(window.internal.framebuffer_size).unwrap();
}