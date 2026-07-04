#![allow(missing_docs)]

//! Parse binary data
//!
//! The is module provides the basis for all font parsing in Allsorts. The parsing approach
//! is inspired by the paper,
//! [The next 700 data description languages](https://collaborate.princeton.edu/en/publications/the-next-700-data-description-languages) by Kathleen Fisher, Yitzhak Mandelbaum, David P. Walker.

use crate::binary::{I16Be, I32Be, I64Be, U16Be, U24Be, U32Be, U64Be, I8, U8};
use crate::error::ParseError;
use crate::layout::{LayoutCache, LayoutTableType};
use crate::{size, SafeFrom};
use std::borrow::Cow;
use std::cmp;
use std::cmp::Ordering;
// [azul web-lift] BTreeMap not HashMap for ReadCache: this cache (coverages/classdefs etc.) starts
// EMPTY (cap-0) and the FIRST insert during GSUB/GPOS shaping hits the lifted hashbrown EMPTY-INSERT
// mis-lift (reserve_rehash-from-0) → the remill-lifted web backend HANGS in shape_text. BTreeMap has
// no ctrl-group/empty-static → immune. Key is `usize` (Ord); entry API is identical.
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

#[derive(Debug, Copy, Clone)]
pub struct ReadEof {}

pub struct ReadBuf<'a> {
    data: Cow<'a, [u8]>,
}

#[derive(Copy, Clone, PartialEq)]
pub struct ReadScope<'a> {
    base: usize,
    data: &'a [u8],
}

pub struct ReadScopeOwned {
    base: usize,
    data: Box<[u8]>,
}

impl ReadScopeOwned {
    pub fn new(scope: ReadScope<'_>) -> ReadScopeOwned {
        ReadScopeOwned {
            base: scope.base,
            data: Box::from(scope.data),
        }
    }

    pub fn scope(&self) -> ReadScope<'_> {
        ReadScope {
            base: self.base,
            data: &self.data,
        }
    }
}

#[derive(Clone)]
pub struct ReadCtxt<'a> {
    scope: ReadScope<'a>,
    offset: usize,
}

pub struct ReadCache<T> {
    map: BTreeMap<usize, Arc<T>>,
}

pub trait ReadBinary {
    type HostType<'a>: Sized; // default = Self

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError>;
}

pub trait ReadBinaryDep {
    type Args<'a>: Copy;
    type HostType<'a>: Sized; // default = Self

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        args: Self::Args<'a>,
    ) -> Result<Self::HostType<'a>, ParseError>;
}

pub trait ReadFixedSizeDep: ReadBinaryDep {
    /// The number of bytes consumed by `ReadBinaryDep::read`.
    fn size(args: Self::Args<'_>) -> usize;
}

/// Read will always succeed if sufficient bytes are available.
pub trait ReadUnchecked {
    type HostType: Sized; // default = Self

    /// The number of bytes consumed by `read_unchecked`.
    const SIZE: usize;

    /// Must read exactly `SIZE` bytes.
    /// Unsafe as it avoids prohibitively expensive per-byte bounds checking.
    unsafe fn read_unchecked(ctxt: &mut ReadCtxt<'_>) -> Self::HostType;
}

pub trait ReadFrom {
    type ReadType: ReadUnchecked;
    fn read_from(value: <Self::ReadType as ReadUnchecked>::HostType) -> Self;
}

impl<T> ReadUnchecked for T
where
    T: ReadFrom,
{
    type HostType = T;

    const SIZE: usize = T::ReadType::SIZE;

    unsafe fn read_unchecked(ctxt: &mut ReadCtxt<'_>) -> Self::HostType {
        let t = T::ReadType::read_unchecked(ctxt);
        T::read_from(t)
    }
}

impl<T> ReadBinary for T
where
    T: ReadUnchecked,
{
    type HostType<'a> = T::HostType;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        ctxt.check_avail(T::SIZE)?;
        Ok(unsafe { T::read_unchecked(ctxt) })
        // Safe because we have `SIZE` bytes available.
    }
}

impl<T> ReadBinaryDep for T
where
    T: ReadBinary,
{
    type Args<'a> = ();
    type HostType<'a> = T::HostType<'a>;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        (): Self::Args<'_>,
    ) -> Result<Self::HostType<'a>, ParseError> {
        T::read(ctxt)
    }
}

