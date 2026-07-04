#![deny(missing_docs)]

//! Write binary data

use std::marker::PhantomData;

use crate::binary::read::{ReadArray, ReadArrayCow, ReadScope, ReadUnchecked};
use crate::binary::{I16Be, I32Be, I64Be, U16Be, U24Be, U32Be, I8, U8};
use crate::error::WriteError;

/// An in-memory buffer that implements `WriteContext`.
pub struct WriteBuffer {
    data: Vec<u8>,
}

struct WriteSlice<'a> {
    offset: usize,
    data: &'a mut [u8],
}

/// A `WriteContext` implementation that just counts the bytes written.
pub struct WriteCounter {
    count: usize,
}

/// A `WriteContext` implementation that writes nothing.
struct NullWriter;

/// A placeholder for a value that will be filled in later using WriteContext::write_placeholder
pub struct Placeholder<T, HostType>
where
    T: WriteBinaryDep<HostType>,
{
    offset: usize,
    length: usize,
    marker: PhantomData<T>,
    host: PhantomData<HostType>,
}

/// Trait that describes a type that can be written to a `WriteContext` in binary form.
pub trait WriteBinary<HostType = Self> {
    /// The type of the value returned by `write`.
    type Output;

    /// Write the binary representation of Self to `ctxt`.
    fn write<C: WriteContext>(ctxt: &mut C, val: HostType) -> Result<Self::Output, WriteError>;
}

/// Trait that describes a type that can be written to a `WriteContext` in binary form with
/// dependent arguments.
pub trait WriteBinaryDep<HostType = Self> {
    /// The type of the arguments supplied to `write_dep`.
    type Args;
    /// The type of the value returned by `write_dep`.
    type Output;

    /// Write the binary representation of Self to `ctxt`.
    fn write_dep<C: WriteContext>(
        ctxt: &mut C,
        val: HostType,
        args: Self::Args,
    ) -> Result<Self::Output, WriteError>;
}

/// Trait for types that can have binary data written to them.
pub trait WriteContext {
    /// Write a `ReadArray` instance to a `WriteContext`.
    fn write_array<T>(&mut self, array: &ReadArray<'_, T>) -> Result<(), WriteError>
    where
        Self: Sized,
        T: ReadUnchecked + WriteBinary<<T as ReadUnchecked>::HostType>,
    {
        <&ReadArray<'_, _>>::write(self, array)
    }

    /// Write a `Vec` into a `WriteContext`.
    fn write_vec<T, HostType>(&mut self, vec: Vec<HostType>) -> Result<(), WriteError>
    where
        Self: Sized,
        T: WriteBinary<HostType>,
    {
        for val in vec {
            T::write(self, val)?;
        }

        Ok(())
    }

    /// Write a slice of values into a `WriteContext`.
    fn write_iter<T, HostType>(
        &mut self,
        iter: impl Iterator<Item = HostType>,
    ) -> Result<(), WriteError>
    where
        Self: Sized,
        T: WriteBinary<HostType>,
    {
        for val in iter {
            T::write(self, val)?;
        }

        Ok(())
    }

    /// Write a slice of bytes to a `WriteContext`.
    fn write_bytes(&mut self, data: &[u8]) -> Result<(), WriteError>;

    /// Write the specified number of zero bytes to the `WriteContext`.
    fn write_zeros(&mut self, count: usize) -> Result<(), WriteError>;

    /// The total number of bytes written so far.
    fn bytes_written(&self) -> usize;

    /// Return a placeholder to `T` in the context for filling in later.
    fn placeholder<T, HostType>(&mut self) -> Result<Placeholder<T, HostType>, WriteError>
    where
        T: WriteBinary<HostType> + ReadUnchecked,
    {
        let offset = self.bytes_written();
        self.write_zeros(T::SIZE)?;

        Ok(Placeholder {
            offset,
            length: T::SIZE,
            marker: PhantomData,
            host: PhantomData,
        })
    }

    /// Reserve space for `count` bytes in the context for filling in later.
    fn reserve<'a, T, HostType>(
        &mut self,
        count: usize,
    ) -> Result<Placeholder<T, &'a HostType>, WriteError>
    where
        T: WriteBinaryDep<&'a HostType>,
    {
        let offset = self.bytes_written();
        self.write_zeros(count)?;

        Ok(Placeholder {
            offset,
            length: count,
            marker: PhantomData,
            host: PhantomData,
        })
    }

