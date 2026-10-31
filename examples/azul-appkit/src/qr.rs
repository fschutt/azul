//! QR codes for paper: byte mode, error correction level M, versions 1 to 10 (ISO/IEC 18004).
//!
//! What the apps print with it: a drive's recovery code on AzDrive's emergency kit, a recovery
//! share for a trusted contact without AzDrive. Small on purpose - one mode, one error
//! correction level (M: about 15 % of the symbol may be lost), up to 213 bytes (version 10,
//! 57 x 57 modules) - and no crate of its own: none was in the tree.
//!
//! The steps, as the standard has them:
//! 1. the data codewords: the mode indicator `0100` (8-bit bytes), the byte count (8 bits; 16
//!    from version 10), the bytes, a terminator of up to four zero bits, zero bits to the byte,
//!    then the pad codewords `0xEC` and `0x11` by turns up to the version's capacity;
//! 2. the version's blocks (Table 9), each followed by its Reed-Solomon error correction
//!    codewords over GF(256) (the polynomial `0x11D`), interleaved;
//! 3. the function patterns - the finders and their separators, the timing patterns, the
//!    alignment patterns, the dark module, the version information from version 7 - then the
//!    codewords in the two-module-wide zigzag from the bottom right corner;
//! 4. each of the eight masks scored with the four penalty rules (section 7.8.3: runs of five
//!    or more, 2 x 2 blocks, finder-like 1:1:3:1:1 runs beside four light modules, the share
//!    of dark modules); the lowest score wins (the first of equal ones), and the format
//!    information names it.
//!
//! Checked against the standard: Annex I's symbol ("01234567", 1-M) module for module, its
//! error correction codewords, the format information of Annex C and the version information
//! of Annex D; every symbol's blocks have zero syndromes and read back to their bytes.
//!
//! A symbol may carry a secret (a recovery code): [`QrCode`] prints as its version and size
//! only, and its modules are cleared when it is dropped.

use std::fmt;

/// The smallest and the largest version made.
pub const MIN_VERSION: u8 = 1;
pub const MAX_VERSION: u8 = 10;
/// The most bytes a symbol holds (version 10-M).
pub const MAX_BYTES: usize = 213;

/// Error correction level M's blocks of versions 1 to 10 (ISO/IEC 18004 Table 9): the error
/// correction codewords of each block, then (blocks, data codewords of each) of the two groups.
const BLOCKS_M: [(usize, [(usize, usize); 2]); 10] = [
    (10, [(1, 16), (0, 0)]),
    (16, [(1, 28), (0, 0)]),
    (26, [(1, 44), (0, 0)]),
    (18, [(2, 32), (0, 0)]),
    (24, [(2, 43), (0, 0)]),
    (16, [(4, 27), (0, 0)]),
    (18, [(4, 31), (0, 0)]),
    (22, [(2, 38), (2, 39)]),
    (22, [(3, 36), (2, 37)]),
    (26, [(4, 43), (1, 44)]),
];

/// The centres of the alignment patterns of versions 1 to 10 (Annex E), on both axes.
const ALIGNMENT: [&[usize]; 10] = [
    &[],
    &[6, 18],
    &[6, 22],
    &[6, 26],
    &[6, 30],
    &[6, 34],
    &[6, 22, 38],
    &[6, 24, 42],
    &[6, 26, 46],
    &[6, 28, 50],
];

/// Error correction level M's two bits in the format information.
const LEVEL_M: u32 = 0b00;
/// The mask the format information is XORed with (Annex C).
const FORMAT_MASK: u32 = 0x5412;
/// The generator polynomials of the format (BCH(15,5)) and version (BCH(18,6)) information.
const FORMAT_GENERATOR: u32 = 0x537;
const VERSION_GENERATOR: u32 = 0x1F25;
/// The pad codewords, by turns.
const PAD: [u8; 2] = [0xEC, 0x11];
/// A finder-like run (1:1:3:1:1) and the four light modules after it (penalty rule 3).
const FINDER_LIKE: [bool; 11] = [
    true, false, true, true, true, false, true, false, false, false, false,
];

/// Why no symbol was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QrError {
    /// More bytes than version 10-M holds ([`MAX_BYTES`]).
    TooLong { bytes: usize },
    /// A version outside 1 to 10 or too small for the data, or a mask outside 0 to 7.
    Unsupported(&'static str),
}

