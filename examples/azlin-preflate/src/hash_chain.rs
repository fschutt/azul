/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE in this crate for license information.
 *--------------------------------------------------------------------------------------------*/

use default_boxed::DefaultBoxed;

use crate::{
    bit_helper::DebugHash,
    deflate::deflate_token::DeflateTokenReference,
    hash_algorithm::{HashImplementation, LibdeflateHash3Secondary, LibdeflateHash4},
    preflate_input::PreflateInput,
};

pub const MAX_UPDATE_HASH_BATCH: u32 = 0x180;

#[derive(Default, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(C)]
struct InternalPosition {
    pos: u16,
}

impl InternalPosition {
    fn reshift(&self, other: u16) -> Self {
        Self {
            pos: self.pos.saturating_sub(other),
        }
    }

    fn to_index(self) -> usize {
        usize::from(self.pos)
    }

    fn inc(&self) -> Self {
        Self { pos: self.pos + 1 }
    }

    fn is_valid(&self) -> bool {
        self.pos > 0
    }

    fn dist(&self, pos: InternalPosition) -> u32 {
        u32::from(self.pos) - u32::from(pos.pos)
    }
}

impl InternalPosition {
    fn from_absolute(pos: u32, total_shift: i32) -> Self {
        Self {
            pos: u16::try_from(pos as i32 - total_shift).unwrap(),
        }
    }
}

#[derive(DefaultBoxed)]
#[repr(C, align(64))]
struct HashTable {
    /// Represents the head of the hash chain for a given hash value, or zero
    /// if there is none. In order to find additional matches, you follow the prev
    /// chain from the head.
    ///
    /// (We start reading with an offset of -8 to avoid confusion with the end of
    /// the chain, so the first offset will be 8)
    head: [InternalPosition; 65536],

    /// Represents the prev chain for a given position. This is used to find
    /// all the potential matches for a given hash. The value points to previous
    /// position in the chain, or 0 if there are no more matches.
    prev: [InternalPosition; 65536],
}

impl HashTable {
    #[inline]
    fn get_head(&self, h: u16) -> InternalPosition {
        self.head[usize::from(h)]
    }

    #[inline]
    fn update_chain<H: HashImplementation>(
        &mut self,
        hash: H,
        chars: &[u8],
        mut pos: InternalPosition,
        mut length: u32,
    ) {
        debug_assert!(length as usize <= chars.len());

        if length as usize + H::NUM_HASH_BYTES - 1 >= chars.len() {
            // if we reached the end of the buffer, hash only while we have characters left
            length = chars.len().saturating_sub(H::NUM_HASH_BYTES - 1) as u32;
        }

        for i in 0..length {
            {
                let h = usize::from(hash.get_hash(&chars[i as usize..]));

                self.prev[pos.to_index()] = self.head[h];

                self.head[h] = pos;
            }

            pos = pos.inc();
        }
    }

    fn reshift_block<const DELTA: usize>(src: &[InternalPosition], dst: &mut [InternalPosition]) {
        for i in 0..16 {
            dst[i] = src[i].reshift(DELTA as u16);
        }
    }

    fn reshift<const DELTA: usize>(&mut self) {
        for x in self.head.iter_mut() {
            *x = x.reshift(DELTA as u16);
        }

        assert!((DELTA % 16) == 0, "assuming we can do blocks of 16");

        for i in DELTA / 16..65536 / 16 {
            let (a, b) = self.prev.split_at_mut(i * 16);

            // optimizer turns this into SSE2 saturated subtraction
            Self::reshift_block::<DELTA>(b, &mut a[i * 16 - DELTA..]);
        }
    }
}

/// Implements a chained hash table that is used to find matches in the input stream
pub trait HashChain {
    fn get_num_hash_bytes() -> usize;

    fn iterate<'a, const OFFSET: u32>(
        &'a self,
        input: &PreflateInput,
    ) -> impl Iterator<Item = u32> + 'a;

    fn update_hash(&mut self, input: &[u8], pos: u32, length: u32);

    fn checksum(&self, checksum: &mut DebugHash);

    /// debug function to validate that a hash table entry
    /// was correctly added for the match we were expection
    fn assert_dictionary_valid(
        &self,
        target_reference: DeflateTokenReference,
        input: &PreflateInput,
    );
}

