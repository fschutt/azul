//! Thread callback information and utilities for azul-layout
//!
//! This module provides thread-related callback structures for background tasks
//! that need to interact with the UI thread and query layout information.

#[cfg(feature = "std")]
use alloc::sync::Arc;
#[cfg(feature = "std")]
use core::sync::atomic::AtomicBool;
#[cfg(feature = "std")]
use core::sync::atomic::Ordering;
#[cfg(feature = "std")]
use std::sync::{
    mpsc::{channel, Receiver, Sender},
    Mutex,
};
#[cfg(feature = "std")]
use std::thread::{self, JoinHandle};

use azul_core::{
    callbacks::Update,
    refany::RefAny,
    task::{
        OptionThreadSendMsg, ThreadId, ThreadReceiver, ThreadReceiverDestructorCallback,
        ThreadReceiverInner, ThreadRecvCallback, ThreadSendMsg,
    },
};

use crate::callbacks::CallbackInfo;

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
        match inner {
            None => OptionThreadReceiveMsg::None,
            Some(v) => OptionThreadReceiveMsg::Some(v),
        }
    }
}

impl OptionThreadReceiveMsg {
    pub fn into_option(self) -> Option<ThreadReceiveMsg> {
        match self {
            OptionThreadReceiveMsg::None => None,
            OptionThreadReceiveMsg::Some(v) => Some(v),
        }
    }

    pub fn as_ref(&self) -> Option<&ThreadReceiveMsg> {
        match self {
            OptionThreadReceiveMsg::None => None,
            OptionThreadReceiveMsg::Some(v) => Some(v),
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
    pub fn new(callback: WriteBackCallbackType, data: RefAny) -> Self {
        Self {
            refany: data,
            callback: WriteBackCallback { cb: callback },
        }
    }
}

/// ThreadSender allows sending messages from the background thread to the main thread
#[derive(Debug)]
#[repr(C)]
pub struct ThreadSender {
    #[cfg(feature = "std")]
    pub ptr: alloc::boxed::Box<Arc<Mutex<ThreadSenderInner>>>,
    #[cfg(not(feature = "std"))]
    pub ptr: *const core::ffi::c_void,
    pub run_destructor: bool,
}

impl Clone for ThreadSender {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr.clone(),
            run_destructor: true,
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
        }
    }

    #[cfg(feature = "std")]
    pub fn new(t: ThreadSenderInner) -> Self {
        Self {
            ptr: alloc::boxed::Box::new(Arc::new(Mutex::new(t))),
            run_destructor: true,
        }
    }

    #[cfg(not(feature = "std"))]
    pub fn send(&mut self, _msg: ThreadReceiveMsg) -> bool {
        false
    }

    #[cfg(feature = "std")]
    pub fn send(&mut self, msg: ThreadReceiveMsg) -> bool {
        let ts = match self.ptr.lock().ok() {
            Some(s) => s,
            None => return false,
        };
        (ts.send_fn.cb)(ts.ptr.as_ref() as *const _ as *const core::ffi::c_void, msg)
    }
}

#[derive(Debug)]
#[cfg_attr(not(feature = "std"), derive(PartialEq, PartialOrd, Eq, Ord))]
#[repr(C)]
pub struct ThreadSenderInner {
    #[cfg(feature = "std")]
    pub ptr: alloc::boxed::Box<Sender<ThreadReceiveMsg>>,
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
        (self.ptr.as_ref() as *const _ as usize).hash(state);
    }
}

#[cfg(feature = "std")]
impl PartialEq for ThreadSenderInner {
    fn eq(&self, other: &Self) -> bool {
        (self.ptr.as_ref() as *const _ as usize) == (other.ptr.as_ref() as *const _ as usize)
    }
}

#[cfg(feature = "std")]
impl Eq for ThreadSenderInner {}

#[cfg(feature = "std")]
impl PartialOrd for ThreadSenderInner {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(
            (self.ptr.as_ref() as *const _ as usize)
                .cmp(&(other.ptr.as_ref() as *const _ as usize)),
        )
    }
}