impl<T> ReadFixedSizeDep for T
where
    T: ReadUnchecked,
{
    fn size((): ()) -> usize {
        T::SIZE
    }
}

pub trait CheckIndex {
    fn check_index(&self, index: usize) -> Result<(), ParseError>;
}

/// Wrapper type for Debug impl of byte slices
pub(crate) struct DebugData<'a>(pub(crate) &'a [u8]);

pub struct ReadArray<'a, T: ReadFixedSizeDep> {
    scope: ReadScope<'a>,
    length: usize,
    stride: usize,
    args: T::Args<'a>,
}

impl<T: ReadFixedSizeDep> Clone for ReadArray<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ReadFixedSizeDep> Copy for ReadArray<'_, T> {}

pub struct ReadArrayIter<'a, T: ReadUnchecked> {
    scope: ReadScope<'a>,
    index: usize,
    end: usize,
    stride: usize,
    phantom: PhantomData<T>,
}

pub struct ReadArrayDepIter<'a, 'b, T: ReadFixedSizeDep> {
    array: &'b ReadArray<'a, T>,
    index: usize,
}

#[derive(Clone)]
pub enum ReadArrayCow<'a, T>
where
    T: ReadUnchecked,
{
    Owned(Vec<T::HostType>),
    Borrowed(ReadArray<'a, T>),
}

pub struct ReadArrayCowIter<'a, 'b, T: ReadUnchecked> {
    array: &'b ReadArrayCow<'a, T>,
    index: usize,
}

impl<'a, T: ReadUnchecked> ReadArrayCow<'a, T> {
    pub fn len(&self) -> usize {
        match self {
            ReadArrayCow::Borrowed(array) => array.len(),
            ReadArrayCow::Owned(vec) => vec.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            ReadArrayCow::Borrowed(array) => array.is_empty(),
            ReadArrayCow::Owned(vec) => vec.is_empty(),
        }
    }

    pub fn read_item(&self, index: usize) -> Result<T::HostType, ParseError>
    where
        T::HostType: Copy,
    {
        match self {
            ReadArrayCow::Borrowed(array) => array.read_item(index),
            ReadArrayCow::Owned(vec) => vec.get(index).copied().ok_or(ParseError::BadIndex),
        }
    }

    pub fn get_item(&self, index: usize) -> Option<<T as ReadUnchecked>::HostType>
    where
        T: ReadUnchecked,
        <T as ReadUnchecked>::HostType: Copy,
    {
        match self {
            ReadArrayCow::Borrowed(array) => array.get_item(index),
            ReadArrayCow::Owned(vec) => vec.get(index).copied(),
        }
    }

    // subarray and iter_res are not yet implemented

    pub fn iter<'b>(&'b self) -> ReadArrayCowIter<'a, 'b, T> {
        ReadArrayCowIter {
            array: self,
            index: 0,
        }
    }
}

impl<T: ReadUnchecked> CheckIndex for ReadArrayCow<'_, T> {
    fn check_index(&self, index: usize) -> Result<(), ParseError> {
        if index < self.len() {
            Ok(())
        } else {
            Err(ParseError::BadIndex)
        }
    }
}

impl<'a> ReadScope<'a> {
    pub fn new(data: &'a [u8]) -> ReadScope<'a> {
        let base = 0;
        ReadScope { base, data }
    }

    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    pub fn offset(&self, offset: usize) -> ReadScope<'a> {
        let base = self.base + offset;
        let data = self.data.get(offset..).unwrap_or(&[]);
        ReadScope { base, data }
    }