/// Default hash chain for a given hash function periodically normalizes the hash table
pub struct HashChainDefault<H: HashImplementation> {
    hash_table: Box<HashTable>,
    total_shift: i32,
    hash: H,
}

impl<H: HashImplementation> HashChainDefault<H> {
    pub fn new(hash: H) -> Self {
        // Important: total_shift starts at -8 since 0 indicates the end of the hash chain
        // so this means that all valid values will be >= 8, otherwise the very first hash
        // offset would be zero and so it would get missed
        HashChainDefault {
            total_shift: -8,
            hash_table: HashTable::default_boxed(),
            hash: hash,
        }
    }

    fn reshift(&mut self) {
        const DELTA: usize = 0x7e00;

        self.hash_table.reshift::<DELTA>();

        self.total_shift += DELTA as i32;
    }
}

impl<H: HashImplementation> HashChain for HashChainDefault<H> {
    #[inline]
    fn iterate<'a, const OFFSET: u32>(
        &'a self,
        input: &PreflateInput,
    ) -> impl Iterator<Item = u32> + 'a {
        let ref_pos: InternalPosition =
            InternalPosition::from_absolute(input.pos() + OFFSET, self.total_shift);

        // if we have a match that needs to be inserted at the head first before
        // we start walking the chain
        let mut first_match = None;

        let h1 = self.hash.get_hash(input.cur_chars(0));

        let curr_hash;

        if OFFSET == 0 {
            curr_hash = h1;
        } else {
            assert_eq!(OFFSET, 1);

            // current hash is the next hash since we are starting at offset 1
            curr_hash = self.hash.get_hash(input.cur_chars(1));

            // we are a lazy match, then we haven't added the last byte to the hash yet
            // which is a problem if that hash should have been part of this hash chain
            // (ie the same hash chain) and we have a limited number of enumerations
            // throught the hash chain.
            //
            // In order to fix this, we see if the hashes are the same, and then add
            // a distance 1 item to the iterator that we return.
            if h1 == curr_hash {
                first_match = Some(1);
            }
        }

        let mut cur_pos = self.hash_table.get_head(curr_hash);

        std::iter::from_fn(move || {
            if OFFSET != 0 {
                if let Some(d) = first_match {
                    first_match = None;
                    return Some(d);
                }
            }
            if cur_pos.is_valid() {
                let d = ref_pos.dist(cur_pos);
                cur_pos = self.hash_table.prev[cur_pos.to_index()];
                Some(d)
            } else {
                None
            }
        })
    }

    #[allow(dead_code)]
    fn checksum(&self, _checksum: &mut DebugHash) {
        //checksum.update_slice(&self.hash_table.chain_depth);
        //checksum.update_slice(&self.hash_table.head);
        //checksum.update_slice(&self.hash_table.prev);
        //checksum.update(self.hash_shift);
        //checksum.update(self.running_hash.hash(self.hash_mask));
        //checksum.update(self.total_shift);
    }

    #[inline]
    fn update_hash(&mut self, input: &[u8], pos: u32, length: u32) {
        assert!(length <= MAX_UPDATE_HASH_BATCH);

        if pos as i32 - self.total_shift >= 0xfe08 {
            self.reshift();
        }

        let pos = InternalPosition::from_absolute(pos, self.total_shift);

        self.hash_table.update_chain(self.hash, input, pos, length);
    }

    fn assert_dictionary_valid(
        &self,
        target_reference: DeflateTokenReference,
        input: &PreflateInput,
    ) {
        println!(
            "tried to match {:?} at input: {:?}",
            target_reference, input
        );

        assert_eq!(
            &input.cur_chars(0)[..(target_reference.len() as usize)],
            &input.cur_chars(-(target_reference.dist() as i32))
                [..(target_reference.len() as usize)],
            "dictionary out of sync for {:?}",
            target_reference
        );

        let curr_hash = self.hash.get_hash(input.cur_chars(0));
        println!("hash {}", curr_hash);

        for dist in self.iterate::<0>(input) {
            println!(" dist = {}", dist);
            if dist > target_reference.dist() {
                break;
            }
        }
    }

    fn get_num_hash_bytes() -> usize {
        H::NUM_HASH_BYTES
    }
}

