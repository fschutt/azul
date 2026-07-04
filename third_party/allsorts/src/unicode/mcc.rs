use unicode_canonical_combining_class::get_canonical_combining_class;

/// An enumeration of the Unicode
/// [Canonical_Combining_Class values](http://www.unicode.org/reports/tr44/#Canonical_Combining_Class_Values),
/// with the following modifications:
///
/// * Remove: `CCC84`, `CCC91`, `CCC103`.
/// * Add: `CCC3`, `CCC4`, `CCC5`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum ModifiedCombiningClass {
    NotReordered = 0,
    Overlay = 1,
    CCC3 = 3,
    CCC4 = 4,
    CCC5 = 5,
    HanReading = 6,
    Nukta = 7,
    KanaVoicing = 8,
    Virama = 9,
    CCC10 = 10,
    CCC11 = 11,
    CCC12 = 12,
    CCC13 = 13,
    CCC14 = 14,
    CCC15 = 15,
    CCC16 = 16,
    CCC17 = 17,
    CCC18 = 18,
    CCC19 = 19,
    CCC20 = 20,
    CCC21 = 21,
    CCC22 = 22,
    CCC23 = 23,
    CCC24 = 24,
    CCC25 = 25,
    CCC26 = 26,
    CCC27 = 27,
    CCC28 = 28,
    CCC29 = 29,
    CCC30 = 30,
    CCC31 = 31,
    CCC32 = 32,
    CCC33 = 33,
    CCC34 = 34,
    CCC35 = 35,
    CCC36 = 36,
    CCC107 = 107,
    CCC118 = 118,
    CCC122 = 122,
    CCC129 = 129,
    CCC130 = 130,
    CCC132 = 132,
    AttachedBelow = 202,
    AttachedAbove = 214,
    AttachedAboveRight = 216,
    BelowLeft = 218,
    Below = 220,
    BelowRight = 222,
    Left = 224,
    Right = 226,
    AboveLeft = 228,
    Above = 230,
    AboveRight = 232,
    DoubleBelow = 233,
    DoubleAbove = 234,
    IotaSubscript = 240,
}

