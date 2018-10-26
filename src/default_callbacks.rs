//! WARNING: Unsafe code ahead - calls the default methods

use app_state::AppStateNoData;
use window::WindowEvent;

pub type DefaultCallbackType<T, U> = fn(&mut U, app_state_no_data: AppStateNoData<T>, window_event: WindowEvent<T>) -> UpdateScreen;
pub type DefaultCallbackTypeUnchecked<T> = fn(&StackCheckedPointer<T>, app_state_no_data: AppStateNoData<T>, window_event: WindowEvent<T>) -> UpdateScreen;

mod stack_checked_pointer {

    use std::{
        fmt,
        hash::{Hash, Hasher},
        marker::PhantomData,
    };
    use {
        traits::Layout,
        dom::{UpdateScreen, Dom, Texture},
        default_callbacks::DefaultCallbackType,
        app_state::AppStateNoData,
        window::{WindowEvent, WindowInfo, HidpiAdjustedBounds},
    };

    /// A `StackCheckedPointer` is a type-erased, non-boxed pointer to a
    /// value **inside** of `T`, i.e. contained within `&T as usize` and
    /// `&T as usize + mem::size_of::<T>()`. `StackCheckedPointer<T>`
    /// has the same lifetime as `T`.
    pub struct StackCheckedPointer<T: Layout> {
        /// Type-erased pointer to a value on the stack in the `app_data.data`
        /// model. When invoking default methods, we have to store a pointer to
        /// the data we should update, but storing it in a `Box<T>` to
        /// erase the type doesn't help anything - we trust the user of this
        /// pointer to know the exact type of this pointer.
        internal: *const (),
        /// Marker so that one stack checked pointer can't be shared across
        /// two data models that are both `T: Layout`.
        marker: PhantomData<T>,
    }

    impl<T: Layout> StackCheckedPointer<T> {

        /// Validates that the pointer to U is contained in T.
        ///
        /// This means that the lifetime of U is the same lifetime as T -
        /// the returned `StackCheckedPointer` is valid for as long as `stack`
        /// is valid.
        pub fn new<U: Sized>(stack: &T, pointer: &U) -> Option<Self> {
            if is_subtype_of(stack, pointer) {
                Some(Self {
                    internal: pointer as *const _ as *const (),
                    marker: PhantomData,
                })
            } else {
                None
            }
        }

        /// **UNSAFE**: Invoke the pointer with a closure that can
        /// modify the pointer. It isn't checked that the `U` that the
        /// `StackCheckedPointer` was created with is the same as this `U`,
        /// but the **must be the same type**. This can't be checked since
        /// the type has been (deliberately) erased.
        ///
        /// **NOTE**: To avoid undefined behaviour, you **must** check that
        /// the `StackCheckedPointer` isn't mutably aliased at the time of
        /// calling the callback.
        pub unsafe fn invoke_mut<U: Sized>(
            &self,
            callback: DefaultCallbackType<T, U>,
            app_state_no_data: AppStateNoData<T>,
            window_event: WindowEvent<T>)
        -> UpdateScreen
        {
            // VERY UNSAFE, TRIPLE-CHECK FOR UNDEFINED BEHAVIOUR
            callback(&mut *(self.internal as *mut U), app_state_no_data, window_event)
        }

        pub unsafe fn invoke_mut_iframe<U: Sized>(
            &self,
            callback: fn(&mut U, WindowInfo<T>, HidpiAdjustedBounds) -> Dom<T>,
            window_info: WindowInfo<T>,
            dimensions: HidpiAdjustedBounds)
        -> Dom<T>
        {
            callback(&mut *(self.internal as *mut U), window_info, dimensions)
        }

        pub unsafe fn invoke_mut_texture<U: Sized>(
            &self,
            callback: fn(&mut U, WindowInfo<T>, HidpiAdjustedBounds) -> Option<Texture>,
            window_info: WindowInfo<T>,
            dimensions: HidpiAdjustedBounds)
        -> Option<Texture>
        {
            callback(&mut *(self.internal as *mut U), window_info, dimensions)
        }
    }

    // #[derive(Debug, Clone, PartialEq, Hash, Eq)] for StackCheckedPointer<T>

