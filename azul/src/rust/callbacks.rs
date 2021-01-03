    #![allow(dead_code, unused_imports)]
    //! Callback type definitions + struct definitions of `CallbackInfo`s
    use crate::dll::*;
    use std::ffi::c_void;

    #[derive(Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
    #[repr(C)]
    pub struct Ref<'a, T> {
        ptr: &'a T,
        _sharing_info_ptr: *const RefAnySharingInfo,
    }

    impl<'a, T> Drop for Ref<'a, T> {
        fn drop(&mut self) {
            (crate::dll::get_azul_dll().az_ref_any_sharing_info_decrease_ref)(unsafe { &mut *(self._sharing_info_ptr as *mut RefAnySharingInfo) });
        }
    }

    impl<'a, T> std::ops::Deref for Ref<'a, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            self.ptr
        }
    }

    #[derive(Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
    #[repr(C)]
    pub struct RefMut<'a, T> {
        ptr: &'a mut T,
        _sharing_info_ptr: *const RefAnySharingInfo,
    }

    impl<'a, T> Drop for RefMut<'a, T> {
        fn drop(&mut self) {
            (crate::dll::get_azul_dll().az_ref_any_sharing_info_decrease_refmut)(unsafe { &mut *(self._sharing_info_ptr as *mut RefAnySharingInfo) });
        }
    }

    impl<'a, T> std::ops::Deref for RefMut<'a, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &*self.ptr
        }
    }

    impl<'a, T> std::ops::DerefMut for RefMut<'a, T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            self.ptr
        }
    }

    impl RefAny {

        /// Creates a new, type-erased pointer by casting the `T` value into a `Vec<u8>` and saving the length + type ID
        pub fn new<T: 'static>(value: T) -> Self {
            use crate::dll::*;

            extern "C" fn default_custom_destructor<U: 'static>(ptr: *const c_void) {
                use std::{mem, ptr};

                // note: in the default constructor, we do not need to check whether U == T

                unsafe {
                    // copy the struct from the heap to the stack and call mem::drop on U to run the destructor
                    let mut stack_mem = mem::MaybeUninit::<U>::uninit().assume_init();
                    ptr::copy_nonoverlapping(ptr as *const U, &mut stack_mem as *mut U, mem::size_of::<U>());
                    mem::drop(stack_mem);
                }
            }

            let type_name_str = ::std::any::type_name::<T>();
            let st = crate::str::String::from_utf8_unchecked(type_name_str.as_ptr(), type_name_str.len());
            let s = (crate::dll::get_azul_dll().az_ref_any_new_c)(
                (&value as *const T) as *const c_void,
                ::std::mem::size_of::<T>(),
                Self::get_type_id::<T>(),
                st,
                default_custom_destructor::<T>,
            );
            ::std::mem::forget(value); // do not run the destructor of T here!
            s
        }

        /// Downcasts the type-erased pointer to a type `&U`, returns `None` if the types don't match
        #[inline]
        pub fn borrow<'a, U: 'static>(&'a self) -> Option<Ref<'a, U>> {
            let is_same_type = (crate::dll::get_azul_dll().az_ref_any_is_type)(self, Self::get_type_id::<U>());
            if !is_same_type { return None; }

            let can_be_shared = (crate::dll::get_azul_dll().az_ref_any_can_be_shared)(self);
            if !can_be_shared { return None; }

            Some(Ref {
                ptr: unsafe { &*(self._internal_ptr as *const U) },
                _sharing_info_ptr: self._sharing_info_ptr,
            })
        }

        /// Downcasts the type-erased pointer to a type `&mut U`, returns `None` if the types don't match
        #[inline]
        pub fn borrow_mut<'a, U: 'static>(&'a mut self) -> Option<RefMut<'a, U>> {
            let is_same_type = (crate::dll::get_azul_dll().az_ref_any_is_type)(self, Self::get_type_id::<U>());
            if !is_same_type { return None; }

            let can_be_shared_mut = (crate::dll::get_azul_dll().az_ref_any_can_be_shared_mut)(self);
            if !can_be_shared_mut { return None; }

            Some(RefMut {
                ptr: unsafe { &mut *(self._internal_ptr as *mut U) },
                _sharing_info_ptr: self._sharing_info_ptr,
            })
        }

        // Returns the typeid of `T` as a u64 (necessary because `std::any::TypeId` is not C-ABI compatible)
        #[inline]
        pub fn get_type_id<T: 'static>() -> u64 {
            use std::any::TypeId;
            use std::mem;

            // fast method to serialize the type id into a u64
            let t_id = TypeId::of::<T>();
            let struct_as_bytes = unsafe { ::std::slice::from_raw_parts((&t_id as *const TypeId) as *const u8, mem::size_of::<TypeId>()) };
            struct_as_bytes.into_iter().enumerate().map(|(s_pos, s)| ((*s as u64) << s_pos)).sum()
        }
    }    use crate::dom::NodeType;
    use crate::str::String;
    use crate::window::{WindowCreateOptions, WindowState};
    use crate::css::CssProperty;
    use crate::task::{Task, Timer, TimerId};


    /// `NodeId` struct
    pub use crate::dll::AzNodeId as NodeId;

    impl std::fmt::Debug for NodeId { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_node_id_fmt_debug)(self)) } }
    impl Clone for NodeId { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_node_id_deep_copy)(self) } }
    impl Drop for NodeId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_node_id_delete)(self); } }


    /// `DomId` struct
    pub use crate::dll::AzDomId as DomId;

    impl std::fmt::Debug for DomId { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_dom_id_fmt_debug)(self)) } }
    impl Clone for DomId { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_dom_id_deep_copy)(self) } }
    impl Drop for DomId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_dom_id_delete)(self); } }


    /// `DomNodeId` struct
    pub use crate::dll::AzDomNodeId as DomNodeId;

    impl std::fmt::Debug for DomNodeId { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_dom_node_id_fmt_debug)(self)) } }
    impl Clone for DomNodeId { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_dom_node_id_deep_copy)(self) } }
    impl Drop for DomNodeId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_dom_node_id_delete)(self); } }


    /// `HidpiAdjustedBounds` struct
    pub use crate::dll::AzHidpiAdjustedBounds as HidpiAdjustedBounds;

    impl HidpiAdjustedBounds {
        /// Returns the size of the bounds in logical units
        pub fn get_logical_size(&self)  -> crate::window::LogicalSize { (crate::dll::get_azul_dll().az_hidpi_adjusted_bounds_get_logical_size)(self) }
        /// Returns the size of the bounds in physical units
        pub fn get_physical_size(&self)  -> crate::window::PhysicalSizeU32 { (crate::dll::get_azul_dll().az_hidpi_adjusted_bounds_get_physical_size)(self) }
        /// Returns the hidpi factor of the bounds
        pub fn get_hidpi_factor(&self)  -> f32 { (crate::dll::get_azul_dll().az_hidpi_adjusted_bounds_get_hidpi_factor)(self) }
    }

    impl std::fmt::Debug for HidpiAdjustedBounds { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_hidpi_adjusted_bounds_fmt_debug)(self)) } }
    impl Clone for HidpiAdjustedBounds { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_hidpi_adjusted_bounds_deep_copy)(self) } }
    impl Drop for HidpiAdjustedBounds { fn drop(&mut self) { (crate::dll::get_azul_dll().az_hidpi_adjusted_bounds_delete)(self); } }


    /// `LayoutCallback` struct
    pub use crate::dll::AzLayoutCallback as LayoutCallback;

    impl std::fmt::Debug for LayoutCallback { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_callback_fmt_debug)(self)) } }
    impl Clone for LayoutCallback { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_callback_deep_copy)(self) } }
    impl Drop for LayoutCallback { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_callback_delete)(self); } }


    pub use crate::dll::AzLayoutCallbackType as LayoutCallbackType;

    /// `Callback` struct
    pub use crate::dll::AzCallback as Callback;

    impl std::fmt::Debug for Callback { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_callback_fmt_debug)(self)) } }
    impl Clone for Callback { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_callback_deep_copy)(self) } }
    impl Drop for Callback { fn drop(&mut self) { (crate::dll::get_azul_dll().az_callback_delete)(self); } }


    /// Defines the focus target for the next frame
    pub use crate::dll::AzFocusTarget as FocusTarget;

    impl std::fmt::Debug for FocusTarget { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_focus_target_fmt_debug)(self)) } }
    impl Clone for FocusTarget { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_focus_target_deep_copy)(self) } }
    impl Drop for FocusTarget { fn drop(&mut self) { (crate::dll::get_azul_dll().az_focus_target_delete)(self); } }


    /// `FocusTargetPath` struct
    pub use crate::dll::AzFocusTargetPath as FocusTargetPath;

    impl std::fmt::Debug for FocusTargetPath { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_focus_target_path_fmt_debug)(self)) } }
    impl Clone for FocusTargetPath { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_focus_target_path_deep_copy)(self) } }
    impl Drop for FocusTargetPath { fn drop(&mut self) { (crate::dll::get_azul_dll().az_focus_target_path_delete)(self); } }


    pub use crate::dll::AzCallbackReturn as CallbackReturn;
    pub use crate::dll::AzCallbackType as CallbackType;

    /// `CallbackInfo` struct
    pub use crate::dll::AzCallbackInfoPtr as CallbackInfo;

    impl CallbackInfo {
        /// Returns the `DomNodeId` of the element that the callback was attached to.
        pub fn get_hit_node(&self)  -> crate::callbacks::DomNodeId { (crate::dll::get_azul_dll().az_callback_info_ptr_get_hit_node)(self) }
        /// Returns the `LayoutPoint` of the cursor in the viewport (relative to the origin of the `Dom`). Set to `None` if the cursor is not in the current window.
        pub fn get_cursor_relative_to_viewport(&self)  -> crate::option::OptionLayoutPoint { (crate::dll::get_azul_dll().az_callback_info_ptr_get_cursor_relative_to_viewport)(self) }
        /// Returns the `LayoutPoint` of the cursor in the viewport (relative to the origin of the `Dom`). Set to `None` if the cursor is not hovering over the current node.
        pub fn get_cursor_relative_to_node(&self)  -> crate::option::OptionLayoutPoint { (crate::dll::get_azul_dll().az_callback_info_ptr_get_cursor_relative_to_node)(self) }
        /// Returns the parent `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_parent(&self, node_id: DomNodeId)  -> crate::option::OptionDomNodeId { (crate::dll::get_azul_dll().az_callback_info_ptr_get_parent)(self, node_id) }
        /// Returns the previous siblings `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_previous_sibling(&self, node_id: DomNodeId)  -> crate::option::OptionDomNodeId { (crate::dll::get_azul_dll().az_callback_info_ptr_get_previous_sibling)(self, node_id) }
        /// Returns the next siblings `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_next_sibling(&self, node_id: DomNodeId)  -> crate::option::OptionDomNodeId { (crate::dll::get_azul_dll().az_callback_info_ptr_get_next_sibling)(self, node_id) }
        /// Returns the next siblings `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_first_child(&self, node_id: DomNodeId)  -> crate::option::OptionDomNodeId { (crate::dll::get_azul_dll().az_callback_info_ptr_get_first_child)(self, node_id) }
        /// Returns the next siblings `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_last_child(&self, node_id: DomNodeId)  -> crate::option::OptionDomNodeId { (crate::dll::get_azul_dll().az_callback_info_ptr_get_last_child)(self, node_id) }
        /// Returns a copy of the current windows `WindowState`.
        pub fn get_window_state(&self)  -> crate::window::WindowState { (crate::dll::get_azul_dll().az_callback_info_ptr_get_window_state)(self) }
        /// Returns a copy of the internal `KeyboardState`. Same as `self.get_window_state().keyboard_state`
        pub fn get_keyboard_state(&self)  -> crate::window::KeyboardState { (crate::dll::get_azul_dll().az_callback_info_ptr_get_keyboard_state)(self) }
        /// Returns a copy of the internal `MouseState`. Same as `self.get_window_state().mouse_state`
        pub fn get_mouse_state(&self)  -> crate::window::MouseState { (crate::dll::get_azul_dll().az_callback_info_ptr_get_mouse_state)(self) }
        /// Returns a copy of the current windows `RawWindowHandle`.
        pub fn get_current_window_handle(&self)  -> crate::window::RawWindowHandle { (crate::dll::get_azul_dll().az_callback_info_ptr_get_current_window_handle)(self) }
        /// Returns a **reference-counted copy** of the current windows `GlContextPtr`. You can use this to render OpenGL textures.
        pub fn get_gl_context(&self)  -> crate::gl::GlContextPtr { (crate::dll::get_azul_dll().az_callback_info_ptr_get_gl_context)(self) }
        /// Returns whether the node has a given `NodeType`. Returns `false` on an invalid NodeId.
        pub fn node_is_type(&self, node_id: DomNodeId, node_type: NodeType)  -> bool { (crate::dll::get_azul_dll().az_callback_info_ptr_node_is_type)(self, node_id, node_type) }
        /// Returns whether the node has a given `id`. Returns `false` on an invalid NodeId.
        pub fn node_has_id(&self, node_id: DomNodeId, id: String)  -> bool { (crate::dll::get_azul_dll().az_callback_info_ptr_node_has_id)(self, node_id, id) }
        /// Returns whether the node has a given `id`. Returns `false` on an invalid NodeId.
        pub fn node_has_class(&self, node_id: DomNodeId, id: String)  -> bool { (crate::dll::get_azul_dll().az_callback_info_ptr_node_has_class)(self, node_id, id) }
        /// Sets the new `WindowState` for the next frame. The window is updated after all callbacks are run.
        pub fn set_window_state(&mut self, new_state: WindowState)  { (crate::dll::get_azul_dll().az_callback_info_ptr_set_window_state)(self, new_state) }
        /// Sets the new `FocusTarget` for the next frame. Note that this will emit a `On::FocusLost` and `On::FocusReceived` event, if the focused node has changed.
        pub fn set_focus(&mut self, target: FocusTarget)  { (crate::dll::get_azul_dll().az_callback_info_ptr_set_focus)(self, target) }
        /// Sets a `CssProperty` on a given ndoe to its new value. If this property change affects the layout, this will automatically trigger a relayout and redraw of the screen.
        pub fn set_css_property(&mut self, node_id: DomNodeId, new_property: CssProperty)  { (crate::dll::get_azul_dll().az_callback_info_ptr_set_css_property)(self, node_id, new_property) }
        /// Stops the propagation of the current callback event type to the parent. Events are bubbled from the inside out (children first, then parents), this event stops the propagation of the event to the parent.
        pub fn stop_propagation(&mut self)  { (crate::dll::get_azul_dll().az_callback_info_ptr_stop_propagation)(self) }
        /// Spawns a new window with the given `WindowCreateOptions`.
        pub fn create_window(&mut self, new_window: WindowCreateOptions)  { (crate::dll::get_azul_dll().az_callback_info_ptr_create_window)(self, new_window) }
        /// Adds a new `Task` to the runtime. See the documentation for `Task` for more information.
        pub fn add_task(&mut self, task: Task)  { (crate::dll::get_azul_dll().az_callback_info_ptr_add_task)(self, task) }
        /// Adds a new `Timer` to the runtime. See the documentation for `Timer` for more information.
        pub fn add_timer(&mut self, id: TimerId, timer: Timer)  { (crate::dll::get_azul_dll().az_callback_info_ptr_add_timer)(self, id, timer) }
    }

    impl std::fmt::Debug for CallbackInfo { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_callback_info_ptr_fmt_debug)(self)) } }
    impl Drop for CallbackInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_callback_info_ptr_delete)(self); } }


    /// `UpdateScreen` struct
    pub use crate::dll::AzUpdateScreen as UpdateScreen;

    impl std::fmt::Debug for UpdateScreen { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_update_screen_fmt_debug)(self)) } }
    impl Clone for UpdateScreen { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_update_screen_deep_copy)(self) } }
    impl Drop for UpdateScreen { fn drop(&mut self) { (crate::dll::get_azul_dll().az_update_screen_delete)(self); } }

    impl From<Option<()>> for UpdateScreen { fn from(o: Option<()>) -> Self { match o { None => UpdateScreen::DontRedraw, Some(_) => UpdateScreen::Redraw } } }

    impl From<UpdateScreen> for Option<()> { fn from(o: UpdateScreen) -> Self { match o { UpdateScreen::Redraw => Some(()), _ => None } } }


    /// `IFrameCallback` struct
    pub use crate::dll::AzIFrameCallback as IFrameCallback;

    impl std::fmt::Debug for IFrameCallback { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_i_frame_callback_fmt_debug)(self)) } }
    impl Clone for IFrameCallback { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_i_frame_callback_deep_copy)(self) } }
    impl Drop for IFrameCallback { fn drop(&mut self) { (crate::dll::get_azul_dll().az_i_frame_callback_delete)(self); } }


    pub use crate::dll::AzIFrameCallbackType as IFrameCallbackType;

    /// `IFrameCallbackInfo` struct
    pub use crate::dll::AzIFrameCallbackInfoPtr as IFrameCallbackInfo;

    impl std::fmt::Debug for IFrameCallbackInfo { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_i_frame_callback_info_ptr_fmt_debug)(self)) } }
    impl Drop for IFrameCallbackInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_i_frame_callback_info_ptr_delete)(self); } }


    /// `IFrameCallbackReturn` struct
    pub use crate::dll::AzIFrameCallbackReturn as IFrameCallbackReturn;

    impl std::fmt::Debug for IFrameCallbackReturn { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_i_frame_callback_return_fmt_debug)(self)) } }
    impl Clone for IFrameCallbackReturn { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_i_frame_callback_return_deep_copy)(self) } }
    impl Drop for IFrameCallbackReturn { fn drop(&mut self) { (crate::dll::get_azul_dll().az_i_frame_callback_return_delete)(self); } }


    /// `GlCallback` struct
    pub use crate::dll::AzGlCallback as GlCallback;

    impl std::fmt::Debug for GlCallback { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_gl_callback_fmt_debug)(self)) } }
    impl Clone for GlCallback { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_gl_callback_deep_copy)(self) } }
    impl Drop for GlCallback { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gl_callback_delete)(self); } }


    pub use crate::dll::AzGlCallbackType as GlCallbackType;

    /// `GlCallbackInfo` struct
    pub use crate::dll::AzGlCallbackInfoPtr as GlCallbackInfo;

    impl GlCallbackInfo {
        /// Returns a copy of the internal `GlContextPtr`
        pub fn get_gl_context(&self)  -> crate::gl::GlContextPtr { (crate::dll::get_azul_dll().az_gl_callback_info_ptr_get_gl_context)(self) }
    }

    impl std::fmt::Debug for GlCallbackInfo { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_gl_callback_info_ptr_fmt_debug)(self)) } }
    impl Drop for GlCallbackInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gl_callback_info_ptr_delete)(self); } }


    /// `GlCallbackReturn` struct
    pub use crate::dll::AzGlCallbackReturn as GlCallbackReturn;

    impl std::fmt::Debug for GlCallbackReturn { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_gl_callback_return_fmt_debug)(self)) } }
    impl Drop for GlCallbackReturn { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gl_callback_return_delete)(self); } }


    /// `TimerCallback` struct
    pub use crate::dll::AzTimerCallback as TimerCallback;

    impl std::fmt::Debug for TimerCallback { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_timer_callback_fmt_debug)(self)) } }
    impl Clone for TimerCallback { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_timer_callback_deep_copy)(self) } }
    impl Drop for TimerCallback { fn drop(&mut self) { (crate::dll::get_azul_dll().az_timer_callback_delete)(self); } }


    /// `TimerCallbackType` struct
    pub use crate::dll::AzTimerCallbackTypePtr as TimerCallbackType;

    impl std::fmt::Debug for TimerCallbackType { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_timer_callback_type_ptr_fmt_debug)(self)) } }


    /// `TimerCallbackInfo` struct
    pub use crate::dll::AzTimerCallbackInfoPtr as TimerCallbackInfo;

    impl TimerCallbackInfo {
        /// Returns a copy of the internal `RefAny`
        pub fn get_state(&self)  -> crate::callbacks::RefAny { (crate::dll::get_azul_dll().az_timer_callback_info_ptr_get_state)(self) }
    }

    impl std::fmt::Debug for TimerCallbackInfo { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_timer_callback_info_ptr_fmt_debug)(self)) } }
    impl Drop for TimerCallbackInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_timer_callback_info_ptr_delete)(self); } }


    /// `TimerCallbackReturn` struct
    pub use crate::dll::AzTimerCallbackReturn as TimerCallbackReturn;

    impl std::fmt::Debug for TimerCallbackReturn { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_timer_callback_return_fmt_debug)(self)) } }
    impl Clone for TimerCallbackReturn { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_timer_callback_return_deep_copy)(self) } }
    impl Drop for TimerCallbackReturn { fn drop(&mut self) { (crate::dll::get_azul_dll().az_timer_callback_return_delete)(self); } }


    pub use crate::dll::AzThreadCallbackType as ThreadCallbackType;

    pub use crate::dll::AzTaskCallbackType as TaskCallbackType;

    pub use crate::dll::AzRefAnyDestructorType as RefAnyDestructorType;

    /// `RefAnySharingInfo` struct
    pub use crate::dll::AzRefAnySharingInfo as RefAnySharingInfo;

    impl RefAnySharingInfo {
        /// Calls the `RefAnySharingInfo::can_be_shared` function.
        pub fn can_be_shared(&self)  -> bool { (crate::dll::get_azul_dll().az_ref_any_sharing_info_can_be_shared)(self) }
        /// Calls the `RefAnySharingInfo::can_be_shared_mut` function.
        pub fn can_be_shared_mut(&self)  -> bool { (crate::dll::get_azul_dll().az_ref_any_sharing_info_can_be_shared_mut)(self) }
        /// Calls the `RefAnySharingInfo::increase_ref` function.
        pub fn increase_ref(&mut self)  { (crate::dll::get_azul_dll().az_ref_any_sharing_info_increase_ref)(self) }
        /// Calls the `RefAnySharingInfo::decrease_ref` function.
        pub fn decrease_ref(&mut self)  { (crate::dll::get_azul_dll().az_ref_any_sharing_info_decrease_ref)(self) }
        /// Calls the `RefAnySharingInfo::increase_refmut` function.
        pub fn increase_refmut(&mut self)  { (crate::dll::get_azul_dll().az_ref_any_sharing_info_increase_refmut)(self) }
        /// Calls the `RefAnySharingInfo::decrease_refmut` function.
        pub fn decrease_refmut(&mut self)  { (crate::dll::get_azul_dll().az_ref_any_sharing_info_decrease_refmut)(self) }
    }

    impl std::fmt::Debug for RefAnySharingInfo { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_ref_any_sharing_info_fmt_debug)(self)) } }
    impl Drop for RefAnySharingInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_ref_any_sharing_info_delete)(self); } }


    /// RefAny is a reference-counted, type-erased pointer, which stores a reference to a struct. `RefAny` can be up- and downcasted (this usually done via generics and can't be expressed in the Rust API)
    pub use crate::dll::AzRefAny as RefAny;

    impl RefAny {
        /// Creates a new `RefAny` instance.
        pub fn new_c(ptr: *const c_void, len: usize, type_id: u64, type_name: String, destructor: RefAnyDestructorType) -> Self { (crate::dll::get_azul_dll().az_ref_any_new_c)(ptr, len, type_id, type_name, destructor) }
        /// Calls the `RefAny::is_type` function.
        pub fn is_type(&self, type_id: u64)  -> bool { (crate::dll::get_azul_dll().az_ref_any_is_type)(self, type_id) }
        /// Calls the `RefAny::get_type_name` function.
        pub fn get_type_name(&self)  -> crate::str::String { (crate::dll::get_azul_dll().az_ref_any_get_type_name)(self) }
        /// Calls the `RefAny::can_be_shared` function.
        pub fn can_be_shared(&self)  -> bool { (crate::dll::get_azul_dll().az_ref_any_can_be_shared)(self) }
        /// Calls the `RefAny::can_be_shared_mut` function.
        pub fn can_be_shared_mut(&self)  -> bool { (crate::dll::get_azul_dll().az_ref_any_can_be_shared_mut)(self) }
        /// Calls the `RefAny::increase_ref` function.
        pub fn increase_ref(&self)  { (crate::dll::get_azul_dll().az_ref_any_increase_ref)(self) }
        /// Calls the `RefAny::decrease_ref` function.
        pub fn decrease_ref(&self)  { (crate::dll::get_azul_dll().az_ref_any_decrease_ref)(self) }
        /// Calls the `RefAny::increase_refmut` function.
        pub fn increase_refmut(&self)  { (crate::dll::get_azul_dll().az_ref_any_increase_refmut)(self) }
        /// Calls the `RefAny::decrease_refmut` function.
        pub fn decrease_refmut(&self)  { (crate::dll::get_azul_dll().az_ref_any_decrease_refmut)(self) }
    }

    impl std::fmt::Debug for RefAny { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_ref_any_fmt_debug)(self)) } }
    impl Clone for RefAny { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_ref_any_deep_copy)(self) } }
    impl Drop for RefAny { fn drop(&mut self) { (crate::dll::get_azul_dll().az_ref_any_delete)(self); } }


    /// `LayoutInfo` struct
    pub use crate::dll::AzLayoutInfoPtr as LayoutInfo;

    impl std::fmt::Debug for LayoutInfo { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_info_ptr_fmt_debug)(self)) } }
    impl Drop for LayoutInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_info_ptr_delete)(self); } }