impl fmt::Display for QrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QrError::TooLong { bytes } => write!(
                f,
                "{bytes} bytes do not fit a QR code of version 10-M (at most {MAX_BYTES})"
            ),
            QrError::Unsupported(what) => write!(f, "this QR encoder does not make {what}"),
        }
    }
}

impl std::error::Error for QrError {}

/// The most bytes version `version` (1 to 10) holds at level M.
#[must_use]
pub fn capacity(version: u8) -> usize {
    (data_codewords(version) * 8 - 4 - count_bits(version)) / 8
}

/// A QR symbol: its version, its mask and its modules (`true`: dark), row by row.
pub struct QrCode {
    version: u8,
    size: usize,
    mask: u8,
    modules: Vec<bool>,
}

impl QrCode {
    /// The symbol of `data` in the smallest version that holds it, with the mask of the lowest
    /// penalty.
    pub fn encode(data: &[u8]) -> Result<QrCode, QrError> {
        let version = (MIN_VERSION..=MAX_VERSION)
            .find(|&v| data.len() <= capacity(v))
            .ok_or(QrError::TooLong { bytes: data.len() })?;
        QrCode::encode_with(data, version, None)
    }

    /// The symbol of `data` in version `version` (at least the smallest that holds it), with
    /// mask `mask` (`None`: the one of the lowest penalty).
    pub fn encode_with(data: &[u8], version: u8, mask: Option<u8>) -> Result<QrCode, QrError> {
        if !(MIN_VERSION..=MAX_VERSION).contains(&version) {
            return Err(QrError::Unsupported("versions other than 1 to 10"));
        }
        if data.len() > MAX_BYTES {
            return Err(QrError::TooLong { bytes: data.len() });
        }
        if data.len() > capacity(version) {
            return Err(QrError::Unsupported("a version too small for the data"));
        }
        let mut codewords = byte_mode_codewords(data, version);
        let (ec_len, groups) = BLOCKS_M[usize::from(version - 1)];
        let mut all = with_error_correction(&codewords, ec_len, &groups);
        let symbol = QrCode::from_codewords(version, &all, mask);
        wipe(&mut codewords);
        wipe(&mut all);
        symbol
    }

    /// The symbol of codewords already made (data and error correction, interleaved).
    fn from_codewords(version: u8, codewords: &[u8], mask: Option<u8>) -> Result<QrCode, QrError> {
        if mask.is_some_and(|m| m > 7) {
            return Err(QrError::Unsupported("masks other than 0 to 7"));
        }
        let mut grid = Grid::with_function_patterns(version);
        for (bit, (x, y)) in placement_order(&grid).into_iter().enumerate() {
            let dark = codewords
                .get(bit / 8)
                .is_some_and(|byte| (byte >> (7 - bit % 8)) & 1 == 1);
            grid.dark[y * grid.size + x] = dark;
        }
        let mask = match mask {
            Some(mask) => mask,
            None => {
                let mut best = (u32::MAX, 0);
                for mask in 0..8 {
                    grid.apply_mask(mask);
                    grid.draw_format(mask);
                    let score = grid.penalty();
                    if score < best.0 {
                        best = (score, mask);
                    }
                    grid.apply_mask(mask);
                }
                best.1
            }
        };
        grid.apply_mask(mask);
        grid.draw_format(mask);
        let size = grid.size;
        let modules = std::mem::take(&mut grid.dark);
        Ok(QrCode {
            version,
            size,
            mask,
            modules,
        })
    }

    #[must_use]
    pub fn version(&self) -> u8 {
        self.version
    }

    /// Modules on a side (`17 + 4 x version`), without the quiet zone (four light modules
    /// around it).
    #[must_use]
    pub fn size(&self) -> usize {
        self.size
    }

    /// The mask pattern (0 to 7) the format information names.
    #[must_use]
    pub fn mask(&self) -> u8 {
        self.mask
    }

    /// Whether the module in column `x`, row `y` is dark (outside the symbol: light).
    #[must_use]
    pub fn is_dark(&self, x: usize, y: usize) -> bool {
        x < self.size && y < self.size && self.modules[y * self.size + x]
    }