    impl<T: Layout> fmt::Debug for StackCheckedPointer<T> {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "StackCheckedPointer {{ internal: 0x{:x}, marker: {:?} }}", self.internal as usize, self.marker)
        }
    }

    impl<T: Layout> Clone for StackCheckedPointer<T> {
        fn clone(&self) -> Self {
            StackCheckedPointer { internal: self.internal, marker: self.marker.clone() }
        }
    }

    impl<T: Layout> Hash for StackCheckedPointer<T> {
      fn hash<H>(&self, state: &mut H) where H: Hasher {
        state.write_usize(self.internal as usize);
      }
    }

    impl<T: Layout> PartialEq for StackCheckedPointer<T> {
      fn eq(&self, rhs: &Self) -> bool {
        self.internal as usize == rhs.internal as usize
      }
    }

    impl<T: Layout> Eq for StackCheckedPointer<T> { }
    impl<T: Layout> Copy for StackCheckedPointer<T> { }


    /// Returns true if U is a type inside of T
    ///
    /// i.e:
    ///
    /// ```ignore
    /// # struct Data { i: usize, p: Vec<usize> }
    /// let data = Data { i: 5, p: vec![5] };
    ///
    /// // true because i is inside of data
    /// assert_eq!(is_subtype_of(&data, &data.i), true);
    /// // true because p is inside of data
    /// assert_eq!(is_subtype_of(&data, &data.p), true);
    /// // false because p is heap-allocated, therefore not inside of data
    /// assert_eq!(is_subtype_of(&data, &data.p[0]), false);
    /// ```
    fn is_subtype_of<T, U>(data: &T, subtype: &U) -> bool {

        // determine in which direction the stack grows
        use std::mem::size_of;

        struct Invalid {
            a: u64,
            b: u64,
        }

        let invalid = Invalid { a: 0, b: 0 };

        let stack_grows_down = &invalid.b as *const _ as usize > &invalid.a as *const _ as usize;

        // calculate if U is a subtype of T
        let st = subtype as *const _ as usize;
        let t = data as *const _ as usize;

        if stack_grows_down {
            st >= t && st + size_of::<U>() <= t + size_of::<T>()
        } else {
            st <= t && st - size_of::<U>() >= t - size_of::<T>()
        }
    }

    #[test]
    fn test_reflection_subtyping() {

        struct Data { i: usize, p: Vec<usize> }
        let data = Data { i: 5, p: vec![5] };

        assert_eq!(is_subtype_of(&data, &data.i), true);
        assert_eq!(is_subtype_of(&data, &data.p), true);
        assert_eq!(is_subtype_of(&data, &data.p[0]), false);
    }
}


pub use self::stack_checked_pointer::StackCheckedPointer;
use std::{
    collections::BTreeMap,
    fmt,
    hash::{Hash, Hasher},
    sync::atomic::{AtomicUsize, Ordering},
};
use {
    dom::{UpdateScreen},
    traits::Layout,
};

static LAST_DEFAULT_CALLBACK_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct DefaultCallbackId(usize);

pub(crate) fn get_new_unique_default_callback_id() -> DefaultCallbackId {
    DefaultCallbackId(LAST_DEFAULT_CALLBACK_ID.fetch_add(1, Ordering::SeqCst))
}

pub struct DefaultCallback<T: Layout>(pub DefaultCallbackTypeUnchecked<T>);

// #[derive(Debug, Clone, PartialEq, Hash, Eq)] for DefaultCallback<T>

impl<T: Layout> fmt::Debug for DefaultCallback<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "DefaultCallback @ 0x{:x}", self.0 as usize)
    }
}

impl<T: Layout> Clone for DefaultCallback<T> {
    fn clone(&self) -> Self {
        DefaultCallback(self.0.clone())
    }
}

impl<T: Layout> Hash for DefaultCallback<T> {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        state.write_usize(self.0 as usize);
    }
}

impl<T: Layout> PartialEq for DefaultCallback<T> {
    fn eq(&self, rhs: &Self) -> bool {
        self.0 as usize == rhs.0 as usize
    }
}

impl<T: Layout> Eq for DefaultCallback<T> { }

impl<T: Layout> Copy for DefaultCallback<T> { }

pub(crate) struct DefaultCallbackSystem<T: Layout> {
    callbacks: BTreeMap<DefaultCallbackId, (StackCheckedPointer<T>, DefaultCallback<T>)>,
}

impl<T: Layout> DefaultCallbackSystem<T> {

    /// Creates a new, empty list of callbacks
    pub(crate) fn new() -> Self {
        Self {
            callbacks: BTreeMap::new(),
        }
    }

    pub fn push_callback(
        &mut self,
        callback_id: DefaultCallbackId,
        ptr: StackCheckedPointer<T>,
        func: DefaultCallback<T>)
    {
        self.callbacks.insert(callback_id, (ptr, func));
    }

    /// NOTE: `app_data` is required so we know that we don't
    /// accidentally alias the data in `self.internal` (which could lead to UB).
    ///
    /// What we know is that the pointer (`self.internal`) points to somewhere
    /// in `T`, so we know that `self.internal` isn't aliased
    pub(crate) fn run_callback(
        &self,
        _app_data: &mut T,
        callback_id: &DefaultCallbackId,
        app_state_no_data: AppStateNoData<T>,
        window_event: WindowEvent<T>)
    -> UpdateScreen
    {
        if let Some((callback_ptr, callback_fn)) = self.callbacks.get(callback_id) {
            (callback_fn.0)(callback_ptr, app_state_no_data, window_event)
        } else {
            #[cfg(feature = "logging")] {
                warn!("Calling default callback with invalid ID {:?}", callback_id);
            }
            UpdateScreen::DontRedraw
        }
    }

    /// Clears all callbacks
    pub(crate) fn clear(&mut self) {
        self.callbacks.clear();
    }
}

impl<T: Layout> Clone for DefaultCallbackSystem<T> {
    fn clone(&self) -> Self {
        Self {
            callbacks: self.callbacks.clone(),
        }
    }
}