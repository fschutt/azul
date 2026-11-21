//! The one macro the tables of contract 4 use: a closed list of the plan's names as an enum.

/// A closed list of the plan's names: each variant has the text the plan writes. `ALL` is the plan's order,
/// `NAMES` the same names, `name` and `parse` go both ways.
macro_rules! text_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $($(#[$vmeta:meta])* $variant:ident = $text:literal,)+
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        $vis enum $name {
            $($(#[$vmeta])* $variant,)+
        }

        impl $name {
            /// Every variant, in the order the plan lists them.
            pub const ALL: &'static [$name] = &[$($name::$variant,)+];
            /// Their names, in the same order.
            pub const NAMES: &'static [&'static str] = &[$($text,)+];

            /// The name the plan and the wire use.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $($name::$variant => $text,)+
                }
            }

            /// The variant of a name, if it is one.
            #[must_use]
            pub fn parse(text: &str) -> Option<$name> {
                match text {
                    $($text => Some($name::$variant),)+
                    _ => None,
                }
            }
        }
    };
}
