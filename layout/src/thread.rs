//! Thread callback information and utilities for azul-layout
//!
//! This module provides thread-related callback structures for background tasks
//! that need to interact with the UI thread and query layout information.

#[cfg(feature = "std")]
use alloc::sync::Arc;
#[cfg(feature = "std")]
use std::sync::{
    mpsc::{channel, Receiver, Sender},
    Mutex,
};
#[cfg(feature = "std")]
use std::thread::{self, JoinHandle};

use azul_core::{
    callbacks::Update,
    refany::{OptionRefAny, RefAny},
    task::{
        CheckThreadFinishedCallback, CheckThreadFinishedCallbackType, LibrarySendThreadMsgCallback,
        LibrarySendThreadMsgCallbackType, OptionThreadSendMsg, ThreadId, ThreadReceiver,
        ThreadReceiverDestructorCallback, ThreadReceiverInner, ThreadRecvCallback, ThreadSendMsg,
    },
};

use crate::callbacks::CallbackInfo;

macro_rules! impl_callback_traits {
    ($name:ident) => {
        impl core::fmt::Debug for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                write!(f, concat!(stringify!($name), " {{ cb: {:p} }}"), self.cb as *const ())
            }
        }
        impl Clone for $name {
            fn clone(&self) -> Self { Self { cb: self.cb } }
        }
        impl PartialEq for $name {
            fn eq(&self, other: &Self) -> bool {
                self.cb as *const () as usize == other.cb as *const () as usize
            }
        }
        impl Eq for $name {}
        impl PartialOrd for $name {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for $name {
            fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                (self.cb as *const () as usize).cmp(&(other.cb as *const () as usize))
            }
        }
        impl core::hash::Hash for $name {
            fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
                (self.cb as *const () as usize).hash(state);
            }
        }
    };
}

// Types that need to be defined locally (not in azul-core)

/// Message that is sent back from the running thread to the main thread
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C, u8)]
pub enum ThreadReceiveMsg {
    WriteBack(ThreadWriteBackMsg),
    Update(Update),
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C, u8)]
pub enum OptionThreadReceiveMsg {
    None,
    Some(ThreadReceiveMsg),
}

impl From<Option<ThreadReceiveMsg>> for OptionThreadReceiveMsg {
    fn from(inner: Option<ThreadReceiveMsg>) -> Self {
        inner.map_or_else(|| Self::None, |v| Self::Some(v))
    }
}

impl OptionThreadReceiveMsg {
    #[must_use] pub fn into_option(self) -> Option<ThreadReceiveMsg> {
        match self {
            Self::None => None,
            Self::Some(v) => Some(v),
        }
    }

    #[must_use] pub const fn as_ref(&self) -> Option<&ThreadReceiveMsg> {
        match self {
            Self::None => None,
            Self::Some(v) => Some(v),
        }
    }
}

/// Message containing writeback data and callback
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct ThreadWriteBackMsg {
    pub refany: RefAny,
    pub callback: WriteBackCallback,
}

impl ThreadWriteBackMsg {
    pub fn new<C: Into<WriteBackCallback>>(callback: C, data: RefAny) -> Self {
        Self {
            refany: data,
            callback: callback.into(),
        }
    }
}

/// `ThreadSender` allows sending messages from the background thread to the main thread
#[derive(Debug)]
#[repr(C)]
pub struct ThreadSender {
    #[cfg(feature = "std")]
    pub ptr: Box<Arc<Mutex<ThreadSenderInner>>>,
    #[cfg(not(feature = "std"))]
    pub ptr: *const core::ffi::c_void,
    pub run_destructor: bool,
    /// For FFI: stores the foreign callable (e.g., `PyFunction`)
    pub ctx: OptionRefAny,
}

impl Clone for ThreadSender {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr.clone(),
            run_destructor: true,
            ctx: self.ctx.clone(),
        }
    }
}

impl Drop for ThreadSender {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

impl ThreadSender {
    #[cfg(not(feature = "std"))]
    pub fn new(_t: ThreadSenderInner) -> Self {
        Self {
            ptr: core::ptr::null(),
            run_destructor: false,
            ctx: OptionRefAny::None,
        }
    }

    #[cfg(feature = "std")]
    #[must_use] pub fn new(t: ThreadSenderInner) -> Self {
        Self {
            ptr: Box::new(Arc::new(Mutex::new(t))),
            run_destructor: true,
            ctx: OptionRefAny::None,
        }
    }

