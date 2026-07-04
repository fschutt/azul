#![deny(missing_docs)]

//!  Utilities for handling the Mac OS Roman character set.

/// Returns `true` if the supplied `char` exists in the Mac OS Roman character encoding.
///
/// <https://en.wikipedia.org/wiki/Mac_OS_Roman>
pub fn is_macroman(chr: char) -> bool {
    char_to_macroman(chr).is_some()
}

/// Converts a `char` to its Mac OS Roman character encoding.
///
/// Returns `None` if the character is not part of the Mac OS Roman character set.
#[rustfmt::skip]
pub fn char_to_macroman(chr: char) -> Option<u8> {
    if (chr as u32) < 0x7F {
        Some(chr as u8)
    } else {
        match chr {
            'Ä' => Some(128), // A dieresis
            'Å' => Some(129), // A ring
            'Ç' => Some(130), // C cedilla
            'É' => Some(131), // E acute
            'Ñ' => Some(132), // N tilde
            'Ö' => Some(133), // O dieresis
            'Ü' => Some(134), // U dieresis
            'á' => Some(135), // a acute
            'à' => Some(136), // a grave
            'â' => Some(137), // a circumflex
            'ä' => Some(138), // a dieresis
            'ã' => Some(139), // a tilde
            'å' => Some(140), // a ring
            'ç' => Some(141), // c cedilla
            'é' => Some(142), // e acute
            'è' => Some(143), // e grave
            'ê' => Some(144), // e circumflex
            'ë' => Some(145), // e dieresis
            'í' => Some(146), // i acute
            'ì' => Some(147), // i grave
            'î' => Some(148), // i circumflex
            'ï' => Some(149), // i dieresis
            'ñ' => Some(150), // n tilde
            'ó' => Some(151), // o acute
            'ò' => Some(152), // o grave
            'ô' => Some(153), // o circumflex
            'ö' => Some(154), // o dieresis
            'õ' => Some(155), // o tilde
            'ú' => Some(156), // u acute
            'ù' => Some(157), // u grave
            'û' => Some(158), // u circumflex
            'ü' => Some(159), // u dieresis
            '†' => Some(160), // dagger
            '°' => Some(161), // degree
            '¢' => Some(162), // cent
            '£' => Some(163), // sterling
            '§' => Some(164), // section
            '•' => Some(165), // bullet
            '¶' => Some(166), // paragraph
            'ß' => Some(167), // German double s
            '®' => Some(168), // registered
            '©' => Some(169), // copyright
            '™' => Some(170), // trademark
            '´' => Some(171), // acute
            '¨' => Some(172), // diaeresis
            'Æ' => Some(174), // AE
            'Ø' => Some(175), // O slash
            '±' => Some(177), // plusminus
            '¥' => Some(180), // yen
            'µ' => Some(181), // micro
            'ª' => Some(187), // ordfeminine
            'º' => Some(188), // ordmasculine
            'æ' => Some(190), // ae
            'ø' => Some(191), // o slash
            '¿' => Some(192), // question down
            '¡' => Some(193), // exclamation down
            '¬' => Some(194), // not
            'ƒ' => Some(196), // florin
            '«' => Some(199), // left guille
            '»' => Some(200), // right guille
            '…' => Some(201), // ellipsis
            ' ' => Some(202), // non-breaking space
            'À' => Some(203), // A grave
            'Ã' => Some(204), // A tilde
            'Õ' => Some(205), // O tilde
            'Œ' => Some(206), // OE
            'œ' => Some(207), // oe
            '–' => Some(208), // endash
            '—' => Some(209), // emdash
            '“' => Some(210), // ldquo
            '”' => Some(211), // rdquo
            '‘' => Some(212), // lsquo
            '’' => Some(213), // rsquo
            '÷' => Some(214), // divide
            'ÿ' => Some(216), // y dieresis
            'Ÿ' => Some(217), // Y dieresis
            '⁄' => Some(218), // fraction
            '¤' => Some(219), // currency
            '‹' => Some(220), // left single guille
            '›' => Some(221), // right single guille
            'ﬁ' => Some(222), // fi
            'ﬂ' => Some(223), // fl
            '‡' => Some(224), // double dagger
            '·' => Some(225), // middle dot
            '‚' => Some(226), // single quote base
            '„' => Some(227), // double quote base
            '‰' => Some(228), // perthousand
            'Â' => Some(229), // A circumflex
            'Ê' => Some(230), // E circumflex
            'Á' => Some(231), // A acute
            'Ë' => Some(232), // E dieresis
            'È' => Some(233), // E grave
            'Í' => Some(234), // I acute
            'Î' => Some(235), // I circumflex
            'Ï' => Some(236), // I dieresis
            'Ì' => Some(237), // I grave
            'Ó' => Some(238), // O acute
            'Ô' => Some(239), // O circumflex
            'Ò' => Some(241), // O grave
            'Ú' => Some(242), // U acute
            'Û' => Some(243), // U circumflex
            'Ù' => Some(244), // U grave
            'ı' => Some(245), // dot-less i
            '^' => Some(246), // circumflex
            '˜' => Some(247), // tilde
            '¯' => Some(248), // macron
            '˘' => Some(249), // breve
            '˙' => Some(250), // dot accent
            '˚' => Some(251), // ring
            '¸' => Some(252), // cedilla
            '˝' => Some(253), // Hungarian umlaut (double acute accent)
            '˛' => Some(254), // ogonek
            'ˇ' => Some(255), // caron
            _ => None,
        }
    }
}