    pub fn offset_length(&self, offset: usize, length: usize) -> Result<ReadScope<'a>, ParseError> {
        if offset < self.data.len() || length == 0 {
            let data = self.data.get(offset..).unwrap_or(&[]);
            if length <= data.len() {
                let base = self.base + offset;
                let data = &data[0..length];
                Ok(ReadScope { base, data })
            } else {
                Err(ParseError::BadEof)
            }
        } else {
            Err(ParseError::BadOffset)
        }
    }

    pub fn ctxt(&self) -> ReadCtxt<'a> {
        ReadCtxt::new(*self)
    }

    pub fn read<T: ReadBinaryDep<Args<'a> = ()>>(&self) -> Result<T::HostType<'a>, ParseError> {
        self.ctxt().read::<T>()
    }

    pub fn read_dep<T: ReadBinaryDep>(
        &self,
        args: T::Args<'a>,
    ) -> Result<T::HostType<'a>, ParseError> {
        self.ctxt().read_dep::<T>(args)
    }

    pub fn read_cache<T>(
        &self,
        cache: &mut ReadCache<T::HostType<'a>>,
    ) -> Result<Arc<T::HostType<'a>>, ParseError>
    where
        T: 'static + ReadBinaryDep<Args<'a> = ()>,
    {
        match cache.map.entry(self.base) {
            Entry::Vacant(entry) => {
                let t = Arc::new(self.read::<T>()?);
                Ok(Arc::clone(entry.insert(t)))
            }
            Entry::Occupied(entry) => Ok(Arc::clone(entry.get())),
        }
    }

    pub fn read_cache_state<T, Table>(
        &self,
        cache: &mut ReadCache<T::HostType<'a>>,
        state: LayoutCache<Table>,
    ) -> Result<Arc<T::HostType<'a>>, ParseError>
    where
        T: 'static + ReadBinaryDep<Args<'a> = LayoutCache<Table>>,
        Table: LayoutTableType,
    {
        match cache.map.entry(self.base) {
            Entry::Vacant(entry) => {
                let t = Arc::new(self.read_dep::<T>(state)?);
                Ok(Arc::clone(entry.insert(t)))
            }
            Entry::Occupied(entry) => Ok(Arc::clone(entry.get())),
        }
    }

    pub(crate) fn read_optional_array<T>(
        &self,
        offset: u32,
        num: u16,
    ) -> Result<Option<ReadArray<'a, T>>, ParseError>
    where
        T: ReadUnchecked,
    {
        (num > 0 && offset != 0)
            .then(|| {
                self.offset(usize::safe_from(offset))
                    .ctxt()
                    .read_array(usize::from(num))
            })
            .transpose()
    }
}

impl fmt::Debug for ReadScope<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ReadScope { base, data } = self;
        f.debug_struct("ReadScope")
            .field("base", base)
            .field("data", &DebugData(data))
            .finish()
    }
}

impl<T> ReadCache<T> {
    pub fn new() -> Self {
        let map = BTreeMap::new();
        ReadCache { map }
    }
}

impl<'a> ReadCtxt<'a> {
    /// ReadCtxt is constructed by calling `ReadScope::ctxt`.
    fn new(scope: ReadScope<'a>) -> ReadCtxt<'a> {
        ReadCtxt { scope, offset: 0 }
    }

    pub fn check(&self, cond: bool) -> Result<(), ParseError> {
        match cond {
            true => Ok(()),
            false => Err(ParseError::BadValue),
        }
    }

    /// Check a condition, returning `ParseError::BadIndex` if `false`.
    ///
    /// ```
    /// use allsorts::binary::read::ReadScope;
    /// use allsorts::error::ParseError;
    ///
    /// # fn main() -> Result<(), ParseError> {
    /// let ctxt = ReadScope::new(b"some data").ctxt();
    ///
    /// // Demonstration values
    /// let count = 3;
    /// let index = 1;
    /// ctxt.check_index(index < count)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn check_index(&self, cond: bool) -> Result<(), ParseError> {
        match cond {
            true => Ok(()),
            false => Err(ParseError::BadIndex),
        }
    }

    /// Check a condition, returning `ParseError::BadVersion` if `false`.
    ///
    /// Intended for use in checking versions read from data. Example:
    ///
    /// ```
    /// use allsorts::binary::read::ReadScope;
    /// use allsorts::error::ParseError;
    ///
    /// let scope = ReadScope::new(&[0, 2]);
    /// let mut ctxt = scope.ctxt();
    /// let major_version = ctxt.read_u16be().expect("unable to read version");
    ///
    /// assert!(ctxt.check_version(major_version == 2).is_ok());
    /// assert_eq!(
    ///     ctxt.check_version(major_version == 1),
    ///     Err(ParseError::BadVersion)
    /// );
    /// ```
    pub fn check_version(&self, cond: bool) -> Result<(), ParseError> {
        match cond {
            true => Ok(()),
            false => Err(ParseError::BadVersion),
        }
    }