    /// Get the FFI context (e.g., Python callable)
    #[must_use] pub fn get_ctx(&self) -> OptionRefAny {
        self.ctx.clone()
    }

    #[cfg(not(feature = "std"))]
    pub fn send(&mut self, _msg: ThreadReceiveMsg) -> bool {
        false
    }

    #[cfg(feature = "std")]
    pub fn send(&mut self, msg: ThreadReceiveMsg) -> bool {
        let Some(ts) = self.ptr.lock().ok() else {
            return false;
        };
        (ts.send_fn.cb)(std::ptr::from_ref(ts.ptr.as_ref()).cast::<core::ffi::c_void>(), msg)
    }
}

/// Inner state of a `ThreadSender`, holding the channel sender and associated callbacks
#[derive(Debug)]
#[cfg_attr(not(feature = "std"), derive(PartialEq, PartialOrd, Eq, Ord))]
#[repr(C)]
pub struct ThreadSenderInner {
    #[cfg(feature = "std")]
    pub ptr: Box<Sender<ThreadReceiveMsg>>,
    #[cfg(not(feature = "std"))]
    pub ptr: *const core::ffi::c_void,
    pub send_fn: ThreadSendCallback,
    pub destructor: ThreadSenderDestructorCallback,
}

#[cfg(not(feature = "std"))]
unsafe impl Send for ThreadSenderInner {}

#[cfg(feature = "std")]
impl core::hash::Hash for ThreadSenderInner {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        (std::ptr::from_ref(self.ptr.as_ref()) as usize).hash(state);
    }
}

#[cfg(feature = "std")]
impl PartialEq for ThreadSenderInner {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.ptr.as_ref(), other.ptr.as_ref())
    }
}

#[cfg(feature = "std")]
impl Eq for ThreadSenderInner {}

#[cfg(feature = "std")]
impl PartialOrd for ThreadSenderInner {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(
            (std::ptr::from_ref(self.ptr.as_ref()) as usize)
                .cmp(&(std::ptr::from_ref(other.ptr.as_ref()) as usize)),
        )
    }
}

#[cfg(feature = "std")]
impl Ord for ThreadSenderInner {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (std::ptr::from_ref(self.ptr.as_ref()) as usize).cmp(&(std::ptr::from_ref(other.ptr.as_ref()) as usize))
    }
}

impl Drop for ThreadSenderInner {
    fn drop(&mut self) {
        (self.destructor.cb)(self);
    }
}

/// Callback for sending messages from thread to main thread
pub type ThreadSendCallbackType = extern "C" fn(*const core::ffi::c_void, ThreadReceiveMsg) -> bool;

#[repr(C)]
pub struct ThreadSendCallback {
    pub cb: ThreadSendCallbackType,
}

impl_callback_traits!(ThreadSendCallback);

/// Destructor callback for `ThreadSender`
pub type ThreadSenderDestructorCallbackType = extern "C" fn(*mut ThreadSenderInner);

#[repr(C)]
pub struct ThreadSenderDestructorCallback {
    pub cb: ThreadSenderDestructorCallbackType,
}

impl_callback_traits!(ThreadSenderDestructorCallback);

/// Callback that runs when a thread receives a `WriteBack` message
///
/// This callback runs on the main UI thread and has access to:
/// - The thread's original data
/// - Data sent back from the background thread
/// - Full `CallbackInfo` for DOM queries and UI updates
pub type WriteBackCallbackType = extern "C" fn(
    /* original thread data */ RefAny,
    /* data to write back */ RefAny,
    /* callback info */ CallbackInfo,
) -> Update;

/// Callback that can run when a thread receives a `WriteBack` message
#[repr(C)]
pub struct WriteBackCallback {
    pub cb: WriteBackCallbackType,
    /// For FFI: stores the foreign callable (e.g., `PyFunction`)
    /// Native Rust code sets this to None
    pub ctx: OptionRefAny,
}

impl WriteBackCallback {
    /// Create a new `WriteBackCallback`
    pub fn new(cb: WriteBackCallbackType) -> Self {
        Self {
            cb,
            ctx: OptionRefAny::None,
        }
    }

