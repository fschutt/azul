use {
    app::AppStateNoData,
    callbacks::{
        CallbackInfo, DefaultCallback, DontRedraw, Redraw, StackCheckedPointer, UpdateScreen,
    },
    dom::{Dom, On, TabIndex},
    prelude::VirtualKeyCode,
    window::FakeWindow,
};

/// A checkbox ☑ that can be activated with either the left mouse button or
/// the space bar when the widget has focus.
///
/// TODO: Use an actual checkbox icon/glyph to indicate the state of the checkbox
///       instead of a "true"/"false" label.
#[derive(Clone, Copy, Debug, Default)]
pub struct CheckBox;

impl CheckBox {
    pub fn new() -> Self {
        CheckBox {}
    }

    pub fn dom<T>(self, state: &CheckBoxState, t: &T, window: &mut FakeWindow<T>) -> Dom<T> {
        if let Some(ptr) = StackCheckedPointer::new(t, state) {
            let mut dom = Dom::div()
                .with_class("__azul-native-checkbox-container")
                .with_tab_index(TabIndex::Auto)
                .with_child(
                    Dom::label(if state.checked {
                        // "\u{2611}"
                        "true"
                    } else {
                        // "\u{2610}"
                        "false"
                    })
                    .with_class("__azul-native-checkbox-ballot"),
                );

            if let Some(label) = &state.label {
                dom.add_child(Dom::label(label.clone()).with_class("__azul-native-checkbox-label"));
            }

            let lmb_down_callback_id =
                window.add_callback(ptr, DefaultCallback(Self::on_left_mouse_down));
            let lmb_up_callback_id =
                window.add_callback(ptr, DefaultCallback(Self::on_left_mouse_up));
            let focus_recieved_callback_id =
                window.add_callback(ptr, DefaultCallback(Self::on_focus_recieved));
            let focus_lost_callback_id =
                window.add_callback(ptr, DefaultCallback(Self::on_focus_lost));
            let virtual_key_down_callback_id =
                window.add_callback(ptr, DefaultCallback(Self::on_virtual_key_down));
            let virtual_key_up_callback_id =
                window.add_callback(ptr, DefaultCallback(Self::on_virtual_key_up));

            dom.add_default_callback_id(On::LeftMouseDown, lmb_down_callback_id);
            dom.add_default_callback_id(On::LeftMouseUp, lmb_up_callback_id);
            dom.add_default_callback_id(On::FocusReceived, focus_recieved_callback_id);
            dom.add_default_callback_id(On::FocusLost, focus_lost_callback_id);
            dom.add_default_callback_id(On::VirtualKeyDown, virtual_key_down_callback_id);
            dom.add_default_callback_id(On::VirtualKeyUp, virtual_key_up_callback_id);

            dom
        } else {
            Dom::label("Cannot create checkbox from heap-allocated CheckBoxState")
        }
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn on_left_mouse_down<T>(
        ptr: &StackCheckedPointer<T>,
        data: &mut AppStateNoData<T>,
        event: &mut CallbackInfo<T>,
    ) -> UpdateScreen {
        unsafe { ptr.invoke_mut(CheckBoxState::on_left_mouse_down, data, event) }
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn on_left_mouse_up<T>(
        ptr: &StackCheckedPointer<T>,
        data: &mut AppStateNoData<T>,
        event: &mut CallbackInfo<T>,
    ) -> UpdateScreen {
        unsafe { ptr.invoke_mut(CheckBoxState::on_left_mouse_up, data, event) }
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn on_focus_recieved<T>(
        ptr: &StackCheckedPointer<T>,
        data: &mut AppStateNoData<T>,
        event: &mut CallbackInfo<T>,
    ) -> UpdateScreen {
        unsafe { ptr.invoke_mut(CheckBoxState::on_focus_recieved, data, event) }
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn on_focus_lost<T>(
        ptr: &StackCheckedPointer<T>,
        data: &mut AppStateNoData<T>,
        event: &mut CallbackInfo<T>,
    ) -> UpdateScreen {
        unsafe { ptr.invoke_mut(CheckBoxState::on_focus_lost, data, event) }
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn on_virtual_key_down<T>(
        ptr: &StackCheckedPointer<T>,
        data: &mut AppStateNoData<T>,
        event: &mut CallbackInfo<T>,
    ) -> UpdateScreen {
        unsafe { ptr.invoke_mut(CheckBoxState::on_virtual_key_down, data, event) }
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn on_virtual_key_up<T>(
        ptr: &StackCheckedPointer<T>,
        data: &mut AppStateNoData<T>,
        event: &mut CallbackInfo<T>,
    ) -> UpdateScreen {
        unsafe { ptr.invoke_mut(CheckBoxState::on_virtual_key_up, data, event) }
    }
}

/// The model that drives the `CheckBox` widget.
#[derive(Debug)]
pub struct CheckBoxState {
    used: bool,
    initial: bool,
    checked: bool,
    label: Option<String>,
    lmb_pressed_down: bool,
    has_focus: bool,
    space_pressed_down: bool,
}

impl Default for CheckBoxState {
    fn default() -> Self {
        Self {
            used: false,
            initial: false,
            checked: false,
            label: None,
            lmb_pressed_down: false,
            has_focus: false,
            space_pressed_down: false,
        }
    }
}

impl CheckBoxState {
    pub fn new(initial: bool, label: Option<String>) -> Self {
        Self {
            used: false,
            initial,
            checked: initial,
            label,
            lmb_pressed_down: false,
            has_focus: false,
            space_pressed_down: false,
        }
    }

    /// Return `true` if the checkbox is checed
    pub fn checked(&self) -> bool {
        self.checked
    }

    /// Return an immutable reference to the label string
    pub fn label(&self) -> &Option<String> {
        &self.label
    }

    /// Return a mutable reference to the label string
    pub fn label_mut(&mut self) -> &mut Option<String> {
        &mut self.label
    }

    /// Return `true` if the checkbox has changed state at alll
    pub fn used(&self) -> bool {
        self.used
    }

    /// Reset the `used` flag and set `checked` to its initial state
    pub fn reset(&mut self) {
        self.checked = self.initial;
        self.used = false;
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn on_left_mouse_down<T>(
        &mut self,
        _app_state: &mut AppStateNoData<T>,
        _window_event: &mut CallbackInfo<T>,
    ) -> UpdateScreen {
        if !self.space_pressed_down {
            self.lmb_pressed_down = true;
        }

        DontRedraw
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn on_left_mouse_up<T>(
        &mut self,
        _app_state: &mut AppStateNoData<T>,
        _window_event: &mut CallbackInfo<T>,
    ) -> UpdateScreen {
        if self.lmb_pressed_down {
            self.lmb_pressed_down = false;

            if self.has_focus {
                self.toggle();

                return Redraw;
            }
        }

        DontRedraw
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn on_focus_recieved<T>(
        &mut self,
        _app_state: &mut AppStateNoData<T>,
        _window_event: &mut CallbackInfo<T>,
    ) -> UpdateScreen {
        self.has_focus = true;

        DontRedraw
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn on_focus_lost<T>(
        &mut self,
        _app_state: &mut AppStateNoData<T>,
        _window_event: &mut CallbackInfo<T>,
    ) -> UpdateScreen {
        self.lmb_pressed_down = false;
        self.space_pressed_down = false;
        self.has_focus = false;

        DontRedraw
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn on_virtual_key_down<T>(
        &mut self,
        app_state: &mut AppStateNoData<T>,
        window_event: &mut CallbackInfo<T>,
    ) -> UpdateScreen {
        if self.has_focus && !self.space_pressed_down && !self.lmb_pressed_down {
            self.space_pressed_down = app_state
                .windows
                .get(window_event.window_id)
                .map(|window| window.state.get_keyboard_state())
                .and_then(|keyboard_state| keyboard_state.latest_virtual_keycode)
                .map(|virtual_key_code| virtual_key_code == VirtualKeyCode::Space)
                .unwrap_or(false);
        }

        DontRedraw
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn on_virtual_key_up<T>(
        &mut self,
        app_state: &mut AppStateNoData<T>,
        window_event: &mut CallbackInfo<T>,
    ) -> UpdateScreen {
        if self.space_pressed_down && self.has_focus {
            self.space_pressed_down = false;

            let is_space_released = app_state
                .windows
                .get(window_event.window_id)
                .map(|window| window.state.get_keyboard_state())
                .map(|keyboard_state| {
                    // Note: `keyboard_state.latest_virtual_keycode` is always
                    //       `None` for a `VirtualKeyUp` event. This is why we
                    //        check that the desired `VirtualKeyCode` is not
                    //        among the keys which are currently pressed down.
                    !keyboard_state
                        .current_virtual_keycodes
                        .contains(&VirtualKeyCode::Space)
                })
                .unwrap_or(false);

            if is_space_released {
                self.toggle();
                return Redraw;
            }
        }

        DontRedraw
    }

    fn toggle(&mut self) {
        self.checked = !self.checked;
        self.used = true;
    }
}
