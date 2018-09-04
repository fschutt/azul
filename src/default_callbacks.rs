//! WARNING: Unsafe code ahead - calls the default methods

mod stack_checked_pointer {

    use std::marker::PhantomData;
    use traits::Layout;
    use std::fmt;

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
        pub unsafe fn invoke_mut<U: Sized>(&self, callback: fn(&mut U)) {
            // VERY UNSAFE, TRIPLE-CHECK FOR UNDEFINED BEHAVIOUR
            callback(&mut *(self.internal as *mut U))
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
}


pub use self::stack_checked_pointer::StackCheckedPointer;
use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    hash::{Hash, Hasher},
};
use {
    dom::On,
    id_tree::NodeId,
    traits::Layout,
};

pub struct DefaultCallback<T: Layout>(pub fn(&StackCheckedPointer<T>));

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
    callbacks: BTreeMap<NodeId, HashMap<On, (StackCheckedPointer<T>, DefaultCallback<T>)>>,
}

impl<T: Layout> DefaultCallbackSystem<T> {

    /// Creates a new, empty list of callbacks
    pub(crate) fn new() -> Self {
        Self {
            callbacks: BTreeMap::new(),
        }
    }

    pub fn push_callback<U>(
        &mut self,
        app_data: &T,
        node_id: NodeId,
        on: On,
        ptr: &U,
        func: DefaultCallback<T>)
    {
        use std::collections::hash_map::Entry::*;

        let stack_checked_pointer = match StackCheckedPointer::new(app_data, ptr) {
            Some(s) => s,
            None => panic!(
                "Default callback for function {:?} ({:?} - {:?}) constructed with \
                non-stack pointer at 0x{:x}. This is a potential security risk \
                and it is unsafe to continue execution. \n\
                \n\
                If you create a App<T> and want to register a default function, \
                you can only create function that take pointers to the data of T, you \
                can't use reference to heap-allocated data, since the lifetimes \
                of these references can't be controlled by the framework.",
                func, node_id, on, ptr as *const _ as usize),
        };

        match self.callbacks.entry(node_id).or_insert_with(|| HashMap::new()).entry(on) {
            Occupied(mut o) => {
                warn!("Overwriting {:?} for DOM node {:?}", on, node_id);
                o.insert((stack_checked_pointer, func));
            },
            Vacant(v) => { v.insert((stack_checked_pointer, func)); },
        }
    }

    /// NOTE: `app_data` is required so we know that we don't
    /// accidentally alias the data in `T` (which could lead to UB).
    pub(crate) fn run_all_callbacks(&self, _app_data: &mut T) {
        for callback_list in self.callbacks.values() {
            for (on, (callback_ptr, callback_fn)) in callback_list.iter() {
                // The actual pointer isn't a fn(&StackCheckedPtr), but a fn(&mut U)
                println!("calling default callback!");
                // callback_ptr.invoke_mut()
            }
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