    /// Invoke the callback
    #[must_use] pub fn invoke(
        &self,
        thread_data: RefAny,
        writeback_data: RefAny,
        callback_info: CallbackInfo,
    ) -> Update {
        (self.cb)(thread_data, writeback_data, callback_info)
    }
}

impl core::fmt::Debug for WriteBackCallback {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "WriteBackCallback {{ cb: {:p} }}", self.cb as *const ())
    }
}

impl Clone for WriteBackCallback {
    fn clone(&self) -> Self {
        Self {
            cb: self.cb,
            ctx: self.ctx.clone(),
        }
    }
}

impl From<WriteBackCallbackType> for WriteBackCallback {
    fn from(cb: WriteBackCallbackType) -> Self {
        Self {
            cb,
            ctx: OptionRefAny::None,
        }
    }
}

impl PartialEq for WriteBackCallback {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.cb as *const (), other.cb as *const ())
    }
}

impl Eq for WriteBackCallback {}

impl PartialOrd for WriteBackCallback {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WriteBackCallback {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.cb as *const () as usize).cmp(&(other.cb as *const () as usize))
    }
}

impl core::hash::Hash for WriteBackCallback {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        (self.cb as *const () as usize).hash(state);
    }
}

/// Callback type for the function that runs in the background thread
pub type ThreadCallbackType = extern "C" fn(RefAny, ThreadSender, ThreadReceiver);

#[repr(C)]
pub struct ThreadCallback {
    pub cb: ThreadCallbackType,
    /// For FFI: stores the foreign callable (e.g., `PyFunction`)
    /// Native Rust code sets this to None
    pub ctx: OptionRefAny,
}

impl ThreadCallback {
    /// Create a new `ThreadCallback`
    pub fn new(cb: ThreadCallbackType) -> Self {
        Self {
            cb,
            ctx: OptionRefAny::None,
        }
    }
}

impl core::fmt::Debug for ThreadCallback {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "ThreadCallback {{ cb: {:p} }}", self.cb as *const ())
    }
}

impl Clone for ThreadCallback {
    fn clone(&self) -> Self {
        Self {
            cb: self.cb,
            ctx: self.ctx.clone(),
        }
    }
}

impl From<ThreadCallbackType> for ThreadCallback {
    fn from(cb: ThreadCallbackType) -> Self {
        Self {
            cb,
            ctx: OptionRefAny::None,
        }
    }
}

impl PartialEq for ThreadCallback {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.cb as *const (), other.cb as *const ())
    }
}

impl Eq for ThreadCallback {}

impl PartialOrd for ThreadCallback {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ThreadCallback {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.cb as *const () as usize).cmp(&(other.cb as *const () as usize))
    }
}

impl core::hash::Hash for ThreadCallback {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        (self.cb as *const () as usize).hash(state);
    }
}

// Host-invoker plumbing for ThreadCallback. NOTE: this callback fires
// on a worker thread (spawned by `Thread::create`), not the main
// `App.run` thread. The per-language host-invoker thunk MUST acquire
// the host VM lock before dispatching to user code:
//   * CPython: PyGILState_Ensure / _Release
//   * MRI Ruby: rb_thread_call_with_gvl
//   * OpenJDK: AttachCurrentThread / DetachCurrentThread
//   * CLR / .NET: nothing ([UnmanagedCallersOnly] auto-trampolines)
//   * OCaml: caml_acquire_runtime_system / _release
//   * Lua / Perl / PHP / Pharo: cannot be called from worker thread
//     (single-threaded interpreter) — fall back to writeback-only
//     pattern (Rust extern "C" cb on worker, host fn on main via
//     WriteBackCallback).
// See `scripts/BINDING_STRATEGY_PER_LANGUAGE.md` for the lock-acquire
// table per VM.
azul_core::impl_managed_callback! {
    wrapper:        ThreadCallback,
    info_ty:        ThreadSender,
    return_ty:      (),
    default_ret:    (),
    invoker_static: THREAD_CALLBACK_INVOKER,
    invoker_ty:     AzThreadCallbackInvoker,
    thunk_fn:       az_thread_callback_thunk,
    setter_fn:      AzApp_setThreadCallbackInvoker,
    from_handle_fn: AzThreadCallback_createFromHostHandle,
    extra_args:     [receiver: ThreadReceiver],
}

/// Callback type for receiving messages from a background thread
pub type LibraryReceiveThreadMsgCallbackType =
    extern "C" fn(*const core::ffi::c_void) -> OptionThreadReceiveMsg;