    /// Row `y`'s dark runs, each as (first column, modules): what a drawing makes boxes of.
    #[must_use]
    pub fn dark_runs(&self, y: usize) -> Vec<(usize, usize)> {
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for x in 0..self.size {
            if !self.is_dark(x, y) {
                continue;
            }
            match runs.last_mut() {
                Some((start, len)) if *start + *len == x => *len += 1,
                _ => runs.push((x, 1)),
            }
        }
        runs
    }

    /// The modules as text, a line per row: `dark` and `light` characters.
    #[must_use]
    pub fn to_text(&self, dark: char, light: char) -> String {
        let mut text = String::with_capacity(self.size * (self.size + 1));
        for y in 0..self.size {
            if y > 0 {
                text.push('\n');
            }
            for x in 0..self.size {
                text.push(if self.is_dark(x, y) { dark } else { light });
            }
        }
        text
    }
}

impl fmt::Debug for QrCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "QrCode({}-M, {}x{})", self.version, self.size, self.size)
    }
}

impl Drop for QrCode {
    fn drop(&mut self) {
        wipe(&mut self.modules);
    }
}

/// Clears `values` (what a symbol of a secret leaves in memory).
fn wipe<T: Default + Copy>(values: &mut [T]) {
    values.fill(T::default());
    let _ = std::hint::black_box(values);
}

fn data_codewords(version: u8) -> usize {
    let (_, groups) = BLOCKS_M[usize::from(version - 1)];
    groups.iter().map(|(blocks, len)| blocks * len).sum()
}

/// Bits of the byte count in byte mode.
fn count_bits(version: u8) -> usize {
    if version < 10 {
        8
    } else {
        16
    }
}

fn push_bits(bits: &mut Vec<bool>, value: u32, count: usize) {
    for i in (0..count).rev() {
        bits.push((value >> i) & 1 == 1);
    }
}

/// Step 1: the data codewords of `data` (at most the version's capacity) in byte mode,
/// padded to the version's data codewords.
fn byte_mode_codewords(data: &[u8], version: u8) -> Vec<u8> {
    let total = data_codewords(version);
    let mut bits: Vec<bool> = Vec::with_capacity(total * 8);
    push_bits(&mut bits, 0b0100, 4);
    push_bits(&mut bits, data.len() as u32, count_bits(version));
    for &byte in data {
        push_bits(&mut bits, u32::from(byte), 8);
    }
    let terminator = (total * 8 - bits.len()).min(4);
    push_bits(&mut bits, 0, terminator);
    let to_byte = (8 - bits.len() % 8) % 8;
    push_bits(&mut bits, 0, to_byte);
    let mut out: Vec<u8> = bits
        .chunks(8)
        .map(|byte| {
            byte.iter()
                .fold(0u8, |acc, &bit| (acc << 1) | u8::from(bit))
        })
        .collect();
    wipe(&mut bits);
    let mut pad = 0;
    while out.len() < total {
        out.push(PAD[pad % 2]);
        pad += 1;
    }
    out
}

/// The product of `x` and `y` in GF(256) with the polynomial `0x11D`, bit by bit (no table
/// indexed by a secret).
fn gf_mul(x: u8, y: u8) -> u8 {
    let mut z: u8 = 0;
    for i in (0..8).rev() {
        z = (z << 1) ^ ((z >> 7) * 0x1D);
        z ^= ((y >> i) & 1) * x;
    }
    z
}

/// The Reed-Solomon generator polynomial of `degree`, its leading 1 left out, highest power
/// first: the product of `(x - 2^i)` for `i` below `degree`.
fn rs_divisor(degree: usize) -> Vec<u8> {
    let mut result = vec![0u8; degree];
    result[degree - 1] = 1;
    let mut root: u8 = 1;
    for _ in 0..degree {
        for j in 0..degree {
            result[j] = gf_mul(result[j], root);
            if j + 1 < degree {
                result[j] ^= result[j + 1];
            }
        }
        root = gf_mul(root, 0x02);
    }
    result
}

/// The error correction codewords of `data`: the remainder of its polynomial divided by the
/// generator.
fn rs_remainder(data: &[u8], divisor: &[u8]) -> Vec<u8> {
    let mut result = vec![0u8; divisor.len()];
    for &byte in data {
        let factor = byte ^ result.remove(0);
        result.push(0);
        for (r, &coefficient) in result.iter_mut().zip(divisor) {
            *r ^= gf_mul(coefficient, factor);
        }
    }
    result
}