const X: ModifiedCombiningClass = ModifiedCombiningClass::NotReordered;
use ModifiedCombiningClass::*;
const MODIFIED_COMBINING_CLASS: &[ModifiedCombiningClass; 256] = &[
    NotReordered, // NotReordered
    Overlay,      // Overlay
    X,            // CCC2
    X,            // CCC3
    X,            // CCC4
    X,            // CCC5
    HanReading,   // HanReading
    Nukta,        // Nukta
    KanaVoicing,  // KanaVoicing
    Virama,       // Virama
    // Hebrew
    // Reordered in accordance with the SBL Hebrew Font User Manual:
    // https://www.sbl-site.org/Fonts/SBLHebrewUserManual1.5x.pdf.
    CCC22, // CCC10
    CCC15, // CCC11
    CCC16, // CCC12
    CCC17, // CCC13
    CCC23, // CCC14
    CCC18, // CCC15
    CCC19, // CCC16
    CCC20, // CCC17
    CCC21, // CCC18
    CCC14, // CCC19
    CCC24, // CCC20
    CCC12, // CCC21
    CCC25, // CCC22
    CCC13, // CCC23
    CCC10, // CCC24
    CCC11, // CCC25
    CCC26, // CCC26
    // Arabic
    CCC27, // CCC27
    CCC28, // CCC28
    CCC29, // CCC29
    CCC30, // CCC30
    CCC31, // CCC31
    CCC32, // CCC32
    CCC33, // CCC33
    CCC34, // CCC34
    CCC35, // CCC35
    // Syriac
    CCC36, // CCC36
    X,     // CCC37
    X,     // CCC38
    X,     // CCC39
    X,     // CCC40
    X,     // CCC41
    X,     // CCC42
    X,     // CCC43
    X,     // CCC44
    X,     // CCC45
    X,     // CCC46
    X,     // CCC47
    X,     // CCC48
    X,     // CCC49
    X,     // CCC50
    X,     // CCC51
    X,     // CCC52
    X,     // CCC53
    X,     // CCC54
    X,     // CCC55
    X,     // CCC56
    X,     // CCC57
    X,     // CCC58
    X,     // CCC59
    X,     // CCC60
    X,     // CCC61
    X,     // CCC62
    X,     // CCC63
    X,     // CCC64
    X,     // CCC65
    X,     // CCC66
    X,     // CCC67
    X,     // CCC68
    X,     // CCC69
    X,     // CCC70
    X,     // CCC71
    X,     // CCC72
    X,     // CCC73
    X,     // CCC74
    X,     // CCC75
    X,     // CCC76
    X,     // CCC77
    X,     // CCC78
    X,     // CCC79
    X,     // CCC80
    X,     // CCC81
    X,     // CCC82
    X,     // CCC83
    // Telugu
    // Map `CCC84` and `CCC91` to the otherwise unassigned `CCC4` and `CCC5` values. If
    // left as-is, the Telugu length marks U+0C55 and U+0C56 have the undesirable effect
    // of being reordered after a Halant.
    //
    // Test case: `"\u{0C15}\u{0C4D}\u{0C56}"` should not produce a dotted circle.
    CCC4, // CCC84
    X,    // CCC85
    X,    // CCC86
    X,    // CCC87
    X,    // CCC88
    X,    // CCC89
    X,    // CCC90
    CCC5, // CCC91
    X,    // CCC92
    X,    // CCC93
    X,    // CCC94
    X,    // CCC95
    X,    // CCC96
    X,    // CCC97
    X,    // CCC98
    X,    // CCC99
    X,    // CCC100
    X,    // CCC101
    X,    // CCC102
    // Thai
    // Map `CCC103` to the otherwise unassigned `CCC3` value. If left as-is, the Thai marks
    // U+0E38 and U+0E39 have the undesirable effect of being reordered after a Phinthu.
    CCC3,   // CCC103
    X,      // CCC104
    X,      // CCC105
    X,      // CCC106
    CCC107, // CCC107
    X,      // CCC108
    X,      // CCC109
    X,      // CCC110
    X,      // CCC111
    X,      // CCC112
    X,      // CCC113
    X,      // CCC114
    X,      // CCC115
    X,      // CCC116
    X,      // CCC117
    // Lao
    CCC118, // CCC118
    X,      // CCC119
    X,      // CCC120
    X,      // CCC121
    CCC122, // CCC122
    X,      // CCC123
    X,      // CCC124
    X,      // CCC125
    X,      // CCC126
    X,      // CCC127
    X,      // CCC128
    // Tibetan
    CCC129,             // CCC129
    CCC130,             // CCC130
    X,                  // CCC131
    CCC132,             // CCC132
    X,                  // CCC133
    X,                  // CCC134
    X,                  // CCC135
    X,                  // CCC136
    X,                  // CCC137
    X,                  // CCC138
    X,                  // CCC139
    X,                  // CCC140
    X,                  // CCC141
    X,                  // CCC142
    X,                  // CCC143
    X,                  // CCC144
    X,                  // CCC145
    X,                  // CCC146
    X,                  // CCC147
    X,                  // CCC148
    X,                  // CCC149
    X,                  // CCC150
    X,                  // CCC151
    X,                  // CCC152
    X,                  // CCC153
    X,                  // CCC154
    X,                  // CCC155
    X,                  // CCC156
    X,                  // CCC157
    X,                  // CCC158
    X,                  // CCC159
    X,                  // CCC160
    X,                  // CCC161
    X,                  // CCC162
    X,                  // CCC163
    X,                  // CCC164
    X,                  // CCC165
    X,                  // CCC166
    X,                  // CCC167
    X,                  // CCC168
    X,                  // CCC169
    X,                  // CCC170
    X,                  // CCC171
    X,                  // CCC172
    X,                  // CCC173
    X,                  // CCC174
    X,                  // CCC175
    X,                  // CCC176
    X,                  // CCC177
    X,                  // CCC178
    X,                  // CCC179
    X,                  // CCC180
    X,                  // CCC181
    X,                  // CCC182
    X,                  // CCC183
    X,                  // CCC184
    X,                  // CCC185
    X,                  // CCC186
    X,                  // CCC187
    X,                  // CCC188
    X,                  // CCC189
    X,                  // CCC190
    X,                  // CCC191
    X,                  // CCC192
    X,                  // CCC193
    X,                  // CCC194
    X,                  // CCC195
    X,                  // CCC196
    X,                  // CCC197
    X,                  // CCC198
    X,                  // CCC199
    X,                  // CCC200
    X,                  // CCC201
    AttachedBelow,      // AttachedBelow
    X,                  // CCC203
    X,                  // CCC204
    X,                  // CCC205
    X,                  // CCC206
    X,                  // CCC207
    X,                  // CCC208
    X,                  // CCC209
    X,                  // CCC210
    X,                  // CCC211
    X,                  // CCC212
    X,                  // CCC213
    AttachedAbove,      // AttachedAbove
    X,                  // CCC215
    AttachedAboveRight, // AttachedAboveRight
    X,                  // CCC217
    BelowLeft,          // BelowLeft
    X,                  // CCC219
    Below,              // Below
    X,                  // CCC221
    BelowRight,         // BelowRight
    X,                  // CCC223
    Left,               // Left
    X,                  // CCC225
    Right,              // Right
    X,                  // CCC227
    AboveLeft,          // AboveLeft
    X,                  // CCC229
    Above,              // Above
    X,                  // CCC231
    AboveRight,         // AboveRight
    DoubleBelow,        // DoubleBelow
    DoubleAbove,        // DoubleAbove
    X,                  // CCC235
    X,                  // CCC236
    X,                  // CCC237
    X,                  // CCC238
    X,                  // CCC239
    IotaSubscript,      // IotaSubscript
    X,                  // CCC241
    X,                  // CCC242
    X,                  // CCC243
    X,                  // CCC244
    X,                  // CCC245
    X,                  // CCC246
    X,                  // CCC247
    X,                  // CCC248
    X,                  // CCC249
    X,                  // CCC250
    X,                  // CCC251
    X,                  // CCC252
    X,                  // CCC253
    X,                  // CCC254
    X,                  // CCC255
];

/// Returns the modified combining class value of a `char`. Retrieves the _canonical_ combining
/// class value, then maps it to its corresponding _modified_ value.
pub fn modified_combining_class(c: char) -> ModifiedCombiningClass {
    if c <= '\u{02FF}' {
        // Fast path, primarily for Latin. None of the code points in:
        //     U+0000..U+007F | Basic Latin
        //     U+0080..U+00FF | Latin-1 Supplement
        //     U+0100..U+017F | Latin Extended-A
        //     U+0180..U+024F | Latin Extended-B
        //     U+0250..U+02AF | IPA Extensions
        //     U+02B0..U+02FF | Spacing Modifier Letters
        // are reordering marks.
        ModifiedCombiningClass::NotReordered
    } else {
        MODIFIED_COMBINING_CLASS[get_canonical_combining_class(c) as usize]
    }
}

/// Sorts sub-slices of non-starter `char`s (i.e. `char`s with non-zero combining class values) by
/// their modified combining class values. This sort is stable.
pub fn sort_by_modified_combining_class(cs: &mut [char]) {
    for css in
        cs.split_mut(|&c| modified_combining_class(c) == ModifiedCombiningClass::NotReordered)
    {
        css.sort_by_key(|&c| modified_combining_class(c));
    }
}