    pub fn scope(&self) -> ReadScope<'a> {
        self.scope.offset(self.offset)
    }

    pub fn read<T: ReadBinaryDep<Args<'a> = ()>>(&mut self) -> Result<T::HostType<'a>, ParseError> {
        T::read_dep(self, ())
    }

    pub fn read_dep<T: ReadBinaryDep>(
        &mut self,
        args: T::Args<'a>,
    ) -> Result<T::HostType<'a>, ParseError> {
        T::read_dep(self, args)
    }

    pub fn bytes_available(&self) -> bool {
        self.offset < self.scope.data.len()
    }

    fn check_avail(&self, length: usize) -> Result<(), ReadEof> {
        match self.offset.checked_add(length) {
            Some(endpos) if endpos <= self.scope.data.len() => Ok(()),
            _ => Err(ReadEof {}),
        }
    }

    unsafe fn read_unchecked_u8(&mut self) -> u8 {
        let byte = *self.scope.data.get_unchecked(self.offset);
        self.offset += 1;
        byte
    }

    unsafe fn read_unchecked_i8(&mut self) -> i8 {
        self.read_unchecked_u8() as i8
    }

    unsafe fn read_unchecked_u16be(&mut self) -> u16 {
        let hi = u16::from(*self.scope.data.get_unchecked(self.offset));
        let lo = u16::from(*self.scope.data.get_unchecked(self.offset + 1));
        self.offset += 2;
        (hi << 8) | lo
    }

    unsafe fn read_unchecked_i16be(&mut self) -> i16 {
        self.read_unchecked_u16be() as i16
    }

    unsafe fn read_unchecked_u24be(&mut self) -> u32 {
        let b0 = u32::from(*self.scope.data.get_unchecked(self.offset));
        let b1 = u32::from(*self.scope.data.get_unchecked(self.offset + 1));
        let b2 = u32::from(*self.scope.data.get_unchecked(self.offset + 2));
        self.offset += 3;
        (b0 << 16) | (b1 << 8) | b2
    }

    unsafe fn read_unchecked_u32be(&mut self) -> u32 {
        let b0 = u32::from(*self.scope.data.get_unchecked(self.offset));
        let b1 = u32::from(*self.scope.data.get_unchecked(self.offset + 1));
        let b2 = u32::from(*self.scope.data.get_unchecked(self.offset + 2));
        let b3 = u32::from(*self.scope.data.get_unchecked(self.offset + 3));
        self.offset += 4;
        (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
    }

    unsafe fn read_unchecked_i32be(&mut self) -> i32 {
        self.read_unchecked_u32be() as i32
    }

    unsafe fn read_unchecked_u64be(&mut self) -> u64 {
        let hi = u64::from(self.read_unchecked_u32be());
        let lo = u64::from(self.read_unchecked_u32be());
        (hi << 32) | lo
    }

    unsafe fn read_unchecked_i64be(&mut self) -> i64 {
        self.read_unchecked_u64be() as i64
    }

    pub fn read_u8(&mut self) -> Result<u8, ReadEof> {
        self.check_avail(1)?;
        Ok(unsafe { self.read_unchecked_u8() })
        // Safe because we have 1 byte available.
    }

    pub fn read_i8(&mut self) -> Result<i8, ReadEof> {
        self.check_avail(1)?;
        Ok(unsafe { self.read_unchecked_i8() })
        // Safe because we have 1 byte available.
    }

    pub fn read_u16be(&mut self) -> Result<u16, ReadEof> {
        self.check_avail(2)?;
        Ok(unsafe { self.read_unchecked_u16be() })
        // Safe because we have 2 bytes available.
    }

    pub fn read_i16be(&mut self) -> Result<i16, ReadEof> {
        self.check_avail(2)?;
        Ok(unsafe { self.read_unchecked_i16be() })
        // Safe because we have 2 bytes available.
    }

    pub fn read_u32be(&mut self) -> Result<u32, ReadEof> {
        self.check_avail(4)?;
        Ok(unsafe { self.read_unchecked_u32be() })
        // Safe because we have 4 bytes available.
    }

    pub fn read_i32be(&mut self) -> Result<i32, ReadEof> {
        self.check_avail(4)?;
        Ok(unsafe { self.read_unchecked_i32be() })
        // Safe because we have 4 bytes available.
    }

    pub fn read_u64be(&mut self) -> Result<u64, ReadEof> {
        self.check_avail(8)?;
        Ok(unsafe { self.read_unchecked_u64be() })
        // Safe because we have 8 bytes available.
    }

    pub fn read_i64be(&mut self) -> Result<i64, ReadEof> {
        self.check_avail(8)?;
        Ok(unsafe { self.read_unchecked_i64be() })
        // Safe because we have 8 bytes available.
    }

    pub fn read_array<T: ReadUnchecked>(
        &mut self,
        length: usize,
    ) -> Result<ReadArray<'a, T>, ParseError> {
        let scope = self.read_scope(length * T::SIZE)?;
        let args = ();
        Ok(ReadArray {
            scope,
            length,
            stride: T::SIZE,
            args,
        })
    }

    pub fn read_array_stride<T: ReadUnchecked>(
        &mut self,
        length: usize,
        stride: usize,
    ) -> Result<ReadArray<'a, T>, ParseError> {
        if T::SIZE > stride {
            return Err(ParseError::BadValue);
        }
        let scope = self.read_scope(length * stride)?;
        let args = ();
        Ok(ReadArray {
            scope,
            length,
            stride,
            args,
        })
    }

    pub fn read_array_upto_hack<T: ReadUnchecked>(
        &mut self,
        length: usize,
    ) -> Result<ReadArray<'a, T>, ParseError> {
        let start_pos = self.offset;
        let buf_size = self.scope.data.len();
        let avail_bytes = cmp::max(0, buf_size - start_pos);
        let max_length = avail_bytes / T::SIZE;
        let length = cmp::min(length, max_length);
        self.read_array(length)
    }

    /// Read up to and including the supplied nibble.
    pub fn read_until_nibble(&mut self, nibble: u8) -> Result<&'a [u8], ReadEof> {
        let end = self.scope.data[self.offset..]
            .iter()
            .position(|&b| (b >> 4) == nibble || (b & 0xF) == nibble)
            .ok_or(ReadEof {})?;
        self.read_slice(end + 1)
    }

    pub fn read_array_dep<T: ReadFixedSizeDep>(
        &mut self,
        length: usize,
        args: T::Args<'a>,
    ) -> Result<ReadArray<'a, T>, ParseError> {
        let stride = T::size(args);
        let scope = self.read_scope(length * stride)?;
        Ok(ReadArray {
            scope,
            length,
            stride,
            args,
        })
    }

    pub fn read_scope(&mut self, length: usize) -> Result<ReadScope<'a>, ReadEof> {
        if let Ok(scope) = self.scope.offset_length(self.offset, length) {
            self.offset += length;
            Ok(scope)
        } else {
            Err(ReadEof {})
        }
    }

    pub fn read_slice(&mut self, length: usize) -> Result<&'a [u8], ReadEof> {
        let scope = self.read_scope(length)?;
        Ok(scope.data)
    }
}

