//! Internal macros for reducing boilerplate in property definitions.

/// Implement `Display` for an enum.
///
/// Example usage:
/// ```no_run,ignore
/// enum Foo<'a> {
///     Bar(&'a str),
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

/// Implements `Debug` to use `Display` instead - assumes the that the type has implemented
/// `Display`
macro_rules! impl_debug_as_display {
    // For a type with a lifetime
    ($enum:ident < $lt:lifetime >) => {
        impl<$lt> ::core::fmt::Debug for $enum<$lt> {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{}", self)
            }
        }
    };

    // For a type without a lifetime
    ($enum:ident) => {
        impl ::core::fmt::Debug for $enum {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{}", self)
            }
        }
    };
}

/// Implement the `From` trait for any type.
/// Example usage:
/// ```no_run,ignore
/// enum MyError<'a> {
///     Bar(BarError<'a>),
///     Foo(FooError<'a>)
/// }
///
/// impl_from!(BarError<'a>, Error::Bar);
/// impl_from!(BarError<'a>, Error::Bar);
/// ```
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

/// Creates `pt`, `px` and `em` constructors for any struct that has a
/// `PixelValue` as it's self.0 field.
macro_rules! impl_pixel_value {
    ($struct:ident) => {
        impl $struct {
            #[inline]
            pub const fn zero() -> Self {
                Self {
                    inner: crate::props::basic::value::PixelValue::zero(),
                }
            }

            #[inline]
            pub const fn const_px(value: isize) -> Self {
                Self {
                    inner: crate::props::basic::value::PixelValue::const_px(value),
                }
            }

            #[inline]
            pub const fn const_em(value: isize) -> Self {
                Self {
                    inner: crate::props::basic::value::PixelValue::const_em(value),
                }
            }

            #[inline]
            pub const fn const_pt(value: isize) -> Self {
                Self {
                    inner: crate::props::basic::value::PixelValue::const_pt(value),
                }
            }

            #[inline]
            pub const fn const_percent(value: isize) -> Self {
                Self {
                    inner: crate::props::basic::value::PixelValue::const_percent(value),
                }
            }

            #[inline]
            pub const fn const_in(value: isize) -> Self {
                Self {
                    inner: crate::props::basic::value::PixelValue::const_in(value),
                }
            }

            #[inline]
            pub const fn const_cm(value: isize) -> Self {
                Self {
                    inner: crate::props::basic::value::PixelValue::const_cm(value),
                }
            }

            #[inline]
            pub const fn const_mm(value: isize) -> Self {
                Self {
                    inner: crate::props::basic::value::PixelValue::const_mm(value),
                }
            }

            #[inline]
            pub const fn const_from_metric(
                metric: crate::props::basic::value::SizeMetric,
                value: isize,
            ) -> Self {
                Self {
                    inner: crate::props::basic::value::PixelValue::const_from_metric(metric, value),
                }
            }

            #[inline]
            pub fn px(value: f32) -> Self {
                Self {
                    inner: crate::props::basic::value::PixelValue::px(value),
                }
            }

            #[inline]
            pub fn em(value: f32) -> Self {
                Self {
                    inner: crate::props::basic::value::PixelValue::em(value),
                }
            }

            #[inline]
            pub fn pt(value: f32) -> Self {
                Self {
                    inner: crate::props::basic::value::PixelValue::pt(value),
                }
            }

            #[inline]
            pub fn percent(value: f32) -> Self {
                Self {
                    inner: crate::props::basic::value::PixelValue::percent(value),
                }
            }

            #[inline]
            pub fn from_metric(metric: crate::props::basic::value::SizeMetric, value: f32) -> Self {
                Self {
                    inner: crate::props::basic::value::PixelValue::from_metric(metric, value),
                }
            }

            #[inline]
            pub fn interpolate(&self, other: &Self, t: f32) -> Self {
                $struct {
                    inner: self.inner.interpolate(&other.inner, t),
                }
            }
        }
    };
}

/// Trait to allow `define_dimension_property!` to work.
pub(crate) trait PixelValueTaker {
    fn from_pixel_value(inner: crate::props::basic::value::PixelValue) -> Self;
}
