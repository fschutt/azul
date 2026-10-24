/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE in this crate for license information.
 *--------------------------------------------------------------------------------------------*/

use std::io::BufRead;

use crate::preflate_error::{ExitCode, Result, err_exit_code};

use crate::deflate::{
    bit_reader::ReadBits, bit_writer::BitWriter, deflate_constants::TREE_CODE_ORDER_TABLE,
};

use super::bit_reader::BitReader;
use super::deflate_constants::{DistCode, LITLEN_CODE_COUNT, MAX_DIST_CODE};

/// Calculates Huffman code array given an array of Huffman Code Lengths using the RFC 1951 algorithm
pub fn calc_huffman_codes(code_lengths: &[u8]) -> Result<Vec<u16>> {
    let mut result: Vec<u16> = vec![0; code_lengths.len()];

    // The following algorithm generates the codes as integers, intended to be read
    // from least- to most-significant bit.

    // 1)  Count the number of codes for each code length.  Let
    // bl_count[N] be the number of codes of length N, N >= 1.

    // azlin-preflate: the lengths may come from damaged corrections, so a length over 15 and
    // an oversubscribed code are errors (counted in u32: they cannot overflow).
    let mut maxbits = 0;
    let mut bl_count: [u32; 16] = [0; 16];
    for cbit in code_lengths {
        if *cbit > 15 {
            return err_exit_code(ExitCode::InvalidDeflate, "a Huffman code length over 15");
        }
        bl_count[*cbit as usize] += 1;
        if *cbit > maxbits {
            maxbits = *cbit;
        }
    }

    //	2)  Find the numerical value of the smallest code for each code length:

    let mut code: u32 = 0;
    bl_count[0] = 0;
    let mut next_code: [u32; 16] = [0; 16];
    for bits in 1..=maxbits {
        code = (code + bl_count[bits as usize - 1]) << 1;
        next_code[bits as usize] = code;
    }

    // 3)  Assign numerical values to all codes, using consecutive
    // values for all codes of the same length with the base
    // values determined at step 2. Codes that are never used
    // (which have a bit length of zero) must not be assigned a
    // value.

    for n in 0..code_lengths.len() {
        let len = code_lengths[n];
        if len != 0 {
            let mut code = next_code[len as usize];
            if code >= 1 << len {
                return err_exit_code(ExitCode::InvalidDeflate, "an oversubscribed Huffman code");
            }

            // code should be stored in reverse bit order
            let mut rev_code: u16 = 0;
            for _ in 0..len {
                rev_code = (rev_code << 1) | (code & 1) as u16;
                code >>= 1;
            }

            result[n] = rev_code;
            next_code[len as usize] += 1;
        }
    }

    Ok(result)
}

fn is_valid_huffman_code_lengths(code_lengths: &[u8]) -> bool {
    // Ensure that the array is not empty
    if code_lengths.is_empty() {
        return false;
    }

    // Count the number of codes for each code length using an array
    const MAX_CODE_LENGTH: usize = 16;
    let mut length_count = [0; MAX_CODE_LENGTH];
    for &length in code_lengths.iter() {
        if length as usize >= MAX_CODE_LENGTH {
            return false;
        }
        length_count[length as usize] += 1;
    }

    // essential property of huffman codes is that all internal nodes
    // have exactly two children. This means that the number of internal
    // nodes doubles each time we go down one level in the tree.
    let mut internal_nodes = 2;
    for i in 1..length_count.len() {
        internal_nodes -= length_count[i];
        if internal_nodes < 0 {
            return false;
        }
        internal_nodes *= 2;
    }

    // there should be no more internal nodes left
    internal_nodes == 0
}