impl<'a> ReadBuf<'a> {
    pub fn scope(&'a self) -> ReadScope<'a> {
        ReadScope::new(&self.data)
    }

    pub fn into_data(self) -> Cow<'a, [u8]> {
        self.data
    }
}

impl<'a> From<&'a [u8]> for ReadBuf<'a> {
    fn from(data: &'a [u8]) -> ReadBuf<'a> {
        ReadBuf {
            data: Cow::Borrowed(data),
        }
    }
}

impl<'a> From<Vec<u8>> for ReadBuf<'a> {
    fn from(data: Vec<u8>) -> ReadBuf<'a> {
        ReadBuf {
            data: Cow::Owned(data),
        }
    }
}

impl<'a, T: ReadFixedSizeDep> ReadArray<'a, T> {
    pub fn len(&self) -> usize {
        self.length
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    pub fn args(&self) -> &T::Args<'a> {
        &self.args
    }

    pub fn read_item(&self, index: usize) -> Result<T::HostType<'a>, ParseError> {
        if index < self.length {
            let size = T::size(self.args);
            let offset = index * size;
            let scope = self.scope.offset_length(offset, size).unwrap();
            let mut ctxt = scope.ctxt();
            T::read_dep(&mut ctxt, self.args)
        } else {
            Err(ParseError::BadIndex)
        }
    }

    pub fn get_item(&self, index: usize) -> Option<<T as ReadUnchecked>::HostType>
    where
        T: ReadUnchecked,
    {
        if index < self.length {
            let offset = index * self.stride;
            let scope = self.scope.offset_length(offset, self.stride).unwrap();
            let mut ctxt = scope.ctxt();
            Some(unsafe { T::read_unchecked(&mut ctxt) }) // Safe because we have `SIZE` bytes available.
        } else {
            None
        }
    }

    pub fn last(&self) -> Option<<T as ReadUnchecked>::HostType>
    where
        T: ReadUnchecked,
    {
        let index = self.length.checked_sub(1)?;
        self.get_item(index)
    }

    pub fn to_vec(&self) -> Vec<<T as ReadUnchecked>::HostType>
    where
        T: ReadUnchecked,
    {
        let mut vec = Vec::with_capacity(self.length);
        for t in self.iter() {
            vec.push(t);
        }
        vec
    }

    pub fn read_to_vec(&self) -> Result<Vec<T::HostType<'a>>, ParseError> {
        let mut vec = Vec::with_capacity(self.length);
        for res in self.iter_res() {
            let t = res?;
            vec.push(t);
        }
        Ok(vec)
    }

    pub fn iter(&self) -> ReadArrayIter<'a, T>
    where
        T: ReadUnchecked,
    {
        ReadArrayIter {
            scope: self.scope,
            index: 0,
            end: self.length,
            stride: self.stride,
            phantom: PhantomData,
        }
    }

    pub fn iter_res<'b>(&'b self) -> ReadArrayDepIter<'a, 'b, T> {
        ReadArrayDepIter {
            array: self,
            index: 0,
        }
    }

    // This is derived from the function on slice in the standard library
    pub fn binary_search_by<F>(&self, mut f: F) -> Result<usize, usize>
    where
        F: FnMut(<T as ReadUnchecked>::HostType) -> Ordering,
        T: ReadUnchecked,
    {
        // INVARIANTS:
        // - 0 <= left <= left + size = right <= self.len()
        // - f returns Less for everything in self[..left]
        // - f returns Greater for everything in self[right..]
        let mut size = self.len();
        let mut left = 0;
        let mut right = size;
        while left < right {
            let mid = left + size / 2;

            let offset = mid * self.stride;
            // NOTE(unwrap): the while condition means `size` is strictly positive, so
            // `size/2 < size`. Thus `left + size/2 < left + size`, which
            // coupled with the `left + size <= self.len()` invariant means
            // we have `left + size/2 < self.len()`, and this is in-bounds.
            let scope = self.scope.offset_length(offset, self.stride).unwrap();
            let mut ctxt = scope.ctxt();
            // SAFTEY: Safe because we have checked that we have `SIZE` bytes available in the
            // offset_length call.
            let cmp = f(unsafe { T::read_unchecked(&mut ctxt) });
            // let cmp = f(unsafe { self.get_unchecked(mid) });

            // The reason why we use if/else control flow rather than match
            // is because match reorders comparison operations, which is perf sensitive.
            // This is x86 asm for u8: https://rust.godbolt.org/z/8Y8Pra.
            if cmp == Ordering::Less {
                left = mid + 1;
            } else if cmp == Ordering::Greater {
                right = mid;
            } else {
                return Ok(mid);
            }

            size = right - left;
        }

        Err(left)
    }
}

