/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE in this crate for license information.
 *--------------------------------------------------------------------------------------------*/

use crate::preflate_error::Result;

use super::deflate_token::{DeflateHuffmanType, DeflateTokenBlockType};
use super::{
    deflate_constants::{
        DIST_BASE_TABLE, DIST_EXTRA_TABLE, LENGTH_BASE_TABLE, LENGTH_EXTRA_TABLE, MIN_MATCH,
        NONLEN_CODE_COUNT, quantize_distance, quantize_length,
    },
    deflate_token::{DeflateToken, DeflateTokenBlock},
};

use super::bit_writer::BitWriter;
use super::huffman_encoding::HuffmanWriter;

/// Takes a tokenized block and writes it to the original compressed output.
pub struct DeflateWriter {
    /// bit writer to write partial bits to output
    bitwriter: BitWriter,

    /// compressed output
    output: Vec<u8>,
}

impl std::fmt::Debug for DeflateWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DeflateWriter {{ bitwriter: {:?}, output: len={} }}",
            self.bitwriter,
            self.output.len()
        )
    }
}

impl DeflateWriter {
    pub fn new() -> Self {
        Self {
            output: Vec::new(),
            bitwriter: BitWriter::default(),
        }
    }

    /// The whole bytes written so far.
    pub fn output_len(&self) -> usize {
        self.output.len()
    }

    pub fn detach_output(&mut self) -> Vec<u8> {
        let mut o = Vec::new();
        o.append(&mut self.output);
        o
    }

    pub fn encode_block(&mut self, block: &DeflateTokenBlock) -> Result<()> {
        match &block.block_type {
            DeflateTokenBlockType::Stored { uncompressed, .. } => {
                self.bitwriter.write(block.last as u32, 1, &mut self.output);
                self.bitwriter.write(0, 2, &mut self.output);
                self.bitwriter.pad(0, &mut self.output);
                self.bitwriter.flush_whole_bytes(&mut self.output);

                self.output
                    .extend_from_slice(&(uncompressed.len() as u16).to_le_bytes());
                self.output
                    .extend_from_slice(&(!uncompressed.len() as u16).to_le_bytes());

                self.output.extend_from_slice(&uncompressed);
            }
            DeflateTokenBlockType::Huffman {
                tokens,
                huffman_type,
            } => {
                match huffman_type {
                    DeflateHuffmanType::Static => {
                        self.bitwriter.write(block.last as u32, 1, &mut self.output);
                        self.bitwriter.write(1, 2, &mut self.output);
                        let huffman_writer = HuffmanWriter::start_fixed_huffman_table();
                        self.encode_huffman(tokens, &huffman_writer);
                    }
                    DeflateHuffmanType::Dynamic {
                        huffman_encoding, ..
                    } => {
                        self.bitwriter.write(block.last as u32, 1, &mut self.output);
                        self.bitwriter.write(2, 2, &mut self.output);

                        let huffman_writer = HuffmanWriter::start_dynamic_huffman_table(
                            &mut self.bitwriter,
                            &huffman_encoding,
                            &mut self.output,
                        )?;

                        self.encode_huffman(tokens, &huffman_writer);
                    }
                }

                if block.last {
                    self.bitwriter.pad(0, &mut self.output);
                }
            }
        }

        Ok(())
    }

    pub fn flush(&mut self) {
        self.bitwriter.flush_whole_bytes(&mut self.output);
    }

    fn encode_huffman(&mut self, tokens: &Vec<DeflateToken>, huffman_writer: &HuffmanWriter) {
        for token in tokens {
            match token {
                DeflateToken::Literal(lit) => {
                    huffman_writer.write_literal(
                        &mut self.bitwriter,
                        &mut self.output,
                        u16::from(*lit),
                    );
                }
                DeflateToken::Reference(reference) => {
                    let lencode = quantize_length(reference.len());
                    huffman_writer.write_literal(
                        &mut self.bitwriter,
                        &mut self.output,
                        NONLEN_CODE_COUNT as u16 + u16::from(lencode.get()),
                    );

                    let lenextra: u8 = LENGTH_EXTRA_TABLE[usize::from(lencode.get())];
                    if lenextra > 0 {
                        self.bitwriter.write(
                            reference.len()
                                - MIN_MATCH
                                - LENGTH_BASE_TABLE[usize::from(lencode.get())] as u32,
                            lenextra.into(),
                            &mut self.output,
                        );
                    }

                    let distcode = quantize_distance(reference.dist());
                    huffman_writer.write_distance(&mut self.bitwriter, &mut self.output, distcode);

                    let distextra = DIST_EXTRA_TABLE[usize::from(distcode.get())];
                    if distextra > 0 {
                        self.bitwriter.write(
                            reference.dist()
                                - 1
                                - DIST_BASE_TABLE[usize::from(distcode.get())] as u32,
                            distextra.into(),
                            &mut self.output,
                        );
                    }
                }
            }
        }

        huffman_writer.write_literal(&mut self.bitwriter, &mut self.output, 256);
    }
}

/// Create a set of blocks and read them back to see if they are identical
#[test]
fn roundtrip_deflate_writer() {
    use super::deflate_reader::parse_deflate_whole;

    let mut w = DeflateWriter::new();

    let blocks = [
        DeflateTokenBlock {
            block_type: DeflateTokenBlockType::Huffman {
                tokens: vec![
                    DeflateToken::Literal(0),
                    DeflateToken::Literal(2),
                    DeflateToken::Literal(3),
                ],
                huffman_type: DeflateHuffmanType::Static,
            },
            last: false,
        },
        DeflateTokenBlock {
            block_type: DeflateTokenBlockType::Huffman {
                tokens: vec![
                    DeflateToken::Literal(0),
                    DeflateToken::Literal(2),
                    DeflateToken::Literal(3),
                ],
                huffman_type: DeflateHuffmanType::Static,
            },
            last: false,
        },
        DeflateTokenBlock {
            block_type: DeflateTokenBlockType::Stored {
                uncompressed: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            },
            last: false,
        },
        DeflateTokenBlock {
            block_type: DeflateTokenBlockType::Huffman {
                tokens: vec![
                    DeflateToken::Literal(0),
                    DeflateToken::Literal(1),
                    DeflateToken::new_ref(100, 1),
                    DeflateToken::new_ref(258, 1),
                    DeflateToken::Literal(3),
                ],
                huffman_type: DeflateHuffmanType::Static,
            },
            last: true,
        },
    ];

    for i in 0..blocks.len() {
        w.encode_block(&blocks[i]).unwrap();
    }
    w.flush();

    let output = w.detach_output();

    let (r, _) = parse_deflate_whole(&output).unwrap();

    assert_eq!(blocks.len(), r.blocks.len());
    for i in 0..blocks.len() {
        assert_eq!(blocks[i], r.blocks[i], "block {}", i);
    }
}

#[cfg(test)]
pub fn write_deflate_blocks(blocks: &[DeflateTokenBlock]) -> Vec<u8> {
    let mut w = DeflateWriter::new();
    for i in 0..blocks.len() {
        w.encode_block(&blocks[i]).unwrap();
    }
    w.flush();

    let output = w.detach_output();
    output
}
