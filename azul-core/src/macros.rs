
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
/// ```
/// enum MyError<'a> {
///     Bar(BarError<'a>)
///     Foo(FooError<'a>)
/// }
///
/// impl_from!(BarError<'a>, Error::Bar);
/// impl_from!(BarError<'a>, Error::Bar);
///
/// ```
#[macro_export]
macro_rules! impl_from {
    // From a type with a lifetime to a type which also has a lifetime
    ($a:ident<$c:lifetime>, $b:ident::$enum_type:ident) => {
        impl<$c> From<$a<$c>> for $b<$c> {
            fn from(e: $a<$c>) -> Self {
                $b::$enum_type(e)
            }
        }
    };

    // From a type without a lifetime to a type which also does not have a lifetime
    ($a:ident, $b:ident::$enum_type:ident) => {
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
/// ```
/// enum Foo<'a> {
///     Bar(&'a str)
///     Baz(i32)
/// }
///
/// impl_display!{ Foo<'a>, {
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