#[repr(C)]
pub struct LibraryReceiveThreadMsgCallback {
    pub cb: LibraryReceiveThreadMsgCallbackType,
}

impl_callback_traits!(LibraryReceiveThreadMsgCallback);

/// Callback type for the destructor that cleans up a `ThreadInner`
pub type ThreadDestructorCallbackType = extern "C" fn(*mut ThreadInner);

#[repr(C)]
pub struct ThreadDestructorCallback {
    pub cb: ThreadDestructorCallbackType,
}

impl_callback_traits!(ThreadDestructorCallback);

/// Wrapper around Thread because Thread needs to be clone-able
#[derive(Debug)]
#[repr(C)]
pub struct Thread {
    #[cfg(feature = "std")]
    pub ptr: Box<Arc<Mutex<ThreadInner>>>,
    #[cfg(not(feature = "std"))]
    pub ptr: *const core::ffi::c_void,
    pub run_destructor: bool,
}

impl Clone for Thread {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr.clone(),
            run_destructor: true,
        }
    }
}

impl Drop for Thread {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

impl Thread {
    #[cfg(feature = "std")]
    #[must_use] pub fn new(ti: ThreadInner) -> Self {
        Self {
            ptr: Box::new(Arc::new(Mutex::new(ti))),
            run_destructor: true,
        }
    }

    #[cfg(not(feature = "std"))]
    pub fn new(_ti: ThreadInner) -> Self {
        Self {
            ptr: core::ptr::null(),
            run_destructor: false,
        }
    }

    /// Creates a new thread that will execute the given callback function.
    ///
    /// # Arguments
    /// * `thread_initialize_data` - Data passed to the callback when the thread starts
    /// * `writeback_data` - Data that will be passed back when writeback messages are received
    /// * `callback` - The callback to execute in the background thread
    ///
    /// # Returns
    /// A new Thread handle that can be added to the event loop with `CallbackInfo::add_thread`
    pub fn create<C: Into<ThreadCallback>>(
        thread_initialize_data: RefAny,
        writeback_data: RefAny,
        callback: C,
    ) -> Self {
        create_thread_libstd(thread_initialize_data, writeback_data, callback.into())
    }

    /// Send a control message to the running worker. Interior-mutable (the worker
    /// state is behind a `Mutex`), so this takes `&self` and is callable from a
    /// callback that only holds `&Thread` via `CallbackInfo::get_thread`. Used to push
    /// resize / seek / source-change messages to a persistent worker. Returns false
    /// if the channel is closed (or always, on `no_std`).
    #[cfg(feature = "std")]
    #[must_use] pub fn send_message(&self, msg: ThreadSendMsg) -> bool {
        self.ptr.lock().map_or(false, |inner| inner.sender.send(msg).is_ok())
    }
    #[cfg(not(feature = "std"))]
    pub fn send_message(&self, _msg: ThreadSendMsg) -> bool {
        false
    }

    /// Clone the main→worker `Sender` so a holder without a `CallbackInfo` (e.g. a
    /// dataset-merge callback) can message the running worker later — used for the
    /// scrub/seek path, where the merge callback compares the old/new `VideoConfig`
    /// and pushes a seek to the worker. `None` on `no_std`.
    #[cfg(feature = "std")]
    #[must_use] pub fn clone_sender(&self) -> Option<Sender<ThreadSendMsg>> {
        self.ptr.lock().ok().map(|inner| (*inner.sender).clone())
    }
    #[cfg(not(feature = "std"))]
    pub fn clone_sender(&self) -> Option<Sender<ThreadSendMsg>> {
        None
    }
}

/// A `Thread` is a separate thread that is owned by the framework.
///
/// In difference to a regular thread, you don't have to `await()` the result,
/// you can just hand the Thread to the framework and it will automatically
/// update the UI when the Thread is finished.
#[derive(Debug)]
#[repr(C)]
pub struct ThreadInner {
    #[cfg(feature = "std")]
    pub thread_handle: Box<Option<JoinHandle<()>>>,
    #[cfg(not(feature = "std"))]
    pub thread_handle: *const core::ffi::c_void,

    #[cfg(feature = "std")]
    pub sender: Box<Sender<ThreadSendMsg>>,
    #[cfg(not(feature = "std"))]
    pub sender: *const core::ffi::c_void,

