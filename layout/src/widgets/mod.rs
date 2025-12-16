//! Built-in widgets for the Azul GUI system

/// Implements `Display, Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Hash`
/// for a Callback with a `.cb` field:
///
/// ```
/// # use azul_core::impl_callback;
/// # use std::fmt;
/// struct T {}
///
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
    (
        $callback_wrapper:ident,
        $option_callback_wrapper:ident,
        $callback_value:ident,
        $callback_ty:ident
    ) => {
        #[derive(Debug, Clone, PartialEq, PartialOrd)]
        #[repr(C)]
        pub struct $callback_wrapper {
            pub data: RefAny,
            pub callback: $callback_value,
        }

        #[repr(C)]
        pub struct $callback_value {
            pub cb: $callback_ty,
            /// For FFI: stores the foreign callable (e.g., PyFunction)
            /// Native Rust code sets this to None
            pub callable: azul_core::refany::OptionRefAny,
        }

        azul_css::impl_option!(
            $callback_wrapper,
            $option_callback_wrapper,
            copy = false,
            [Debug, Clone, PartialEq, PartialOrd]
        );

        impl $callback_value {
            /// Create a new callback with just a function pointer (for native Rust code)
            pub fn new(cb: $callback_ty) -> Self {
                Self { cb, callable: azul_core::refany::OptionRefAny::None }
            }
        }

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

        impl core::hash::Hash for $callback_value {
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

        /// Allow creating callback from a raw function pointer
        /// Sets callable to None (for native Rust/C usage)
        impl From<$callback_ty> for $callback_value {
            fn from(cb: $callback_ty) -> Self {
                $callback_value {
                    cb,
                    callable: azul_core::refany::OptionRefAny::None,
                }
            }
        }
    };
}

/// Button widget
pub mod button;
/// Checkbox widget
pub mod check_box;
/// Box displaying a color which opens a color picker dialog on being clicked
pub mod color_input;
/// File input widget
pub mod file_input;
/// Label widget (centered text)
pub mod label;
// /// Single line text input widget
/// Drop-down select widget
pub mod drop_down;
/// Frame container widget
pub mod frame;
/// List view widget
pub mod list_view;
/// Node graph widget
pub mod node_graph;
/// Same as text input, but only allows numeric input
pub mod number_input;
/// Progress bar widget
pub mod progressbar;
/// Ribbon widget
pub mod ribbon;
/// Tab container widgets
pub mod tabs;
pub mod text_input;
/// Tree view widget
pub mod tree_view;
// /// Spreadsheet (iframe) widget
// pub mod spreadsheet;
// /// Slider widget
// pub mod slider;
// /// Multi-line text input
// pub mod text_edit;
