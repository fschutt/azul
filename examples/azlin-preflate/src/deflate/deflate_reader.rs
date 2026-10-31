/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE in this crate for license information.
 *--------------------------------------------------------------------------------------------*/

use byteorder::{LittleEndian, ReadBytesExt};

use crate::{
    deflate::deflate_token::{DeflateHuffmanType, DeflateToken, DeflateTokenReference},
    preflate_error::{AddContext, ExitCode, Result, err_exit_code},
    preflate_input::PlainText,
};

use std::io::Cursor;

use super::{
    bit_reader::ReadBits,
    deflate_constants,
    deflate_token::{DeflateTokenBlock, DeflateTokenBlockType},
};

use super::{
    bit_reader::BitReader,
    huffman_encoding::{HuffmanOriginalEncoding, HuffmanReader},
};

struct Checkpoint {
    bit_reader: BitReader,
    plain_text: usize,
    position: u64,
}

#[derive(Debug)]
pub struct DeflateParser {
    state: DeflateParserState,
    bit_reader: BitReader,
    plain_text: PlainText,

    /// how big the plain text can get before we return an error
    /// this is to prevent a zipbomb from blowing up our memory usage
    plain_text_limit: usize,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum DeflateParserState {
    StartBlock,
    Done,
}

impl DeflateParser {
    pub fn new(plain_text_limit: usize) -> Self {
        Self {
            state: DeflateParserState::StartBlock,
            bit_reader: BitReader::new(),
            plain_text: PlainText::new(),
            plain_text_limit,
        }
    }

    pub fn is_done(&self) -> bool {
        self.state == DeflateParserState::Done
    }

    pub fn shrink_to_dictionary(&mut self) {
        self.plain_text.shrink_to_dictionary();
    }

    pub fn plain_text(&self) -> &PlainText {
        &self.plain_text
    }

    pub fn detach_plain_text(self) -> PlainText {
        self.plain_text
    }

    fn checkpoint(&mut self, reader: &mut Cursor<&[u8]>) -> Checkpoint {
        self.bit_reader.undo_read_ahead(reader);

        Checkpoint {
            plain_text: self.plain_text().len(),
            bit_reader: self.bit_reader.clone(),
            position: reader.position(),
        }
    }

    pub fn parse(&mut self, compressed_data: &[u8]) -> Result<DeflateContents> {
        let mut blocks = Vec::new();

        let mut cursor = &mut Cursor::new(compressed_data);

        // our first checkpoint is right at the beginning so that if we get to the
        // end of the stream before seeing a whole block, we just revert back to
        // the beginning to try again with more data.
        let mut checkpoint = self.checkpoint(&mut cursor);

        let bits_left = self.bit_reader.bits_left();
        if bits_left > 0 {
            // get the bitreader to read the first byte since we are
            // carrying over state from the previous block but don't have
            // the new bits to read from this block.
            self.bit_reader.read_padding_bits();
            self.bit_reader.get(8 - bits_left, cursor)?;
        }

        match self.read_blocks_internal(&mut cursor, &mut blocks, &mut checkpoint) {
            Err(e) => {
                // reset back to known good checkpoint before returning the error.
                // this allows callers to try again once they have more data
                self.bit_reader = checkpoint.bit_reader;
                self.plain_text.truncate(checkpoint.plain_text);

                // if nothing was successfully read or we didn't
                // get the out-of-data error, then just exit
                // if we get a plain-text too big error, also checkpoint back
                // to how far we got and the next part of the plaintext will be put in the next chunk
                if checkpoint.position == 0
                    || (e.exit_code() != ExitCode::ShortRead
                        && e.exit_code() != ExitCode::PlainTextLimit)
                {
                    return Err(e);
                }

                // if we had bits left to read, then we didn't compress the entire
                // block, and save the bits for later
                let compressed_size = if (self.bit_reader.bits_left() & 0x7) > 0 {
                    (checkpoint.position - 1) as usize
                } else {
                    checkpoint.position as usize
                };

                return Ok(DeflateContents {
                    compressed_size,
                    blocks,
                });
            }
            Ok(()) => {}
        }

        self.bit_reader.undo_read_ahead(&mut cursor);

        Ok(DeflateContents {
            compressed_size: cursor.position() as usize,
            blocks,
        })
    }

