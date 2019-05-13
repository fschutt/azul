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
macro_rules! impl_display {
    // For a type with a lifetime
    ($enum:ident<$lt:lifetime>, {$($variant:pat => $fmt_string:expr),+$(,)* }) => {

        impl<$lt> ::std::fmt::Display for $enum<$lt> {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
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

        impl ::std::fmt::Display for $enum {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
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

/// Implements `Debug` to use `Display` instead - assumes the that the type has implemented `Display`
macro_rules! impl_debug_as_display {
    // For a type with a lifetime
    ($enum:ident<$lt:lifetime>) => {

        impl<$lt> ::std::fmt::Debug for $enum<$lt> {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                write!(f, "{}", self)
            }
        }

    };

    // For a type without a lifetime
    ($enum:ident) => {

        impl ::std::fmt::Debug for $enum {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                write!(f, "{}", self)
            }
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
