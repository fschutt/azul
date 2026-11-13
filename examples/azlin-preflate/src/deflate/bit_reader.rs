/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE in this crate for license information.
 *--------------------------------------------------------------------------------------------*/

use std::io::{BufRead, Error, Result};

use byteorder::ReadBytesExt;

pub trait ReadBits {
    fn get(&mut self, cbit: u32, reader: &mut impl BufRead) -> Result<u32>;
    fn peek_byte(&self) -> u8;
    fn bits_left(&self) -> u32;
    fn consume(&mut self, cbit: u32);
}

/// BitReader reads a variable number of bits from a byte stream.
#[derive(Debug, Clone)]
pub struct BitReader {
    bits: u64,
    bits_left: u32,
    read_ahead: u32,
}

impl BitReader {
    pub fn new() -> Self {
        BitReader {
            bits: 0,
            bits_left: 0,
            read_ahead: 0,
        }
    }

    /// reads the bits until the next byte boundary
    pub fn read_padding_bits(&mut self) -> u8 {
        let cbit = self.bits_left & 7;

        let wret = self.bits & ((1 << cbit) - 1);

        self.bits >>= cbit;
        self.bits_left -= cbit;

        wret as u8
    }

    /// Fill the buffer with the next bytes from the BufRead without consuming them.
    pub fn read_ahead(&mut self, reader: &mut impl BufRead) {
        if let Ok(buffer) = reader.fill_buf() {
            let buffer = &buffer[self.read_ahead as usize..];

            if buffer.len() > 8 {
                let mut i = 0;
                while self.bits_left <= 56 {
                    let b = buffer[i] as u64;

                    self.bits |= b << self.bits_left;
                    self.bits_left += 8;
                    i += 1;
                }

                self.read_ahead += i as u32;
            }
        }
    }

    /// Undo the opportunistic fill by consuming bytes that were actually read,
    /// and removing the extra bits that were read-ahead
    ///
    /// This is important when we want to position in the reader to reflect
    /// the actual bytes read (for example if we want to look at the position
    /// or read bytes directly).
    pub fn undo_read_ahead(&mut self, reader: &mut impl BufRead) {
        while self.bits_left >= 8 && self.read_ahead > 0 {
            self.bits_left -= 8;
            self.read_ahead -= 1;
        }

        self.bits = self.bits & ((1 << self.bits_left) - 1);

        if self.read_ahead > 0 {
            reader.consume(self.read_ahead as usize);
            self.read_ahead = 0;
        }
    }

    /// Read cbit bits from the bit buffer, reading
    /// more bytes from the input stream if there are not enough bits.
    #[cold]
    fn get_fill_register(
        &mut self,
        cbit: u32,
        reader: &mut impl BufRead,
    ) -> std::result::Result<u32, Error> {
        reader.consume(self.read_ahead as usize);
        self.read_ahead = 0;

        while self.bits_left < cbit {
            let b = reader.read_u8()? as u64;

            self.bits |= b << self.bits_left;
            self.bits_left += 8;
        }

        let wret = self.bits & ((1 << cbit) - 1);

        self.bits >>= cbit;
        self.bits_left -= cbit;

        Ok(wret as u32)
    }
}

impl ReadBits for BitReader {
    /// number of bits that are available to read without reading more bytes
    fn bits_left(&self) -> u32 {
        self.bits_left
    }

    /// Consume cbit bits from the input stream (must be less than bits_left)
    fn consume(&mut self, cbit: u32) {
        debug_assert!(cbit <= self.bits_left);

        self.bits >>= cbit;
        self.bits_left -= cbit;
    }

    /// Peek at the next 8 bits in the input buffer. Must be 8 bits in the buffer.
    fn peek_byte(&self) -> u8 {
        debug_assert!(self.bits_left >= 8);

        self.bits as u8
    }

    /// Read cbit bits from the input stream return
    /// Only supports read of 1 to 32 bits.
    #[inline]
    fn get(&mut self, cbit: u32, reader: &mut impl BufRead) -> Result<u32> {
        let mask = (1u64 << cbit) - 1;

        if cbit < self.bits_left {
            let wret = self.bits & mask;

            self.bits >>= cbit;
            self.bits_left -= cbit;

            return Ok(wret as u32);
        }

        // there weren't enough bits, so go to cold path
        self.get_fill_register(cbit, reader)
    }
}