    #[cfg(feature = "std")]
    pub receiver: Box<Receiver<ThreadReceiveMsg>>,
    #[cfg(not(feature = "std"))]
    pub receiver: *const core::ffi::c_void,

    #[cfg(feature = "std")]
    pub dropcheck: Box<alloc::sync::Weak<()>>,
    #[cfg(not(feature = "std"))]
    pub dropcheck: *const core::ffi::c_void,

    pub writeback_data: RefAny,
    pub check_thread_finished_fn: CheckThreadFinishedCallback,
    pub send_thread_msg_fn: LibrarySendThreadMsgCallback,
    pub receive_thread_msg_fn: LibraryReceiveThreadMsgCallback,
    pub thread_destructor_fn: ThreadDestructorCallback,
}

#[cfg(feature = "std")]
impl ThreadInner {
    /// Returns true if the Thread has been finished, false otherwise
    #[must_use] pub fn is_finished(&self) -> bool {
        (self.check_thread_finished_fn.cb)(
            std::ptr::from_ref(self.dropcheck.as_ref()).cast::<core::ffi::c_void>()
        )
    }

    /// Send a message to the thread
    pub fn sender_send(&mut self, msg: ThreadSendMsg) -> bool {
        (self.send_thread_msg_fn.cb)(
            std::ptr::from_ref(self.sender.as_ref()).cast::<core::ffi::c_void>(),
            msg,
        )
    }

    /// Try to receive a message from the thread (non-blocking)
    pub fn receiver_try_recv(&mut self) -> OptionThreadReceiveMsg {
        (self.receive_thread_msg_fn.cb)(
            std::ptr::from_ref(self.receiver.as_ref()).cast::<core::ffi::c_void>()
        )
    }
}

#[cfg(not(feature = "std"))]
impl ThreadInner {
    /// Returns true if the Thread has been finished, false otherwise
    pub fn is_finished(&self) -> bool {
        true
    }

    /// Send a message to the thread (no-op in no_std)
    pub fn sender_send(&mut self, _msg: ThreadSendMsg) -> bool {
        false
    }

    /// Try to receive a message from the thread (always returns None in no_std)
    pub fn receiver_try_recv(&mut self) -> OptionThreadReceiveMsg {
        None.into()
    }
}

impl Drop for ThreadInner {
    fn drop(&mut self) {
        (self.thread_destructor_fn.cb)(self);
    }
}

// Default callback implementations for std
#[cfg(feature = "std")]
extern "C" fn default_thread_destructor_fn(thread: *mut ThreadInner) {
    let thread = unsafe { &mut *thread };

    if let Some(thread_handle) = thread.thread_handle.take() {
        let _ = thread.sender.send(ThreadSendMsg::TerminateThread);
        let _ = thread_handle.join(); // ignore the result, don't panic
    }
}

#[cfg(not(feature = "std"))]
extern "C" fn default_thread_destructor_fn(_thread: *mut ThreadInner) {}

#[cfg(feature = "std")]
extern "C" fn library_send_thread_msg_fn(
    sender: *const core::ffi::c_void,
    msg: ThreadSendMsg,
) -> bool {
    unsafe { &*sender.cast::<Sender<ThreadSendMsg>>() }
        .send(msg)
        .is_ok()
}

#[cfg(not(feature = "std"))]
extern "C" fn library_send_thread_msg_fn(
    _sender: *const core::ffi::c_void,
    _msg: ThreadSendMsg,
) -> bool {
    false
}

#[cfg(feature = "std")]
extern "C" fn library_receive_thread_msg_fn(
    receiver: *const core::ffi::c_void,
) -> OptionThreadReceiveMsg {
    unsafe { &*receiver.cast::<Receiver<ThreadReceiveMsg>>() }
        .try_recv()
        .ok()
        .into()
}

#[cfg(not(feature = "std"))]
extern "C" fn library_receive_thread_msg_fn(
    _receiver: *const core::ffi::c_void,
) -> OptionThreadReceiveMsg {
    None.into()
}

#[cfg(feature = "std")]
extern "C" fn default_send_thread_msg_fn(
    sender: *const core::ffi::c_void,
    msg: ThreadReceiveMsg,
) -> bool {
    unsafe { &*sender.cast::<Sender<ThreadReceiveMsg>>() }
        .send(msg)
        .is_ok()
}

