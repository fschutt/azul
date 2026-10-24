/// Different versions of Zlib use some length criterea to decide whether to add all the substrings of
/// a large match to the hash table. For example, zlib level 1 will only add all the substrings of matches
/// of length 4 in order to save on CPU.
///
/// What we do here is walk through all the matches and record how long the matching
/// substrings are. The we see what the largest string was that we fully added to the
/// dictionary.
///
/// This will be the limit that we use when we decide whether to
/// use skip_hash or update_hash.

use crate::deflate::deflate_token::{DeflateToken, DeflateTokenBlock, DeflateTokenBlockType};

#[derive(Default, Eq, PartialEq, Debug, Clone, Copy)]
pub enum DictionaryAddPolicy {
    /// Add all substrings of a match to the dictionary
    #[default]
    AddAll,
    /// Add only the first substring of a match to the dictionary that are larger than the limit
    AddFirst(u16),
    /// Add only the first and last substring of a match to the dictionary that are larger than the limit
    AddFirstAndLast(u16),

    /// This policy is used by MiniZ in fastest mode. It adds all substrings of a match to the dictionary except
    /// literals that are 4 bytes away from the end of the block.
    ///
    /// In addition, we don't attempt to look for matches if we are within 3 bytes of the 4k boundary.
    AddFirstExcept4kBoundary,

    /// This policy is used by fast mode in zlibng, it is the same
    /// as AddFirst(0) but it also add the last character for the
    /// last match in the 32k window.
    ///
    /// This is due to the fact that
    /// each time the dictionary is reset, it explicitly adds the
    /// last character to the dictionary which ends up being the
    /// last chacacter of the previous match.
    AddFirstWith32KBoundary,
}

/// Check if we are crossing the 4k boundary. MiniZ in fast mode
/// doesn't process the last 3 bytes before the 4k boundary for optimization reasons.
pub fn cross_4k_boundary(pos: u32) -> bool {
    (pos & 4095) >= 4093
}

impl DictionaryAddPolicy {
    /// Updates the hash based on the dictionary add policy
    #[inline(always)]
    pub fn update_hash<U: FnMut(&[u8], u32, u32)>(
        self,
        input: &[u8],
        pos: u32,
        length: u32,
        mut update_fn: U,
    ) {
        if length == 1 {
            match self {
                DictionaryAddPolicy::AddFirstExcept4kBoundary => {
                    if !cross_4k_boundary(pos) {
                        update_fn(input, pos, 1);
                    }
                }
                _ => {
                    update_fn(input, pos, 1);
                }
            }
        } else {
            match self {
                DictionaryAddPolicy::AddAll => update_fn(input, pos, length),
                DictionaryAddPolicy::AddFirst(limit) => {
                    if length <= u32::from(limit) {
                        update_fn(input, pos, length);
                    } else {
                        update_fn(input, pos, 1);
                    }
                }
                DictionaryAddPolicy::AddFirstAndLast(limit) => {
                    if length <= u32::from(limit) {
                        update_fn(input, pos, length);
                    } else {
                        update_fn(input, pos, 1);
                        update_fn(&input[length as usize - 1..], pos + length - 1, 1);
                    }
                }
                DictionaryAddPolicy::AddFirstExcept4kBoundary => {
                    if !cross_4k_boundary(pos) {
                        update_fn(input, pos, 1);
                    }
                }
                DictionaryAddPolicy::AddFirstWith32KBoundary => {
                    update_fn(input, pos, 1);
                    if is_at_32k_boundary(length, pos) {
                        update_fn(&input[length as usize - 1..], pos + length - 1, 1);
                    }
                }
            }
        }
    }
}

/// Check if the match is crossing the 32k boundary as which happens
/// in zlibng.  
fn is_at_32k_boundary(length: u32, pos: u32) -> bool {
    length > 1
        && (((pos) & 0x7fff) <= (32768 - 0x106))
        && (((pos + length) & 0x7fff) >= (32768 - 0x106))
}