/// implementation of the hash chain that uses the libdeflate rotating hash.
/// This consists of two hash tables, one for length 3 and one for length 4.
pub struct HashChainLibflate4 {
    hash_table: Box<HashTable>,
    hash_table_3: Box<HashTable>,
    total_shift: i32,
}

impl HashChainLibflate4 {
    pub fn new() -> Self {
        // Important: total_shift starts at -8 since 0 indicates the end of the hash chain
        // so this means that all valid values will be >= 8, otherwise the very first hash
        // offset would be zero and so it would get missed
        HashChainLibflate4 {
            total_shift: -8,
            hash_table: HashTable::default_boxed(),
            hash_table_3: HashTable::default_boxed(),
        }
    }
}

const LIBFLATE_HASH_3: LibdeflateHash3Secondary = LibdeflateHash3Secondary {};
const LIBFLATE_HASH_4: LibdeflateHash4 = LibdeflateHash4 {};

impl HashChain for HashChainLibflate4 {
    fn iterate<'a, const OFFSET: u32>(
        &'a self,
        input: &PreflateInput,
    ) -> impl Iterator<Item = u32> + 'a {
        let ref_pos = InternalPosition::from_absolute(input.pos() + OFFSET, self.total_shift);

        // if we have a match that needs to be inserted at the head first before
        // we start walking the chain
        let mut first_match = None;

        let mut cur_pos;

        if OFFSET == 0 {
            // for libflate, we look once at the 3 length hash table for a match
            // and then walk the length 4 hash table
            let curr_hash = LIBFLATE_HASH_3.get_hash(input.cur_chars(0));
            let start_pos = self.hash_table_3.get_head(curr_hash);

            if start_pos.is_valid() {
                first_match = Some(ref_pos.dist(start_pos));
            }

            let curr_hash = LIBFLATE_HASH_4.get_hash(input.cur_chars(0));
            cur_pos = self.hash_table.get_head(curr_hash);
        } else {
            assert_eq!(OFFSET, 1);

            // current hash is the next hash since we are starting at offset 1
            let curr_hash = LIBFLATE_HASH_4.get_hash(input.cur_chars(1));

            // we are a lazy match, then we haven't added the last byte to the hash yet
            // which is a problem if that hash should have been part of this hash chain
            // (ie the same hash chain) and we have a limited number of enumerations
            // throught the hash chain.
            //
            // In order to fix this, we see if the hashes are the same, and then add
            // a distance 1 item to the iterator that we return.
            let prev_hash = LIBFLATE_HASH_4.get_hash(input.cur_chars(0));
            if prev_hash == curr_hash {
                first_match = Some(1);
            }

            cur_pos = self.hash_table.get_head(curr_hash);
        }

        std::iter::from_fn(move || {
            if let Some(d) = first_match {
                first_match = None;
                Some(d)
            } else if cur_pos.is_valid() {
                let d = ref_pos.dist(cur_pos);
                cur_pos = self.hash_table.prev[cur_pos.to_index()];
                Some(d)
            } else {
                None
            }
        })
    }

    #[allow(dead_code)]
    fn checksum(&self, _checksum: &mut DebugHash) {
        //checksum.update_slice(&self.hash_table.chain_depth);
    }

    fn assert_dictionary_valid(
        &self,
        _target_reference: DeflateTokenReference,
        _input: &PreflateInput,
    ) {
        todo!();
    }

    fn update_hash(&mut self, input: &[u8], pos: u32, length: u32) {
        assert!(length <= MAX_UPDATE_HASH_BATCH);

        if pos as i32 - self.total_shift >= 0xfe08 {
            const DELTA: usize = 0x7e00;

            self.hash_table.reshift::<DELTA>();
            self.hash_table_3.reshift::<DELTA>();

            self.total_shift += DELTA as i32;
        }

        let pos = InternalPosition::from_absolute(pos, self.total_shift);

        self.hash_table
            .update_chain(LIBFLATE_HASH_4, input, pos, length);

        self.hash_table_3
            .update_chain(LIBFLATE_HASH_3, input, pos, length);
    }

    fn get_num_hash_bytes() -> usize {
        4
    }
}