#[cfg(feature = "std")]
impl Ord for ThreadSenderInner {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.ptr.as_ref() as *const _ as usize).cmp(&(other.ptr.as_ref() as *const _ as usize))
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

impl core::fmt::Debug for ThreadSendCallback {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "ThreadSendCallback {{ cb: {:p} }}", self.cb as *const ())
    }
}

impl Clone for ThreadSendCallback {
    fn clone(&self) -> Self {
        Self { cb: self.cb }
    }
}

/// Destructor callback for ThreadSender
pub type ThreadSenderDestructorCallbackType = extern "C" fn(*mut ThreadSenderInner);

#[repr(C)]
pub struct ThreadSenderDestructorCallback {
    pub cb: ThreadSenderDestructorCallbackType,
}

impl core::fmt::Debug for ThreadSenderDestructorCallback {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "ThreadSenderDestructorCallback {{ cb: {:p} }}",
            self.cb as *const ()
        )
    }
}

impl Clone for ThreadSenderDestructorCallback {
    fn clone(&self) -> Self {
        Self { cb: self.cb }
    }
}

/// Callback that runs when a thread receives a `WriteBack` message
///
/// This callback runs on the main UI thread and has access dir_to: 
/// - The thread's original data
/// - Data sent back from the background thread
/// - Full CallbackInfo for DOM queries and UI updates
pub type WriteBackCallbackType = extern "C" fn(
    /* original thread data */ RefAny,
    /* data to write back */ RefAny,
    /* callback info */ CallbackInfo,
) -> Update;

/// Callback that can run when a thread receives a `WriteBack` message
#[repr(C)]
pub struct WriteBackCallback {
    pub cb: WriteBackCallbackType,
}

impl WriteBackCallback {
    /// Create a new WriteBackCallback
    pub fn new(cb: WriteBackCallbackType) -> Self {
        Self { cb }
    }

    /// Invoke the callback
    pub fn invoke(
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
        Self { cb: self.cb }
    }
}

impl PartialEq for WriteBackCallback {
    fn eq(&self, other: &Self) -> bool {
        self.cb as usize == other.cb as usize
    }
}

impl Eq for WriteBackCallback {}

impl PartialOrd for WriteBackCallback {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        (self.cb as usize).partial_cmp(&(other.cb as usize))
    }
}

impl Ord for WriteBackCallback {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.cb as usize).cmp(&(other.cb as usize))
    }
}

impl core::hash::Hash for WriteBackCallback {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        (self.cb as usize).hash(state);
    }
}

// ThreadCallback type
pub type ThreadCallbackType = extern "C" fn(RefAny, ThreadSender, ThreadReceiver);

#[repr(C)]
pub struct ThreadCallback {
    pub cb: ThreadCallbackType,
}

impl core::fmt::Debug for ThreadCallback {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "ThreadCallback {{ cb: {:p} }}", self.cb as *const ())
    }
}

impl Clone for ThreadCallback {
    fn clone(&self) -> Self {
        Self { cb: self.cb }
    }
}

impl PartialEq for ThreadCallback {
    fn eq(&self, other: &Self) -> bool {
        self.cb as usize == other.cb as usize
    }
}

impl Eq for ThreadCallback {}

impl PartialOrd for ThreadCallback {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        (self.cb as usize).partial_cmp(&(other.cb as usize))
    }
}

impl Ord for ThreadCallback {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.cb as usize).cmp(&(other.cb as usize))
    }
}

impl core::hash::Hash for ThreadCallback {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        (self.cb as usize).hash(state);
    }
}

// Callback types for thread operations
pub type CheckThreadFinishedCallbackType = extern "C" fn(*const core::ffi::c_void) -> bool;

#[repr(C)]
pub struct CheckThreadFinishedCallback {
    pub cb: CheckThreadFinishedCallbackType,
}

impl core::fmt::Debug for CheckThreadFinishedCallback {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "CheckThreadFinishedCallback {{ cb: {:p} }}",
            self.cb as *const ()
        )
    }
}

impl Clone for CheckThreadFinishedCallback {
    fn clone(&self) -> Self {
        Self { cb: self.cb }
    }
}

pub type LibrarySendThreadMsgCallbackType =
    extern "C" fn(*const core::ffi::c_void, ThreadSendMsg) -> bool;