impl<T: ReadFixedSizeDep> CheckIndex for ReadArray<'_, T> {
    fn check_index(&self, index: usize) -> Result<(), ParseError> {
        if index < self.len() {
            Ok(())
        } else {
            Err(ParseError::BadIndex)
        }
    }
}

impl<T> CheckIndex for Vec<T> {
    fn check_index(&self, index: usize) -> Result<(), ParseError> {
        if index < self.len() {
            Ok(())
        } else {
            Err(ParseError::BadIndex)
        }
    }
}

impl<'a, T: ReadUnchecked> IntoIterator for &ReadArray<'a, T> {
    type Item = T::HostType;
    type IntoIter = ReadArrayIter<'a, T>;
    fn into_iter(self) -> ReadArrayIter<'a, T> {
        self.iter()
    }
}

impl<T: ReadUnchecked> Iterator for ReadArrayIter<'_, T> {
    type Item = T::HostType;

    fn next(&mut self) -> Option<T::HostType> {
        // From the docs:
        // It is important to note that both back and forth work on the same range,
        // and do not cross: iteration is over when they meet in the middle.
        if self.index >= self.end {
            return None;
        }
        let mut ctxt = self.scope.offset(self.index * self.stride).ctxt();
        ctxt.check_avail(self.stride).ok()?;
        // SAFETY: Ok because we have (at least) `stride` bytes available and T::SIZE is <= stride.
        self.index += 1;
        Some(unsafe { T::read_unchecked(&mut ctxt) })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.scope.data().len() / self.stride;
        (remaining, Some(remaining))
    }
}