/// Calculates Huffman code array given an array of Huffman Code Lengths using the RFC 1951 algorithm
/// Huffman tree will be returned in tree where:
/// 1. when N is an even number tree[N] is the array index of the '0' child and
///    tree[N+1] is the array index of the '1' child
/// 2. If tree[i] is less than zero then it is a leaf and the literal alphabet value is !tree[i]
/// 3. The root node index 'N' is tree.len() - 2. Search should start at that node.
fn calculate_huffman_code_tree(code_lengths: &[u8]) -> Result<HuffmanTree> {
    if !is_valid_huffman_code_lengths(code_lengths) {
        return err_exit_code(ExitCode::InvalidDeflate, "Invalid Huffman code lengths");
    }

    let mut c_codes: i32 = 0;
    let mut c_bits_largest = 0;

    // First calculate total number of leaf nodes in the Huffman Tree and the max huffman code length
    for &c_bits in code_lengths {
        if c_bits != 0 {
            c_codes += 1;
        }

        if c_bits > c_bits_largest {
            c_bits_largest = c_bits;
        }
    }

    // Number of internal nodes in the tree will be the ((number of leaf nodes) - 1)
    let mut tree: Vec<u16> = vec![0; ((c_codes - 1) * 2) as usize]; // Allocation is double as each node has 2 links

    let mut i_huff_nodes: i32 = 0;
    let mut i_huff_nodes_previous_level: i32 = 0;

    // Build the tree from the bottom starting with leafs of longs codes
    for c_bits_cur in (1..=c_bits_largest).rev() {
        let i_huff_nodes_start = i_huff_nodes;
        // Create parent nodes for all leaf codes at current bit length
        for j in 0..code_lengths.len() {
            if code_lengths[j] == c_bits_cur {
                tree[i_huff_nodes as usize] = !(j as u16); // Leaf nodes links store the actual literal character negative biased by -1
                i_huff_nodes += 1;
            }
        }

        // Create parent node links for all remaining nodes from previous iteration
        for j in (i_huff_nodes_previous_level..i_huff_nodes_start).step_by(2) {
            tree[i_huff_nodes as usize] = j as u16;
            i_huff_nodes += 1;
        }

        i_huff_nodes_previous_level = i_huff_nodes_start;
    }

    // build a fast decoder that lets us decode the entire symbol given a byte of input
    let mut fast_decode = [(0u8, 0u16); 256];
    for i in 0..256 {
        let mut i_node_cur = tree.len() - 2; // Start at the root of the Huffman tree

        let mut v = i;
        let mut num_bits = 1;
        loop {
            // Use next bit of input to decide next node
            let next = tree[(v & 1) + i_node_cur];

            // High bit indicates a leaf node, return alphabet char for this leaf
            if (next & 0x8000) != 0 || num_bits == 8 {
                fast_decode[i] = (num_bits, next);
                break;
            }

            i_node_cur = next as usize;
            v >>= 1;
            num_bits += 1;
        }
    }

    Ok(HuffmanTree { tree, fast_decode })
}

/// Reads the next Huffman encoded char from bitReader using the Huffman tree encoded in huffman_tree
/// Huffman Nodes are encoded in the array of ints as follows:
/// '0' child link of node 'N' is at huffman_tree[N], '1' child link is at huffman_tree[N + 1]
/// Root of tree is at huffman_tree.len() - 2
#[inline(always)]
fn decode_symbol(
    bit_reader: &mut impl ReadBits,
    reader: &mut impl BufRead,
    huffman_tree: &HuffmanTree,
) -> Result<u16> {
    // we need at least 8 bits to fast decode the symbol, which is almost always available
    if bit_reader.bits_left() < 8 {
        return decode_symbol_slow(bit_reader, reader, huffman_tree);
    }

    let (num_bits, mut node) = huffman_tree.fast_decode[bit_reader.peek_byte() as usize];
    bit_reader.consume(num_bits as u32);

    loop {
        // High bit indicates a leaf node, return alphabet char for this leaf
        if (node & 0x8000) != 0 {
            return Ok(!node);
        }

        // Use next bit of input to decide next node
        node = huffman_tree.tree[bit_reader.get(1, reader)? as usize + node as usize];
    }
}

#[cold]
fn decode_symbol_slow(
    bit_reader: &mut impl ReadBits,
    reader: &mut impl BufRead,
    huffman_tree: &HuffmanTree,
) -> Result<u16> {
    let mut i_node_cur: usize = huffman_tree.tree.len() - 2;
    loop {
        // Use next bit of input to decide next node
        let node = huffman_tree.tree[bit_reader.get(1, reader)? as usize + i_node_cur];

        // High bit indicates a leaf node, return alphabet char for this leaf
        if (node & 0x8000) != 0 {
            return Ok(!node);
        }

        i_node_cur = node as usize;
    }
}

#[cfg(test)]
/// A ReadBits implementation that reads bits from a single u32 used for unit tests
struct SingleCode {
    pub code: u32,
}

#[cfg(test)]
impl ReadBits for SingleCode {
    fn get(&mut self, cbits: u32, _: &mut impl BufRead) -> std::io::Result<u32> {
        let result = self.code & ((1 << cbits) - 1);
        self.code >>= cbits;

        Ok(result)
    }