#[repr(C)]
pub struct LibrarySendThreadMsgCallback {
    pub cb: LibrarySendThreadMsgCallbackType,
}

impl core::fmt::Debug for LibrarySendThreadMsgCallback {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "LibrarySendThreadMsgCallback {{ cb: {:p} }}",
            self.cb as *const ()
        )
    }
}

impl Clone for LibrarySendThreadMsgCallback {
    fn clone(&self) -> Self {
        Self { cb: self.cb }
    }
}

pub type LibraryReceiveThreadMsgCallbackType =
    extern "C" fn(*const core::ffi::c_void) -> OptionThreadReceiveMsg;

#[repr(C)]
pub struct LibraryReceiveThreadMsgCallback {
    pub cb: LibraryReceiveThreadMsgCallbackType,
}

impl core::fmt::Debug for LibraryReceiveThreadMsgCallback {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "LibraryReceiveThreadMsgCallback {{ cb: {:p} }}",
            self.cb as *const ()
        )
    }
}

impl Clone for LibraryReceiveThreadMsgCallback {
    fn clone(&self) -> Self {
        Self { cb: self.cb }
    }
}

pub type ThreadDestructorCallbackType = extern "C" fn(*mut ThreadInner);

#[repr(C)]
pub struct ThreadDestructorCallback {
    pub cb: ThreadDestructorCallbackType,
}

impl core::fmt::Debug for ThreadDestructorCallback {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "ThreadDestructorCallback {{ cb: {:p} }}",
            self.cb as *const ()
        )
    }
}

impl Clone for ThreadDestructorCallback {
    fn clone(&self) -> Self {
        Self { cb: self.cb }
    }
}

/// Wrapper around Thread because Thread needs to be clone-able
#[derive(Debug)]
#[repr(C)]
pub struct Thread {
    #[cfg(feature = "std")]
    pub ptr: alloc::boxed::Box<Arc<Mutex<ThreadInner>>>,
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
    pub fn new(ti: ThreadInner) -> Self {
        Self {
            ptr: alloc::boxed::Box::new(Arc::new(Mutex::new(ti))),
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
    pub thread_handle: alloc::boxed::Box<Option<JoinHandle<()>>>,
    #[cfg(not(feature = "std"))]
    pub thread_handle: *const core::ffi::c_void,

    #[cfg(feature = "std")]
    pub sender: alloc::boxed::Box<Sender<ThreadSendMsg>>,
    #[cfg(not(feature = "std"))]
    pub sender: *const core::ffi::c_void,

    #[cfg(feature = "std")]
    pub receiver: alloc::boxed::Box<Receiver<ThreadReceiveMsg>>,
    #[cfg(not(feature = "std"))]
    pub receiver: *const core::ffi::c_void,

    #[cfg(feature = "std")]
    pub dropcheck: alloc::boxed::Box<alloc::sync::Weak<()>>,
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
    pub fn is_finished(&self) -> bool {
        (self.check_thread_finished_fn.cb)(
            self.dropcheck.as_ref() as *const _ as *const core::ffi::c_void
        )
    }

    /// Send a message to the thread
    pub fn sender_send(&mut self, msg: ThreadSendMsg) -> bool {
        (self.send_thread_msg_fn.cb)(
            self.sender.as_ref() as *const _ as *const core::ffi::c_void,
            msg,
        )
    }

    /// Try to receive a message from the thread (non-blocking)
    pub fn receiver_try_recv(&mut self) -> OptionThreadReceiveMsg {
        (self.receive_thread_msg_fn.cb)(
            self.receiver.as_ref() as *const _ as *const core::ffi::c_void
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
    unsafe { &*(sender as *const Sender<ThreadSendMsg>) }
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
    unsafe { &*(receiver as *const Receiver<ThreadReceiveMsg>) }
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
    unsafe { &*(sender as *const Sender<ThreadReceiveMsg>) }
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
    unsafe { &*(receiver as *const Receiver<ThreadSendMsg>) }
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
    unsafe { &*(dropcheck as *const alloc::sync::Weak<()>) }
        .upgrade()
        .is_none()
}

#[cfg(not(feature = "std"))]
extern "C" fn default_check_thread_finished(_dropcheck: *const core::ffi::c_void) -> bool {
    true
}

#[cfg(feature = "std")]
extern "C" fn thread_sender_drop(_: *mut ThreadSenderInner) {}

#[cfg(not(feature = "std"))]
extern "C" fn thread_sender_drop(_: *mut ThreadSenderInner) {}

#[cfg(feature = "std")]
extern "C" fn thread_receiver_drop(_: *mut ThreadReceiverInner) {}

#[cfg(not(feature = "std"))]
extern "C" fn thread_receiver_drop(_: *mut ThreadReceiverInner) {}

/// Function that creates a new Thread object
pub type CreateThreadCallbackType = extern "C" fn(RefAny, RefAny, ThreadCallback) -> Thread;

#[repr(C)]
pub struct CreateThreadCallback {
    pub cb: CreateThreadCallbackType,
}

impl core::fmt::Debug for CreateThreadCallback {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "CreateThreadCallback {{ cb: {:p} }}",
            self.cb as *const ()
        )
    }
}

impl Clone for CreateThreadCallback {
    fn clone(&self) -> Self {
        Self { cb: self.cb }
    }
}

impl Copy for CreateThreadCallback {}

impl PartialEq for CreateThreadCallback {
    fn eq(&self, other: &Self) -> bool {
        self.cb as usize == other.cb as usize
    }
}

impl Eq for CreateThreadCallback {}

impl PartialOrd for CreateThreadCallback {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        (self.cb as usize).partial_cmp(&(other.cb as usize))
    }
}

impl Ord for CreateThreadCallback {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.cb as usize).cmp(&(other.cb as usize))
    }
}

