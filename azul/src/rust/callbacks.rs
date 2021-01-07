    #![allow(dead_code, unused_imports)]
    //! Callback type definitions + struct definitions of `CallbackInfo`s
    use crate::dll::*;
    use std::ffi::c_void;

    #[derive(Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
    #[repr(C)]
    pub struct Ref<'a, T> {
        ptr: &'a T,
        _sharing_info_ptr: *const AtomicRefCount,
    }

    impl<'a, T> Drop for Ref<'a, T> {
        fn drop(&mut self) {
            (crate::dll::get_azul_dll().az_atomic_ref_count_decrease_ref)(unsafe { &mut *(self._sharing_info_ptr as *mut AtomicRefCount) });
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
        _sharing_info_ptr: *const AtomicRefCount,
    }

    impl<'a, T> Drop for RefMut<'a, T> {
        fn drop(&mut self) {
            (crate::dll::get_azul_dll().az_atomic_ref_count_decrease_refmut)(unsafe { &mut *(self._sharing_info_ptr as *mut AtomicRefCount) });
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
    }    use crate::window::{WindowCreateOptions, WindowState};
    use crate::css::CssProperty;
    use crate::task::{ThreadId, Timer, TimerId};
    use crate::str::String;


    /// `NodeId` struct
    pub use crate::dll::AzNodeId as NodeId;

    impl Clone for NodeId { fn clone(&self) -> Self { *self } }
    impl Copy for NodeId { }


    /// `DomId` struct
    pub use crate::dll::AzDomId as DomId;

    impl Clone for DomId { fn clone(&self) -> Self { *self } }
    impl Copy for DomId { }


    /// `DomNodeId` struct
    pub use crate::dll::AzDomNodeId as DomNodeId;

    impl Clone for DomNodeId { fn clone(&self) -> Self { *self } }
    impl Copy for DomNodeId { }


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

    impl Clone for HidpiAdjustedBounds { fn clone(&self) -> Self { *self } }
    impl Copy for HidpiAdjustedBounds { }


    /// `LayoutCallback` struct
    pub use crate::dll::AzLayoutCallback as LayoutCallback;

    impl Clone for LayoutCallback { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutCallback { }


    pub use crate::dll::AzLayoutCallbackType as LayoutCallbackType;

    /// `Callback` struct
    pub use crate::dll::AzCallback as Callback;

    impl Clone for Callback { fn clone(&self) -> Self { *self } }
    impl Copy for Callback { }


    /// Defines the focus target for the next frame
    pub use crate::dll::AzFocusTarget as FocusTarget;

    impl Clone for FocusTarget { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_focus_target_deep_copy)(self) } }
    impl Drop for FocusTarget { fn drop(&mut self) { (crate::dll::get_azul_dll().az_focus_target_delete)(self); } }


    /// `FocusTargetPath` struct
    pub use crate::dll::AzFocusTargetPath as FocusTargetPath;

    impl Clone for FocusTargetPath { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_focus_target_path_deep_copy)(self) } }
    impl Drop for FocusTargetPath { fn drop(&mut self) { (crate::dll::get_azul_dll().az_focus_target_path_delete)(self); } }


    pub use crate::dll::AzCallbackReturn as CallbackReturn;
    pub use crate::dll::AzCallbackType as CallbackType;

    /// `CallbackInfo` struct
    pub use crate::dll::AzCallbackInfo as CallbackInfo;

    impl CallbackInfo {
        /// Returns the `DomNodeId` of the element that the callback was attached to.
        pub fn get_hit_node(&self)  -> crate::callbacks::DomNodeId { (crate::dll::get_azul_dll().az_callback_info_get_hit_node)(self) }
        /// Returns the `LayoutPoint` of the cursor in the viewport (relative to the origin of the `Dom`). Set to `None` if the cursor is not in the current window.
        pub fn get_cursor_relative_to_viewport(&self)  -> crate::option::OptionLayoutPoint { (crate::dll::get_azul_dll().az_callback_info_get_cursor_relative_to_viewport)(self) }
        /// Returns the `LayoutPoint` of the cursor in the viewport (relative to the origin of the `Dom`). Set to `None` if the cursor is not hovering over the current node.
        pub fn get_cursor_relative_to_node(&self)  -> crate::option::OptionLayoutPoint { (crate::dll::get_azul_dll().az_callback_info_get_cursor_relative_to_node)(self) }
        /// Returns the parent `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_parent(&self, node_id: DomNodeId)  -> crate::option::OptionDomNodeId { (crate::dll::get_azul_dll().az_callback_info_get_parent)(self, node_id) }
        /// Returns the previous siblings `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_previous_sibling(&self, node_id: DomNodeId)  -> crate::option::OptionDomNodeId { (crate::dll::get_azul_dll().az_callback_info_get_previous_sibling)(self, node_id) }
        /// Returns the next siblings `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_next_sibling(&self, node_id: DomNodeId)  -> crate::option::OptionDomNodeId { (crate::dll::get_azul_dll().az_callback_info_get_next_sibling)(self, node_id) }
        /// Returns the next siblings `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_first_child(&self, node_id: DomNodeId)  -> crate::option::OptionDomNodeId { (crate::dll::get_azul_dll().az_callback_info_get_first_child)(self, node_id) }
        /// Returns the next siblings `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_last_child(&self, node_id: DomNodeId)  -> crate::option::OptionDomNodeId { (crate::dll::get_azul_dll().az_callback_info_get_last_child)(self, node_id) }
        /// Returns the `Dataset` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_dataset(&self, node_id: DomNodeId)  -> crate::option::OptionRefAny { (crate::dll::get_azul_dll().az_callback_info_get_dataset)(self, node_id) }
        /// Returns a copy of the current windows `WindowState`.
        pub fn get_window_state(&self)  -> crate::window::WindowState { (crate::dll::get_azul_dll().az_callback_info_get_window_state)(self) }
        /// Returns a copy of the internal `KeyboardState`. Same as `self.get_window_state().keyboard_state`
        pub fn get_keyboard_state(&self)  -> crate::window::KeyboardState { (crate::dll::get_azul_dll().az_callback_info_get_keyboard_state)(self) }
        /// Returns a copy of the internal `MouseState`. Same as `self.get_window_state().mouse_state`
        pub fn get_mouse_state(&self)  -> crate::window::MouseState { (crate::dll::get_azul_dll().az_callback_info_get_mouse_state)(self) }
        /// Returns a copy of the current windows `RawWindowHandle`.
        pub fn get_current_window_handle(&self)  -> crate::window::RawWindowHandle { (crate::dll::get_azul_dll().az_callback_info_get_current_window_handle)(self) }
        /// Returns a **reference-counted copy** of the current windows `GlContextPtr`. You can use this to render OpenGL textures.
        pub fn get_gl_context(&self)  -> crate::gl::GlContextPtr { (crate::dll::get_azul_dll().az_callback_info_get_gl_context)(self) }
        /// Sets the new `WindowState` for the next frame. The window is updated after all callbacks are run.
        pub fn set_window_state(&mut self, new_state: WindowState)  { (crate::dll::get_azul_dll().az_callback_info_set_window_state)(self, new_state) }
        /// Sets the new `FocusTarget` for the next frame. Note that this will emit a `On::FocusLost` and `On::FocusReceived` event, if the focused node has changed.
        pub fn set_focus(&mut self, target: FocusTarget)  { (crate::dll::get_azul_dll().az_callback_info_set_focus)(self, target) }
        /// Sets a `CssProperty` on a given ndoe to its new value. If this property change affects the layout, this will automatically trigger a relayout and redraw of the screen.
        pub fn set_css_property(&mut self, node_id: DomNodeId, new_property: CssProperty)  { (crate::dll::get_azul_dll().az_callback_info_set_css_property)(self, node_id, new_property) }
        /// Stops the propagation of the current callback event type to the parent. Events are bubbled from the inside out (children first, then parents), this event stops the propagation of the event to the parent.
        pub fn stop_propagation(&mut self)  { (crate::dll::get_azul_dll().az_callback_info_stop_propagation)(self) }
        /// Spawns a new window with the given `WindowCreateOptions`.
        pub fn create_window(&mut self, new_window: WindowCreateOptions)  { (crate::dll::get_azul_dll().az_callback_info_create_window)(self, new_window) }
        /// Starts a new `Thread` to the runtime. See the documentation for `Thread` for more information.
        pub fn start_thread(&mut self, id: ThreadId, thread_initialize_data: RefAny, writeback_data: RefAny, callback: ThreadCallbackType)  { (crate::dll::get_azul_dll().az_callback_info_start_thread)(self, id, thread_initialize_data, writeback_data, callback) }
        /// Adds a new `Timer` to the runtime. See the documentation for `Timer` for more information.
        pub fn start_timer(&mut self, id: TimerId, timer: Timer)  { (crate::dll::get_azul_dll().az_callback_info_start_timer)(self, id, timer) }
    }

    impl Drop for CallbackInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_callback_info_delete)(self); } }


    /// `UpdateScreen` struct
    pub use crate::dll::AzUpdateScreen as UpdateScreen;

    impl Clone for UpdateScreen { fn clone(&self) -> Self { *self } }
    impl Copy for UpdateScreen { }

    /// `IFrameCallback` struct
    pub use crate::dll::AzIFrameCallback as IFrameCallback;

    impl Clone for IFrameCallback { fn clone(&self) -> Self { *self } }
    impl Copy for IFrameCallback { }


    pub use crate::dll::AzIFrameCallbackType as IFrameCallbackType;

    /// `IFrameCallbackInfo` struct
    pub use crate::dll::AzIFrameCallbackInfo as IFrameCallbackInfo;

    impl Drop for IFrameCallbackInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_i_frame_callback_info_delete)(self); } }


    /// `IFrameCallbackReturn` struct
    pub use crate::dll::AzIFrameCallbackReturn as IFrameCallbackReturn;

    impl Clone for IFrameCallbackReturn { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_i_frame_callback_return_deep_copy)(self) } }
    impl Drop for IFrameCallbackReturn { fn drop(&mut self) { (crate::dll::get_azul_dll().az_i_frame_callback_return_delete)(self); } }


    /// `GlCallback` struct
    pub use crate::dll::AzGlCallback as GlCallback;

    impl Clone for GlCallback { fn clone(&self) -> Self { *self } }
    impl Copy for GlCallback { }


    pub use crate::dll::AzGlCallbackType as GlCallbackType;

    /// `GlCallbackInfo` struct
    pub use crate::dll::AzGlCallbackInfo as GlCallbackInfo;

    impl GlCallbackInfo {
        /// Returns a copy of the internal `GlContextPtr`
        pub fn get_gl_context(&self)  -> crate::gl::GlContextPtr { (crate::dll::get_azul_dll().az_gl_callback_info_get_gl_context)(self) }
    }

    impl Drop for GlCallbackInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gl_callback_info_delete)(self); } }


    /// `GlCallbackReturn` struct
    pub use crate::dll::AzGlCallbackReturn as GlCallbackReturn;

    impl Drop for GlCallbackReturn { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gl_callback_return_delete)(self); } }


    /// `TimerCallback` struct
    pub use crate::dll::AzTimerCallback as TimerCallback;

    impl Clone for TimerCallback { fn clone(&self) -> Self { *self } }
    impl Copy for TimerCallback { }


    pub use crate::dll::AzTimerCallbackType as TimerCallbackType;

    /// `TimerCallbackInfo` struct
    pub use crate::dll::AzTimerCallbackInfo as TimerCallbackInfo;

    impl Drop for TimerCallbackInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_timer_callback_info_delete)(self); } }


    /// `TimerCallbackReturn` struct
    pub use crate::dll::AzTimerCallbackReturn as TimerCallbackReturn;

    impl Clone for TimerCallbackReturn { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_timer_callback_return_deep_copy)(self) } }
    impl Drop for TimerCallbackReturn { fn drop(&mut self) { (crate::dll::get_azul_dll().az_timer_callback_return_delete)(self); } }


    pub use crate::dll::AzWriteBackCallbackType as WriteBackCallbackType;

    /// `WriteBackCallback` struct
    pub use crate::dll::AzWriteBackCallback as WriteBackCallback;

    impl Clone for WriteBackCallback { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_write_back_callback_deep_copy)(self) } }
    impl Drop for WriteBackCallback { fn drop(&mut self) { (crate::dll::get_azul_dll().az_write_back_callback_delete)(self); } }


    pub use crate::dll::AzThreadCallbackType as ThreadCallbackType;

    pub use crate::dll::AzRefAnyDestructorType as RefAnyDestructorType;

    /// `AtomicRefCount` struct
    pub use crate::dll::AzAtomicRefCount as AtomicRefCount;

    impl AtomicRefCount {
        /// Calls the `AtomicRefCount::can_be_shared` function.
        pub fn can_be_shared(&self)  -> bool { (crate::dll::get_azul_dll().az_atomic_ref_count_can_be_shared)(self) }
        /// Calls the `AtomicRefCount::can_be_shared_mut` function.
        pub fn can_be_shared_mut(&self)  -> bool { (crate::dll::get_azul_dll().az_atomic_ref_count_can_be_shared_mut)(self) }
        /// Calls the `AtomicRefCount::increase_ref` function.
        pub fn increase_ref(&mut self)  { (crate::dll::get_azul_dll().az_atomic_ref_count_increase_ref)(self) }
        /// Calls the `AtomicRefCount::decrease_ref` function.
        pub fn decrease_ref(&mut self)  { (crate::dll::get_azul_dll().az_atomic_ref_count_decrease_ref)(self) }
        /// Calls the `AtomicRefCount::increase_refmut` function.
        pub fn increase_refmut(&mut self)  { (crate::dll::get_azul_dll().az_atomic_ref_count_increase_refmut)(self) }
        /// Calls the `AtomicRefCount::decrease_refmut` function.
        pub fn decrease_refmut(&mut self)  { (crate::dll::get_azul_dll().az_atomic_ref_count_decrease_refmut)(self) }
    }

    impl Drop for AtomicRefCount { fn drop(&mut self) { (crate::dll::get_azul_dll().az_atomic_ref_count_delete)(self); } }


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

    impl Clone for RefAny { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_ref_any_deep_copy)(self) } }
    impl Drop for RefAny { fn drop(&mut self) { (crate::dll::get_azul_dll().az_ref_any_delete)(self); } }


    /// `LayoutInfo` struct
    pub use crate::dll::AzLayoutInfo as LayoutInfo;

    impl LayoutInfo {
        /// Calls the `LayoutInfo::window_width_larger_than` function.
        pub fn window_width_larger_than(&mut self, width: f32)  -> bool { (crate::dll::get_azul_dll().az_layout_info_window_width_larger_than)(self, width) }
        /// Calls the `LayoutInfo::window_width_smaller_than` function.
        pub fn window_width_smaller_than(&mut self, width: f32)  -> bool { (crate::dll::get_azul_dll().az_layout_info_window_width_smaller_than)(self, width) }
        /// Calls the `LayoutInfo::window_height_larger_than` function.
        pub fn window_height_larger_than(&mut self, width: f32)  -> bool { (crate::dll::get_azul_dll().az_layout_info_window_height_larger_than)(self, width) }
        /// Calls the `LayoutInfo::window_height_smaller_than` function.
        pub fn window_height_smaller_than(&mut self, width: f32)  -> bool { (crate::dll::get_azul_dll().az_layout_info_window_height_smaller_than)(self, width) }
    }

    impl Drop for LayoutInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_info_delete)(self); } }