/// Step 2: the data codewords split into the blocks of `groups` ((blocks, codewords of each)),
/// each block's `ec_len` error correction codewords made, then the data and the error
/// correction codewords interleaved block by block.
fn with_error_correction(data: &[u8], ec_len: usize, groups: &[(usize, usize)]) -> Vec<u8> {
    let divisor = rs_divisor(ec_len);
    let mut blocks: Vec<&[u8]> = Vec::new();
    let mut at = 0;
    for &(count, len) in groups {
        for _ in 0..count {
            blocks.push(&data[at..at + len]);
            at += len;
        }
    }
    let mut corrections: Vec<Vec<u8>> = blocks
        .iter()
        .map(|block| rs_remainder(block, &divisor))
        .collect();
    let longest = blocks.iter().map(|block| block.len()).max().unwrap_or(0);
    let mut out = Vec::with_capacity(data.len() + ec_len * blocks.len());
    for i in 0..longest {
        for block in &blocks {
            if let Some(&byte) = block.get(i) {
                out.push(byte);
            }
        }
    }
    for i in 0..ec_len {
        for correction in &corrections {
            out.push(correction[i]);
        }
    }
    for correction in &mut corrections {
        wipe(correction);
    }
    out
}

/// The format information of level M and `mask`: 15 bits, BCH-coded and masked (Annex C).
fn format_bits(mask: u8) -> u32 {
    let data = (LEVEL_M << 3) | u32::from(mask);
    let mut rem = data;
    for _ in 0..10 {
        rem = (rem << 1) ^ ((rem >> 9) * FORMAT_GENERATOR);
    }
    ((data << 10) | rem) ^ FORMAT_MASK
}

/// The version information of `version` (7 and up): 18 bits, BCH-coded (Annex D).
fn version_bits(version: u8) -> u32 {
    let data = u32::from(version);
    let mut rem = data;
    for _ in 0..12 {
        rem = (rem << 1) ^ ((rem >> 11) * VERSION_GENERATOR);
    }
    (data << 12) | rem
}

/// Whether mask pattern `mask` flips the module in column `x`, row `y` (Table 10).
fn masked(mask: u8, x: usize, y: usize) -> bool {
    match mask {
        0 => (x + y) % 2 == 0,
        1 => y % 2 == 0,
        2 => x % 3 == 0,
        3 => (x + y) % 3 == 0,
        4 => (x / 3 + y / 2) % 2 == 0,
        5 => x * y % 2 + x * y % 3 == 0,
        6 => (x * y % 2 + x * y % 3) % 2 == 0,
        _ => ((x + y) % 2 + x * y % 3) % 2 == 0,
    }
}

/// A symbol while it is made: the modules, and which of them are function patterns.
struct Grid {
    size: usize,
    dark: Vec<bool>,
    function: Vec<bool>,
}

impl Grid {
    /// A symbol of `version` with its function patterns drawn and its format information
    /// reserved (drawn for mask 0).
    fn with_function_patterns(version: u8) -> Grid {
        let size = 17 + 4 * usize::from(version);
        let mut grid = Grid {
            size,
            dark: vec![false; size * size],
            function: vec![false; size * size],
        };
        for i in 0..size {
            grid.set_function(6, i, i % 2 == 0);
            grid.set_function(i, 6, i % 2 == 0);
        }
        let far = size as isize - 4;
        for (cx, cy) in [(3, 3), (far, 3), (3, far)] {
            for dy in -4isize..=4 {
                for dx in -4isize..=4 {
                    let (x, y) = (cx + dx, cy + dy);
                    if (0..size as isize).contains(&x) && (0..size as isize).contains(&y) {
                        let ring = dx.abs().max(dy.abs());
                        grid.set_function(x as usize, y as usize, ring != 2 && ring != 4);
                    }
                }
            }
        }
        let centres = ALIGNMENT[usize::from(version - 1)];
        let last = centres.len().saturating_sub(1);
        for (i, &cx) in centres.iter().enumerate() {
            for (j, &cy) in centres.iter().enumerate() {
                let on_a_finder =
                    (i == 0 && j == 0) || (i == 0 && j == last) || (i == last && j == 0);
                if on_a_finder {
                    continue;
                }
                for dy in 0..5usize {
                    for dx in 0..5usize {
                        let ring = dx.abs_diff(2).max(dy.abs_diff(2));
                        grid.set_function(cx + dx - 2, cy + dy - 2, ring != 1);
                    }
                }
            }
        }
        grid.draw_format(0);
        if version >= 7 {
            let bits = version_bits(version);
            for i in 0..18usize {
                let dark = (bits >> i) & 1 == 1;
                let (a, b) = (size - 11 + i % 3, i / 3);
                grid.set_function(a, b, dark);
                grid.set_function(b, a, dark);
            }
        }
        grid
    }