impl core::hash::Hash for CreateThreadCallback {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        (self.cb as usize).hash(state);
    }
}

/// Create a new thread using the standard library
#[cfg(feature = "std")]
pub extern "C" fn create_thread_libstd(
    thread_initialize_data: RefAny,
    writeback_data: RefAny,
    callback: ThreadCallback,
) -> Thread {
    let (sender_receiver, receiver_receiver) = channel::<ThreadReceiveMsg>();
    let sender_receiver = ThreadSender::new(ThreadSenderInner {
        ptr: alloc::boxed::Box::new(sender_receiver),
        send_fn: ThreadSendCallback {
            cb: default_send_thread_msg_fn,
        },
        destructor: ThreadSenderDestructorCallback {
            cb: thread_sender_drop,
        },
    });

    let (sender_sender, receiver_sender) = channel::<ThreadSendMsg>();
    let receiver_sender = ThreadReceiver::new(ThreadReceiverInner {
        ptr: alloc::boxed::Box::new(receiver_sender),
        recv_fn: ThreadRecvCallback {
            cb: default_receive_thread_msg_fn,
        },
        destructor: ThreadReceiverDestructorCallback {
            cb: thread_receiver_drop,
        },
    });

    let thread_check = Arc::new(());
    let dropcheck = Arc::downgrade(&thread_check);

    let thread_handle = Some(thread::spawn(move || {
        let _ = thread_check;
        (callback.cb)(thread_initialize_data, sender_receiver, receiver_sender);
        // thread_check gets dropped here, signals that the thread has finished
    }));

    let thread_handle: alloc::boxed::Box<Option<JoinHandle<()>>> =
        alloc::boxed::Box::new(thread_handle);
    let sender: alloc::boxed::Box<Sender<ThreadSendMsg>> = alloc::boxed::Box::new(sender_sender);
    let receiver: alloc::boxed::Box<Receiver<ThreadReceiveMsg>> =
        alloc::boxed::Box::new(receiver_receiver);
    let dropcheck: alloc::boxed::Box<alloc::sync::Weak<()>> = alloc::boxed::Box::new(dropcheck);

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
        assert_eq!(callback.cb as usize, test_writeback_callback as usize);
    }

    #[test]
    fn test_writeback_callback_clone() {
        let callback = WriteBackCallback::new(test_writeback_callback);
        let cloned = callback.clone();
        assert_eq!(callback, cloned);
    }
}
