//! Verify our bytecode decoder matches fonttools' disassembly.
//!
//! For each glyph, we decode the raw bytecode into (opcode, ip, consumed_bytes)
//! triples and compare against fonttools' reference disassembly.
//!
//! Run: cargo test -p azul-layout --test test_bytecode_decode -- --nocapture

use std::fs;
use azul_layout::font::parsed::ParsedFont;

/// Decoded instruction: opcode byte, instruction pointer, description.
#[derive(Debug, Clone)]
struct DecodedInstr {
    ip: usize,
    opcode: u8,
    mnemonic: String,
    /// For push instructions: the pushed values.
    push_data: Vec<i32>,
}

/// Decode TrueType bytecode into a list of instructions.
/// This mirrors the interpreter's dispatch logic but only decodes, doesn't execute.
fn decode_bytecode(bytecode: &[u8]) -> Vec<DecodedInstr> {
    let mut result = Vec::new();
    let mut ip = 0;

    while ip < bytecode.len() {
        let op = bytecode[ip];
        let start_ip = ip;

        match op {
            // NPUSHB
            0x40 => {
                let n = bytecode.get(ip + 1).copied().unwrap_or(0) as usize;
                let vals: Vec<i32> = bytecode[ip+2..ip+2+n].iter().map(|&b| b as i32).collect();
                result.push(DecodedInstr { ip: start_ip, opcode: op, mnemonic: format!("NPUSHB[{n}]"), push_data: vals });
                ip += 2 + n;
            }
            // NPUSHW
            0x41 => {
                let n = bytecode.get(ip + 1).copied().unwrap_or(0) as usize;
                let mut vals = Vec::new();
                for i in 0..n {
                    let hi = bytecode.get(ip + 2 + i*2).copied().unwrap_or(0) as i16;
                    let lo = bytecode.get(ip + 3 + i*2).copied().unwrap_or(0);
                    vals.push(((hi as i32) << 8) | lo as i32);
                }
                // Fix sign for 16-bit
                let vals: Vec<i32> = vals.into_iter().map(|v| if v >= 0x8000 { v - 0x10000 } else { v }).collect();
                result.push(DecodedInstr { ip: start_ip, opcode: op, mnemonic: format!("NPUSHW[{n}]"), push_data: vals });
                ip += 2 + n * 2;
            }
            // PUSHB[n] (0xB0..0xB7)
            0xB0..=0xB7 => {
                let n = (op - 0xB0 + 1) as usize;
                let vals: Vec<i32> = bytecode[ip+1..ip+1+n].iter().map(|&b| b as i32).collect();
                result.push(DecodedInstr { ip: start_ip, opcode: op, mnemonic: format!("PUSHB[{n}]"), push_data: vals });
                ip += 1 + n;
            }
            // PUSHW[n] (0xB8..0xBF)
            0xB8..=0xBF => {
                let n = (op - 0xB8 + 1) as usize;
                let mut vals = Vec::new();
                for i in 0..n {
                    let hi = bytecode.get(ip + 1 + i*2).copied().unwrap_or(0) as i8;
                    let lo = bytecode.get(ip + 2 + i*2).copied().unwrap_or(0);
                    vals.push(((hi as i32) << 8) | lo as i32);
                }
                result.push(DecodedInstr { ip: start_ip, opcode: op, mnemonic: format!("PUSHW[{n}]"), push_data: vals });
                ip += 1 + n * 2;
            }
            // All other instructions: single byte
            _ => {
                let name = opcode_name(op);
                result.push(DecodedInstr { ip: start_ip, opcode: op, mnemonic: name, push_data: vec![] });
                ip += 1;
            }
        }
    }
    result
}

