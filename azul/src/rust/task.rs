    #![allow(dead_code, unused_imports)]
    //! Asyncronous timers / task / thread handlers for easy async loading
    use crate::dll::*;
    use std::ffi::c_void;
    use crate::callbacks::{RefAny, TaskCallbackType, ThreadCallbackType};


    /// `DropCheckPtr` struct
    pub use crate::dll::AzDropCheckPtrPtr as DropCheckPtr;

    impl std::fmt::Debug for DropCheckPtr { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_drop_check_ptr_ptr_fmt_debug)(self)) } }
    impl Drop for DropCheckPtr { fn drop(&mut self) { (crate::dll::get_azul_dll().az_drop_check_ptr_ptr_delete)(self); } }


    /// `ArcMutexRefAny` struct
    pub use crate::dll::AzArcMutexRefAnyPtr as ArcMutexRefAny;

    impl std::fmt::Debug for ArcMutexRefAny { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_arc_mutex_ref_any_ptr_fmt_debug)(self)) } }
    impl Drop for ArcMutexRefAny { fn drop(&mut self) { (crate::dll::get_azul_dll().az_arc_mutex_ref_any_ptr_delete)(self); } }


    /// `Timer` struct
    pub use crate::dll::AzTimer as Timer;

    impl std::fmt::Debug for Timer { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_timer_fmt_debug)(self)) } }
    impl Clone for Timer { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_timer_deep_copy)(self) } }
    impl Drop for Timer { fn drop(&mut self) { (crate::dll::get_azul_dll().az_timer_delete)(self); } }


    /// `Task` struct
    pub use crate::dll::AzTaskPtr as Task;

    impl Task {
        /// Creates and starts a new `Task`
        pub fn new(data: ArcMutexRefAny, callback: TaskCallbackType) -> Self { (crate::dll::get_azul_dll().az_task_ptr_new)(data, callback) }
        /// Creates and starts a new `Task`
        pub fn then(self, timer: Timer)  -> crate::task::Task { (crate::dll::get_azul_dll().az_task_ptr_then)(self, timer) }
    }

    impl std::fmt::Debug for Task { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_task_ptr_fmt_debug)(self)) } }
    impl Drop for Task { fn drop(&mut self) { (crate::dll::get_azul_dll().az_task_ptr_delete)(self); } }


    /// `Thread` struct
    pub use crate::dll::AzThreadPtr as Thread;

    impl Thread {
        /// Creates and starts a new thread that calls the `callback` on the `data`.
        pub fn new(data: RefAny, callback: ThreadCallbackType) -> Self { (crate::dll::get_azul_dll().az_thread_ptr_new)(data, callback) }
        /// Blocks until the internal thread has finished and returns the result of the operation
        pub fn block(self)  -> crate::result::ResultRefAnyBlockError { (crate::dll::get_azul_dll().az_thread_ptr_block)(self) }
    }

    impl std::fmt::Debug for Thread { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_thread_ptr_fmt_debug)(self)) } }
    impl Drop for Thread { fn drop(&mut self) { (crate::dll::get_azul_dll().az_thread_ptr_delete)(self); } }


    /// `DropCheck` struct
    pub use crate::dll::AzDropCheckPtr as DropCheck;

    impl std::fmt::Debug for DropCheck { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_drop_check_ptr_fmt_debug)(self)) } }
    impl Drop for DropCheck { fn drop(&mut self) { (crate::dll::get_azul_dll().az_drop_check_ptr_delete)(self); } }


    /// `TimerId` struct
    pub use crate::dll::AzTimerId as TimerId;

    impl std::fmt::Debug for TimerId { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_timer_id_fmt_debug)(self)) } }
    impl Clone for TimerId { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_timer_id_deep_copy)(self) } }
    impl Drop for TimerId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_timer_id_delete)(self); } }


    /// Should a timer terminate or not - used to remove active timers
    pub use crate::dll::AzTerminateTimer as TerminateTimer;

    impl std::fmt::Debug for TerminateTimer { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_terminate_timer_fmt_debug)(self)) } }
    impl Clone for TerminateTimer { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_terminate_timer_deep_copy)(self) } }
    impl Drop for TerminateTimer { fn drop(&mut self) { (crate::dll::get_azul_dll().az_terminate_timer_delete)(self); } }


    /// `BlockError` struct
    pub use crate::dll::AzBlockError as BlockError;

    impl std::fmt::Debug for BlockError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_block_error_fmt_debug)(self)) } }
    impl Clone for BlockError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_block_error_deep_copy)(self) } }
    impl Drop for BlockError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_block_error_delete)(self); } }
