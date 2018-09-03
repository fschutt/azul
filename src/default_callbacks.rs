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
        pub fn invoke_mut<U: Sized, F: FnMut(&mut U)>(&self, mut callback: F) {
            // VERY UNSAFE, TRIPLE-CHECK FOR UNDEFINED BEHAVIOUR
            callback(unsafe { &mut *(self.internal as *mut U) })
        }
    }

    impl<T: Layout> fmt::Debug for StackCheckedPointer<T> {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "StackCheckedPointer {{ internal: 0x{:x}, marker: {:?} }}", self.internal as usize, self.marker)
        }
    }

    /// Returns true if U is a type inside of T
    ///
    /// i.e:
    ///
    /// ```
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


use self::stack_checked_pointer::StackCheckedPointer;
use std::collections::{BTreeMap, HashMap};
use {
    dom::On,
    cache::DomHash,
    traits::Layout,
};

pub struct DefaultCallbackSystem<T: Layout> {
    callbacks: BTreeMap<DomHash, HashMap<On, (StackCheckedPointer<T>, fn(&StackCheckedPointer<T>))>>,
}

impl<T: Layout> DefaultCallbackSystem<T> {
    pub fn new() -> Self {
        Self {
            callbacks: BTreeMap::new(),
        }
    }

    pub fn push_callback<U>(
        &mut self, 
        dom_hash: DomHash, 
        on: On, 
        app_data: &T, 
        ptr: &U, 
        func: fn(&StackCheckedPointer<T>)) 
    {
        
    }

    pub fn run_all_callbacks(&self, app_data: &mut T) {
        println!("running all default callbacks!");
        for callback_list in self.callbacks.values() {
            for (on, (callback_ptr, callback_fn)) in callback_list.iter() {
                println!("on: {:?} ptr: {:?}", on, callback_ptr); 
            }
        }
    }
}