//! Built-in widgets for the Azul GUI system

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
// /// Checkbox widget
// pub mod check_box;
// /// Label widget (centered text)
// pub mod label;
// /// Single line text input widget
// pub mod text_input;
// /// Same as text input, but only allows numeric input
// pub mod number_input;
// /// Box displaying a color which opens a color picker dialog on being clicked
// pub mod color_input;
// /// Spreadsheet (iframe) widget
// pub mod spreadsheet;
// /// Slider widget
// pub mod slider;
// /// Dropdown selection widget
// pub mod drop_down;
// /// Multi-line text input
// pub mod text_edit;