impl<T: ReadUnchecked> DoubleEndedIterator for ReadArrayIter<'_, T> {
    fn next_back(&mut self) -> Option<T::HostType> {
        let index = self.end.checked_sub(1)?;
        // From the docs:
        // It is important to note that both back and forth work on the same range,
        // and do not cross: iteration is over when they meet in the middle.
        if index < self.index {
            return None;
        }
        let mut ctxt = self.scope.offset(index * self.stride).ctxt();
        ctxt.check_avail(self.stride).ok()?;
        // SAFETY: Ok because we have (at least) `stride` bytes available and T::SIZE is <= stride.
        self.end -= 1;
        Some(unsafe { T::read_unchecked(&mut ctxt) })
    }
}

impl<T: ReadUnchecked> ExactSizeIterator for ReadArrayIter<'_, T> {}

impl<'a, 'b, T: ReadUnchecked> IntoIterator for &'b ReadArrayCow<'a, T>
where
    T::HostType: Copy,
{
    type Item = T::HostType;
    type IntoIter = ReadArrayCowIter<'a, 'b, T>;

    fn into_iter(self) -> ReadArrayCowIter<'a, 'b, T> {
        self.iter()
    }
}

impl<T: ReadUnchecked> Iterator for ReadArrayCowIter<'_, '_, T>
where
    T::HostType: Copy,
{
    type Item = T::HostType;

    fn next(&mut self) -> Option<T::HostType> {
        let item = self.array.get_item(self.index)?;
        self.index += 1;
        Some(item)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.index < self.array.len() {
            let length = self.array.len() - self.index;
            (length, Some(length))
        } else {
            (0, Some(0))
        }
    }
}

impl<'a, T: ReadFixedSizeDep> Iterator for ReadArrayDepIter<'a, '_, T> {
    type Item = Result<T::HostType<'a>, ParseError>;

    fn next(&mut self) -> Option<Result<T::HostType<'a>, ParseError>> {
        if self.index < self.array.len() {
            let result = self.array.read_item(self.index);
            self.index += 1;
            Some(result)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.index < self.array.len() {
            let length = self.array.len() - self.index;
            (length, Some(length))
        } else {
            (0, Some(0))
        }
    }
}

impl<'a, T: ReadUnchecked> ReadArray<'a, T> {
    pub fn empty() -> ReadArray<'a, T> {
        ReadArray {
            scope: ReadScope::new(&[]),
            length: 0,
            stride: T::SIZE,
            args: (),
        }
    }
}

impl ReadUnchecked for U8 {
    type HostType = u8;

    const SIZE: usize = size::U8;

    unsafe fn read_unchecked(ctxt: &mut ReadCtxt<'_>) -> u8 {
        ctxt.read_unchecked_u8()
    }
}

impl ReadUnchecked for I8 {
    type HostType = i8;

    const SIZE: usize = size::I8;

    unsafe fn read_unchecked(ctxt: &mut ReadCtxt<'_>) -> i8 {
        ctxt.read_unchecked_i8()
    }
}

impl ReadUnchecked for U16Be {
    type HostType = u16;

    const SIZE: usize = size::U16;

    unsafe fn read_unchecked(ctxt: &mut ReadCtxt<'_>) -> u16 {
        ctxt.read_unchecked_u16be()
    }
}

impl ReadUnchecked for I16Be {
    type HostType = i16;

    const SIZE: usize = size::I16;

    unsafe fn read_unchecked(ctxt: &mut ReadCtxt<'_>) -> i16 {
        ctxt.read_unchecked_i16be()
    }
}

impl ReadUnchecked for U24Be {
    type HostType = u32;

    const SIZE: usize = size::U24;

    unsafe fn read_unchecked(ctxt: &mut ReadCtxt<'_>) -> u32 {
        ctxt.read_unchecked_u24be()
    }
}

impl ReadUnchecked for U32Be {
    type HostType = u32;

    const SIZE: usize = size::U32;

    unsafe fn read_unchecked(ctxt: &mut ReadCtxt<'_>) -> u32 {
        ctxt.read_unchecked_u32be()
    }
}

impl ReadUnchecked for I32Be {
    type HostType = i32;

    const SIZE: usize = size::I32;

    unsafe fn read_unchecked(ctxt: &mut ReadCtxt<'_>) -> i32 {
        ctxt.read_unchecked_i32be()
    }
}

impl ReadUnchecked for U64Be {
    type HostType = u64;