    fn peek_byte(&self) -> u8 {
        (self.code & 0xff) as u8
    }

    fn bits_left(&self) -> u32 {
        8
    }

    fn consume(&mut self, cbits: u32) {
        self.code >>= cbits;
    }
}

#[cfg(test)]
fn roundtrip(frequencies: &[u16], huffcalc: super::huffman_calc::HufftreeBitCalc) {
    use super::huffman_calc::calc_bit_lengths;

    let code_lengths = calc_bit_lengths(huffcalc, frequencies, 7);

    let codes = calc_huffman_codes(&code_lengths).unwrap();

    let huffman_tree = calculate_huffman_code_tree(&code_lengths).unwrap();

    for i in 0..code_lengths.len() {
        // skip zero length codes
        if code_lengths[i] != 0 {
            // calculate the stream of bits for the code
            let mut code = SingleCode {
                code: codes[i].into(),
            };

            let symbol = decode_symbol(&mut code, &mut std::io::empty(), &huffman_tree).unwrap();

            assert_eq!(i, symbol as usize);
        }
    }
}
/// verify that the huffman codes generated can be decoded with the huffman code tree
#[test]
fn roundtrip_huffman_code() {
    roundtrip(
        &[1, 0, 2, 3, 5, 8, 13, 0],
        super::huffman_calc::HufftreeBitCalc::Miniz,
    );
    roundtrip(
        &[1, 0, 2, 3, 5, 8, 13, 0],
        super::huffman_calc::HufftreeBitCalc::Zlib,
    );

    roundtrip(
        &[1, 0, 2, 3, 5, 1008, 113, 1, 1, 1, 100, 10000],
        super::huffman_calc::HufftreeBitCalc::Zlib,
    );
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum TreeCodeType {
    /// Code length 0 - 15
    Code = 0,
    /// Copy the previous code length 3 - 6 times.
    Repeat = 16,
    /// Repeat a code length of 0 for 3 - 10 times. (3 bits of length)
    ZeroShort = 17,
    /// Repeat a code length of 0 for 11 - 138 times (7 bits of length)
    ZeroLong = 18,
}

/// Represents the original encoding of the huffman table as it was read from the file
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct HuffmanOriginalEncoding {
    /// Huffman literal/distance lengths as RLE encoded in the file
    pub lengths: Vec<(TreeCodeType, u8)>,

    /// huffman lengths for the code length alphabet used to
    /// encode the huffman table
    pub code_lengths: [u8; 19],

    /// # of Literal/Length codes  (257 - 286)
    pub num_literals: usize,

    /// # of Distance codes         (1 - 32)
    pub num_dist: usize,

    /// # of Code Length codes      (4 - 19)
    pub num_code_lengths: usize,
}

impl HuffmanOriginalEncoding {
    /// Reads a dynamic huffman table from the bit reader. The structure
    /// holds all the information necessary to recode the huffman table
    /// exactly as it was written.
    pub fn read(
        bit_reader: &mut BitReader,
        reader: &mut impl BufRead,
    ) -> Result<HuffmanOriginalEncoding> {
        // 5 Bits: HLIT, # of Literal/Length codes - 257 (257 - 286)
        let hlit = bit_reader.get(5, reader)? as usize + 257;
        // 5 Bits: HDIST, # of Distance codes - 1        (1 - 32)
        let hdist = bit_reader.get(5, reader)? as usize + 1;
        // 4 Bits: HCLEN, # of Code Length codes - 4     (4 - 19)
        let hclen = bit_reader.get(4, reader)? as usize + 4;

        //  HCLEN + 4) x 3 bits: code lengths for the code length
        //  alphabet given just above, in the order: 16, 17, 18,
        //  0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15
        //	These code lengths are interpreted as 3-bit integers
        //	(0-7); as above, a code length of 0 means the
        //	corresponding symbol (literal/length or distance code
        //	length) is not used.

        let mut code_length_alphabet_code_lengths = [0; 19];
        for i in 0..hclen {
            code_length_alphabet_code_lengths[TREE_CODE_ORDER_TABLE[i]] =
                bit_reader.get(3, reader)? as u8;
        }

        let code_length_huff_code_tree =
            calculate_huffman_code_tree(&code_length_alphabet_code_lengths)?;

        let c_lengths_combined = hlit + hdist;

        let mut combined_lengths = Vec::new();
        combined_lengths.reserve_exact(c_lengths_combined);

        let mut codes_read: usize = 0;

        while codes_read < c_lengths_combined {
            let w_next: u16 = decode_symbol(bit_reader, reader, &code_length_huff_code_tree)?;

            if w_next <= 15 {
                //	0 - 15: Represent code lengths of 0 - 15
                combined_lengths.push((TreeCodeType::Code, w_next as u8));
                codes_read += 1;
            } else {
                // 16 - 18 represent a repeat code
                let tree_code = match w_next {
                    16 => TreeCodeType::Repeat,
                    17 => TreeCodeType::ZeroShort,
                    18 => TreeCodeType::ZeroLong,
                    _ => {
                        return err_exit_code(ExitCode::InvalidDeflate, "Invalid code length");
                    }
                };

                let (sub, bits) = Self::get_tree_code_adjustment(tree_code);

                let v = bit_reader.get(bits, reader)? as u8 + sub;
                combined_lengths.push((tree_code, v));

                codes_read += v as usize;
            }
        }

        if codes_read != c_lengths_combined {
            return err_exit_code(
                ExitCode::InvalidDeflate,
                "Code table should be same size as hdist + hlit",
            );
        }

        Ok(HuffmanOriginalEncoding {
            lengths: combined_lengths,
            code_lengths: code_length_alphabet_code_lengths,
            num_literals: hlit,
            num_dist: hdist,
            num_code_lengths: hclen,
        })
    }

    /// writes dynamic huffman table to the output buffer using the bitwriter
    pub fn write(&self, bitwriter: &mut BitWriter, output_buffer: &mut Vec<u8>) -> Result<()> {
        bitwriter.write(self.num_literals as u32 - 257, 5, output_buffer);
        bitwriter.write(self.num_dist as u32 - 1, 5, output_buffer);
        bitwriter.write(self.num_code_lengths as u32 - 4, 4, output_buffer);

        for i in 0..self.num_code_lengths {
            bitwriter.write(
                self.code_lengths[TREE_CODE_ORDER_TABLE[i]].into(),
                3,
                output_buffer,
            );
        }

        let codes = calc_huffman_codes(&self.code_lengths)?;

        for &(tree_code, length) in self.lengths.iter() {
            match tree_code {
                TreeCodeType::Code => {
                    bitwriter.write(
                        codes[length as usize].into(),
                        self.code_lengths[length as usize].into(),
                        output_buffer,
                    );
                }
                TreeCodeType::Repeat | TreeCodeType::ZeroShort | TreeCodeType::ZeroLong => {
                    bitwriter.write(
                        codes[tree_code as usize].into(),
                        self.code_lengths[tree_code as usize].into(),
                        output_buffer,
                    );

                    let (sub, bits) = Self::get_tree_code_adjustment(tree_code);
                    bitwriter.write((length - sub).into(), bits, output_buffer);
                }
            }
        }

        Ok(())
    }

    /// returns the length and distance tables for the fixed huffman table
    fn get_fixed_distance_lengths() -> (Vec<u8>, Vec<u8>) {
        let mut lit_code_lengths = Vec::new();
        lit_code_lengths.reserve_exact(288);

        // Create Length table for the Literal Alphabet
        //   Range	Code Length
        //   0 - 143     8
        // 144 - 255     9
        // 256 - 279     7
        // 280 - 287     8
        for i in 0..288 {
            let mut wbits: u8 = 8;
            if (144..=255).contains(&i) {
                wbits = 9;
            } else if (256..=279).contains(&i) {
                wbits = 7;
            }

            lit_code_lengths.push(wbits);
        }

        (lit_code_lengths, vec![5; 32])
    }

    /// returns the combined literal and distance lengths
    pub fn get_literal_distance_lengths(&self) -> (Vec<u8>, Vec<u8>) {
        let mut lengths = Vec::new();
        let mut prevcode = 0;

        for &(tree_code, length) in self.lengths.iter() {
            match tree_code {
                TreeCodeType::Code => {
                    lengths.push(length);
                    prevcode = length;
                }
                TreeCodeType::Repeat => {
                    for _ in 0..length {
                        lengths.push(prevcode);
                    }
                }
                TreeCodeType::ZeroShort | TreeCodeType::ZeroLong => {
                    for _ in 0..length {
                        lengths.push(0);
                    }
                }
            }
        }

        (
            lengths[0..self.num_literals].to_vec(),
            lengths[self.num_literals..].to_vec(),
        )
    }

    /// returns the constants used to adjust the coding of tree code types
    /// (amount to subtract, #bits to encode)
    const fn get_tree_code_adjustment(tree_code: TreeCodeType) -> (u8, u32) {
        match tree_code {
            TreeCodeType::Repeat => (3, 2),
            TreeCodeType::ZeroShort => (3, 3),
            TreeCodeType::ZeroLong => (11, 7),
            TreeCodeType::Code => unreachable!(),
        }
    }
}

/// Huffman tree used to decode the huffman codes
struct HuffmanTree {
    pub tree: Vec<u16>,

    /// Fast decode table to get the symbol directly from the
    /// byte if we have enough bits available. This code
    /// the u8 as the number of bits required and the u16
    /// as the symbol to return.
    pub fast_decode: [(u8, u16); 256],
}

pub(super) struct HuffmanReader {
    lit_huff_code_tree: HuffmanTree,
    dist_huff_code_tree: HuffmanTree,
}

pub(super) struct HuffmanWriter {
    lit_code_lengths: [u8; LITLEN_CODE_COUNT + 1],
    lit_huffman_codes: [u16; LITLEN_CODE_COUNT + 1],
    dist_code_lengths: [u8; MAX_DIST_CODE as usize + 1],
    dist_huffman_codes: [u16; MAX_DIST_CODE as usize + 1],
}

impl HuffmanReader {
    /// Create Fixed Huffman code tables
    ///
    /// The Huffman codes for the two alphabets are fixed, and are not
    /// represented explicitly in the data.  The Huffman code lengths
    /// for the literal/length alphabet are:
    ///
    /// Lit Value    Bits        Codes
    /// ---------    ----        -----
    ///   0 - 143     8          00110000 through
    ///                          10111111
    /// 144 - 255     9          110010000 through
    ///                          111111111
    /// 256 - 279     7          0000000 through
    ///                          0010111
    /// 280 - 287     8          11000000 through
    ///                          11000111
    /// The code lengths are sufficient to generate the actual codes,
    /// as described above; we show the codes in the table for added
    /// clarity.  Literal/length values 286-287 will never actually
    /// occur in the compressed data, but participate in the code
    /// construction.
    pub fn create_fixed() -> Result<Self> {
        let (lit_lengths, dist_lengths) = HuffmanOriginalEncoding::get_fixed_distance_lengths();

        Ok(HuffmanReader {
            lit_huff_code_tree: calculate_huffman_code_tree(&lit_lengths)?,
            dist_huff_code_tree: calculate_huffman_code_tree(&dist_lengths)?,
        })
    }

    /// creates a reader from the encoding of the huffman table
    pub fn create_from_original_encoding(
        huffman_original_encoding: &HuffmanOriginalEncoding,
    ) -> Result<Self> {
        let (lit_lengths, dist_lengths) = huffman_original_encoding.get_literal_distance_lengths();

        Ok(HuffmanReader {
            lit_huff_code_tree: calculate_huffman_code_tree(&lit_lengths)?,
            dist_huff_code_tree: calculate_huffman_code_tree(&dist_lengths)?,
        })
    }

    pub fn fetch_next_literal_code(
        &self,
        bit_reader: &mut BitReader,
        reader: &mut impl BufRead,
    ) -> Result<u16> {
        decode_symbol(bit_reader, reader, &self.lit_huff_code_tree)
    }

    pub fn fetch_next_distance_char(
        &self,
        bit_reader: &mut BitReader,
        reader: &mut impl BufRead,
    ) -> Result<u16> {
        decode_symbol(bit_reader, reader, &self.dist_huff_code_tree)
    }
}

fn copy_to_array<const N: usize, T: Copy + Clone + Default>(src: &[T]) -> [T; N] {
    let mut dst = [T::default(); N];

    for i in 0..src.len().min(N) {
        dst[i] = src[i];
    }

    dst
}

impl HuffmanWriter {
    pub fn start_dynamic_huffman_table(
        bitwriter: &mut BitWriter,
        huffman_encoding: &HuffmanOriginalEncoding,
        output_buffer: &mut Vec<u8>,
    ) -> Result<Self> {
        huffman_encoding.write(bitwriter, output_buffer)?; // write the huffman table

        let (lit_lengths, dist_lengths) = huffman_encoding.get_literal_distance_lengths();

        let lit_codes = calc_huffman_codes(&lit_lengths)?;
        let dist_codes = calc_huffman_codes(&dist_lengths)?;

        Ok(HuffmanWriter {
            lit_code_lengths: copy_to_array(&lit_lengths),
            lit_huffman_codes: copy_to_array(&lit_codes),
            dist_code_lengths: copy_to_array(&dist_lengths),
            dist_huffman_codes: copy_to_array(&dist_codes),
        })
    }

    pub fn start_fixed_huffman_table() -> Self {
        let (lit_lengths, dist_lengths) = HuffmanOriginalEncoding::get_fixed_distance_lengths();

        let lit_codes = calc_huffman_codes(&lit_lengths).unwrap();
        let dist_codes = calc_huffman_codes(&dist_lengths).unwrap();

        HuffmanWriter {
            lit_code_lengths: copy_to_array(&lit_lengths),
            lit_huffman_codes: copy_to_array(&lit_codes),
            dist_code_lengths: copy_to_array(&dist_lengths),
            dist_huffman_codes: copy_to_array(&dist_codes),
        }
    }

    #[inline(always)]
    pub fn write_literal(&self, bitwriter: &mut BitWriter, output_buffer: &mut Vec<u8>, lit: u16) {
        let code = self.lit_huffman_codes[lit as usize];
        let c_bits = self.lit_code_lengths[lit as usize];

        bitwriter.write(code.into(), c_bits.into(), output_buffer);
    }

    #[inline(always)]
    pub fn write_distance(
        &self,
        bitwriter: &mut BitWriter,
        output_buffer: &mut Vec<u8>,
        dist: DistCode,
    ) {
        let code = self.dist_huffman_codes[usize::from(dist.get())];
        let c_bits = self.dist_code_lengths[usize::from(dist.get())];

        bitwriter.write(code.into(), c_bits.into(), output_buffer);
    }
}

#[test]
fn roundtrip_huffman_bitreadwrite() {
    use crate::deflate::bit_reader::BitReader;

    use std::io::Cursor;

    let code_lengths = [1, 0, 3, 3, 4, 4, 3, 0];

    let codes = calc_huffman_codes(&code_lengths).unwrap();

    let mut bit_writer = BitWriter::default();
    let mut data_buffer = Vec::new();
    for i in 0..code_lengths.len() {
        if code_lengths[i] != 0 {
            bit_writer.write(codes[i] as u32, code_lengths[i] as u32, &mut data_buffer);
        }
    }
    // write a sentinal to make sure that we read everything properly
    bit_writer.write(0x1234, 16, &mut data_buffer);
    bit_writer.pad(0, &mut data_buffer);

    let mut reader = Cursor::new(&data_buffer);
    let mut bit_reader = BitReader::new();

    let huffman_tree = calculate_huffman_code_tree(&code_lengths).unwrap();

    for i in 0..code_lengths.len() {
        if code_lengths[i] != 0 {
            assert_eq!(
                i as u16,
                decode_symbol(&mut bit_reader, &mut reader, &huffman_tree).unwrap()
            );
        }
    }

    // read sentinal to make sure we read everything correctly
    assert_eq!(
        bit_reader.get(16, &mut reader).unwrap(),
        0x1234,
        "sentinal value didn't match"
    );
}

#[test]
fn roundtrip_complicated() {
    #[rustfmt::skip]
    let h = HuffmanOriginalEncoding {
        lengths: vec![(TreeCodeType::ZeroShort, 10), (TreeCodeType::Code, 11), (TreeCodeType::Code, 0), (TreeCodeType::Code, 0),
            (TreeCodeType::Code, 11), (TreeCodeType::ZeroLong, 18), (TreeCodeType::Code, 6), (TreeCodeType::Code, 14), (TreeCodeType::ZeroShort, 5),
            (TreeCodeType::Code, 11), (TreeCodeType::Code, 9), (TreeCodeType::Code, 10), (TreeCodeType::Code, 0), (TreeCodeType::Code, 0), (TreeCodeType::Code, 10),
            (TreeCodeType::Code, 11), (TreeCodeType::Code, 8), (TreeCodeType::Code, 0), (TreeCodeType::Code, 7), (TreeCodeType::Code, 6), (TreeCodeType::Repeat, 6),
            (TreeCodeType::Code, 6), (TreeCodeType::Code, 7), (TreeCodeType::Code, 10), (TreeCodeType::Code, 0), (TreeCodeType::Code, 0), (TreeCodeType::Code, 10),
            (TreeCodeType::ZeroShort, 3), (TreeCodeType::Code, 8), (TreeCodeType::Repeat, 5), (TreeCodeType::Code, 11), (TreeCodeType::Code, 0), (TreeCodeType::Code, 9),
            (TreeCodeType::Code, 0), (TreeCodeType::Code, 0), (TreeCodeType::Code, 10), (TreeCodeType::Code, 10), (TreeCodeType::Code, 11), (TreeCodeType::Code, 9),
            (TreeCodeType::Code, 10), (TreeCodeType::Code, 12), (TreeCodeType::Code, 10), (TreeCodeType::Code, 9), (TreeCodeType::Code, 10),
            (TreeCodeType::Code, 11), (TreeCodeType::Code, 0), (TreeCodeType::Code, 11), (TreeCodeType::ZeroShort, 3), (TreeCodeType::Code, 11), (TreeCodeType::Code, 9), (TreeCodeType::Code, 11),
            (TreeCodeType::Code, 0), (TreeCodeType::Code, 11), (TreeCodeType::Code, 12), (TreeCodeType::Code, 7), (TreeCodeType::Code, 10), (TreeCodeType::Code, 8),
            (TreeCodeType::Code, 8), (TreeCodeType::Code, 6), (TreeCodeType::Code, 9), (TreeCodeType::Code, 8), (TreeCodeType::Code, 8),
            (TreeCodeType::Code, 8), (TreeCodeType::Code, 0), (TreeCodeType::Code, 10), (TreeCodeType::Code, 8), (TreeCodeType::Code, 9),
            (TreeCodeType::Code, 7), (TreeCodeType::Code, 7), (TreeCodeType::Code, 8), (TreeCodeType::Code, 13), (TreeCodeType::Code, 7), (TreeCodeType::Code, 7), (TreeCodeType::Code, 7), (TreeCodeType::Code, 8), (TreeCodeType::Code, 11),
            (TreeCodeType::Code, 10), (TreeCodeType::Code, 10), (TreeCodeType::Code, 8), (TreeCodeType::Code, 12), (TreeCodeType::ZeroLong, 133), (TreeCodeType::Code, 14), (TreeCodeType::Code, 5),
            (TreeCodeType::Code, 6), (TreeCodeType::Code, 6), (TreeCodeType::Code, 4), (TreeCodeType::Code, 5), (TreeCodeType::Code, 5), (TreeCodeType::Code, 8), (TreeCodeType::Code, 5),
            (TreeCodeType::Code, 5), (TreeCodeType::Code, 6), (TreeCodeType::Code, 4), (TreeCodeType::Code, 6), (TreeCodeType::Code, 5), (TreeCodeType::Code, 9), (TreeCodeType::Code, 5), (TreeCodeType::Code, 7), (TreeCodeType::Code, 4),
            (TreeCodeType::Code, 5), (TreeCodeType::Code, 6), (TreeCodeType::Code, 7), (TreeCodeType::Code, 4), (TreeCodeType::Code, 6), (TreeCodeType::Code, 6), (TreeCodeType::Code, 6), (TreeCodeType::Code, 7), (TreeCodeType::Code, 7),
            (TreeCodeType::Code, 8), (TreeCodeType::Code, 8), (TreeCodeType::Code, 6), (TreeCodeType::Code, 12), (TreeCodeType::Code, 0), (TreeCodeType::Code, 0), (TreeCodeType::Code, 13),
            (TreeCodeType::Code, 13), (TreeCodeType::Code, 11), (TreeCodeType::Code, 9), (TreeCodeType::Code, 10), (TreeCodeType::Code, 9), (TreeCodeType::Code, 9), (TreeCodeType::Code, 5), (TreeCodeType::Code, 7), (TreeCodeType::Code, 6),
            (TreeCodeType::Code, 5), (TreeCodeType::Code, 5), (TreeCodeType::Code, 6), (TreeCodeType::Code, 5), (TreeCodeType::Code, 5), (TreeCodeType::Code, 4), (TreeCodeType::Code, 4),
            (TreeCodeType::Code, 3), (TreeCodeType::Code, 3), (TreeCodeType::Code, 4), (TreeCodeType::Repeat, 4), (TreeCodeType::Code, 5), (TreeCodeType::Code, 4), (TreeCodeType::Code, 6)],
        code_lengths: [3, 0, 0, 6, 4, 3, 3, 4, 3, 4, 3, 4, 5, 6, 6, 0, 6, 6, 6],
        num_literals: 286,
        num_dist: 30,
        num_code_lengths: 17
    };

    rountrip_test(h);
}

#[test]
fn roundtrip_huffman_table() {
    // simple hardcoded encoding

    let encoding = HuffmanOriginalEncoding {
        lengths: vec![
            (TreeCodeType::Code, 1),
            (TreeCodeType::Code, 2),
            (TreeCodeType::Code, 3),
            (TreeCodeType::ZeroLong, 138),
            (TreeCodeType::ZeroLong, 115),
            (TreeCodeType::Code, 3),
            (TreeCodeType::Code, 1),
            (TreeCodeType::Code, 2),
            (TreeCodeType::Code, 2),
        ],
        code_lengths: [0, 2, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2],
        num_literals: 257,
        num_dist: 3,
        num_code_lengths: 19,
    };

    rountrip_test(encoding);
}

#[cfg(test)]
fn rountrip_test(encoding: HuffmanOriginalEncoding) {
    use super::bit_reader::BitReader;
    use std::io::Cursor;

    let mut output_buffer = Vec::new();
    let mut bit_writer = BitWriter::default();
    encoding.write(&mut bit_writer, &mut output_buffer).unwrap();

    // write a sentinal to make sure that we read everything properly
    bit_writer.write(0x1234, 16, &mut output_buffer);

    // flush everything
    bit_writer.pad(0, &mut output_buffer);
    bit_writer.flush_whole_bytes(&mut output_buffer);

    // now re-read the encoding
    let mut reader = Cursor::new(&output_buffer);
    let mut bit_reader = BitReader::new();

    let encoding2 = HuffmanOriginalEncoding::read(&mut bit_reader, &mut reader).unwrap();
    assert_eq!(encoding, encoding2);

    // verify sentinal to make sure we didn't write anything extra or too little
    assert_eq!(
        bit_reader.get(16, &mut reader).unwrap(),
        0x1234,
        "sentinal value didn't match"
    );
}

/// azlin-preflate: RFC 1951 lets a block's literal/length or distance alphabet hold a single
/// code of one bit (miniz writes one for a run of equal bytes, whose only distance is 1) or no
/// code at all (a block of literals). Both are read; the bit pattern no code has is an error.
#[test]
fn an_alphabet_of_one_one_bit_code_is_read_and_its_unused_pattern_is_an_error() {
    use crate::deflate::bit_reader::BitReader;
    use std::io::Cursor;

    let mut lengths = [0u8; 30];
    lengths[4] = 1;
    let tree = calculate_lit_dist_code_tree(&lengths).unwrap();
    // the code ('0') twice, then the pattern no code has ('1'): first bits come first
    let data = [0b0000_0100u8, 0, 0, 0];
    let mut reader = Cursor::new(&data[..]);
    let mut bits = BitReader::new();
    assert_eq!(decode_symbol(&mut bits, &mut reader, &tree).unwrap(), 4);
    assert_eq!(decode_symbol(&mut bits, &mut reader, &tree).unwrap(), 4);
    assert!(decode_symbol(&mut bits, &mut reader, &tree).is_err());

    // the same through the fast table (8 bits are buffered once a read has filled it)
    let data = [0u8, 0b0000_0010, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
    let mut reader = Cursor::new(&data[..]);
    let mut bits = BitReader::new();
    assert_eq!(bits.get(8, &mut reader).unwrap(), 0);
    assert_eq!(decode_symbol(&mut bits, &mut reader, &tree).unwrap(), 4);
    assert!(decode_symbol(&mut bits, &mut reader, &tree).is_err());

    // an alphabet without codes decodes nothing
    let none = calculate_lit_dist_code_tree(&[0u8; 30]).unwrap();
    let mut reader = Cursor::new(&[0u8; 8][..]);
    let mut bits = BitReader::new();
    assert!(decode_symbol(&mut bits, &mut reader, &none).is_err());

    // the code length alphabet stays complete only, and so does any other incomplete code
    assert!(calculate_huffman_code_tree(&lengths[..19]).is_err());
    let mut two = [0u8; 30];
    two[0] = 1;
    two[1] = 2;
    assert!(calculate_lit_dist_code_tree(&two).is_err());
    let mut long_single = [0u8; 30];
    long_single[3] = 2;
    assert!(calculate_lit_dist_code_tree(&long_single).is_err());
}
