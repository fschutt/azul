/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE in this crate for license information.
 *--------------------------------------------------------------------------------------------*/

/// Used to write a variable number of bits to a byte buffer.
#[derive(Debug, Default)]
pub struct BitWriter {
    pub bit_buffer: u64,
    pub bits_in: u32,
}

// use to write varying sized bits
impl BitWriter {
    #[inline(always)]
    pub fn write(&mut self, bits: u32, len: u32, data_buffer: &mut Vec<u8>) {
        debug_assert!(bits <= ((1u32 << len) - 1u32) && len <= 32);
        self.bit_buffer |= u64::from(bits) << self.bits_in;
        self.bits_in += len;

        if self.bits_in > 32 {
            data_buffer.extend_from_slice(&(self.bit_buffer as u32).to_le_bytes());
            self.bit_buffer >>= 32;
            self.bits_in -= 32;
        }
    }

    pub fn pad(&mut self, fillbit: u8, data_buffer: &mut Vec<u8>) {
        let mut offset = 1;
        while (self.bits_in & 7) != 0 {
            self.write(if (fillbit & offset) != 0 { 1 } else { 0 }, 1, data_buffer);
            offset <<= 1;
        }

        self.flush_whole_bytes(data_buffer);
    }

    #[cold]
    pub fn flush_whole_bytes(&mut self, data_buffer: &mut Vec<u8>) {
        while self.bits_in >= 8 {
            data_buffer.push(self.bit_buffer as u8);
            self.bit_buffer >>= 8;
            self.bits_in -= 8;
        }
    }
}

// write a fixed pattern and see if it matches the expected fixed output
#[test]
fn write_simple() {
    let mut b = BitWriter::default();
    let mut data_buffer = Vec::new();

    b.write(1, 4, &mut data_buffer);
    b.write(2, 4, &mut data_buffer);
    b.write(3, 4, &mut data_buffer);
    b.write(4, 4, &mut data_buffer);
    b.write(4, 4, &mut data_buffer);
    b.write(0x56, 8, &mut data_buffer);
    b.write(0x78, 8, &mut data_buffer);
    b.write(0x9f, 8, &mut data_buffer);
    b.write(0xfe, 8, &mut data_buffer);
    b.write(0xe, 4, &mut data_buffer);

    b.flush_whole_bytes(&mut data_buffer);

    assert_eq!(data_buffer[..], [0x21, 0x43, 0x64, 0x85, 0xf7, 0xe9, 0xef]);
}

/// write various bit patterns and see if the result matches the input
#[test]
fn write_roundtrip() {
    use super::bit_reader::{BitReader, ReadBits};

    let mut b = BitWriter::default();
    let mut data_buffer = Vec::new();

    let pattern = [
        (0, 1),
        (1, 1),
        (1, 2),
        (2, 3),
        (3, 4),
        (4, 5),
        (4, 6),
        (0x156, 9),
        (0x78, 8),
        (0x9f, 8),
        (0xfe, 8),
        (0x7fff, 15),
        (0xffff, 16),
        (0xe, 4),
    ];

    for &(bits, len) in pattern.iter() {
        b.write(bits, len, &mut data_buffer);
    }

    b.pad(0, &mut data_buffer);
    b.flush_whole_bytes(&mut data_buffer);

    let mut cursor = std::io::Cursor::new(data_buffer);
    let mut reader = BitReader::new();

    for &(bits, len) in pattern.iter() {
        assert_eq!(reader.get(len, &mut cursor).unwrap(), bits);
    }
}
