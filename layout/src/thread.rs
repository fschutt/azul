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
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, concat!(stringify!($name), " {{ cb: {:p} }}"), self.cb as *const ())
            }
        }
        // generated for both Copy and non-Copy callback structs; the explicit field
        // copy works uniformly (a derive can't be emitted for an externally-defined struct).
        #[allow(clippy::expl_impl_clone_on_copy, clippy::non_canonical_clone_impl)]
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
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// Message that is sent back from the running thread to the main thread
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C, u8)]
pub enum ThreadReceiveMsg {
    WriteBack(ThreadWriteBackMsg),
    Update(Update),
}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C, u8)]
pub enum OptionThreadReceiveMsg {
    None,
    Some(ThreadReceiveMsg),
}

impl From<Option<ThreadReceiveMsg>> for OptionThreadReceiveMsg {
    fn from(inner: Option<ThreadReceiveMsg>) -> Self {
        inner.map_or_else(|| Self::None, Self::Some)
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

#[allow(missing_copy_implementations)] // C-ABI fn-ptr wrapper; Clone is macro-generated (impl_callback_traits!), so Copy would trip expl_impl_clone_on_copy
#[repr(C)]
pub struct ThreadSendCallback {
    pub cb: ThreadSendCallbackType,
}

impl_callback_traits!(ThreadSendCallback);

/// Destructor callback for `ThreadSender`
pub type ThreadSenderDestructorCallbackType = extern "C" fn(*mut ThreadSenderInner);

#[allow(missing_copy_implementations)] // C-ABI fn-ptr wrapper; Clone is macro-generated (impl_callback_traits!), so Copy would trip expl_impl_clone_on_copy
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
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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
    // unit default-return; written via Default::default() so clippy's unused_unit
    // doesn't fire on a bare `()` in this macro-argument position.
    default_ret:    Default::default(),
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

#[allow(missing_copy_implementations)] // C-ABI fn-ptr wrapper; Clone is macro-generated (impl_callback_traits!), so Copy would trip expl_impl_clone_on_copy
#[repr(C)]
pub struct LibraryReceiveThreadMsgCallback {
    pub cb: LibraryReceiveThreadMsgCallbackType,
}

impl_callback_traits!(LibraryReceiveThreadMsgCallback);

/// Callback type for the destructor that cleans up a `ThreadInner`
pub type ThreadDestructorCallbackType = extern "C" fn(*mut ThreadInner);

#[allow(missing_copy_implementations)] // C-ABI fn-ptr wrapper; Clone is macro-generated (impl_callback_traits!), so Copy would trip expl_impl_clone_on_copy
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
        self.ptr.lock().is_ok_and(|inner| inner.sender.send(msg).is_ok())
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
        drop(thread.sender.send(ThreadSendMsg::TerminateThread));
        drop(thread_handle.join()); // ignore the result, don't panic
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
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// Optional Thread type for API compatibility
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum OptionThread {
    None,
    Some(Thread),
}

impl From<Option<Thread>> for OptionThread {
    fn from(o: Option<Thread>) -> Self {
        o.map_or_else(|| Self::None, Self::Some)
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

// ============================================================================
// Generated adversarial tests
// ============================================================================

#[cfg(all(test, feature = "std"))]
#[allow(clippy::too_many_lines, clippy::unreadable_literal)]
mod autotest_generated {
    use core::{
        hash::{Hash, Hasher},
        sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrd},
    };
    use std::{
        collections::{hash_map::DefaultHasher, BTreeMap},
        sync::{Arc, Mutex},
        time::Instant as StdInstant,
    };

    use azul_core::{
        dom::{DomId, DomNodeId},
        geom::OptionLogicalPosition,
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        resources::RendererResources,
        styled_dom::NodeHierarchyItemId,
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::{corety::EmptyStruct, system::SystemStyle};
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        window::LayoutWindow,
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Harness
    // ------------------------------------------------------------------

    /// Upper bound on a worker's non-blocking poll loop: `ThreadReceiver::recv` is
    /// `try_recv` under the hood, so a worker that waits for `TerminateThread` MUST
    /// be bounded or a lost message would hang the whole test binary.
    const MAX_WORKER_POLLS: usize = 10_000;

    fn hash_of<T: Hash>(value: &T) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    /// A live `ThreadSender` plus the receiving end of its channel.
    fn make_sender() -> (Receiver<ThreadReceiveMsg>, ThreadSender) {
        let (tx, rx) = channel::<ThreadReceiveMsg>();
        let sender = ThreadSender::new(ThreadSenderInner {
            ptr: Box::new(tx),
            send_fn: ThreadSendCallback {
                cb: default_send_thread_msg_fn,
            },
            destructor: ThreadSenderDestructorCallback {
                cb: thread_sender_drop,
            },
        });
        (rx, sender)
    }

    /// Drain every message currently queued on the main->worker side.
    fn drain(inner: &mut ThreadInner) -> Vec<ThreadReceiveMsg> {
        let mut out = Vec::new();
        loop {
            match inner.receiver_try_recv() {
                OptionThreadReceiveMsg::Some(msg) => out.push(msg),
                OptionThreadReceiveMsg::None => break,
            }
        }
        out
    }

    /// Runs the thread destructor by hand (terminate + join), so every assertion
    /// after it observes a *finished* worker instead of racing one.
    fn join_worker(t: &Thread) {
        let mut guard = t.ptr.lock().expect("thread mutex must not be poisoned");
        default_thread_destructor_fn(core::ptr::from_mut::<ThreadInner>(&mut guard));
    }

    /// Builds a real `CallbackInfo` (the only way to exercise `WriteBackCallback::invoke`)
    /// over an otherwise-empty `LayoutWindow`.
    fn with_callback_info<R>(f: impl FnOnce(CallbackInfo) -> R) -> R {
        let layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let current_window_state = FullWindowState::default();
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();

        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };
        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));

        let info = CallbackInfo::new(
            &ref_data,
            &changes,
            DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::NONE,
            },
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );
        f(info)
    }