    fn set_function(&mut self, x: usize, y: usize, dark: bool) {
        let at = y * self.size + x;
        self.dark[at] = dark;
        self.function[at] = true;
    }

    /// The format information of `mask`, both copies, and the dark module.
    fn draw_format(&mut self, mask: u8) {
        let bits = format_bits(mask);
        let bit = |i: usize| (bits >> i) & 1 == 1;
        let size = self.size;
        for i in 0..6 {
            self.set_function(8, i, bit(i));
        }
        self.set_function(8, 7, bit(6));
        self.set_function(8, 8, bit(7));
        self.set_function(7, 8, bit(8));
        for i in 9..15 {
            self.set_function(14 - i, 8, bit(i));
        }
        for i in 0..8 {
            self.set_function(size - 1 - i, 8, bit(i));
        }
        for i in 8..15 {
            self.set_function(8, size - 15 + i, bit(i));
        }
        self.set_function(8, size - 8, true);
    }

    /// Flips the modules `mask` covers, function patterns aside (twice: as before).
    fn apply_mask(&mut self, mask: u8) {
        for y in 0..self.size {
            for x in 0..self.size {
                let at = y * self.size + x;
                if !self.function[at] && masked(mask, x, y) {
                    self.dark[at] = !self.dark[at];
                }
            }
        }
    }

    /// The penalty score of the modules as they are (section 7.8.3).
    fn penalty(&self) -> u32 {
        let n = self.size;
        let at = |x: usize, y: usize| self.dark[y * n + x];
        let mut score: u32 = 0;
        let mut line = vec![false; n];
        for horizontal in [true, false] {
            for a in 0..n {
                for (b, module) in line.iter_mut().enumerate() {
                    *module = if horizontal { at(b, a) } else { at(a, b) };
                }
                // Rule 1: five or more of a colour in a row: 3, and 1 for each more.
                let mut run = 1;
                for i in 1..=n {
                    if i < n && line[i] == line[i - 1] {
                        run += 1;
                    } else {
                        if run >= 5 {
                            score += 3 + (run - 5) as u32;
                        }
                        run = 1;
                    }
                }
                // Rule 3: a finder-like run beside four light modules: 40.
                for i in 0..n - 10 {
                    let window = &line[i..i + 11];
                    if window == &FINDER_LIKE[..] || window.iter().rev().eq(FINDER_LIKE.iter()) {
                        score += 40;
                    }
                }
            }
        }
        // Rule 2: each 2 x 2 block of a colour: 3.
        for y in 0..n - 1 {
            for x in 0..n - 1 {
                let c = at(x, y);
                if c == at(x + 1, y) && c == at(x, y + 1) && c == at(x + 1, y + 1) {
                    score += 3;
                }
            }
        }
        // Rule 4: 10 for each whole 5 % the dark modules are off half.
        let dark = self.dark.iter().filter(|&&d| d).count();
        let total = n * n;
        let deviation = (dark * 20).abs_diff(total * 10);
        let mut steps = 0;
        while deviation > (steps + 1) * total {
            steps += 1;
        }
        score + 10 * steps as u32
    }
}