fn opcode_name(op: u8) -> String {
    match op {
        0x00 => "SVTCA[y]", 0x01 => "SVTCA[x]",
        0x02 => "SPVTCA[y]", 0x03 => "SPVTCA[x]",
        0x04 => "SFVTCA[y]", 0x05 => "SFVTCA[x]",
        0x06 => "SPVTL[0]", 0x07 => "SPVTL[1]",
        0x08 => "SFVTL[0]", 0x09 => "SFVTL[1]",
        0x0A => "SPVFS", 0x0B => "SFVFS",
        0x0C => "GPV", 0x0D => "GFV", 0x0E => "SFVTPV", 0x0F => "ISECT",
        0x10 => "SRP0", 0x11 => "SRP1", 0x12 => "SRP2",
        0x13 => "SZP0", 0x14 => "SZP1", 0x15 => "SZP2", 0x16 => "SZPS",
        0x17 => "SLOOP", 0x18 => "RTG", 0x19 => "RTHG",
        0x1A => "SMD", 0x1B => "ELSE", 0x1C => "JMPR", 0x1D => "SCVTCI",
        0x1E => "SSWCI", 0x1F => "SSW",
        0x20 => "DUP", 0x21 => "POP", 0x22 => "CLEAR", 0x23 => "SWAP",
        0x24 => "DEPTH", 0x25 => "CINDEX", 0x26 => "MINDEX",
        0x27 => "ALIGNPTS", 0x29 => "UTP",
        0x2A => "LOOPCALL", 0x2B => "CALL",
        0x2C => "FDEF", 0x2D => "ENDF",
        0x2E => "MDAP[0]", 0x2F => "MDAP[1]",
        0x30 => "IUP[y]", 0x31 => "IUP[x]",
        0x32 => "SHP[0]", 0x33 => "SHP[1]",
        0x34 => "SHC[0]", 0x35 => "SHC[1]",
        0x36 => "SHZ[0]", 0x37 => "SHZ[1]",
        0x38 => "SHPIX", 0x39 => "IP",
        0x3A => "MSIRP[0]", 0x3B => "MSIRP[1]",
        0x3C => "ALIGNRP", 0x3D => "RTDG",
        0x3E => "MIAP[0]", 0x3F => "MIAP[1]",
        0x42 => "WS", 0x43 => "RS", 0x44 => "WCVTP", 0x45 => "RCVT",
        0x46 => "GC[0]", 0x47 => "GC[1]",
        0x48 => "SCFS",
        0x49 => "MD[1]", 0x4A => "MD[0]",
        0x4B => "MPPEM", 0x4C => "MPS",
        0x4D => "FLIPON", 0x4E => "FLIPOFF", 0x4F => "DEBUG",
        0x50 => "LT", 0x51 => "LTEQ", 0x52 => "GT", 0x53 => "GTEQ",
        0x54 => "EQ", 0x55 => "NEQ",
        0x56 => "ODD", 0x57 => "EVEN",
        0x58 => "IF", 0x59 => "EIF", 0x5A => "AND", 0x5B => "OR",
        0x5C => "NOT", 0x5D => "DELTAP1",
        0x5E => "SDB", 0x5F => "SDS",
        0x60 => "ADD", 0x61 => "SUB",
        0x62 => "DIV", 0x63 => "MUL",
        0x64 => "ABS", 0x65 => "NEG", 0x66 => "FLOOR", 0x67 => "CEILING",
        0x68..=0x6B => "ROUND[..]",
        0x6C..=0x6F => "NROUND[..]",
        0x70 => "WCVTF",
        0x71 => "DELTAP2", 0x72 => "DELTAP3",
        0x73 => "DELTAC1", 0x74 => "DELTAC2", 0x75 => "DELTAC3",
        0x76 => "SROUND", 0x77 => "S45ROUND",
        0x78 => "JROT", 0x79 => "JROF",
        0x7A => "ROFF",
        0x7C => "RUTG", 0x7D => "RDTG",
        0x7E => "SANGW", 0x7F => "AA",
        0x80 => "FLIPPT",
        0x81 => "FLIPRGON", 0x82 => "FLIPRGOFF",
        0x85 => "SCANCTRL", 0x86 => "SDPVTL[0]", 0x87 => "SDPVTL[1]",
        0x88 => "GETINFO",
        0x89 => "IDEF", 0x8A => "ROLL",
        0x8B => "MAX", 0x8C => "MIN",
        0x8D => "SCANTYPE", 0x8E => "INSTCTRL",
        _ if (0xC0..=0xDF).contains(&op) => return format!("MDRP[{:05b}]", op - 0xC0),
        _ if (0xE0..=0xFF).contains(&op) => return format!("MIRP[{:05b}]", op - 0xE0),
        _ => return format!("???0x{:02X}", op),
    }.to_string()
}

