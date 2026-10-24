/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE in this crate for license information.
 *--------------------------------------------------------------------------------------------*/

use crate::deflate::huffman_encoding::HuffmanOriginalEncoding;

use super::deflate_constants::{
    DIST_CODE_COUNT, LITLENDIST_CODE_COUNT, NONLEN_CODE_COUNT, quantize_distance, quantize_length,
};

/// In a DEFLATE stream, tokens are either literals (bytes) or references to previous bytes
/// with a distance and length.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum DeflateToken {
    Literal(u8),
    Reference(DeflateTokenReference),
}

impl DeflateToken {
    #[cfg(test)]
    pub fn new_ref(len: u32, dist: u32) -> DeflateToken {
        DeflateToken::Reference(DeflateTokenReference::new(len, dist))
    }
}

/// In the case of a distance and length, the length is the number of bytes to copy from the
/// previous bytes, and the distance is the number of bytes back to start copying from.
///
/// the irregular258 field is used to indicate that the 258 length code was used but in a
/// suboptimal way (the RFC allows for two different ways to encode 258)
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct DeflateTokenReference {
    len: u8,
    dist: u16,
}

impl std::fmt::Debug for DeflateTokenReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DeflateTokenReference {{ len: {}, dist: {} }}",
            self.len(),
            self.dist()
        )
    }
}

impl DeflateTokenReference {
    pub fn new(len: u32, dist: u32) -> DeflateTokenReference {
        DeflateTokenReference {
            len: (len - 3) as u8,
            dist: dist as u16,
        }
    }

    pub fn len(&self) -> u32 {
        (self.len as u32) + 3
    }

    pub fn dist(&self) -> u32 {
        self.dist as u32
    }
}

pub const BT_STORED: u32 = 1;
pub const BT_DYNAMICHUFF: u32 = 0;
pub const BT_STATICHUFF: u32 = 2;

#[derive(Debug, PartialEq)]
pub enum DeflateHuffmanType {
    Static,
    Dynamic {
        huffman_encoding: HuffmanOriginalEncoding,
    },
}

impl Default for DeflateHuffmanType {
    fn default() -> Self {
        DeflateHuffmanType::Static
    }
}

#[derive(PartialEq)]
pub enum DeflateTokenBlockType {
    Huffman {
        tokens: Vec<DeflateToken>,
        huffman_type: DeflateHuffmanType,
    },
    Stored {
        uncompressed: Vec<u8>,
    },
}

/// Debug implementation for DeflateTokenBlockType doesn't print all the tokens
impl std::fmt::Debug for DeflateTokenBlockType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeflateTokenBlockType::Huffman {
                tokens,
                huffman_type: DeflateHuffmanType::Static { .. },
            } => {
                write!(f, "StaticHuffman {{ tokens: len={} }}", tokens.len(),)
            }
            DeflateTokenBlockType::Huffman {
                tokens,
                huffman_type: DeflateHuffmanType::Dynamic { .. },
            } => {
                write!(f, "DynamicHuffman {{ tokens: len={} }}", tokens.len(),)
            }
            DeflateTokenBlockType::Stored { uncompressed } => {
                write!(f, "Stored {{ uncompressed: len={} }}", uncompressed.len(),)
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct DeflateTokenBlock {
    pub block_type: DeflateTokenBlockType,

    pub last: bool,
}

/// Used to track the frequence of tokens in the DEFLATE stream
/// which are later used to build the huffman encoding.
#[derive(Debug)]
pub struct TokenFrequency {
    pub literal_codes: [u16; LITLENDIST_CODE_COUNT],
    pub distance_codes: [u16; DIST_CODE_COUNT],
}

impl Default for TokenFrequency {
    fn default() -> Self {
        let mut t = TokenFrequency {
            literal_codes: [0; LITLENDIST_CODE_COUNT],
            distance_codes: [0; DIST_CODE_COUNT],
        };

        // include the end of block code
        t.literal_codes[256] = 1;

        t
    }
}

impl TokenFrequency {
    pub fn commit_token(&mut self, token: &DeflateToken) {
        match token {
            DeflateToken::Literal(lit) => {
                self.literal_codes[*lit as usize] += 1;
            }
            DeflateToken::Reference(t) => {
                self.literal_codes
                    [NONLEN_CODE_COUNT + usize::from(quantize_length(t.len()).get())] += 1;
                self.distance_codes[usize::from(quantize_distance(t.dist()).get())] += 1;
            }
        }
    }
}