/// Step 3's order: the modules the codewords' bits go to, first to last - columns in pairs
/// from the right (the vertical timing column skipped), up and down by turns, function
/// patterns left out.
fn placement_order(grid: &Grid) -> Vec<(usize, usize)> {
    let n = grid.size;
    let mut order = Vec::new();
    let mut right = n as isize - 1;
    while right >= 1 {
        if right == 6 {
            right = 5;
        }
        let upward = ((right + 1) & 2) == 0;
        for vertical in 0..n {
            for j in 0..2 {
                let x = (right - j) as usize;
                let y = if upward { n - 1 - vertical } else { vertical };
                if !grid.function[y * n + x] {
                    order.push((x, y));
                }
            }
        }
        right -= 2;
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ISO/IEC 18004 Annex I: "01234567" in numeric mode, version 1-M, mask pattern 2.
    const ANNEX_I_DATA: [u8; 16] = [
        0x10, 0x20, 0x0C, 0x56, 0x61, 0x80, 0xEC, 0x11, 0xEC, 0x11, 0xEC, 0x11, 0xEC, 0x11, 0xEC,
        0x11,
    ];
    const ANNEX_I_EC: [u8; 10] = [0xA5, 0x24, 0xD4, 0xC1, 0xED, 0x36, 0xC7, 0x87, 0x2C, 0x55];
    const ANNEX_I_SYMBOL: &str = "\
#######..#.##.#######
#.....#..####.#.....#
#.###.#.#.....#.###.#
#.###.#.##....#.###.#
#.###.#.#.###.#.###.#
#.....#.#...#.#.....#
#######.#.#.#.#######
........#..##........
#.#####..#..#.#####..
...#.#.##.#.#..#.##..
..#...##.#.#.#..#####
....#....#.....####..
...######..#.#..#....
........#.#####..##..
#######..##.#.##.....
#.....#.#.#####...#.#
#.###.#.#...#..#.##..
#.###.#.##..#..#.....
#.###.#.#.##.#..#.#..
#.....#........##.##.
#######.####.#..#.#..";

    /// A deterministic source of test bytes (SplitMix64).
    struct Bytes(u64);

    impl Bytes {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        fn fill(&mut self, len: usize) -> Vec<u8> {
            (0..len).map(|_| self.next() as u8).collect()
        }
    }

    /// The value of polynomial `codewords` (highest power first) at `x`, in GF(256).
    fn evaluate(codewords: &[u8], x: u8) -> u8 {
        codewords.iter().fold(0u8, |acc, &c| gf_mul(acc, x) ^ c)
    }

    /// The codewords a reader takes off `symbol`: its modules in placement order, unmasked.
    fn read_codewords(symbol: &QrCode) -> Vec<u8> {
        let grid = Grid::with_function_patterns(symbol.version());
        let bits: Vec<bool> = placement_order(&grid)
            .into_iter()
            .map(|(x, y)| symbol.is_dark(x, y) ^ masked(symbol.mask(), x, y))
            .collect();
        bits.chunks_exact(8)
            .map(|byte| {
                byte.iter()
                    .fold(0u8, |acc, &bit| (acc << 1) | u8::from(bit))
            })
            .collect()
    }

    /// The blocks (data codewords, error correction codewords) of a version's interleaved
    /// codewords.
    fn blocks_of(version: u8, codewords: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
        let (ec_len, groups) = BLOCKS_M[usize::from(version - 1)];
        let lens: Vec<usize> = groups
            .iter()
            .flat_map(|&(count, len)| std::iter::repeat_n(len, count))
            .collect();
        let mut blocks: Vec<(Vec<u8>, Vec<u8>)> =
            lens.iter().map(|_| (Vec::new(), Vec::new())).collect();
        let mut at = 0;
        let longest = lens.iter().copied().max().unwrap();
        for i in 0..longest {
            for (b, len) in lens.iter().enumerate() {
                if i < *len {
                    blocks[b].0.push(codewords[at]);
                    at += 1;
                }
            }
        }
        for _ in 0..ec_len {
            for block in &mut blocks {
                block.1.push(codewords[at]);
                at += 1;
            }
        }
        blocks
    }

    /// The bytes of byte-mode data codewords: the mode, the count, the bytes.
    fn bytes_of(version: u8, data: &[u8]) -> Vec<u8> {
        let bits: Vec<bool> = data
            .iter()
            .flat_map(|byte| (0..8).rev().map(move |i| (byte >> i) & 1 == 1))
            .collect();
        let number = |from: usize, len: usize| {
            bits[from..from + len]
                .iter()
                .fold(0usize, |acc, &bit| (acc << 1) | usize::from(bit))
        };
        assert_eq!(number(0, 4), 0b0100, "byte mode");
        let count_len = count_bits(version);
        let count = number(4, count_len);
        (0..count)
            .map(|i| number(4 + count_len + 8 * i, 8) as u8)
            .collect()
    }

    #[test]
    fn the_standards_annex_i_symbol_comes_out_module_for_module() {
        let codewords = with_error_correction(&ANNEX_I_DATA, 10, &[(1, 16)]);
        assert_eq!(
            &codewords[16..],
            &ANNEX_I_EC[..],
            "Annex I's error correction codewords"
        );
        let symbol = QrCode::from_codewords(1, &codewords, Some(2)).unwrap();
        assert_eq!(symbol.to_text('#', '.'), ANNEX_I_SYMBOL);
        assert_eq!((symbol.version(), symbol.size(), symbol.mask()), (1, 21, 2));
    }

    #[test]
    fn blocks_interleave_as_the_standard_lays_them_out() {
        // Version 5-Q: two blocks of 15 data codewords, two of 16, 18 error correction
        // codewords each.
        let data = b"CUF\x86W&U\xc2w2\x06\x12\x06g&\xf6\xf6B\x07v\x86\xf2\x07&V\x16\xc6\xc7\x92\x06\
                     \xb6\xe6\xf7w2\x07v\x86W&R\x06\x86\x972\x07F\xf7vV\xc2\x06\x972\x10\xec\x11\xec\
                     \x11\xec\x11\xec";
        let blocks = b"C\xf6\xb6FU\xf6\xe6\xf7FB\xf7v\x86\x07wVWv2\xc2&\x86\x07\x06U\xf2v\
                       \x97\xc2\x07\x862w&W\x102V&\xec\x06\x16R\x11\x12\xc6\x06\xec\x06\
                       \xc7\x86\x11g\x92\x97\xec&\x062\x11\x07\xec";
        let corrections =
            b"\xd5W\x94\xeb\xc7\xcct\x9f\x0b`\xb1\x05-<\xd4\xads\xcaL\x18\xf7\xb6\x85\
                            \x93\xf1|K;\xdf\x9d\xf2!\xe5\xc8\xeej\xf8\x86L(\x9a\x1b\xc3\xffu\x81\
                            \xe6\xac\x9a\xd1\xbdRo\x11\n\x02V\xa3l\x83\xa1\xa3\xf0 ox\xc0\xb2'\x85\
                            \x8d\xec";
        let out = with_error_correction(data, 18, &[(2, 15), (2, 16)]);
        assert_eq!(&out[..data.len()], &blocks[..]);
        assert_eq!(&out[data.len()..], &corrections[..]);
    }

    #[test]
    fn byte_mode_is_mode_count_bytes_terminator_and_pad_codewords() {
        assert_eq!(
            byte_mode_codewords(b"\x12\x34\x56\x78\x9a\xbc\xde\xf0", 1),
            vec![
                0x40, 0x81, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x00, 0xEC, 0x11, 0xEC, 0x11,
                0xEC, 0x11
            ]
        );
        // Version 10 counts in 16 bits.
        let ten = byte_mode_codewords(b"A", 10);
        assert_eq!(&ten[..4], &[0x40, 0x00, 0x14, 0x10]);
        assert_eq!(ten.len(), 216);
    }

    #[test]
    fn format_and_version_information_are_the_standards_bch_codes() {
        // Annex C: level M, mask 101.
        assert_eq!(format_bits(5), 0b100_0000_1100_1110);
        // Annex D (Table D.1): versions 7 to 10.
        assert_eq!(
            [7, 8, 9, 10].map(version_bits),
            [0x07C94, 0x085BC, 0x09A99, 0x0A4D3]
        );
    }

    #[test]
    fn each_version_holds_its_capacity_and_the_smallest_one_is_chosen() {
        let capacities: Vec<usize> = (1..=10).map(capacity).collect();
        assert_eq!(capacities, [14, 26, 42, 62, 84, 106, 122, 152, 180, 213]);
        assert_eq!(QrCode::encode(&[b'a'; 14]).unwrap().version(), 1);
        assert_eq!(QrCode::encode(&[b'a'; 15]).unwrap().version(), 2);
        assert_eq!(QrCode::encode(&[b'a'; 213]).unwrap().version(), 10);
        assert_eq!(
            QrCode::encode(&[b'a'; 214]).unwrap_err(),
            QrError::TooLong { bytes: 214 }
        );
        assert!(QrCode::encode_with(b"too long for one", 1, None).is_err());
        assert!(QrCode::encode_with(b"x", 11, None).is_err());
        assert!(QrCode::encode_with(b"x", 1, Some(8)).is_err());
    }

    #[test]
    fn a_recovery_code_is_a_version_3_symbol() {
        let symbol = QrCode::encode(b"7ZK2M-QD4XW-B9N3T-HFJ6R-0PVC8A").unwrap();
        assert_eq!((symbol.version(), symbol.size()), (3, 29));
        assert_eq!(
            format!("{symbol:?}"),
            "QrCode(3-M, 29x29)",
            "no module printed"
        );
    }

    #[test]
    fn every_version_reads_back_to_its_bytes_with_zero_syndromes() {
        let mut source = Bytes(18004);
        for version in MIN_VERSION..=MAX_VERSION {
            for len in [0, 1, capacity(version) / 2, capacity(version)] {
                let data = source.fill(len);
                let symbol = QrCode::encode_with(&data, version, None).unwrap();
                assert_eq!(symbol.size(), 17 + 4 * usize::from(version));
                let codewords = read_codewords(&symbol);
                let (ec_len, _) = BLOCKS_M[usize::from(version - 1)];
                let mut joined = Vec::new();
                for (data_part, correction) in blocks_of(version, &codewords) {
                    let whole: Vec<u8> = data_part.iter().chain(&correction).copied().collect();
                    let mut root: u8 = 1;
                    for _ in 0..ec_len {
                        assert_eq!(evaluate(&whole, root), 0, "a syndrome of version {version}");
                        root = gf_mul(root, 2);
                    }
                    joined.extend(data_part);
                }
                assert_eq!(
                    bytes_of(version, &joined),
                    data,
                    "version {version}, {len} bytes"
                );
            }
        }
    }

    #[test]
    fn every_symbol_has_its_finders_timing_dark_module_and_format_twice() {
        let mut source = Bytes(7);
        for version in MIN_VERSION..=MAX_VERSION {
            let symbol =
                QrCode::encode_with(&source.fill(capacity(version)), version, None).unwrap();
            let n = symbol.size();
            for (cx, cy) in [(3, 3), (n - 4, 3), (3, n - 4)] {
                assert!(symbol.is_dark(cx, cy) && !symbol.is_dark(cx - 2, cy));
                assert!(symbol.is_dark(cx - 3, cy - 3) && symbol.is_dark(cx + 3, cy + 3));
            }
            for i in 8..n - 8 {
                assert_eq!(symbol.is_dark(i, 6), i % 2 == 0, "the timing row");
                assert_eq!(symbol.is_dark(6, i), i % 2 == 0, "the timing column");
            }
            assert!(symbol.is_dark(8, n - 8), "the dark module");
            let first: u32 = (0..15).fold(0, |acc, i| {
                let (x, y) = match i {
                    0..=5 => (8, i),
                    6 => (8, 7),
                    7 => (8, 8),
                    8 => (7, 8),
                    _ => (14 - i, 8),
                };
                acc | (u32::from(symbol.is_dark(x, y)) << i)
            });
            let second: u32 = (0..15).fold(0, |acc, i| {
                let (x, y) = if i < 8 {
                    (n - 1 - i, 8)
                } else {
                    (8, n - 15 + i)
                };
                acc | (u32::from(symbol.is_dark(x, y)) << i)
            });
            assert_eq!(first, second, "both copies of the format information");
            assert_eq!(first, format_bits(symbol.mask()));
        }
    }

    #[test]
    fn the_mask_chosen_has_the_lowest_penalty() {
        let data = b"7ZK2M-QD4XW-B9N3T-HFJ6R-0PVC8A";
        let chosen = QrCode::encode(data).unwrap();
        let score = |symbol: &QrCode| {
            let mut grid = Grid::with_function_patterns(symbol.version());
            grid.dark.clone_from(&symbol.modules);
            grid.penalty()
        };
        for mask in 0..8 {
            let other = QrCode::encode_with(data, chosen.version(), Some(mask)).unwrap();
            assert!(score(&chosen) <= score(&other), "mask {mask}");
        }
    }

    #[test]
    fn a_symbols_dark_runs_cover_exactly_its_dark_modules() {
        let symbol = QrCode::encode(b"https://example.com/").unwrap();
        for y in 0..symbol.size() {
            let mut row = vec![false; symbol.size()];
            for (start, len) in symbol.dark_runs(y) {
                for module in &mut row[start..start + len] {
                    assert!(!*module, "runs do not overlap");
                    *module = true;
                }
            }
            let expected: Vec<bool> = (0..symbol.size()).map(|x| symbol.is_dark(x, y)).collect();
            assert_eq!(row, expected);
        }
        assert!(!symbol.is_dark(symbol.size(), 0), "outside: light");
    }
}