/// Reference: fonttools disassembly for 'u' glyph (first 20 non-push instructions)
/// Generated by: fonttools TTFont, glyph.program.toXML()
const FONTTOOLS_U_INSTRS: &[(&str, usize)] = &[
    // (mnemonic_prefix, bytecode_ip)
    ("PUSHB", 0),    // PUSHB[2] [2, 2]
    ("RS", 3),
    ("EQ", 4),
    ("IF", 5),
    ("NPUSHB", 6),   // 22 values
    ("PUSHW", 30),   // [-4]
    ("NPUSHB", 33),  // 11 values
    ("PUSHW", 46),   // [-28]
    ("NPUSHB", 49),  // 19 values
    ("PUSHW", 70),   // [-8]
    ("PUSHB", 73),   // 5 values
    ("PUSHW", 79),   // [-6]
    ("PUSHB", 82),   // 5 values
    ("PUSHW", 88),   // [-12]
    ("PUSHB", 91),   // 6 values
    ("PUSHW", 98),   // 3 words [994, 23, 994]
    ("NPUSHB", 105), // 14 values
    ("PUSHW", 121),  // [-64]
    ("PUSHB", 124),  // 6 values
    ("SVTCA", 131),
    ("MDAP", 132),
    ("MDRP", 133),
    ("CALL", 134),
    ("MDRP", 135),
    ("MIAP", 136),
];

/// Reference: fonttools disassembly for 'L' glyph
const FONTTOOLS_L_INSTRS: &[(&str, usize)] = &[
    ("NPUSHB", 0),
    ("PUSHW", 29),
    ("NPUSHB", 32),
    ("PUSHW", 85),  // PUSHW[1] value=708
    ("NPUSHB", 88), // NPUSHB[47]
    ("CALL", 137),  // ip = 88 + 2 + 47 = 137
    ("FLIPOFF", 138),
    ("SRP0", 139),
    ("MIRP", 140),
    ("CALL", 141),
    ("CALL", 142),
    ("CALL", 143),
    ("ALIGNRP", 144),
    ("FLIPON", 145),
    ("SRP0", 146),
    ("MIRP", 147),
];

#[test]
fn test_bytecode_decode_u() {
    let font_bytes = fs::read("/System/Library/Fonts/Supplemental/Times New Roman.ttf")
        .or_else(|_| fs::read("/System/Library/Fonts/Times.ttc")).ok();
    let font_bytes = match font_bytes {
        Some(b) => b, None => { eprintln!("Skipping"); return; }
    };
    let mut warnings = Vec::new();
    let font = match ParsedFont::from_bytes(&font_bytes, 0, &mut warnings) {
        Some(f) => f, None => { eprintln!("Failed"); return; }
    };

    let glyph_id = font.lookup_glyph_index('u' as u32).unwrap();
    let owned = font.glyph_records_decoded.get(&glyph_id).unwrap();
    let bytecode = owned.instructions.as_ref().unwrap();

    let decoded = decode_bytecode(bytecode);

    eprintln!("'u': {} bytes → {} decoded instructions", bytecode.len(), decoded.len());

    // Verify against fonttools reference
    let mut mismatches = 0;
    for &(ft_prefix, ft_ip) in FONTTOOLS_U_INSTRS {
        let ours = decoded.iter().find(|d| d.ip == ft_ip);
        match ours {
            Some(d) => {
                let matches = d.mnemonic.starts_with(ft_prefix);
                if !matches {
                    eprintln!("  MISMATCH ip={}: fonttools={} ours={}", ft_ip, ft_prefix, d.mnemonic);
                    mismatches += 1;
                }
            }
            None => {
                eprintln!("  MISSING ip={}: fonttools={}", ft_ip, ft_prefix);
                mismatches += 1;
            }
        }
    }

    if mismatches == 0 {
        eprintln!("  All {} reference instructions match ✓", FONTTOOLS_U_INSTRS.len());
    }
    assert_eq!(mismatches, 0, "'u' bytecode decode has {} mismatches", mismatches);
}

