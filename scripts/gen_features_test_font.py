#!/usr/bin/env python3
"""Generate `layout/tests/fonts/azul-mock-features.ttf`, the TEST-ONLY font with one OpenType
feature for each CSS font-feature the text tests drive.

The font is the 95 printable ASCII glyphs, every one a box 500 units wide (the layout of
`azul-mock-liga.ttf` and `azul-mock-kern.ttf`, see `scripts/gen_stress_fonts.py`), and extra glyphs
that only a GSUB feature reaches. A test sets a letter in the font under a CSS property and reads
WHICH glyph came out (the glyph id) and how wide it is, so it sees which feature the property asked
for. Every feature changes ONE letter (or two for a ligature), and no two features that a test
turns on together change the same letter.

    feature   letter(s)  glyph               advance   CSS property that asks for it
    liga      f i        f_i                 700       the default of a Latin script; `font-variant-ligatures`
    clig      c l        c_l                 710       the default; `font-variant-ligatures`
    dlig      d l        d_l                 720       `font-variant-ligatures: discretionary-ligatures`
    hlig      h l        h_l                 730       `font-variant-ligatures: historical-ligatures`
    calt      k          k.calt              540       the default; `font-variant-ligatures: contextual`
    jp78 ...  A, E, R    A.jp78 ... E.fwid E.pwid R.ruby  510 .. 570  `font-variant-east-asian` (nine features: six change A, fwid and pwid E, ruby R)
    smcp      a          a.smcp              480       `font-variant-caps: small-caps`
    c2sc      B          B.c2sc              485       `font-variant-caps: all-small-caps`
    pcap      b          b.pcap              470       `font-variant-caps: petite-caps`
    c2pc      C          C.c2pc              475       `font-variant-caps: all-petite-caps`
    unic      u          u.unic              490       `font-variant-caps: unicase`
    titl      t          t.titl              495       `font-variant-caps: titling-caps`
    lnum ...  1 .. 7, 0  one.lnum ...       505 ...   `font-variant-numeric` (eight features)
    subs      p          p.subs              350       `font-variant-position: sub`
    sups      q          q.sups              360       `font-variant-position: super`
    hist      j          j.hist              530       `font-variant-alternates: historical-forms`
    ss01 ss02 x y        x.ss01 y.ss02       560 570   `font-variant-alternates: styleset(1, 2)`
    cv01 cv02 z v        z.cv01 v.cv02       580 590   `font-variant-alternates: character-variant(1, 2)`
    salt      s          s.alt1 s.alt2 s.alt3  550 .. 552  `font-variant-alternates: stylistic(n)`
    swsh      w          w.swsh1 w.swsh2     600 601   `font-variant-alternates: swash(n)`
    ornm      o          o.orn1 o.orn2       610 611   `font-variant-alternates: ornaments(n)`
    nalt      n          n.nalt1 n.nalt2     620 621   `font-variant-alternates: annotation(n)`

Re-run:  python3 scripts/gen_features_test_font.py [output-path]
Then update layout/tests/fonts/README.md if you change any glyph.

Requires fontTools (v4.63 tested). Pure Python - runs no `cargo`. The output is byte-for-byte
deterministic (the helpers of `gen_stress_fonts.py` pin the timestamps).
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gen_stress_fonts as stress  # noqa: E402  (the shared builder of the stress fonts)

OUT = os.path.join(stress.OUT_DIR, "azul-mock-features.ttf")

# (feature, letter it changes, the glyph it changes it to, that glyph's advance)
SINGLE = [
    # East Asian forms: the six JIS and script forms change `A` (a text asks for one of them), the
    # two widths change `E` (one of them), `ruby` changes `R`: `A E R` shows a value that combines
    # the three groups.
    ("jp78", "A", "A.jp78", 510),
    ("jp83", "A", "A.jp83", 520),
    ("jp90", "A", "A.jp90", 530),
    ("jp04", "A", "A.jp04", 540),
    ("smpl", "A", "A.smpl", 550),
    ("trad", "A", "A.trad", 560),
    ("fwid", "E", "E.fwid", 1000),
    ("pwid", "E", "E.pwid", 400),
    ("ruby", "R", "R.ruby", 570),
    # Caps.
    ("smcp", "a", "a.smcp", 480),
    ("c2sc", "B", "B.c2sc", 485),
    ("pcap", "b", "b.pcap", 470),
    ("c2pc", "C", "C.c2pc", 475),
    ("unic", "u", "u.unic", 490),
    ("titl", "t", "t.titl", 495),
    # Numeric forms.
    ("lnum", "1", "one.lnum", 505),
    ("onum", "2", "two.onum", 506),
    ("pnum", "3", "three.pnum", 400),
    ("tnum", "4", "four.tnum", 600),
    ("frac", "5", "five.frac", 507),
    ("afrc", "6", "six.afrc", 508),
    ("ordn", "7", "seven.ordn", 509),
    ("zero", "0", "zero.zero", 511),
    # Super- and subscript forms.
    ("subs", "p", "p.subs", 350),
    ("sups", "q", "q.sups", 360),
    # A contextual alternate: the shaper turns `calt` on by default.
    ("calt", "k", "k.calt", 540),
    # Alternates.
    ("hist", "j", "j.hist", 530),
    ("ss01", "x", "x.ss01", 560),
    ("ss02", "y", "y.ss02", 570),
    ("cv01", "z", "z.cv01", 580),
    ("cv02", "v", "v.cv02", 590),
]

# (feature, the letters it joins, the glyph it joins them to, that glyph's advance)
LIGATURES = [
    ("liga", "fi", "f_i", 700),
    ("clig", "cl", "c_l", 710),
    ("dlig", "dl", "d_l", 720),
    ("hlig", "hl", "h_l", 730),
]

# (feature, letter, [alternates in the order of the set, each with its advance])
ALTERNATES = [
    ("salt", "s", [("s.alt1", 550), ("s.alt2", 551), ("s.alt3", 552)]),
    ("swsh", "w", [("w.swsh1", 600), ("w.swsh2", 601)]),
    ("ornm", "o", [("o.orn1", 610), ("o.orn2", 611)]),
    ("nalt", "n", [("n.nalt1", 620), ("n.nalt2", 621)]),
]


def glyph_of(letter):
    """The name of the glyph the cmap gives `letter` (`gen_stress_fonts._ascii_specs`)."""
    return "uni%04X" % ord(letter)


def build():
    specs, cmap = stress._ascii_specs(lambda cp: 500)
    extras = []
    for _feature, _letter, name, advance in SINGLE:
        extras.append((name, advance, True))
    for _feature, _letters, name, advance in LIGATURES:
        extras.append((name, advance, True))
    for _feature, _letter, alternates in ALTERNATES:
        for name, advance in alternates:
            extras.append((name, advance, True))
    specs += extras

    fea = ["languagesystem DFLT dflt;", "languagesystem latn dflt;", ""]
    for feature, letter, name, _advance in SINGLE:
        fea.append("feature %s { sub %s by %s; } %s;" % (feature, glyph_of(letter), name, feature))
    for feature, letters, name, _advance in LIGATURES:
        fea.append(
            "feature %s { sub %s by %s; } %s;"
            % (feature, " ".join(glyph_of(c) for c in letters), name, feature)
        )
    for feature, letter, alternates in ALTERNATES:
        fea.append(
            "feature %s { sub %s from [%s]; } %s;"
            % (feature, glyph_of(letter), " ".join(name for name, _a in alternates), feature)
        )
    return stress.build_font("Azul Mock Features", specs, cmap, fea="\n".join(fea) + "\n")


def main():
    target = sys.argv[1] if len(sys.argv) > 1 else OUT
    font = build()
    font.save(target)
    order = font.getGlyphOrder()
    print("wrote", os.path.normpath(target), "(%d glyphs, %d bytes)" % (len(order), os.path.getsize(target)))
    for name in order[96:]:
        print("  gid %3d  %s" % (order.index(name), name))


if __name__ == "__main__":
    main()