    // ------------------------------------------------------------------
    // Callback fixtures
    // ------------------------------------------------------------------

    static WB_THREAD_DATA: AtomicUsize = AtomicUsize::new(0);
    static WB_WRITEBACK_DATA: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn wb_record(
        mut thread_data: RefAny,
        mut writeback_data: RefAny,
        _callback_info: CallbackInfo,
    ) -> Update {
        if let Some(v) = thread_data.downcast_ref::<usize>() {
            WB_THREAD_DATA.store(*v, AtomicOrd::SeqCst);
        }
        if let Some(v) = writeback_data.downcast_ref::<usize>() {
            WB_WRITEBACK_DATA.store(*v, AtomicOrd::SeqCst);
        }
        Update::RefreshDomAllWindows
    }

    extern "C" fn wb_do_nothing(
        _thread_data: RefAny,
        _writeback_data: RefAny,
        _callback_info: CallbackInfo,
    ) -> Update {
        Update::DoNothing
    }

    /// A worker that exits immediately without ever touching its channels.
    extern "C" fn worker_quiet(_d: RefAny, _s: ThreadSender, _r: ThreadReceiver) {}

    /// A worker that pushes exactly one `Update` back to the main thread, then exits.
    extern "C" fn worker_send_update(_d: RefAny, mut sender: ThreadSender, _r: ThreadReceiver) {
        let _sent = sender.send(ThreadReceiveMsg::Update(Update::RefreshDom));
    }