/// Converts a `char` to its Mac OS Roman character encoding.
///
/// Returns `None` if the character is not part of the Mac OS Roman character set.
#[rustfmt::skip]
pub fn macroman_to_char(macroman: u8) -> Option<char> {
        match macroman {
            0..=127 => Some(macroman as char),
            128 => Some('Ä'), // A dieresis
            129 => Some('Å'), // A ring
            130 => Some('Ç'), // C cedilla
            131 => Some('É'), // E acute
            132 => Some('Ñ'), // N tilde
            133 => Some('Ö'), // O dieresis
            134 => Some('Ü'), // U dieresis
            135 => Some('á'), // a acute
            136 => Some('à'), // a grave
            137 => Some('â'), // a circumflex
            138 => Some('ä'), // a dieresis
            139 => Some('ã'), // a tilde
            140 => Some('å'), // a ring
            141 => Some('ç'), // c cedilla
            142 => Some('é'), // e acute
            143 => Some('è'), // e grave
            144 => Some('ê'), // e circumflex
            145 => Some('ë'), // e dieresis
            146 => Some('í'), // i acute
            147 => Some('ì'), // i grave
            148 => Some('î'), // i circumflex
            149 => Some('ï'), // i dieresis
            150 => Some('ñ'), // n tilde
            151 => Some('ó'), // o acute
            152 => Some('ò'), // o grave
            153 => Some('ô'), // o circumflex
            154 => Some('ö'), // o dieresis
            155 => Some('õ'), // o tilde
            156 => Some('ú'), // u acute
            157 => Some('ù'), // u grave
            158 => Some('û'), // u circumflex
            159 => Some('ü'), // u dieresis
            160 => Some('†'), // dagger
            161 => Some('°'), // degree
            162 => Some('¢'), // cent
            163 => Some('£'), // sterling
            164 => Some('§'), // section
            165 => Some('•'), // bullet
            166 => Some('¶'), // paragraph
            167 => Some('ß'), // German double s
            168 => Some('®'), // registered
            169 => Some('©'), // copyright
            170 => Some('™'), // trademark
            171 => Some('´'), // acute
            172 => Some('¨'), // diaeresis
            174 => Some('Æ'), // AE
            175 => Some('Ø'), // O slash
            177 => Some('±'), // plusminus
            180 => Some('¥'), // yen
            181 => Some('µ'), // micro
            187 => Some('ª'), // ordfeminine
            188 => Some('º'), // ordmasculine
            190 => Some('æ'), // ae
            191 => Some('ø'), // o slash
            192 => Some('¿'), // question down
            193 => Some('¡'), // exclamation down
            194 => Some('¬'), // not
            196 => Some('ƒ'), // florin
            199 => Some('«'), // left guille
            200 => Some('»'), // right guille
            201 => Some('…'), // ellipsis
            202 => Some(' '), // non-breaking space
            203 => Some('À'), // A grave
            204 => Some('Ã'), // A tilde
            205 => Some('Õ'), // O tilde
            206 => Some('Œ'), // OE
            207 => Some('œ'), // oe
            208 => Some('–'), // endash
            209 => Some('—'), // emdash
            210 => Some('“'), // ldquo
            211 => Some('”'), // rdquo
            212 => Some('‘'), // lsquo
            213 => Some('’'), // rsquo
            214 => Some('÷'), // divide
            216 => Some('ÿ'), // y dieresis
            217 => Some('Ÿ'), // Y dieresis
            218 => Some('⁄'), // fraction
            219 => Some('¤'), // currency
            220 => Some('‹'), // left single guille
            221 => Some('›'), // right single guille
            222 => Some('ﬁ'), // fi
            223 => Some('ﬂ'), // fl
            224 => Some('‡'), // double dagger
            225 => Some('·'), // middle dot
            226 => Some('‚'), // single quote base
            227 => Some('„'), // double quote base
            228 => Some('‰'), // perthousand
            229 => Some('Â'), // A circumflex
            230 => Some('Ê'), // E circumflex
            231 => Some('Á'), // A acute
            232 => Some('Ë'), // E dieresis
            233 => Some('È'), // E grave
            234 => Some('Í'), // I acute
            235 => Some('Î'), // I circumflex
            236 => Some('Ï'), // I dieresis
            237 => Some('Ì'), // I grave
            238 => Some('Ó'), // O acute
            239 => Some('Ô'), // O circumflex
            241 => Some('Ò'), // O grave
            242 => Some('Ú'), // U acute
            243 => Some('Û'), // U circumflex
            244 => Some('Ù'), // U grave
            245 => Some('ı'), // dot-less i
            246 => Some('^'), // circumflex
            247 => Some('˜'), // tilde
            248 => Some('¯'), // macron
            249 => Some('˘'), // breve
            250 => Some('˙'), // dot accent
            251 => Some('˚'), // ring
            252 => Some('¸'), // cedilla
            253 => Some('˝'), // Hungarian umlaut (double acute accent)
            254 => Some('˛'), // ogonek
            255 => Some('ˇ'), // caron
            _ => None,
    }
}