    /// Return a `Vec` of `count` placeholders of type `T`.
    fn placeholder_array<T, HostType>(
        &mut self,
        count: usize,
    ) -> Result<Vec<Placeholder<T, HostType>>, WriteError>
    where
        T: WriteBinary<HostType> + ReadUnchecked,
    {
        (0..count)
            .map(|_| self.placeholder::<T, HostType>())
            .collect()
    }

    /// Consumes the placeholder and writes the supplied value into it
    fn write_placeholder<T, HostType>(
        &mut self,
        placeholder: Placeholder<T, HostType>,
        val: HostType,
    ) -> Result<T::Output, WriteError>
    where
        T: WriteBinary<HostType>;

    /// Consumes the placeholder and writes the supplied value into it.
    /// `WriteBinaryDep` version
    fn write_placeholder_dep<T, HostType>(
        &mut self,
        placeholder: Placeholder<T, HostType>,
        val: HostType,
        args: T::Args,
    ) -> Result<T::Output, WriteError>
    where
        T: WriteBinaryDep<HostType>;
}

/// Write `T` into a `WriteBuffer` and return it
pub fn buffer<HostType, T: WriteBinaryDep<HostType>>(
    writeable: HostType,
    args: T::Args,
) -> Result<(T::Output, WriteBuffer), WriteError> {
    let mut buffer = WriteBuffer::new();
    let output = T::write_dep(&mut buffer, writeable, args)?;
    Ok((output, buffer))
}

impl<T, HostType> WriteBinaryDep<HostType> for T
where
    T: WriteBinary<HostType>,
{
    type Args = ();
    type Output = T::Output;

    fn write_dep<C: WriteContext>(
        ctxt: &mut C,
        val: HostType,
        (): Self::Args,
    ) -> Result<Self::Output, WriteError> {
        T::write(ctxt, val)
    }
}

impl<T> WriteBinary<T> for U8
where
    T: Into<u8>,
{
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, t: T) -> Result<(), WriteError> {
        let val: u8 = t.into();
        ctxt.write_bytes(&[val])
    }
}

impl<T> WriteBinary<T> for I8
where
    T: Into<i8>,
{
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, t: T) -> Result<(), WriteError> {
        let val: i8 = t.into();
        ctxt.write_bytes(&val.to_be_bytes())
    }
}

impl<T> WriteBinary<T> for I16Be
where
    T: Into<i16>,
{
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, t: T) -> Result<(), WriteError> {
        let val: i16 = t.into();
        ctxt.write_bytes(&val.to_be_bytes())
    }
}

impl<T> WriteBinary<T> for U16Be
where
    T: Into<u16>,
{
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, t: T) -> Result<(), WriteError> {
        let val: u16 = t.into();
        ctxt.write_bytes(&val.to_be_bytes())
    }
}

impl<T> WriteBinary<T> for U24Be
where
    T: Into<u32>,
{
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, t: T) -> Result<(), WriteError> {
        let val: u32 = t.into();
        if val > 0xFF_FFFF {
            return Err(WriteError::BadValue);
        }
        ctxt.write_bytes(&val.to_be_bytes()[1..4])
    }
}

impl<T> WriteBinary<T> for I32Be
where
    T: Into<i32>,
{
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, t: T) -> Result<(), WriteError> {
        let val: i32 = t.into();
        ctxt.write_bytes(&val.to_be_bytes())
    }
}

impl<T> WriteBinary<T> for U32Be
where
    T: Into<u32>,
{
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, t: T) -> Result<(), WriteError> {
        let val: u32 = t.into();
        ctxt.write_bytes(&val.to_be_bytes())
    }
}

impl<T> WriteBinary<T> for I64Be
where
    T: Into<i64>,
{
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, t: T) -> Result<(), WriteError> {
        let val: i64 = t.into();
        ctxt.write_bytes(&val.to_be_bytes())
    }
}

impl WriteContext for WriteBuffer {
    fn write_bytes(&mut self, data: &[u8]) -> Result<(), WriteError> {
        self.data.extend(data.iter());
        Ok(())
    }