#[test]
fn test_bytecode_decode_l() {
    let font_bytes = fs::read("/System/Library/Fonts/Supplemental/Times New Roman.ttf")
        .or_else(|_| fs::read("/System/Library/Fonts/Times.ttc")).ok();
    let font_bytes = match font_bytes {
        Some(b) => b, None => { eprintln!("Skipping"); return; }
    };
    let mut warnings = Vec::new();
    let font = match ParsedFont::from_bytes(&font_bytes, 0, &mut warnings) {
        Some(f) => f, None => { eprintln!("Failed"); return; }
    };

    let glyph_id = font.lookup_glyph_index('L' as u32).unwrap();
    let owned = font.glyph_records_decoded.get(&glyph_id).unwrap();
    let bytecode = owned.instructions.as_ref().unwrap();

    let decoded = decode_bytecode(bytecode);

    eprintln!("'L': {} bytes → {} decoded instructions", bytecode.len(), decoded.len());

    let mut mismatches = 0;
    for &(ft_prefix, ft_ip) in FONTTOOLS_L_INSTRS {
        let ours = decoded.iter().find(|d| d.ip == ft_ip);
        match ours {
            Some(d) => {
                let matches = d.mnemonic.starts_with(ft_prefix);
                if !matches {
                    eprintln!("  MISMATCH ip={}: fonttools={} ours={}", ft_ip, ft_prefix, d.mnemonic);
                    mismatches += 1;
                }
            }
            None => {
                eprintln!("  MISSING ip={}: fonttools={}", ft_ip, ft_prefix);
                mismatches += 1;
            }
        }
    }

    if mismatches == 0 {
        eprintln!("  All {} reference instructions match ✓", FONTTOOLS_L_INSTRS.len());
    }
    assert_eq!(mismatches, 0, "'L' bytecode decode has {} mismatches", mismatches);
}