    const SIZE: usize = size::U64;

    unsafe fn read_unchecked(ctxt: &mut ReadCtxt<'_>) -> u64 {
        ctxt.read_unchecked_u64be()
    }
}

impl ReadUnchecked for I64Be {
    type HostType = i64;

    const SIZE: usize = size::I64;

    unsafe fn read_unchecked(ctxt: &mut ReadCtxt<'_>) -> i64 {
        ctxt.read_unchecked_i64be()
    }
}

impl<T1, T2> ReadUnchecked for (T1, T2)
where
    T1: ReadUnchecked,
    T2: ReadUnchecked,
{
    type HostType = (T1::HostType, T2::HostType);

    const SIZE: usize = T1::SIZE + T2::SIZE;

    unsafe fn read_unchecked(ctxt: &mut ReadCtxt<'_>) -> Self::HostType {
        let t1 = T1::read_unchecked(ctxt);
        let t2 = T2::read_unchecked(ctxt);
        (t1, t2)
    }
}

impl<T1, T2, T3> ReadUnchecked for (T1, T2, T3)
where
    T1: ReadUnchecked,
    T2: ReadUnchecked,
    T3: ReadUnchecked,
{
    type HostType = (T1::HostType, T2::HostType, T3::HostType);

    const SIZE: usize = T1::SIZE + T2::SIZE + T3::SIZE;

    unsafe fn read_unchecked(ctxt: &mut ReadCtxt<'_>) -> Self::HostType {
        let t1 = T1::read_unchecked(ctxt);
        let t2 = T2::read_unchecked(ctxt);
        let t3 = T3::read_unchecked(ctxt);
        (t1, t2, t3)
    }
}

impl<T1, T2, T3, T4> ReadUnchecked for (T1, T2, T3, T4)
where
    T1: ReadUnchecked,
    T2: ReadUnchecked,
    T3: ReadUnchecked,
    T4: ReadUnchecked,
{
    type HostType = (T1::HostType, T2::HostType, T3::HostType, T4::HostType);

    const SIZE: usize = T1::SIZE + T2::SIZE + T3::SIZE + T4::SIZE;

    unsafe fn read_unchecked(ctxt: &mut ReadCtxt<'_>) -> Self::HostType {
        let t1 = T1::read_unchecked(ctxt);
        let t2 = T2::read_unchecked(ctxt);
        let t3 = T3::read_unchecked(ctxt);
        let t4 = T4::read_unchecked(ctxt);
        (t1, t2, t3, t4)
    }
}

impl<T> fmt::Debug for ReadArrayCow<'_, T>
where
    T: ReadUnchecked,
    <T as ReadUnchecked>::HostType: Copy + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<'a, T> fmt::Debug for ReadArray<'a, T>
where
    T: ReadFixedSizeDep,
    T::HostType<'a>: Copy + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let mut list = f.debug_list();
        for item in self.iter_res() {
            list.entry(&item.map_err(|_| fmt::Error)?);
        }
        list.finish()
    }
}

impl fmt::Debug for DebugData<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[<{} bytes>]", self.0.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binary::write::{WriteBuffer, WriteContext};

    #[test]
    fn test_read_u24be() {
        let scope = ReadScope::new(&[1, 2, 3]);
        assert_eq!(scope.read::<U24Be>().unwrap(), 0x10203);
    }

    // Tests that offset_length does not panic when length is 0 but offset is out-of-bounds
    #[test]
    fn test_offset_length_oob() {
        let scope = ReadScope::new(&[1, 2, 3]);
        assert!(scope.offset_length(99, 0).is_ok());
    }

    #[test]
    fn double_ended_read_array_iter() {
        let numbers = [1i32, 2, 3, 4, 5, 6];
        let mut w = WriteBuffer::new();
        w.write_iter::<I32Be, _>(numbers.iter().copied()).unwrap();
        let data = w.into_inner();
        let array = ReadScope::new(&data).ctxt().read_array::<I32Be>(6).unwrap();

        let mut iter = array.iter();

        assert_eq!(Some(1), iter.next());
        assert_eq!(Some(6), iter.next_back());
        assert_eq!(Some(5), iter.next_back());
        assert_eq!(Some(2), iter.next());
        assert_eq!(Some(3), iter.next());
        assert_eq!(Some(4), iter.next());
        assert_eq!(None, iter.next());
        assert_eq!(None, iter.next_back());
    }
}