/// When adding matches to the dictionary, some of the fast variants
/// only add smaller strings in their entirety (ie a substring starting
/// at each position). This function is designed to measure this
/// and determine the policy that should be used.
pub(super) fn estimate_add_policy(token_blocks: &[DeflateTokenBlock]) -> DictionaryAddPolicy {
    const WINDOW_MASK: usize = 0x7fff;

    // used to see if we have the special case of not adding matches on the edge
    // of the 4k boundary. This is used by miniz.
    let mut block_4k = true;

    let mut current_window = vec![0u16; WINDOW_MASK + 1];

    // tracks the maximum length that we've see that was added to the dictionary
    let mut max_length: u32 = 0;

    // tracks the maximum length that we've seen that was added to the dictionary if the last match was also added
    let mut max_length_last_add = 0;

    // same as previous, but tracks if we are inside the 32k boundary
    let mut last_outside_32k_seen = false;

    let mut current_offset: u32 = 0;

    const LAST_ADDED: u16 = 0x8000;
    const LAST_32K: u16 = 0x4000;

    const MASK: u16 = 0x0fff;

    let mut min_len = u32::MAX;

    for i in 0..token_blocks.len() {
        let token_block = &token_blocks[i];

        match &token_block.block_type {
            DeflateTokenBlockType::Stored { uncompressed, .. } => {
                // we assume for stored blocks everything was added to the dictionary
                for _i in 0..uncompressed.len() {
                    current_window[current_offset as usize & WINDOW_MASK] = 0;
                    current_offset += 1;
                }
            }
            DeflateTokenBlockType::Huffman { tokens, .. } => {
                for token in tokens.iter() {
                    match token {
                        DeflateToken::Literal(_) => {
                            current_window[current_offset as usize & WINDOW_MASK] = 0;
                            current_offset += 1;
                        }
                        DeflateToken::Reference(r) => {
                            // Track if we saw something crossing the 4k boundary
                            // MiniZ in fast mode doesn't process the last 3 bytes
                            // before the 4k boundary for optimization reasons.
                            //
                            // These bytes are neither added to the dictionary
                            // nor are matches looked for in this region.
                            if cross_4k_boundary(current_offset)
                                || cross_4k_boundary(current_offset - r.dist())
                            {
                                block_4k = false;
                            }

                            min_len = std::cmp::min(min_len, r.len());

                            let previous_match =
                                current_window[(current_offset - r.dist()) as usize & WINDOW_MASK];

                            let match_length = u32::from(previous_match & MASK);

                            max_length = std::cmp::max(max_length, match_length);
                            if (previous_match & LAST_ADDED) == 0 {
                                max_length_last_add =
                                    std::cmp::max(max_length_last_add, match_length);
                            }

                            if match_length != 0 && (previous_match & LAST_32K) == 0 {
                                last_outside_32k_seen = true;
                            }

                            let last = LAST_ADDED
                                | if is_at_32k_boundary(r.len(), current_offset) {
                                    LAST_32K
                                } else {
                                    0
                                };

                            current_window[current_offset as usize & WINDOW_MASK] = 0;
                            current_offset += 1;

                            for i in 1..r.len() {
                                current_window[current_offset as usize & WINDOW_MASK] =
                                    r.len() as u16 | if i == r.len() - 1 { last } else { 0 };
                                current_offset += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    if max_length == 0 && block_4k {
        DictionaryAddPolicy::AddFirstExcept4kBoundary
    } else if !last_outside_32k_seen {
        DictionaryAddPolicy::AddFirstWith32KBoundary
    } else if max_length_last_add < max_length {
        DictionaryAddPolicy::AddFirstAndLast(max_length_last_add as u16)
    } else if max_length < 258 {
        DictionaryAddPolicy::AddFirst(max_length as u16)
    } else {
        DictionaryAddPolicy::AddAll
    }
}