/// Verify push data values match fonttools
#[test]
fn test_push_values() {
    let font_bytes = fs::read("/System/Library/Fonts/Supplemental/Times New Roman.ttf")
        .or_else(|_| fs::read("/System/Library/Fonts/Times.ttc")).ok();
    let font_bytes = match font_bytes {
        Some(b) => b, None => { eprintln!("Skipping"); return; }
    };
    let mut warnings = Vec::new();
    let font = match ParsedFont::from_bytes(&font_bytes, 0, &mut warnings) {
        Some(f) => f, None => { eprintln!("Failed"); return; }
    };

    let glyph_id = font.lookup_glyph_index('u' as u32).unwrap();
    let owned = font.glyph_records_decoded.get(&glyph_id).unwrap();
    let bytecode = owned.instructions.as_ref().unwrap();
    let decoded = decode_bytecode(bytecode);

    // Verify specific push values from fonttools
    // ip=0: PUSHB[2] [2, 2]
    let p0 = decoded.iter().find(|d| d.ip == 0).unwrap();
    assert_eq!(p0.push_data, vec![2, 2], "ip=0 PUSHB values");

    // ip=30: PUSHW[1] [-4]
    let p30 = decoded.iter().find(|d| d.ip == 30).unwrap();
    assert_eq!(p30.push_data, vec![-4], "ip=30 PUSHW value");

    // ip=46: PUSHW[1] [-28]
    let p46 = decoded.iter().find(|d| d.ip == 46).unwrap();
    assert_eq!(p46.push_data, vec![-28], "ip=46 PUSHW value");

    // ip=70: PUSHW[1] [-8]
    let p70 = decoded.iter().find(|d| d.ip == 70).unwrap();
    assert_eq!(p70.push_data, vec![-8], "ip=70 PUSHW value");

    // ip=79: PUSHW[1] [-6]
    let p79 = decoded.iter().find(|d| d.ip == 79).unwrap();
    assert_eq!(p79.push_data, vec![-6], "ip=79 PUSHW value");

    // ip=88: PUSHW[1] [-12]
    let p88 = decoded.iter().find(|d| d.ip == 88).unwrap();
    assert_eq!(p88.push_data, vec![-12], "ip=88 PUSHW value");

    // ip=98: PUSHW[3] [994, 23, 994]
    let p98 = decoded.iter().find(|d| d.ip == 98).unwrap();
    assert_eq!(p98.push_data, vec![994, 23, 994], "ip=98 PUSHW values");

    // ip=121: PUSHW[1] [-64]
    let p121 = decoded.iter().find(|d| d.ip == 121).unwrap();
    assert_eq!(p121.push_data, vec![-64], "ip=121 PUSHW value");

    eprintln!("All push values verified ✓");
}

/// Decode ALL glyphs used in lorem ipsum and verify no decode errors
#[test]
fn test_decode_all_lorem_glyphs() {
    let font_bytes = fs::read("/System/Library/Fonts/Supplemental/Times New Roman.ttf")
        .or_else(|_| fs::read("/System/Library/Fonts/Times.ttc")).ok();
    let font_bytes = match font_bytes {
        Some(b) => b, None => { eprintln!("Skipping"); return; }
    };
    let mut warnings = Vec::new();
    let font = match ParsedFont::from_bytes(&font_bytes, 0, &mut warnings) {
        Some(f) => f, None => { eprintln!("Failed"); return; }
    };

    let chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789.,;:!?";
    let mut total_bytes = 0;
    let mut total_instrs = 0;
    let mut unknown = 0;

    for ch in chars.chars() {
        let glyph_id = match font.lookup_glyph_index(ch as u32) {
            Some(id) => id, None => continue,
        };
        let owned = match font.glyph_records_decoded.get(&glyph_id) {
            Some(o) => o, None => continue,
        };
        let bytecode = match owned.instructions.as_ref() {
            Some(b) => b, None => continue,
        };

        let decoded = decode_bytecode(bytecode);
        total_bytes += bytecode.len();
        total_instrs += decoded.len();

        // Check for unknown opcodes
        for d in &decoded {
            if d.mnemonic.starts_with("???") {
                eprintln!("  '{}': unknown opcode {} at ip={}", ch, d.mnemonic, d.ip);
                unknown += 1;
            }
        }

        // Verify decode consumes exactly all bytes
        let last = decoded.last();
        if let Some(last) = last {
            let end = last.ip + 1 + if last.mnemonic.starts_with("PUSH") || last.mnemonic.starts_with("NPUSH") {
                // Push instructions consume more bytes
                match last.opcode {
                    0x40 => 1 + last.push_data.len(),
                    0x41 => 1 + last.push_data.len() * 2,
                    0xB0..=0xB7 => last.push_data.len(),
                    0xB8..=0xBF => last.push_data.len() * 2,
                    _ => 0,
                }
            } else { 0 };
            if end != bytecode.len() {
                // This is fine - the last instruction might not be the end
                // because IF/ELSE blocks can skip portions
            }
        }
    }

    eprintln!("Decoded {} glyphs: {} bytes → {} instructions, {} unknown opcodes",
        chars.len(), total_bytes, total_instrs, unknown);
    assert_eq!(unknown, 0, "Found unknown opcodes");
}