#[cfg(not(feature = "std"))]
extern "C" fn default_send_thread_msg_fn(
    _sender: *const core::ffi::c_void,
    _msg: ThreadReceiveMsg,
) -> bool {
    false
}

#[cfg(feature = "std")]
extern "C" fn default_receive_thread_msg_fn(
    receiver: *const core::ffi::c_void,
) -> OptionThreadSendMsg {
    unsafe { &*receiver.cast::<Receiver<ThreadSendMsg>>() }
        .try_recv()
        .ok()
        .into()
}

#[cfg(not(feature = "std"))]
extern "C" fn default_receive_thread_msg_fn(
    _receiver: *const core::ffi::c_void,
) -> OptionThreadSendMsg {
    None.into()
}

#[cfg(feature = "std")]
extern "C" fn default_check_thread_finished(dropcheck: *const core::ffi::c_void) -> bool {
    let weak = unsafe { &*dropcheck.cast::<alloc::sync::Weak<()>>() };
    weak.upgrade().is_none()
}

#[cfg(not(feature = "std"))]
extern "C" fn default_check_thread_finished(_dropcheck: *const core::ffi::c_void) -> bool {
    true
}

#[cfg(feature = "std")]
const extern "C" fn thread_sender_drop(_: *mut ThreadSenderInner) {}

#[cfg(not(feature = "std"))]
extern "C" fn thread_sender_drop(_: *mut ThreadSenderInner) {}

#[cfg(feature = "std")]
const extern "C" fn thread_receiver_drop(_: *mut ThreadReceiverInner) {}

#[cfg(not(feature = "std"))]
extern "C" fn thread_receiver_drop(_: *mut ThreadReceiverInner) {}

/// Function that creates a new Thread object
pub type CreateThreadCallbackType = extern "C" fn(RefAny, RefAny, ThreadCallback) -> Thread;

#[repr(C)]
pub struct CreateThreadCallback {
    pub cb: CreateThreadCallbackType,
}

impl_callback_traits!(CreateThreadCallback);
impl Copy for CreateThreadCallback {}

/// Create a new thread using the standard library
#[cfg(feature = "std")]
#[must_use] pub extern "C" fn create_thread_libstd(
    thread_initialize_data: RefAny,
    writeback_data: RefAny,
    callback: ThreadCallback,
) -> Thread {
    let (sender_receiver, receiver_receiver) = channel::<ThreadReceiveMsg>();
    let mut sender_receiver = ThreadSender::new(ThreadSenderInner {
        ptr: Box::new(sender_receiver),
        send_fn: ThreadSendCallback {
            cb: default_send_thread_msg_fn,
        },
        destructor: ThreadSenderDestructorCallback {
            cb: thread_sender_drop,
        },
    });
    // Set the ctx from the callback for FFI
    sender_receiver.ctx = callback.ctx.clone();

    let (sender_sender, receiver_sender) = channel::<ThreadSendMsg>();
    let mut receiver_sender = ThreadReceiver::new(ThreadReceiverInner {
        ptr: Box::new(receiver_sender),
        recv_fn: ThreadRecvCallback {
            cb: default_receive_thread_msg_fn,
        },
        destructor: ThreadReceiverDestructorCallback {
            cb: thread_receiver_drop,
        },
    });
    // Set the ctx from the callback for FFI
    receiver_sender.ctx = callback.ctx.clone();

    let thread_check = Arc::new(());
    let dropcheck = Arc::downgrade(&thread_check);

    let thread_handle = Some(thread::spawn(move || {
        // Keep thread_check alive for the entire duration of the thread
        // by binding it to a named variable (not `_` which drops immediately)
        let _thread_check_guard = thread_check;
        (callback.cb)(thread_initialize_data, sender_receiver, receiver_sender);
        // _thread_check_guard gets dropped here, signals that the thread has finished
    }));

    let thread_handle: Box<Option<JoinHandle<()>>> =
        Box::new(thread_handle);
    let sender: Box<Sender<ThreadSendMsg>> = Box::new(sender_sender);
    let receiver: Box<Receiver<ThreadReceiveMsg>> =
        Box::new(receiver_receiver);
    let dropcheck: Box<alloc::sync::Weak<()>> = Box::new(dropcheck);

    Thread::new(ThreadInner {
        thread_handle,
        sender,
        receiver,
        writeback_data,
        dropcheck,
        thread_destructor_fn: ThreadDestructorCallback {
            cb: default_thread_destructor_fn,
        },
        check_thread_finished_fn: CheckThreadFinishedCallback {
            cb: default_check_thread_finished,
        },
        send_thread_msg_fn: LibrarySendThreadMsgCallback {
            cb: library_send_thread_msg_fn,
        },
        receive_thread_msg_fn: LibraryReceiveThreadMsgCallback {
            cb: library_receive_thread_msg_fn,
        },
    })
}

