#![allow(unused_macros)]

/// Implements functions for `CallbackInfo` and `Info`,
/// to prevent duplicating the functions
#[macro_export]
macro_rules! impl_task_api {
    () => {
        /// Insert a timer into the list of active timers.
        /// Replaces the existing timer if called with the same TimerId.
        pub fn add_timer(&mut self, id: TimerId, timer: Timer) {
            self.timers.insert(id, timer);
        }

        /// Returns if a timer with the given ID is currently running
        pub fn has_timer(&self, timer_id: &TimerId) -> bool {
            self.get_timer(timer_id).is_some()
        }

        /// Returns a reference to an existing timer (if the `TimerId` is valid)
        pub fn get_timer(&self, timer_id: &TimerId) -> Option<&Timer> {
            self.timers.get(&timer_id)
        }

        /// Deletes a timer and returns it (if the `TimerId` is valid)
        pub fn delete_timer(&mut self, timer_id: &TimerId) -> Option<Timer> {
            self.timers.remove(timer_id)
        }

        /// Adds a (thread-safe) `Task` to the app that runs on a different thread
        pub fn add_task(&mut self, task: Task) {
            self.tasks.push(task);
        }
    };
}

/// Implement the `From` trait for any type.
/// Example usage:
/// ```rust,no_run,ignore
/// # use azul_core::impl_from;
/// enum MyError<'a> {
///     Bar(BarError<'a>)
///     Foo(FooError<'a>)
/// }
///
/// impl_from!(BarError<'a>, Error::Bar);
/// impl_from!(BarError<'a>, Error::Bar);
/// ```
#[macro_export]
macro_rules! impl_from {
    // From a type with a lifetime to a type which also has a lifetime
    ($a:ident < $c:lifetime > , $b:ident:: $enum_type:ident) => {
        impl<$c> From<$a<$c>> for $b<$c> {
            fn from(e: $a<$c>) -> Self {
                $b::$enum_type(e)
            }
        }
    };

    // From a type without a lifetime to a type which also does not have a lifetime
    ($a:ident, $b:ident:: $enum_type:ident) => {
        impl From<$a> for $b {
            fn from(e: $a) -> Self {
                $b::$enum_type(e)
            }
        }
    };
}

/// Implement `Display` for an enum.
///
/// Example usage:
/// ```rust,no_run,ignore
/// # use azul_core::impl_display;
/// enum Foo<'a> {
///     Bar(&'a str),
///     Baz(i32),
/// }
///
/// impl_display! { Foo<'a>, {
///     Bar(s) => s,
///     Baz(i) => format!("{}", i)
/// }}
/// ```
#[macro_export]
macro_rules! impl_display {
    // For a type with a lifetime
    ($enum:ident<$lt:lifetime>, {$($variant:pat => $fmt_string:expr),+$(,)* }) => {

        impl<$lt> ::core::fmt::Display for $enum<$lt> {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                use self::$enum::*;
                match &self {
                    $(
                        $variant => write!(f, "{}", $fmt_string),
                    )+
                }
            }
        }

    };

    // For a type without a lifetime
    ($enum:ident, {$($variant:pat => $fmt_string:expr),+$(,)* }) => {

        impl ::core::fmt::Display for $enum {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                use self::$enum::*;
                match &self {
                    $(
                        $variant => write!(f, "{}", $fmt_string),
                    )+
                }
            }
        }

    };
}

/// Implements `Display, Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Hash`
/// for a Callback with a `.0` field:
///
/// ```
/// # use azul_core::impl_callback;
/// type T = String;
/// struct MyCallback {
///     cb: fn(&T),
/// };
///
/// // impl Display, Debug, etc. for MyCallback
/// impl_callback!(MyCallback);
/// ```
///
/// This is necessary to work around for https://github.com/rust-lang/rust/issues/54508
#[macro_export]
macro_rules! impl_callback {
    // Version with callable field (for UI callbacks that need FFI support)
    ($callback_value:ident) => {
        impl ::core::fmt::Display for $callback_value {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{:?}", self)
            }
        }

        impl ::core::fmt::Debug for $callback_value {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                let callback = stringify!($callback_value);
                write!(f, "{} @ 0x{:x}", callback, self.cb as usize)
            }
        }

        impl Clone for $callback_value {
            fn clone(&self) -> Self {
                $callback_value {
                    cb: self.cb.clone(),
                    callable: self.callable.clone(),
                }
            }
        }

        impl ::core::hash::Hash for $callback_value {
            fn hash<H>(&self, state: &mut H)
            where
                H: ::core::hash::Hasher,
            {
                state.write_usize(self.cb as usize);
            }
        }

        impl PartialEq for $callback_value {
            fn eq(&self, rhs: &Self) -> bool {
                self.cb as usize == rhs.cb as usize
            }
        }

        impl PartialOrd for $callback_value {
            fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {
                Some((self.cb as usize).cmp(&(other.cb as usize)))
            }
        }

        impl Ord for $callback_value {
            fn cmp(&self, other: &Self) -> ::core::cmp::Ordering {
                (self.cb as usize).cmp(&(other.cb as usize))
            }
        }

        impl Eq for $callback_value {}
    };
}

/// Macro to implement callback traits for simple system callbacks (no callable field)
///
/// Use this for destructor callbacks, system callbacks, and other internal callbacks
/// that don't need FFI callable support.
#[macro_export]
macro_rules! impl_callback_simple {
    ($callback_value:ident) => {
        impl ::core::fmt::Display for $callback_value {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{:?}", self)
            }
        }

        impl ::core::fmt::Debug for $callback_value {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                let callback = stringify!($callback_value);
                write!(f, "{} @ 0x{:x}", callback, self.cb as usize)
            }
        }

        impl Clone for $callback_value {
            fn clone(&self) -> Self {
                $callback_value {
                    cb: self.cb.clone(),
                }
            }
        }

        impl ::core::hash::Hash for $callback_value {
            fn hash<H>(&self, state: &mut H)
            where
                H: ::core::hash::Hasher,
            {
                state.write_usize(self.cb as usize);
            }
        }

        impl PartialEq for $callback_value {
            fn eq(&self, rhs: &Self) -> bool {
                self.cb as usize == rhs.cb as usize
            }
        }

        impl PartialOrd for $callback_value {
            fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {
                Some((self.cb as usize).cmp(&(other.cb as usize)))
            }
        }

        impl Ord for $callback_value {
            fn cmp(&self, other: &Self) -> ::core::cmp::Ordering {
                (self.cb as usize).cmp(&(other.cb as usize))
            }
        }

        impl Eq for $callback_value {}

        impl Copy for $callback_value {}
    };
}

#[allow(unused_macros)]
macro_rules! impl_get_gl_context {
    () => {
        /// Returns a reference-counted pointer to the OpenGL context
        pub fn get_gl_context(&self) -> OptionGlContextPtr {
            Some(self.gl_context.clone())
        }
    };
}
