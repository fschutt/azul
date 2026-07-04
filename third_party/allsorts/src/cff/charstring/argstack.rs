// This file is derived from ttf-parser, licenced under Apache-2.0.
// https://github.com/RazrFalcon/ttf-parser/blob/439aaaebd50eb8aed66302e3c1b51fae047f85b2/src/tables/cff/argstack.rs

use std::fmt::Debug;

use crate::cff::CFFError;

/// Storage for the CFF operand stack with processing CharStrings.
pub struct ArgumentsStack<'a, T>
where
    T: Debug,
{
    pub data: &'a mut [T],
    pub len: usize,
    pub max_len: usize,
}

impl<'a, T> ArgumentsStack<'a, T>
where
    T: Copy + Debug,
{
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn push(&mut self, n: T) -> Result<(), CFFError> {
        if self.len == self.max_len {
            Err(CFFError::ArgumentsStackLimitReached)
        } else {
            self.data[self.len] = n;
            self.len += 1;
            Ok(())
        }
    }

    pub fn at(&self, index: usize) -> T {
        self.data[index]
    }

    pub fn pop(&mut self) -> T {
        debug_assert!(!self.is_empty());
        self.len -= 1;
        self.data[self.len]
    }

    /// pop n values from the stack
    pub fn pop_n(&mut self, n: usize) -> &[T] {
        debug_assert!(n <= self.len);
        self.len -= n;
        &self.data[self.len..]
    }

    pub fn pop_all(&mut self) -> &[T] {
        let len = self.len;
        self.len = 0;
        &self.data[..len]
    }

    pub fn all(&self) -> &[T] {
        &self.data[..self.len]
    }

    pub fn offset<E>(
        &mut self,
        offset: usize,
        mut func: impl FnMut(&ArgumentsStack<'_, T>) -> Result<(), E>,
    ) -> Result<(), E> {
        debug_assert!(offset <= self.len);
        let temporary_stack = ArgumentsStack {
            data: &mut self.data[offset..],
            len: self.len - offset,
            max_len: self.max_len - offset,
        };
        func(&temporary_stack)
    }

    pub fn reverse(&mut self) {
        if self.is_empty() {
            return;
        }

        // Reverse only the actual data and not the whole stack.
        let (first, _) = self.data.split_at_mut(self.len);
        first.reverse();
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    pub(crate) fn clone_into(&self, data: &'a mut [T]) -> Self {
        data[..self.len].clone_from_slice(&self.data[..self.len]);
        ArgumentsStack {
            data,
            len: self.len,
            max_len: self.max_len,
        }
    }
}

impl<T: Debug> Debug for ArgumentsStack<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(&self.data[..self.len]).finish()
    }
}