    fn write_zeros(&mut self, count: usize) -> Result<(), WriteError> {
        let zeros = std::iter::repeat_n(0, count);
        self.data.extend(zeros);
        Ok(())
    }

    fn bytes_written(&self) -> usize {
        self.data.len()
    }

    fn write_placeholder<T, HostType>(
        &mut self,
        placeholder: Placeholder<T, HostType>,
        val: HostType,
    ) -> Result<T::Output, WriteError>
    where
        T: WriteBinary<HostType>,
    {
        let data = &mut self.data[placeholder.offset..];
        let data = &mut data[0..placeholder.length];
        let mut slice = WriteSlice { offset: 0, data };
        T::write(&mut slice, val)
    }

    fn write_placeholder_dep<T, HostType>(
        &mut self,
        placeholder: Placeholder<T, HostType>,
        val: HostType,
        args: T::Args,
    ) -> Result<T::Output, WriteError>
    where
        T: WriteBinaryDep<HostType>,
    {
        let data = &mut self.data[placeholder.offset..];
        let data = &mut data[0..placeholder.length];
        let mut slice = WriteSlice { offset: 0, data };
        T::write_dep(&mut slice, val, args)
    }
}

impl WriteContext for WriteSlice<'_> {
    fn write_bytes(&mut self, data: &[u8]) -> Result<(), WriteError> {
        let data_len = data.len();
        let self_len = self.data.len();

        if data_len <= self_len {
            let subslice = &mut self.data[self.offset..][0..data_len];
            subslice.copy_from_slice(data);
            self.offset += data_len;
            Ok(())
        } else {
            Err(WriteError::PlaceholderMismatch)
        }
    }

    fn write_zeros(&mut self, count: usize) -> Result<(), WriteError> {
        for i in 0..count.min(self.data.len()) {
            self.data[i] = 0;
        }

        Ok(())
    }

    fn bytes_written(&self) -> usize {
        self.data.len()
    }

    fn write_placeholder<T, HostType>(
        &mut self,
        _placeholder: Placeholder<T, HostType>,
        _val: HostType,
    ) -> Result<T::Output, WriteError>
    where
        T: WriteBinary<HostType>,
    {
        unimplemented!()
    }

    fn write_placeholder_dep<T, HostType>(
        &mut self,
        _placeholder: Placeholder<T, HostType>,
        _val: HostType,
        _args: T::Args,
    ) -> Result<T::Output, WriteError>
    where
        T: WriteBinaryDep<HostType>,
    {
        unimplemented!()
    }
}

impl WriteContext for WriteCounter {
    fn write_bytes(&mut self, data: &[u8]) -> Result<(), WriteError> {
        self.count += data.len();
        Ok(())
    }

    fn write_zeros(&mut self, count: usize) -> Result<(), WriteError> {
        self.count += count;
        Ok(())
    }

    fn bytes_written(&self) -> usize {
        self.count
    }

    fn write_placeholder<T, HostType>(
        &mut self,
        _placeholder: Placeholder<T, HostType>,
        val: HostType,
    ) -> Result<T::Output, WriteError>
    where
        T: WriteBinary<HostType>,
    {
        let mut null = NullWriter;
        T::write(&mut null, val)
    }

    fn write_placeholder_dep<T, HostType>(
        &mut self,
        _placeholder: Placeholder<T, HostType>,
        val: HostType,
        args: T::Args,
    ) -> Result<T::Output, WriteError>
    where
        T: WriteBinaryDep<HostType>,
    {
        let mut null = NullWriter;
        T::write_dep(&mut null, val, args)
    }
}

impl WriteContext for NullWriter {
    fn write_bytes(&mut self, _data: &[u8]) -> Result<(), WriteError> {
        Ok(())
    }

    fn write_zeros(&mut self, _count: usize) -> Result<(), WriteError> {
        Ok(())
    }

    fn bytes_written(&self) -> usize {
        0
    }

    fn write_placeholder<T, HostType>(
        &mut self,
        _placeholder: Placeholder<T, HostType>,
        _val: HostType,
    ) -> Result<T::Output, WriteError>
    where
        T: WriteBinary<HostType>,
    {
        unimplemented!()
    }

