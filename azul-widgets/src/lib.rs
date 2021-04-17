// #![no_std]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico",
)]

//! Built-in widgets for the Azul GUI system

extern crate azul;
extern crate alloc;

/// Implements `Display, Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Hash`
/// for a Callback with a `.cb` field:
///
/// ```
/// struct MyCallback { cb: fn (&T) };
///
/// // impl Display, Debug, etc. for MyCallback
/// impl_callback!(MyCallback);
/// ```
///
/// This is necessary to work around for https://github.com/rust-lang/rust/issues/54508
#[macro_export]
macro_rules! impl_callback {($callback_value:ident) => (

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
            $callback_value { cb: self.cb.clone() }
        }
    }

    impl core::hash::Hash for $callback_value {
        fn hash<H>(&self, state: &mut H) where H: ::core::hash::Hasher {
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

    impl Eq for $callback_value { }

    impl Copy for $callback_value { }
)}

/// Button widget
pub mod button;
/// Label widget
pub mod label;
/// Text input (two-way binding) widget
pub mod text_input;
/// Table view (iframe) widget
pub mod table_view;