    fn read_blocks_internal(
        &mut self,
        reader: &mut Cursor<&[u8]>,
        blocks: &mut Vec<DeflateTokenBlock>,
        checkpoint: &mut Checkpoint,
    ) -> Result<()> {
        loop {
            let (last, mode) = match self.state {
                DeflateParserState::StartBlock => (
                    self.bit_reader.get(1, reader)? != 0,
                    self.bit_reader.get(2, reader)?,
                ),
                DeflateParserState::Done => {
                    return Ok(());
                }
            };

            match mode {
                0 => {
                    let padding_bits = self.bit_reader.read_padding_bits();
                    if padding_bits != 0 {
                        return err_exit_code(
                            ExitCode::NonZeroPadding,
                            "nonzero padding found before uncompressed block",
                        );
                    }

                    // consume any buffered read bits since we are just going to read bytes now
                    self.bit_reader.undo_read_ahead(reader);

                    assert!(self.bit_reader.bits_left() == 0);

                    let len = reader.read_u16::<LittleEndian>()?;
                    let ilen = reader.read_u16::<LittleEndian>()?;
                    if (len ^ ilen) != 0xffff {
                        return err_exit_code(ExitCode::InvalidDeflate, "Block length mismatch");
                    }

                    let mut uncompressed = Vec::with_capacity(len as usize);

                    for _i in 0..len {
                        let b = reader.read_u8()?;
                        uncompressed.push(b);
                        self.plain_text.push(b);
                    }

                    blocks.push(DeflateTokenBlock {
                        block_type: DeflateTokenBlockType::Stored { uncompressed },
                        last,
                    });
                }
                1 => {
                    // some compressors don't flush blocks at all if they are using static huffman encoding
                    // since there's no need to keep track of statistics etc.

                    let mut tokens = Vec::new();
                    let decoder = HuffmanReader::create_fixed()?;
                    decode_tokens(
                        &decoder,
                        &mut tokens,
                        &mut self.plain_text,
                        &mut self.bit_reader,
                        reader,
                        self.plain_text_limit,
                    )
                    .context()?;

                    blocks.push(DeflateTokenBlock {
                        block_type: DeflateTokenBlockType::Huffman {
                            tokens,
                            huffman_type: DeflateHuffmanType::Static,
                        },
                        last,
                    });
                }

                2 => {
                    let huffman_encoding =
                        HuffmanOriginalEncoding::read(&mut self.bit_reader, reader)?;

                    let decoder = HuffmanReader::create_from_original_encoding(&huffman_encoding)?;

                    let mut tokens = Vec::new();

                    // don't checkpoint dynamic huffman blocks since they don't get very big
                    // and it adds unnecessary complexity
                    decode_tokens(
                        &decoder,
                        &mut tokens,
                        &mut self.plain_text,
                        &mut self.bit_reader,
                        reader,
                        self.plain_text_limit,
                    )
                    .context()?;

                    blocks.push(DeflateTokenBlock {
                        block_type: DeflateTokenBlockType::Huffman {
                            tokens,
                            huffman_type: DeflateHuffmanType::Dynamic { huffman_encoding },
                        },
                        last,
                    });
                }

                _ => return err_exit_code(ExitCode::InvalidDeflate, "Invalid block type"),
            }

            if last {
                if self.bit_reader.read_padding_bits() != 0 {
                    return err_exit_code(
                        ExitCode::NonZeroPadding,
                        "nonzero padding found at end of stream",
                    );
                }

                self.state = DeflateParserState::Done;
            }

            *checkpoint = self.checkpoint(reader);
        }
    }
}

fn decode_tokens(
    decoder: &HuffmanReader,
    tokens: &mut Vec<DeflateToken>,
    plain_text: &mut PlainText,
    bit_reader: &mut BitReader,
    reader: &mut Cursor<&[u8]>,
    plain_text_limit: usize,
) -> Result<()> {
    let mut earliest_reference = i32::MAX;
    let mut cur_pos = 0;

    loop {
        bit_reader.read_ahead(reader);

        if plain_text.len() > plain_text_limit {
            return err_exit_code(ExitCode::PlainTextLimit, "Plain text limit exceeded");
        }

        let lit_len: u32 = decoder.fetch_next_literal_code(bit_reader, reader)?.into();
        if lit_len < 256 {
            plain_text.push(lit_len as u8);
            tokens.push(DeflateToken::Literal(lit_len as u8));
            cur_pos += 1;
        } else if lit_len == 256 {
            return Ok(());
        } else {
            let lcode: u32 = lit_len - deflate_constants::NONLEN_CODE_COUNT as u32;
            if lcode >= deflate_constants::LEN_CODE_COUNT as u32 {
                return err_exit_code(ExitCode::InvalidDeflate, "Invalid length code");
            }
            let len: u32 = deflate_constants::MIN_MATCH
                + deflate_constants::LENGTH_BASE_TABLE[lcode as usize] as u32
                + bit_reader.get(
                    deflate_constants::LENGTH_EXTRA_TABLE[lcode as usize].into(),
                    reader,
                )?;

            // length of 258 can be encoded two ways: 284 with 5 one bits (non-standard) or as 285 with 0 extra bits (standard)
            if len == 258 && lcode != deflate_constants::LEN_CODE_COUNT as u32 - 1 {
                return err_exit_code(ExitCode::InvalidDeflate, "Non-standard 258 length code");
            }

            let dcode = decoder.fetch_next_distance_char(bit_reader, reader)? as u32;
            if dcode >= deflate_constants::DIST_CODE_COUNT as u32 {
                return err_exit_code(ExitCode::InvalidDeflate, "Invalid distance code");
            }

            let dist = 1
                + deflate_constants::DIST_BASE_TABLE[dcode as usize] as u32
                + bit_reader.get(
                    deflate_constants::DIST_EXTRA_TABLE[dcode as usize].into(),
                    reader,
                )?;

            plain_text.append_reference(dist, len)?;
            tokens.push(DeflateToken::Reference(DeflateTokenReference::new(
                len, dist,
            )));

            earliest_reference = std::cmp::min(earliest_reference, cur_pos - (dist as i32));
            cur_pos += len as i32;
        }
    }
}

/// represents the complete deflate stream
pub struct DeflateContents {
    pub compressed_size: usize,
    pub blocks: Vec<DeflateTokenBlock>,
}

impl std::fmt::Debug for DeflateContents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeflateContents")
            .field("compressed_size", &self.compressed_size)
            .field("blocks", &self.blocks.len())
            .finish()
    }
}