    fn write_placeholder_dep<T, HostType>(
        &mut self,
        _placeholder: Placeholder<T, HostType>,
        _val: HostType,
        _args: T::Args,
    ) -> Result<T::Output, WriteError>
    where
        T: WriteBinaryDep<HostType>,
    {
        unimplemented!()
    }
}

impl<T> WriteBinary for &ReadArray<'_, T>
where
    T: ReadUnchecked + WriteBinary<<T as ReadUnchecked>::HostType>,
{
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, array: Self) -> Result<(), WriteError> {
        for val in array.into_iter() {
            T::write(ctxt, val)?;
        }

        Ok(())
    }
}

impl<T> WriteBinary<&Self> for ReadArrayCow<'_, T>
where
    T: ReadUnchecked + WriteBinary<<T as ReadUnchecked>::HostType>,
    T::HostType: Copy,
{
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, array: &Self) -> Result<(), WriteError> {
        for val in array.iter() {
            T::write(ctxt, val)?;
        }

        Ok(())
    }
}

impl WriteBinary for ReadScope<'_> {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, scope: Self) -> Result<(), WriteError> {
        ctxt.write_bytes(scope.data())
    }
}

impl WriteBuffer {
    /// Create a new, empty `WriteBuffer`
    pub fn new() -> Self {
        WriteBuffer { data: Vec::new() }
    }

    /// Retrieve a slice of the data held by this buffer
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }

    /// Clear the internal data so that this buffer can be reused
    pub fn clear(&mut self) {
        self.data.clear();
    }

    /// Returns the current size of the data held by this buffer
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Consume `self` and return the inner buffer
    pub fn into_inner(self) -> Vec<u8> {
        self.data
    }
}

impl WriteCounter {
    /// Create a new, empty `WriteCounter`
    pub fn new() -> Self {
        WriteCounter { count: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag;

    struct TestTable {
        tag: u32,
    }

    struct BigStruct {
        tag: u32,
    }

    impl WriteBinary<Self> for TestTable {
        type Output = ();

        fn write<C: WriteContext>(ctxt: &mut C, val: Self) -> Result<(), WriteError> {
            U32Be::write(ctxt, val.tag)
        }
    }

    impl WriteBinary<&Self> for BigStruct {
        type Output = ();

        fn write<C: WriteContext>(ctxt: &mut C, val: &Self) -> Result<(), WriteError> {
            U32Be::write(ctxt, val.tag)
        }
    }

    #[test]
    fn test_basic() {
        let mut ctxt = WriteBuffer::new();
        let table = TestTable { tag: tag::GLYF };
        let big = BigStruct { tag: tag::BLOC };

        TestTable::write(&mut ctxt, table).unwrap();
        BigStruct::write(&mut ctxt, &big).unwrap();

        assert_eq!(ctxt.bytes(), b"glyfbloc")
    }

    #[test]
    fn test_write_u24be() {
        let mut ctxt = WriteBuffer::new();
        U24Be::write(&mut ctxt, 0x10203u32).unwrap();
        assert_eq!(ctxt.bytes(), &[1, 2, 3]);

        // Check out of range value
        match U24Be::write(&mut ctxt, std::u32::MAX) {
            Err(WriteError::BadValue) => {}
            _ => panic!("Expected WriteError::BadValue"),
        }
    }

    #[test]
    fn test_write_placeholder() {
        let mut ctxt = WriteBuffer::new();
        U8::write(&mut ctxt, 1).unwrap();
        let placeholder = ctxt.placeholder::<U16Be, u16>().unwrap();
        U8::write(&mut ctxt, 3).unwrap();
        ctxt.write_placeholder(placeholder, 2).unwrap();
        assert_eq!(ctxt.bytes(), &[1, 0, 2, 3]);
    }

    #[test]
    fn test_write_placeholder_overflow() {
        // Test that trying to write more data than reserved results in an error
        let mut ctxt = WriteBuffer::new();
        let placeholder = ctxt.reserve::<BigStruct, _>(1).unwrap();
        let value = BigStruct { tag: 1234 };
        assert!(ctxt.write_placeholder(placeholder, &value).is_err());
    }
}
