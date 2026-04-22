//! Utility macros for implementing common trait patterns on callback types
//! and enum conversions (`From`, `Display`). Used by the `core`, `layout`,
//! and `css` crates.

/// Implement the `From` trait for any type.
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

/// Helper macro implementing the shared trait impls (`Display`, `Debug`, `Hash`,
/// `PartialEq`, `Eq`, `PartialOrd`, `Ord`) for callback types.
/// Used internally by [`impl_callback!`] and [`impl_callback_simple!`].
#[macro_export]
macro_rules! impl_callback_traits {
    ($callback_value:ident) => {
        impl ::core::fmt::Display for $callback_value {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{:?}", self)
            }
        }

        impl ::core::fmt::Debug for $callback_value {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                let callback = stringify!($callback_value);
                write!(f, "{} @ 0x{:x}", callback, self.cb as *const () as usize)
            }
        }

        impl ::core::hash::Hash for $callback_value {
            fn hash<H>(&self, state: &mut H)
            where
                H: ::core::hash::Hasher,
            {
                state.write_usize(self.cb as *const () as usize);
            }
        }

        impl PartialEq for $callback_value {
            fn eq(&self, rhs: &Self) -> bool {
                self.cb as *const () as usize == rhs.cb as usize
            }
        }

        impl PartialOrd for $callback_value {
            fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {
                Some((self.cb as *const () as usize).cmp(&(other.cb as *const () as usize)))
            }
        }

        impl Ord for $callback_value {
            fn cmp(&self, other: &Self) -> ::core::cmp::Ordering {
                (self.cb as *const () as usize).cmp(&(other.cb as *const () as usize))
            }
        }

        impl Eq for $callback_value {}
    };
}

/// Implements `Display, Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord`
/// for a callback struct with `.cb` (function pointer) and `.ctx` (`OptionRefAny`) fields.
/// Also implements `From<$callback_ty>` to create a callback from a raw function pointer.
///
/// For callbacks with only a `.cb` field (no `.ctx`), use [`impl_callback_simple!`] instead.
///
/// This is necessary to work around for https://github.com/rust-lang/rust/issues/54508
#[macro_export]
macro_rules! impl_callback {
    // Version with callable field (for UI callbacks that need FFI support)
    ($callback_value:ident, $callback_ty:ty) => {
        $crate::impl_callback_traits!($callback_value);

        impl Clone for $callback_value {
            fn clone(&self) -> Self {
                $callback_value {
                    cb: self.cb.clone(),
                    ctx: self.ctx.clone(),
                }
            }
        }

        /// Allow creating callback from a raw function pointer
        /// Sets callable to None (for native Rust/C usage)
        impl From<$callback_ty> for $callback_value {
            fn from(cb: $callback_ty) -> Self {
                $callback_value {
                    cb,
                    ctx: $crate::refany::OptionRefAny::None,
                }
            }
        }
    };
}

/// Macro to implement callback traits for simple system callbacks (no callable field)
///
/// Use this for destructor callbacks, system callbacks, and other internal callbacks
/// that don't need FFI callable support.
#[macro_export]
macro_rules! impl_callback_simple {
    ($callback_value:ident) => {
        $crate::impl_callback_traits!($callback_value);

        impl Clone for $callback_value {
            fn clone(&self) -> Self {
                $callback_value {
                    cb: self.cb.clone(),
                }
            }
        }

        impl Copy for $callback_value {}
    };
}
