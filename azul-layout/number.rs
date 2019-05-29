// MIT License
//
// Copyright (c) 2018 Visly Inc.
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use std::ops;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Number {
    Defined(f32),
    Undefined,
}

pub trait ToNumber {
    fn to_number(self) -> Number;
}

pub trait OrElse<T> {
    fn or_else(self, other: T) -> T;
}

impl Default for Number {
    fn default() -> Number {
        Number::Undefined
    }
}

impl OrElse<f32> for Number {
    fn or_else(self, other: f32) -> f32 {
        match self {
            Number::Defined(val) => val,
            Number::Undefined => other,
        }
    }
}

impl OrElse<Number> for Number {
    fn or_else(self, other: Number) -> Number {
        match self {
            Number::Defined(_) => self,
            Number::Undefined => other,
        }
    }
}

impl Number {
    #[inline]
    pub fn is_defined(self) -> bool {
        match self {
            Number::Defined(_) => true,
            Number::Undefined => false,
        }
    }

    #[inline]
    pub fn is_undefined(self) -> bool {
        match self {
            Number::Defined(_) => false,
            Number::Undefined => true,
        }
    }

    #[inline]
    pub fn to_option(&self) -> Option<f32> {
        match self {
            Number::Defined(f) => Some(*f),
            Number::Undefined => None,
        }
    }

    #[inline]
    pub fn unwrap_or_zero(&self) -> f32 {
        match self {
            Number::Defined(d) => *d,
            Number::Undefined => 0.0,
        }
    }
}

pub trait MinMax<In, Out> {
    fn maybe_min(self, rhs: In) -> Out;
    fn maybe_max(self, rhs: In) -> Out;
}

impl MinMax<Number, Number> for Number {
    #[inline]
    fn maybe_min(self, rhs: Number) -> Number {
        match self {
            Number::Defined(val) => match rhs {
                Number::Defined(other) => Number::Defined(val.min(other)),
                Number::Undefined => self,
            },
            Number::Undefined => Number::Undefined,
        }
    }

    #[inline]
    fn maybe_max(self, rhs: Number) -> Number {
        match self {
            Number::Defined(val) => match rhs {
                Number::Defined(other) => Number::Defined(val.max(other)),
                Number::Undefined => self,
            },
            Number::Undefined => Number::Undefined,
        }
    }
}

impl MinMax<f32, Number> for Number {
    #[inline]
    fn maybe_min(self, rhs: f32) -> Number {
        match self {
            Number::Defined(val) => Number::Defined(val.min(rhs)),
            Number::Undefined => Number::Undefined,
        }
    }

    #[inline]
    fn maybe_max(self, rhs: f32) -> Number {
        match self {
            Number::Defined(val) => Number::Defined(val.max(rhs)),
            Number::Undefined => Number::Undefined,
        }
    }
}

impl MinMax<Number, f32> for f32 {
    #[inline]
    fn maybe_min(self, rhs: Number) -> f32 {
        match rhs {
            Number::Defined(val) => self.min(val),
            Number::Undefined => self,
        }
    }

    #[inline]
    fn maybe_max(self, rhs: Number) -> f32 {
        match rhs {
            Number::Defined(val) => self.max(val),
            Number::Undefined => self,
        }
    }
}

impl ToNumber for f32 {
    #[inline]
    fn to_number(self) -> Number {
        Number::Defined(self)
    }
}

impl ops::Add<f32> for Number {
    type Output = Number;

    #[inline]
    fn add(self, rhs: f32) -> Number {
        match self {
            Number::Defined(val) => Number::Defined(val + rhs),
            Number::Undefined => Number::Undefined,
        }
    }
}

impl ops::Add<Number> for Number {
    type Output = Number;

    #[inline]
    fn add(self, rhs: Number) -> Number {
        match self {
            Number::Defined(val) => match rhs {
                Number::Defined(other) => Number::Defined(val + other),
                Number::Undefined => self,
            },
            Number::Undefined => Number::Undefined,
        }
    }
}

impl ops::AddAssign<Number> for Number {
    #[inline]
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl ops::Sub<f32> for Number {
    type Output = Number;

    #[inline]
    fn sub(self, rhs: f32) -> Number {
        match self {
            Number::Defined(val) => Number::Defined(val - rhs),
            Number::Undefined => Number::Undefined,
        }
    }
}

impl ops::Sub<Number> for Number {
    type Output = Number;

    #[inline]
    fn sub(self, rhs: Number) -> Number {
        match self {
            Number::Defined(val) => match rhs {
                Number::Defined(other) => Number::Defined(val - other),
                Number::Undefined => self,
            },
            Number::Undefined => Number::Undefined,
        }
    }
}

impl ops::Mul<f32> for Number {
    type Output = Number;

    #[inline]
    fn mul(self, rhs: f32) -> Number {
        match self {
            Number::Defined(val) => Number::Defined(val * rhs),
            Number::Undefined => Number::Undefined,
        }
    }
}

impl ops::Mul<Number> for Number {
    type Output = Number;

    #[inline]
    fn mul(self, rhs: Number) -> Number {
        match self {
            Number::Defined(val) => match rhs {
                Number::Defined(other) => Number::Defined(val * other),
                Number::Undefined => self,
            },
            Number::Undefined => Number::Undefined,
        }
    }
}

impl ops::Div<f32> for Number {
    type Output = Number;

    #[inline]
    fn div(self, rhs: f32) -> Number {
        match self {
            Number::Defined(val) => Number::Defined(val / rhs),
            Number::Undefined => Number::Undefined,
        }
    }
}

impl ops::Div<Number> for Number {
    type Output = Number;

    #[inline]
    fn div(self, rhs: Number) -> Number {
        match self {
            Number::Defined(val) => match rhs {
                Number::Defined(other) => Number::Defined(val / other),
                Number::Undefined => self,
            },
            Number::Undefined => Number::Undefined,
        }
    }
}