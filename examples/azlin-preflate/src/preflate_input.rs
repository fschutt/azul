/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE in this crate for license information.
 *--------------------------------------------------------------------------------------------*/

use crate::{
    ExitCode,
    preflate_error::{Result, err_exit_code},
};

/// represents the uncompressed data, including a prefix that is may be referenced by
/// the compressed data. The prefix data is only visible via the PreflateInput struct.
pub struct PlainText {
    /// the entire data, including the prefix
    data: Vec<u8>,

    /// how long the prefix is, after this the data starts
    prefix_length: i32,

    /// the current position with regard to the shrinking dictionary
    pos_offset: i32,
}

impl std::fmt::Debug for PlainText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "PlainText {{ prefix_length: {}, pos_offset:{} data: len={} }}",
            self.prefix_length,
            self.pos_offset,
            self.data.len()
        )
    }
}

impl Clone for PlainText {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            prefix_length: self.prefix_length,
            pos_offset: self.pos_offset,
        }
    }
}

impl PlainText {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            prefix_length: 0,
            pos_offset: 0,
        }
    }

    /// returns the dictionary to be used as a prefix for the next compression, which
    /// is a maximum of 32KB in size.
    pub fn shrink_to_dictionary(&mut self) {
        //self.prefix_length = self.data.len() as i32;
        self.pos_offset += self.data.len() as i32 - self.prefix_length;

        let amount_to_keep = self.data.len().min(32768);

        self.data.drain(..self.data.len() - amount_to_keep);
        self.prefix_length = self.data.len() as i32;
    }

    pub fn new_with_data(data: Vec<u8>) -> Self {
        Self {
            data,
            prefix_length: 0,
            pos_offset: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.data.len() - (self.prefix_length as usize)
    }

    /// the total length of the data from the beginning
    pub fn total_length(&self) -> u32 {
        self.pos_offset as u32 + self.len() as u32
    }

    /// the data excluding the prefix
    pub fn text(&self) -> &[u8] {
        &self.data[self.prefix_length as usize..]
    }

    pub fn prefix(&self) -> &[u8] {
        &self.data[0..self.prefix_length as usize]
    }

    pub fn truncate(&mut self, len: usize) {
        self.data.truncate(self.prefix_length as usize + len);
    }

    pub fn push(&mut self, c: u8) {
        self.data.push(c);
    }

    pub fn append(&mut self, data: &[u8]) {
        self.data.extend_from_slice(data);
    }

    pub fn append_iter(&mut self, data: impl Iterator<Item = u8>) {
        self.data.extend(data);
    }

    /// writes a reference to the buffer, which copies the text from a previous location
    /// to the current location. In most cases this is non-overlapping, but there are some
    /// cases where there is overlap between the source and destination.
    #[inline(always)]
    pub fn append_reference(&mut self, dist: u32, len: u32) -> Result<()> {
        if dist as usize > self.data.len() {
            return err_exit_code(ExitCode::InvalidDeflate, "Invalid distance in reference");
        }

        if dist == 1 {
            // special case for distance 1, just repeat the last byte n times
            let byte = self.data[self.data.len() - 1];
            self.data.resize(self.data.len() + len as usize, byte);
        } else if dist >= len {
            // no overlap
            self.data.extend_from_within(
                self.data.len() - dist as usize..self.data.len() - dist as usize + len as usize,
            );
        } else {
            // general case, rarely called, copy one character at a time
            let start = self.data.len() - dist as usize;

            self.data.reserve(len as usize);

            for i in 0..len {
                let byte = self.data[start + i as usize];
                self.data.push(byte);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct PreflateInput<'a> {
    data: &'a PlainText,
    pos: i32,
}

impl<'a> PreflateInput<'a> {
    pub fn new(v: &'a PlainText) -> Self {
        PreflateInput {
            data: v,
            pos: v.prefix_length,
        }
    }

    #[inline(always)]
    pub fn pos(&self) -> u32 {
        (self.pos + self.data.pos_offset - self.data.prefix_length) as u32
    }

    /// total length of the data all the way back to the beginning
    #[inline(always)]
    pub fn total_length(&self) -> u32 {
        self.data.total_length()
    }

    #[inline(always)]
    pub fn cur_chars(&self, offset: i32) -> &[u8] {
        &self.data.data[(self.pos + offset) as usize..]
    }

    #[inline(always)]
    pub fn cur_char(&self, offset: i32) -> u8 {
        self.data.data[(self.pos + offset) as usize]
    }

    #[inline(always)]
    pub fn advance(&mut self, l: u32) {
        self.pos += l as i32;
        debug_assert!((self.pos) <= self.data.data.len() as i32);
    }

    #[inline(always)]
    pub fn remaining(&self) -> u32 {
        (self.data.data.len() as i32 - self.pos) as u32
    }
}

#[test]
fn test_length_behavior() {
    let mut data = PlainText::new_with_data(vec![0; 10000]);

    let mut input = PreflateInput::new(&data);
    assert_eq!(input.total_length(), 10000);
    assert_eq!(input.pos(), 0);
    assert_eq!(input.remaining(), 10000);

    input.advance(1000);
    assert_eq!(input.total_length(), 10000);
    assert_eq!(input.pos(), 1000);
    assert_eq!(input.remaining(), 9000);

    input.advance(9000);
    assert_eq!(input.total_length(), 10000);
    assert_eq!(input.pos(), 10000);
    assert_eq!(input.remaining(), 0);

    data.shrink_to_dictionary();
    data.append(&[1; 10000]);

    let mut input = PreflateInput::new(&data);
    assert_eq!(input.total_length(), 20000);
    assert_eq!(input.pos(), 10000);
    assert_eq!(input.remaining(), 10000);
    assert_eq!(input.cur_char(0), 1);
    assert_eq!(input.cur_char(-1), 0);
    assert_eq!(input.cur_char(-1000), 0);

    input.advance(1000);
    assert_eq!(input.total_length(), 20000);
    assert_eq!(input.pos(), 11000);
    assert_eq!(input.remaining(), 9000);

    assert_eq!(input.cur_char(-1), 1);
    assert_eq!(input.cur_char(-1000), 1);
}