#[cfg(not(feature = "std"))]
pub extern "C" fn create_thread_libstd(
    _thread_initialize_data: RefAny,
    _writeback_data: RefAny,
    _callback: ThreadCallback,
) -> Thread {
    Thread {
        ptr: core::ptr::null(),
        run_destructor: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" fn test_writeback_callback(
        _thread_data: RefAny,
        _writeback_data: RefAny,
        _callback_info: CallbackInfo,
    ) -> Update {
        Update::DoNothing
    }

    #[test]
    fn test_writeback_callback_creation() {
        let callback = WriteBackCallback::new(test_writeback_callback);
        assert_eq!(callback.cb as *const () as usize, test_writeback_callback as *const () as usize);
    }

    #[test]
    fn test_writeback_callback_clone() {
        let callback = WriteBackCallback::new(test_writeback_callback);
        let cloned = callback.clone();
        assert_eq!(callback, cloned);
    }
}

/// Optional Thread type for API compatibility
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum OptionThread {
    None,
    Some(Thread),
}

impl From<Option<Thread>> for OptionThread {
    fn from(o: Option<Thread>) -> Self {
        o.map_or_else(|| Self::None, |t| Self::Some(t))
    }
}

impl OptionThread {
    #[must_use] pub fn into_option(self) -> Option<Thread> {
        match self {
            Self::None => None,
            Self::Some(t) => Some(t),
        }
    }
}

// ============================================================================
// Sleep utilities
// ============================================================================

/// Sleeps the current thread for the specified number of milliseconds.
///
/// This is a cross-platform utility that can be called from C/C++/Python.
///
/// # Arguments
/// * `milliseconds` - Number of milliseconds to sleep
#[cfg(feature = "std")]
#[must_use] pub fn thread_sleep_ms(milliseconds: u64) -> azul_css::corety::EmptyStruct {
    thread::sleep(std::time::Duration::from_millis(milliseconds));
    azul_css::corety::EmptyStruct::new()
}

/// Sleeps the current thread for the specified number of milliseconds (no-op on no_std).
#[cfg(not(feature = "std"))]
pub fn thread_sleep_ms(_milliseconds: u64) -> azul_css::corety::EmptyStruct {
    // No-op on no_std - can't sleep without OS
    azul_css::corety::EmptyStruct::new()
}

/// Sleeps the current thread for the specified number of microseconds.
///
/// # Arguments
/// * `microseconds` - Number of microseconds to sleep
#[cfg(feature = "std")]
#[must_use] pub fn thread_sleep_us(microseconds: u64) -> azul_css::corety::EmptyStruct {
    thread::sleep(std::time::Duration::from_micros(microseconds));
    azul_css::corety::EmptyStruct::new()
}

/// Sleeps the current thread for the specified number of microseconds (no-op on no_std).
#[cfg(not(feature = "std"))]
pub fn thread_sleep_us(_microseconds: u64) -> azul_css::corety::EmptyStruct {
    // No-op on no_std - can't sleep without OS
    azul_css::corety::EmptyStruct::new()
}

/// Sleeps the current thread for the specified number of nanoseconds.
///
/// # Arguments
/// * `nanoseconds` - Number of nanoseconds to sleep
#[cfg(feature = "std")]
#[must_use] pub fn thread_sleep_ns(nanoseconds: u64) -> azul_css::corety::EmptyStruct {
    thread::sleep(std::time::Duration::from_nanos(nanoseconds));
    azul_css::corety::EmptyStruct::new()
}

/// Sleeps the current thread for the specified number of nanoseconds (no-op on no_std).
#[cfg(not(feature = "std"))]
pub fn thread_sleep_ns(_nanoseconds: u64) -> azul_css::corety::EmptyStruct {
    // No-op on no_std - can't sleep without OS
    azul_css::corety::EmptyStruct::new()
}