/// parses an entire deflate stream and asserts if it isn't complete
#[cfg(test)]
pub fn parse_deflate_whole(compressed_data: &[u8]) -> Result<(DeflateContents, PlainText)> {
    let mut parse = DeflateParser::new(usize::MAX);
    let deflate_content = parse.parse(compressed_data)?;

    match parse.state {
        DeflateParserState::Done => (),
        _ => panic!("Deflate stream not complete"),
    }

    Ok((deflate_content, parse.plain_text))
}

/// A stream read in pieces gives the same blocks as one read, and writing them gives back its
/// bytes (the streams made with flate2 in place of upstream's sample files).
#[test]
fn reading_a_stream_in_pieces_gives_the_blocks_and_bytes_of_one_read() {
    let text = crate::utils::text_for_tests(300_000);
    for level in [0, 1, 6, 9] {
        test_partial_read_for_buffer(&crate::utils::deflate_for_tests(&text, level));
    }
}

#[cfg(test)]
pub fn test_partial_read_for_buffer(d: &[u8]) {
    use crate::utils::{assert_block_eq, assert_eq_array};

    let (complete, _plain_text_complete) = parse_deflate_whole(&d).unwrap();

    let mut start_offset = 0;
    let mut end_offset = 1;
    let mut allblocks = Vec::new();

    let mut deflate_parser = DeflateParser::new(1 * 1024 * 1024);

    while !deflate_parser.is_done() {
        match deflate_parser.parse(&d[start_offset..end_offset]) {
            Ok(mut dc) => {
                assert!(dc.compressed_size != 0);
                println!(
                    "segment blocks={} plaintext={} comp={}",
                    dc.blocks.len(),
                    deflate_parser.plain_text().len(),
                    dc.compressed_size
                );
                allblocks.append(&mut dc.blocks);

                // advance by the amount of data we just read
                start_offset += dc.compressed_size;

                // end offset plus a bit
                end_offset = (start_offset + 50977).min(d.len());

                deflate_parser.shrink_to_dictionary();
            }
            Err(e) => {
                assert!(
                    end_offset != d.len(),
                    "shouldnt have reached the end with an error {:?}",
                    e
                );

                assert!(e.exit_code() == ExitCode::ShortRead);
                // get some more this buffer was readable due to lack of data
                end_offset = (end_offset + 50997).min(d.len());
            }
        }
    }

    assert_eq!(allblocks.len(), complete.blocks.len());
    for i in 0..allblocks.len() {
        assert_block_eq(&allblocks[i], &complete.blocks[i]);
    }

    let reconstruct = crate::deflate::deflate_writer::write_deflate_blocks(&allblocks);

    // data we reconstruct should be the same, minus if there was extra data passed
    // start_offset that we dedidn't decode
    assert_eq_array(&reconstruct, &d[..start_offset]);
}
