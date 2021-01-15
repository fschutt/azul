    #![allow(dead_code, unused_imports)]
    //! Asyncronous timers / task / thread handlers for easy async loading
    use crate::dll::*;
    use std::ffi::c_void;
    use crate::callbacks::{RefAny, TimerCallbackType};
    use crate::time::Duration;


    /// `TimerId` struct
    #[doc(inline)] pub use crate::dll::AzTimerId as TimerId;

    impl TimerId {
        /// Creates a new `TimerId` instance.
        pub fn unique() -> Self { unsafe { crate::dll::az_timer_id_unique() } }
    }

    impl Clone for TimerId { fn clone(&self) -> Self { *self } }
    impl Copy for TimerId { }


    /// `Timer` struct
    #[doc(inline)] pub use crate::dll::AzTimer as Timer;

    impl Timer {
        /// Creates a new `Timer` instance.
        pub fn new(timer_data: RefAny, callback: TimerCallbackType) -> Self { unsafe { crate::dll::az_timer_new(timer_data, callback) } }
        /// Calls the `Timer::with_delay` function.
        pub fn with_delay(self, delay: Duration)  -> crate::task::Timer { unsafe { crate::dll::az_timer_with_delay(self, delay) } }
        /// Calls the `Timer::with_interval` function.
        pub fn with_interval(self, interval: Duration)  -> crate::task::Timer { unsafe { crate::dll::az_timer_with_interval(self, interval) } }
        /// Calls the `Timer::with_timeout` function.
        pub fn with_timeout(self, timeout: Duration)  -> crate::task::Timer { unsafe { crate::dll::az_timer_with_timeout(self, timeout) } }
    }

    impl Clone for Timer { fn clone(&self) -> Self { unsafe { crate::dll::az_timer_deep_copy(self) } } }
    impl Drop for Timer { fn drop(&mut self) { unsafe { crate::dll::az_timer_delete(self) }; } }


    /// Should a timer terminate or not - used to remove active timers
    #[doc(inline)] pub use crate::dll::AzTerminateTimer as TerminateTimer;

    impl Clone for TerminateTimer { fn clone(&self) -> Self { *self } }
    impl Copy for TerminateTimer { }


    /// `ThreadSender` struct
    #[doc(inline)] pub use crate::dll::AzThreadSender as ThreadSender;

    impl ThreadSender {
        /// Calls the `ThreadSender::send` function.
        pub fn send(&mut self, msg: ThreadReceiveMsg)  -> bool { unsafe { crate::dll::az_thread_sender_send(self, msg) } }
    }

    impl Drop for ThreadSender { fn drop(&mut self) { unsafe { crate::dll::az_thread_sender_delete(self) }; } }


    /// `ThreadReceiver` struct
    #[doc(inline)] pub use crate::dll::AzThreadReceiver as ThreadReceiver;

    impl ThreadReceiver {
        /// Calls the `ThreadReceiver::receive` function.
        pub fn receive(&mut self)  -> crate::option::OptionThreadSendMsg { unsafe { crate::dll::az_thread_receiver_receive(self) } }
    }

    impl Drop for ThreadReceiver { fn drop(&mut self) { unsafe { crate::dll::az_thread_receiver_delete(self) }; } }


    /// `ThreadSendMsg` struct
    #[doc(inline)] pub use crate::dll::AzThreadSendMsg as ThreadSendMsg;

    impl Clone for ThreadSendMsg { fn clone(&self) -> Self { *self } }
    impl Copy for ThreadSendMsg { }


    /// `ThreadReceiveMsg` struct
    #[doc(inline)] pub use crate::dll::AzThreadReceiveMsg as ThreadReceiveMsg;

    impl Drop for ThreadReceiveMsg { fn drop(&mut self) { unsafe { crate::dll::az_thread_receive_msg_delete(self) }; } }


    /// `ThreadWriteBackMsg` struct
    #[doc(inline)] pub use crate::dll::AzThreadWriteBackMsg as ThreadWriteBackMsg;

    impl Drop for ThreadWriteBackMsg { fn drop(&mut self) { unsafe { crate::dll::az_thread_write_back_msg_delete(self) }; } }


    /// `ThreadId` struct
    #[doc(inline)] pub use crate::dll::AzThreadId as ThreadId;

    impl Clone for ThreadId { fn clone(&self) -> Self { *self } }
    impl Copy for ThreadId { }