    /// A worker that pushes one `WriteBack` message (the RefAny-carrying variant).
    extern "C" fn worker_send_writeback(_d: RefAny, mut sender: ThreadSender, _r: ThreadReceiver) {
        let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
            wb_do_nothing as WriteBackCallbackType,
            RefAny::new(77_usize),
        )));
    }

    /// Generates a worker that echoes `Tick`s back until it is told to terminate.
    /// Each instance gets its own statics so tests can run in parallel without racing.
    macro_rules! terminating_worker {
        ($fn_name:ident, $ticks:ident, $terminated:ident) => {
            static $ticks: AtomicUsize = AtomicUsize::new(0);
            static $terminated: AtomicBool = AtomicBool::new(false);

            extern "C" fn $fn_name(
                _d: RefAny,
                mut sender: ThreadSender,
                mut receiver: ThreadReceiver,
            ) {
                for _ in 0..MAX_WORKER_POLLS {
                    match receiver.recv() {
                        OptionThreadSendMsg::Some(ThreadSendMsg::TerminateThread) => {
                            $terminated.store(true, AtomicOrd::SeqCst);
                            break;
                        }
                        OptionThreadSendMsg::Some(ThreadSendMsg::Tick) => {
                            $ticks.fetch_add(1, AtomicOrd::SeqCst);
                            let _sent = sender.send(ThreadReceiveMsg::Update(Update::RefreshDom));
                        }
                        OptionThreadSendMsg::Some(ThreadSendMsg::Custom(_)) => {
                            $ticks.fetch_add(1, AtomicOrd::SeqCst);
                        }
                        OptionThreadSendMsg::None => {
                            let _slept = thread_sleep_ms(1);
                        }
                    }
                }
            }
        };
    }

    terminating_worker!(worker_wait_a, WAIT_A_TICKS, WAIT_A_TERMINATED);
    terminating_worker!(worker_wait_b, WAIT_B_TICKS, WAIT_B_TERMINATED);
    terminating_worker!(worker_wait_c, WAIT_C_TICKS, WAIT_C_TERMINATED);
    terminating_worker!(worker_wait_d, WAIT_D_TICKS, WAIT_D_TERMINATED);
    terminating_worker!(worker_wait_e, WAIT_E_TICKS, WAIT_E_TERMINATED);

    // ==================================================================
    // OptionThreadReceiveMsg — getters / predicates
    // ==================================================================

    #[test]
    fn option_thread_receive_msg_none_into_option_is_none() {
        assert!(OptionThreadReceiveMsg::None.into_option().is_none());
        assert!(OptionThreadReceiveMsg::None.as_ref().is_none());
    }

    #[test]
    fn option_thread_receive_msg_round_trips_through_from_and_into_option() {
        // Round-trip: Option -> OptionThreadReceiveMsg -> Option must be the identity.
        for update in [
            Update::DoNothing,
            Update::RefreshDom,
            Update::RefreshDomAllWindows,
        ] {
            let msg = ThreadReceiveMsg::Update(update);
            let ffi: OptionThreadReceiveMsg = Some(msg.clone()).into();
            assert_eq!(ffi.into_option(), Some(msg));
        }
        let empty: OptionThreadReceiveMsg = None.into();
        assert_eq!(empty, OptionThreadReceiveMsg::None);
        assert!(empty.into_option().is_none());
    }

    #[test]
    fn option_thread_receive_msg_as_ref_does_not_consume() {
        let opt = OptionThreadReceiveMsg::Some(ThreadReceiveMsg::Update(Update::RefreshDom));
        // as_ref() borrows: calling it repeatedly must keep returning the same payload.
        for _ in 0..3 {
            assert_eq!(
                opt.as_ref(),
                Some(&ThreadReceiveMsg::Update(Update::RefreshDom))
            );
        }
        // ... and the value is still intact afterwards.
        assert_eq!(
            opt.into_option(),
            Some(ThreadReceiveMsg::Update(Update::RefreshDom))
        );
    }

    #[test]
    fn option_thread_receive_msg_as_ref_handles_writeback_variant() {
        let opt = OptionThreadReceiveMsg::Some(ThreadReceiveMsg::WriteBack(
            ThreadWriteBackMsg::new(
                wb_do_nothing as WriteBackCallbackType,
                RefAny::new(1_usize),
            ),
        ));
        let Some(ThreadReceiveMsg::WriteBack(inner)) = opt.as_ref() else {
            panic!("as_ref() must expose the WriteBack payload");
        };
        assert_eq!(
            inner.callback.cb as *const () as usize,
            wb_do_nothing as *const () as usize
        );
        assert!(opt.into_option().is_some());
    }

    #[test]
    fn option_thread_receive_msg_ord_and_hash_are_consistent() {
        let none = OptionThreadReceiveMsg::None;
        let some = OptionThreadReceiveMsg::Some(ThreadReceiveMsg::Update(Update::DoNothing));
        // Declaration order: None < Some.
        assert!(none < some);
        assert_eq!(none.cmp(&none), core::cmp::Ordering::Equal);
        // Eq => equal hashes.
        assert_eq!(hash_of(&none), hash_of(&OptionThreadReceiveMsg::None));
        assert_eq!(
            hash_of(&some),
            hash_of(&OptionThreadReceiveMsg::Some(ThreadReceiveMsg::Update(
                Update::DoNothing
            )))
        );
    }

    #[test]
    fn thread_receive_msg_orders_writeback_before_update() {
        let wb = ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
            wb_do_nothing as WriteBackCallbackType,
            RefAny::new(0_usize),
        ));
        let up = ThreadReceiveMsg::Update(Update::DoNothing);
        assert!(wb < up, "variant order (WriteBack=0, Update=1) must decide");
        assert!(
            ThreadReceiveMsg::Update(Update::DoNothing)
                < ThreadReceiveMsg::Update(Update::RefreshDom)
        );
    }

    // ==================================================================
    // ThreadWriteBackMsg — constructor invariants
    // ==================================================================

    #[test]
    fn thread_write_back_msg_new_stores_both_fields() {
        let mut msg = ThreadWriteBackMsg::new(
            wb_do_nothing as WriteBackCallbackType,
            RefAny::new(0xDEAD_BEEF_usize),
        );
        assert_eq!(
            msg.callback.cb as *const () as usize,
            wb_do_nothing as *const () as usize
        );
        assert_eq!(
            msg.refany
                .downcast_ref::<usize>()
                .map(|v| *v)
                .expect("payload type must survive construction"),
            0xDEAD_BEEF_usize
        );
        // A fn-ptr-built callback carries no FFI ctx.
        assert_eq!(msg.callback.ctx, OptionRefAny::None);
    }

    #[test]
    fn thread_write_back_msg_new_accepts_both_into_impls() {
        // `C: Into<WriteBackCallback>` must accept a bare fn pointer *and* an
        // already-built WriteBackCallback; both must land on the same cb.
        let from_fn = ThreadWriteBackMsg::new(
            wb_record as WriteBackCallbackType,
            RefAny::new(1_usize),
        );
        let from_struct =
            ThreadWriteBackMsg::new(WriteBackCallback::new(wb_record), RefAny::new(1_usize));
        assert_eq!(from_fn.callback, from_struct.callback);
    }

    #[test]
    fn thread_write_back_msg_clone_shares_payload_but_compares_unequal() {
        // FINDING: RefAny's derived PartialEq includes `instance_id`, which
        // RefAny::clone() increments. Every type that transitively contains a
        // RefAny and derives PartialEq (ThreadWriteBackMsg, ThreadReceiveMsg,
        // OptionThreadReceiveMsg, ThreadSendMsg) therefore violates the
        // `a.clone() == a` contract, even though the clone shares the same heap
        // payload. This test pins the (surprising) status quo.
        let msg = ThreadWriteBackMsg::new(
            wb_do_nothing as WriteBackCallbackType,
            RefAny::new(5_usize),
        );
        let mut cloned = msg.clone();

        // Same callback, same underlying data ...
        assert_eq!(msg.callback, cloned.callback);
        assert_eq!(
            cloned.refany.downcast_ref::<usize>().map(|v| *v),
            Some(5_usize)
        );
        // ... yet not `==`, because the clone got a fresh instance_id.
        assert_ne!(msg, cloned);
        // Two clones of the same message are unequal to each other as well.
        assert_ne!(msg.clone(), msg.clone());
    }

    // ==================================================================
    // WriteBackCallback / ThreadCallback — fn-pointer identity semantics
    // ==================================================================

    #[test]
    fn writeback_callback_new_has_no_ctx_and_matches_fn_ptr() {
        let cb = WriteBackCallback::new(wb_record);
        assert_eq!(cb.ctx, OptionRefAny::None);
        assert_eq!(cb.cb as *const () as usize, wb_record as *const () as usize);
        // The From<fn ptr> impl must be equivalent to ::new.
        assert_eq!(cb, WriteBackCallback::from(wb_record as WriteBackCallbackType));
    }

    #[test]
    fn writeback_callback_eq_ord_hash_key_off_the_fn_pointer_only() {
        let a = WriteBackCallback::new(wb_record);
        let b = WriteBackCallback::new(wb_record);
        let c = WriteBackCallback::new(wb_do_nothing);

        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
        assert_eq!(a.cmp(&b), core::cmp::Ordering::Equal);

        assert_ne!(a, c);
        // Ord must be a strict total order: exactly one direction holds.
        assert!((a < c) ^ (c < a));
        assert_eq!(a.partial_cmp(&c), Some(a.cmp(&c)));

        // Clone preserves identity (no RefAny in the compared fields).
        assert_eq!(a, a.clone());
        assert_eq!(hash_of(&a), hash_of(&a.clone()));
    }

    #[test]
    fn writeback_callback_debug_does_not_panic() {
        let s = format!("{:?}", WriteBackCallback::new(wb_record));
        assert!(s.starts_with("WriteBackCallback {"), "got {s}");
    }

    #[test]
    fn writeback_callback_invoke_forwards_args_and_returns_callback_update() {
        WB_THREAD_DATA.store(0, AtomicOrd::SeqCst);
        WB_WRITEBACK_DATA.store(0, AtomicOrd::SeqCst);

        let cb = WriteBackCallback::new(wb_record);
        let update = with_callback_info(|info| {
            cb.invoke(RefAny::new(11_usize), RefAny::new(22_usize), info)
        });

        // The return value must be whatever the callback returned, unmodified.
        assert_eq!(update, Update::RefreshDomAllWindows);
        // ... and the two RefAnys must arrive in the documented order (not swapped).
        assert_eq!(WB_THREAD_DATA.load(AtomicOrd::SeqCst), 11);
        assert_eq!(WB_WRITEBACK_DATA.load(AtomicOrd::SeqCst), 22);
    }

    #[test]
    fn writeback_callback_invoke_is_repeatable() {
        let cb = WriteBackCallback::new(wb_do_nothing);
        with_callback_info(|info| {
            // CallbackInfo is Copy, so the same info can back several invocations.
            for _ in 0..4 {
                assert_eq!(
                    cb.invoke(RefAny::new(0_usize), RefAny::new(0_usize), info),
                    Update::DoNothing
                );
            }
        });
    }

    #[test]
    fn thread_callback_new_has_no_ctx_and_orders_by_fn_ptr() {
        let a = ThreadCallback::new(worker_quiet);
        let b = ThreadCallback::from(worker_quiet as ThreadCallbackType);
        let c = ThreadCallback::new(worker_send_update);

        assert_eq!(a.ctx, OptionRefAny::None);
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
        assert_ne!(a, c);
        assert!((a < c) ^ (c < a));
        assert_eq!(a, a.clone());
        assert!(format!("{a:?}").starts_with("ThreadCallback {"));
    }

    #[test]
    fn thread_send_callback_wrapper_traits_are_consistent() {
        // impl_callback_traits! generated Clone/Eq/Ord/Hash for the FFI wrappers.
        let a = ThreadSendCallback {
            cb: default_send_thread_msg_fn,
        };
        let b = a.clone();
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
        assert_eq!(a.cmp(&b), core::cmp::Ordering::Equal);
        assert!(format!("{a:?}").starts_with("ThreadSendCallback {"));
    }

    // ==================================================================
    // ThreadSender
    // ==================================================================

    #[test]
    fn thread_sender_send_delivers_the_exact_message() {
        let (rx, mut sender) = make_sender();
        assert!(sender.send(ThreadReceiveMsg::Update(Update::RefreshDom)));
        assert_eq!(
            rx.try_recv().ok(),
            Some(ThreadReceiveMsg::Update(Update::RefreshDom))
        );
        assert!(rx.try_recv().is_err(), "channel must now be empty");
    }

    #[test]
    fn thread_sender_send_returns_false_when_receiver_is_gone() {
        let (rx, mut sender) = make_sender();
        drop(rx);
        // Disconnected channel: must report failure, not panic.
        assert!(!sender.send(ThreadReceiveMsg::Update(Update::RefreshDom)));
        assert!(!sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
            wb_do_nothing as WriteBackCallbackType,
            RefAny::new(0_usize),
        ))));
    }

    #[test]
    fn thread_sender_send_survives_a_poisoned_mutex() {
        let (rx, mut sender) = make_sender();
        let arc = Arc::clone(&*sender.ptr);
        let handle = std::thread::spawn(move || {
            let _guard = arc.lock().expect("mutex is fresh here");
            panic!("intentional poison");
        });
        assert!(handle.join().is_err(), "helper thread must have panicked");

        // ThreadSender::send does `.lock().ok()` — a poisoned lock must degrade to
        // `false`, never to an unwrap panic.
        assert!(!sender.send(ThreadReceiveMsg::Update(Update::RefreshDom)));
        // ... and nothing was actually pushed onto the channel.
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn thread_sender_get_ctx_is_none_by_default_and_clones_the_payload() {
        let (_rx, mut sender) = make_sender();
        assert_eq!(sender.get_ctx(), OptionRefAny::None);

        sender.ctx = OptionRefAny::Some(RefAny::new(9_usize));
        let OptionRefAny::Some(mut ctx) = sender.get_ctx() else {
            panic!("get_ctx must hand back the ctx that was set");
        };
        assert_eq!(ctx.downcast_ref::<usize>().map(|v| *v), Some(9_usize));
        // get_ctx clones rather than moves: the sender still holds its ctx.
        assert!(matches!(sender.get_ctx(), OptionRefAny::Some(_)));
    }

    #[test]
    fn thread_sender_clone_shares_the_underlying_channel() {
        let (rx, mut sender) = make_sender();
        sender.ctx = OptionRefAny::Some(RefAny::new(3_usize));
        let mut cloned = sender.clone();

        assert!(sender.send(ThreadReceiveMsg::Update(Update::DoNothing)));
        assert!(cloned.send(ThreadReceiveMsg::Update(Update::RefreshDom)));

        // Both endpoints feed the same channel, in order.
        assert_eq!(
            rx.try_recv().ok(),
            Some(ThreadReceiveMsg::Update(Update::DoNothing))
        );
        assert_eq!(
            rx.try_recv().ok(),
            Some(ThreadReceiveMsg::Update(Update::RefreshDom))
        );
        // The FFI ctx survives the clone.
        assert!(matches!(cloned.get_ctx(), OptionRefAny::Some(_)));
    }

    // ==================================================================
    // Private FFI callbacks — the raw-pointer trampolines
    // ==================================================================

    #[test]
    fn default_send_thread_msg_fn_reports_disconnect_instead_of_panicking() {
        let (tx, rx) = channel::<ThreadReceiveMsg>();
        let tx_ptr = core::ptr::from_ref::<Sender<ThreadReceiveMsg>>(&tx).cast::<core::ffi::c_void>();

        assert!(default_send_thread_msg_fn(
            tx_ptr,
            ThreadReceiveMsg::Update(Update::RefreshDom)
        ));
        assert_eq!(
            rx.try_recv().ok(),
            Some(ThreadReceiveMsg::Update(Update::RefreshDom))
        );

        drop(rx);
        assert!(!default_send_thread_msg_fn(
            tx_ptr,
            ThreadReceiveMsg::Update(Update::RefreshDom)
        ));
    }

    #[test]
    fn library_send_thread_msg_fn_reports_disconnect_instead_of_panicking() {
        let (tx, rx) = channel::<ThreadSendMsg>();
        let tx_ptr = core::ptr::from_ref::<Sender<ThreadSendMsg>>(&tx).cast::<core::ffi::c_void>();

        assert!(library_send_thread_msg_fn(tx_ptr, ThreadSendMsg::Tick));
        assert!(library_send_thread_msg_fn(
            tx_ptr,
            ThreadSendMsg::Custom(RefAny::new(4_usize))
        ));
        assert_eq!(rx.try_recv().ok(), Some(ThreadSendMsg::Tick));
        assert!(matches!(rx.try_recv(), Ok(ThreadSendMsg::Custom(_))));

        drop(rx);
        assert!(!library_send_thread_msg_fn(
            tx_ptr,
            ThreadSendMsg::TerminateThread
        ));
    }

    #[test]
    fn library_receive_thread_msg_fn_is_non_blocking_on_empty_and_disconnected() {
        let (tx, rx) = channel::<ThreadReceiveMsg>();
        let rx_ptr =
            core::ptr::from_ref::<Receiver<ThreadReceiveMsg>>(&rx).cast::<core::ffi::c_void>();

        // Empty but connected: must return immediately with None (not block).
        assert_eq!(library_receive_thread_msg_fn(rx_ptr), OptionThreadReceiveMsg::None);

        tx.send(ThreadReceiveMsg::Update(Update::RefreshDomAllWindows))
            .expect("receiver is alive");
        assert_eq!(
            library_receive_thread_msg_fn(rx_ptr),
            OptionThreadReceiveMsg::Some(ThreadReceiveMsg::Update(Update::RefreshDomAllWindows))
        );

        // Disconnected: still None, still no panic, and it stays None.
        drop(tx);
        assert_eq!(library_receive_thread_msg_fn(rx_ptr), OptionThreadReceiveMsg::None);
        assert_eq!(library_receive_thread_msg_fn(rx_ptr), OptionThreadReceiveMsg::None);
    }

    #[test]
    fn default_receive_thread_msg_fn_is_non_blocking_on_empty_and_disconnected() {
        let (tx, rx) = channel::<ThreadSendMsg>();
        let rx_ptr = core::ptr::from_ref::<Receiver<ThreadSendMsg>>(&rx).cast::<core::ffi::c_void>();

        assert_eq!(default_receive_thread_msg_fn(rx_ptr), OptionThreadSendMsg::None);

        tx.send(ThreadSendMsg::TerminateThread)
            .expect("receiver is alive");
        assert_eq!(
            default_receive_thread_msg_fn(rx_ptr),
            OptionThreadSendMsg::Some(ThreadSendMsg::TerminateThread)
        );

        drop(tx);
        assert_eq!(default_receive_thread_msg_fn(rx_ptr), OptionThreadSendMsg::None);
    }

    #[test]
    fn default_check_thread_finished_tracks_the_dropcheck_arc() {
        let alive = Arc::new(());
        let weak = Arc::downgrade(&alive);
        let weak_ptr =
            core::ptr::from_ref::<alloc::sync::Weak<()>>(&weak).cast::<core::ffi::c_void>();

        // Strong ref still held by the (simulated) worker => not finished.
        assert!(!default_check_thread_finished(weak_ptr));
        drop(alive);
        // Worker gone => finished, and the answer is stable across calls.
        assert!(default_check_thread_finished(weak_ptr));
        assert!(default_check_thread_finished(weak_ptr));
    }

    #[test]
    fn sender_and_receiver_drop_stubs_ignore_their_argument() {
        // Both destructors are documented no-ops: they must never dereference the
        // pointer, so even a null one is safe to hand them.
        thread_sender_drop(core::ptr::null_mut::<ThreadSenderInner>());
        thread_receiver_drop(core::ptr::null_mut::<ThreadReceiverInner>());
    }

    // ==================================================================
    // Thread / create_thread_libstd — the live-worker paths
    // ==================================================================

    #[test]
    fn create_thread_libstd_runs_the_callback_and_delivers_its_message() {
        let t = create_thread_libstd(
            RefAny::new(0_usize),
            RefAny::new(0_usize),
            ThreadCallback::new(worker_send_update),
        );
        join_worker(&t);

        let mut guard = t.ptr.lock().expect("not poisoned");
        assert!(
            guard.is_finished(),
            "after join the dropcheck Arc must be gone"
        );
        assert_eq!(
            drain(&mut guard),
            vec![ThreadReceiveMsg::Update(Update::RefreshDom)]
        );
    }

    #[test]
    fn thread_create_delivers_a_writeback_message_intact() {
        let t = Thread::create(
            RefAny::new(0_usize),
            RefAny::new(0_usize),
            worker_send_writeback as ThreadCallbackType,
        );
        join_worker(&t);

        let mut guard = t.ptr.lock().expect("not poisoned");
        let msgs = drain(&mut guard);
        assert_eq!(msgs.len(), 1);
        let ThreadReceiveMsg::WriteBack(wb) = &msgs[0] else {
            panic!("expected a WriteBack message, got {:?}", msgs[0]);
        };
        assert_eq!(
            wb.callback.cb as *const () as usize,
            wb_do_nothing as *const () as usize
        );
        // The RefAny payload survived the channel hop between threads.
        let mut payload = wb.refany.clone();
        assert_eq!(payload.downcast_ref::<usize>().map(|v| *v), Some(77_usize));
    }

    #[test]
    fn quiet_worker_leaves_the_receive_queue_empty() {
        let t = Thread::create(
            RefAny::new(0_usize),
            RefAny::new(0_usize),
            worker_quiet as ThreadCallbackType,
        );
        join_worker(&t);

        let mut guard = t.ptr.lock().expect("not poisoned");
        // try_recv on a worker that sent nothing must be None, never a block/panic.
        assert!(drain(&mut guard).is_empty());
        assert_eq!(guard.receiver_try_recv(), OptionThreadReceiveMsg::None);
    }

    #[test]
    fn thread_destructor_is_idempotent() {
        let t = create_thread_libstd(
            RefAny::new(0_usize),
            RefAny::new(0_usize),
            ThreadCallback::new(worker_send_update),
        );
        // Running the destructor twice by hand must not double-join (which would
        // panic / abort); `thread_handle.take()` makes the second call a no-op.
        join_worker(&t);
        join_worker(&t);
        // ... and the real Drop impl will run it a third time when `t` goes away.
        drop(t);
    }

    #[test]
    fn thread_send_message_reaches_the_worker_in_order() {
        let t = Thread::create(
            RefAny::new(0_usize),
            RefAny::new(0_usize),
            worker_wait_a as ThreadCallbackType,
        );

        // The worker holds its ThreadReceiver alive, so every send must succeed.
        for _ in 0..3 {
            assert!(t.send_message(ThreadSendMsg::Tick));
        }
        assert!(t.send_message(ThreadSendMsg::Custom(RefAny::new(1_usize))));

        join_worker(&t); // queues TerminateThread behind the 4 messages, then joins

        assert!(WAIT_A_TERMINATED.load(AtomicOrd::SeqCst));
        assert_eq!(WAIT_A_TICKS.load(AtomicOrd::SeqCst), 4);

        let mut guard = t.ptr.lock().expect("not poisoned");
        assert!(guard.is_finished());
        // 3 Ticks echoed back; Custom is counted but not echoed.
        assert_eq!(drain(&mut guard).len(), 3);
    }

    #[test]
    fn thread_send_message_returns_false_once_the_worker_is_gone() {
        let t = Thread::create(
            RefAny::new(0_usize),
            RefAny::new(0_usize),
            worker_wait_b as ThreadCallbackType,
        );
        assert!(t.send_message(ThreadSendMsg::Tick));

        join_worker(&t); // worker exits, dropping its Receiver<ThreadSendMsg>

        assert!(WAIT_B_TERMINATED.load(AtomicOrd::SeqCst));
        // Disconnected channel: report false rather than panicking.
        assert!(!t.send_message(ThreadSendMsg::Tick));
        assert!(!t.send_message(ThreadSendMsg::TerminateThread));
    }

    #[test]
    fn thread_is_finished_is_false_while_the_worker_is_alive() {
        let t = Thread::create(
            RefAny::new(0_usize),
            RefAny::new(0_usize),
            worker_wait_c as ThreadCallbackType,
        );
        {
            // The dropcheck Arc is moved into the closure before spawn, so it is
            // alive from creation until the worker body returns: deterministic false.
            let guard = t.ptr.lock().expect("not poisoned");
            assert!(!guard.is_finished());
        }
        join_worker(&t);
        assert!(t.ptr.lock().expect("not poisoned").is_finished());
        assert!(WAIT_C_TERMINATED.load(AtomicOrd::SeqCst));
    }

    #[test]
    fn thread_clone_sender_shares_the_worker_channel() {
        let t = Thread::create(
            RefAny::new(0_usize),
            RefAny::new(0_usize),
            worker_wait_d as ThreadCallbackType,
        );
        let sender = t.clone_sender().expect("std build must hand back a Sender");
        // Same channel as `send_message`: both reach the worker's receiver, which
        // stays alive until it is told to terminate.
        assert!(sender.send(ThreadSendMsg::Tick).is_ok());
        assert!(t.send_message(ThreadSendMsg::Tick));
        drop(sender);

        join_worker(&t);
        assert!(WAIT_D_TERMINATED.load(AtomicOrd::SeqCst));
        assert_eq!(WAIT_D_TICKS.load(AtomicOrd::SeqCst), 2);
    }

    #[test]
    fn thread_send_message_and_clone_sender_survive_a_poisoned_mutex() {
        let t = Thread::create(
            RefAny::new(0_usize),
            RefAny::new(0_usize),
            worker_wait_e as ThreadCallbackType,
        );
        let arc = Arc::clone(&*t.ptr);
        let handle = std::thread::spawn(move || {
            let _guard = arc.lock().expect("mutex is fresh here");
            panic!("intentional poison");
        });
        assert!(handle.join().is_err(), "helper thread must have panicked");

        // Both accessors are `.lock()`-fallible by design; poison must degrade
        // gracefully, not unwind through the FFI boundary.
        assert!(!t.send_message(ThreadSendMsg::Tick));
        assert!(t.clone_sender().is_none());

        // Teardown still works: Mutex::drop hands out the inner value regardless of
        // poison, so the Drop impl can still terminate + join the worker.
        drop(t);
        assert!(WAIT_E_TERMINATED.load(AtomicOrd::SeqCst));
    }

    #[test]
    fn thread_clone_shares_the_same_inner_state() {
        let t = create_thread_libstd(
            RefAny::new(0_usize),
            RefAny::new(0_usize),
            ThreadCallback::new(worker_quiet),
        );
        let cloned = t.clone();
        assert_eq!(Arc::strong_count(&*t.ptr), 2, "clone must be shallow");
        assert!(cloned.run_destructor);

        join_worker(&t);
        // The clone sees the same (now finished) ThreadInner.
        assert!(cloned.ptr.lock().expect("not poisoned").is_finished());
        drop(cloned);
        drop(t);
    }

    #[test]
    fn option_thread_into_option_round_trips() {
        assert!(OptionThread::None.into_option().is_none());

        let t = create_thread_libstd(
            RefAny::new(0_usize),
            RefAny::new(0_usize),
            ThreadCallback::new(worker_quiet),
        );
        let opt: OptionThread = Some(t).into();
        let recovered = opt.into_option().expect("Some must round-trip to Some");
        join_worker(&recovered);
        assert!(recovered.ptr.lock().expect("not poisoned").is_finished());
    }

    // ==================================================================
    // thread_sleep_* — numeric boundaries
    // ==================================================================

    #[test]
    fn thread_sleep_zero_returns_immediately_for_every_unit() {
        let start = StdInstant::now();
        assert_eq!(thread_sleep_ms(0), EmptyStruct::new());
        assert_eq!(thread_sleep_us(0), EmptyStruct::new());
        assert_eq!(thread_sleep_ns(0), EmptyStruct::new());
        // A zero sleep must not become an unbounded one.
        assert!(start.elapsed() < core::time::Duration::from_secs(5));
        assert_eq!(EmptyStruct::new()._reserved, 0);
    }

    #[test]
    fn thread_sleep_sleeps_at_least_the_requested_duration() {
        // std::thread::sleep guarantees *at least* the requested time.
        let start = StdInstant::now();
        let _slept = thread_sleep_ms(5);
        assert!(start.elapsed() >= core::time::Duration::from_millis(5));

        let start = StdInstant::now();
        let _slept = thread_sleep_us(5_000);
        assert!(start.elapsed() >= core::time::Duration::from_micros(5_000));

        let start = StdInstant::now();
        let _slept = thread_sleep_ns(5_000_000);
        assert!(start.elapsed() >= core::time::Duration::from_nanos(5_000_000));
    }

    #[test]
    fn thread_sleep_one_unit_does_not_panic() {
        // Smallest non-zero input in each unit: no truncation panic, no overflow.
        let _ms = thread_sleep_ms(1);
        let _us = thread_sleep_us(1);
        let _ns = thread_sleep_ns(1);
    }

    static MAX_SLEEP_ENTERED: AtomicBool = AtomicBool::new(false);
    static MAX_SLEEP_PANICKED: AtomicBool = AtomicBool::new(false);

    #[test]
    fn thread_sleep_max_converts_without_overflow() {
        // u64::MAX is representable in every Duration constructor these fns use, so
        // the conversion itself must not overflow-panic ...
        let _d_ms = core::time::Duration::from_millis(u64::MAX);
        let _d_us = core::time::Duration::from_micros(u64::MAX);
        let _d_ns = core::time::Duration::from_nanos(u64::MAX);

        // ... but the *sleep* is genuinely unbounded (~584 million years at MAX), so
        // it can only be exercised on a detached thread: assert it reaches the sleep
        // rather than unwinding. Nothing ever joins this thread by design.
        let _detached = std::thread::spawn(|| {
            MAX_SLEEP_ENTERED.store(true, AtomicOrd::SeqCst);
            if std::panic::catch_unwind(|| {
                let _slept = thread_sleep_ms(u64::MAX);
            })
            .is_err()
            {
                MAX_SLEEP_PANICKED.store(true, AtomicOrd::SeqCst);
            }
        });

        for _ in 0..200 {
            if MAX_SLEEP_ENTERED.load(AtomicOrd::SeqCst) {
                break;
            }
            let _slept = thread_sleep_ms(10);
        }
        assert!(
            MAX_SLEEP_ENTERED.load(AtomicOrd::SeqCst),
            "detached sleeper never started"
        );
        assert!(
            !MAX_SLEEP_PANICKED.load(AtomicOrd::SeqCst),
            "thread_sleep_ms(u64::MAX) must not panic"
        );
    }
}
