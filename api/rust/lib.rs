// The AZUL library (package "azul-dll") is provided under
// the terms of the GNU Library General Public License (LGPL)
// with the following modifications:
// 
// 1. Modifications to feature flags to enable / disable existing
//    features in AZUL do not constitute a modified or derivative work.
// 
// 2. Static linking of applications and widgets to the AZUL library
//    does not constitute a derivative work and does not require the
//    author to provide source code for the application or widget,
//    use the shared AZUL libraries, or link their applications or
//    widgets against a user-supplied version of AZUL.
// 
// 3. You do not have to provide a copy of this license with programs
//    that are linked to the AZUL library, nor do you have to identify
//    the AZUL license in your program or documentation as required by
//    section 6 of the LGPL.
// 
//    However, programs must still identify their use of AZUL. The
//    following example statement can be included in user documentation
//    to satisfy this requirement:
// 
//    [program] is based in part on the work of the AZUL GUI toolkit
//    (https://azul.rs).
// 
// 4. ANTI-AUTOPGRADE-CLAUSE: This license is not implicitly
//    compatible with future versions of the LGPL.
// 
// 5. In order to comply with the licenses used by the crates that this
//    library depends on, the documentation must include the following
//    section:
// 
// ----------------------------------------------------------------------
// 
// The AZUL GUI toolkit uses the following projects under the
// following licenses:
// 
//     adler version 1.0.2:
//         licensed 0BSD OR Apache-2.0 OR MIT by Jonas Schievink
//     adler32 version 1.2.0:
//         licensed Zlib by Remi Rampin
//     ahash version 0.7.2:
//         licensed Apache-2.0 OR MIT by Tom Kaitchuck
//     alloc-no-stdlib version 2.0.1:
//         licensed BSD-3-Clause by Daniel Reiter Horn
//     allsorts-rental version 0.5.6:
//         licensed Apache-2.0 OR MIT by Jameson Ernst
//     allsorts_no_std version 0.5.2:
//         licensed Apache-2.0 by YesLogic Pty. Ltd.
//     app_units version 0.7.1:
//         licensed MPL-2.0 by The Servo Project Developers
//     arrayref version 0.3.6:
//         licensed BSD-2-Clause by David Roundy
//     arrayvec version 0.5.2:
//         licensed Apache-2.0 OR MIT by bluss
//     azul-core version 0.0.2:
//         licensed LGPL-3.0-only WITH LGPL-3.0-linking-exception by Felix Schütt
//     azul-css version 0.0.1:
//         licensed LGPL-3.0-only WITH LGPL-3.0-linking-exception by Felix Schütt
//     azul-css-parser version 0.0.1:
//         licensed LGPL-3.0-only WITH LGPL-3.0-linking-exception by Felix Schütt
//     azul-desktop version 0.0.5:
//         licensed LGPL-3.0-only WITH LGPL-3.0-linking-exception by Felix Schütt
//     azul-dll version 0.0.1:
//         licensed LGPL-3.0-only WITH LGPL-3.0-linking-exception by Felix Schütt
//     azul-layout version 0.0.4:
//         licensed LGPL-3.0-only WITH LGPL-3.0-linking-exception by Felix Schütt
//     azul-simplecss version 0.1.1:
//         licensed MPL-2.0 by Reizner Evgeniy
//     azul-text-layout version 0.0.5:
//         licensed LGPL-3.0-only WITH LGPL-3.0-linking-exception by Felix Schütt
//     azulc version 0.0.3:
//         licensed LGPL-3.0-only WITH LGPL-3.0-linking-exception by Felix Schütt
//     backtrace version 0.3.56:
//         licensed Apache-2.0 OR MIT by The Rust Project Developers
//     base64 version 0.13.0:
//         licensed Apache-2.0 OR MIT by Alice Maz, Marshall Pierce
//     binary-space-partition version 0.1.2:
//         licensed MPL-2.0 by Dzmitry Malyshau
//     bincode version 1.3.1:
//         licensed MIT by Ty Overby, Francesco Mazzoli, David Tolnay , Zoey Riordan
//     bitflags version 1.2.1:
//         licensed Apache-2.0 OR MIT by The Rust Project Developers
//     bitreader version 0.3.3:
//         licensed Apache-2.0 OR MIT by Ilkka Rauta
//     brotli-decompressor version 2.3.1:
//         licensed BSD-3-Clause OR MIT by Daniel Reiter Horn, The Brotli Authors
//     bytemuck version 1.5.1:
//         licensed Apache-2.0 OR MIT OR Zlib by Lokathor
//     byteorder version 1.4.3:
//         licensed MIT OR Unlicense by Andrew Gallant
//     cfg-if version 0.1.10:
//         licensed Apache-2.0 OR MIT by Alex Crichton
//     cfg-if version 1.0.0:
//         licensed Apache-2.0 OR MIT by Alex Crichton
//     chrono version 0.4.19:
//         licensed Apache-2.0 OR MIT by Kang Seonghoon, Brandon W Maister
//     clipboard-win version 2.2.0:
//         licensed MIT by Douman
//     clipboard2 version 0.1.1:
//         licensed MIT by Avi Weinstock, Felix Schütt
//     color_quant version 1.1.0:
//         licensed MIT by nwin
//     convert_case version 0.4.0:
//         licensed MIT by David Purdum
//     crc32fast version 1.2.1:
//         licensed Apache-2.0 OR MIT by Sam Rijs, Alex Crichton
//     crossbeam-channel version 0.5.0:
//         licensed Apache-2.0 OR MIT by The Crossbeam Project Developers
//     crossbeam-deque version 0.8.0:
//         licensed Apache-2.0 OR MIT by The Crossbeam Project Developers
//     crossbeam-epoch version 0.9.3:
//         licensed Apache-2.0 OR MIT by The Crossbeam Project Developers
//     crossbeam-utils version 0.8.3:
//         licensed Apache-2.0 OR MIT by The Crossbeam Project Developers
//     cstr version 0.2.8:
//         licensed MIT by Xidorn Quan
//     data-url version 0.1.0:
//         licensed Apache-2.0 OR MIT by Simon Sapin
//     deflate version 0.8.6:
//         licensed Apache-2.0 OR MIT by oyvindln
//     derive_more version 0.99.13:
//         licensed MIT by Jelte Fennema
//     dwrote version 0.11.0:
//         licensed MPL-2.0 by The Servo Project Developers, Vladimir Vukicevic
//     either version 1.6.1:
//         licensed Apache-2.0 OR MIT by bluss
//     encoding_rs version 0.8.28:
//         licensed Apache-2.0 OR MIT by Henri Sivonen
//     etagere version 0.2.4:
//         licensed Apache-2.0 OR MIT by Nicolas Silva
//     euclid version 0.20.14:
//         licensed Apache-2.0 OR MIT by The Servo Project Developers
//     euclid version 0.22.2:
//         licensed Apache-2.0 OR MIT by The Servo Project Developers
//     fern version 0.5.9:
//         licensed MIT by David Ross
//     flate2 version 1.0.20:
//         licensed Apache-2.0 OR MIT by Alex Crichton, Josh Triplett
//     float-cmp version 0.5.3:
//         licensed MIT by Mike Dilger
//     fxhash version 0.2.1:
//         licensed Apache-2.0 OR MIT by cbreeden
//     getrandom version 0.2.2:
//         licensed Apache-2.0 OR MIT by The Rand Project Developers
//     gif version 0.11.2:
//         licensed Apache-2.0 OR MIT by nwin
//     gleam version 0.13.1:
//         licensed Apache-2.0 OR MIT by The Servo Project Developers
//     glutin version 0.26.0:
//         licensed Apache-2.0 by The glutin contributors, Pierre Krieger
//     glutin_egl_sys version 0.1.5:
//         licensed Apache-2.0 by The glutin contributors, Hal Gentz
//     glutin_wgl_sys version 0.1.5:
//         licensed Apache-2.0 by The glutin contributors, Hal Gentz
//     glyph-names version 0.1.0:
//         licensed BSD-3-Clause by YesLogic Pty. Ltd.
//     image version 0.23.14:
//         licensed MIT by The image-rs Developers
//     instant version 0.1.9:
//         licensed BSD-3-Clause by sebcrozet
//     itertools version 0.8.2:
//         licensed Apache-2.0 OR MIT by bluss
//     jpeg-decoder version 0.1.22:
//         licensed Apache-2.0 OR MIT by Ulf Nilsson
//     kurbo version 0.8.0:
//         licensed Apache-2.0 OR MIT by Raph Levien
//     lazy_static version 1.4.0:
//         licensed Apache-2.0 OR MIT by Marvin Löbel
//     libc version 0.2.91:
//         licensed Apache-2.0 OR MIT by The Rust Project Developers
//     libloading version 0.6.7:
//         licensed ISC by Simonas Kazlauskas
//     libm version 0.2.1:
//         licensed Apache-2.0 OR MIT by Jorge Aparicio
//     lock_api version 0.4.2:
//         licensed Apache-2.0 OR MIT by Amanieu d'Antras
//     log version 0.4.14:
//         licensed Apache-2.0 OR MIT by The Rust Project Developers
//     lyon version 0.15.9:
//         licensed Apache-2.0 OR MIT by Nicolas Silva
//     lyon_algorithms version 0.15.1:
//         licensed Apache-2.0 OR MIT by Nicolas Silva
//     lyon_geom version 0.15.3:
//         licensed Apache-2.0 OR MIT by Nicolas Silva
//     lyon_path version 0.15.2:
//         licensed Apache-2.0 OR MIT by Nicolas Silva
//     lyon_tessellation version 0.15.9:
//         licensed Apache-2.0 OR MIT by Nicolas Silva
//     malloc_size_of_derive version 0.1.2:
//         licensed Apache-2.0 OR MIT by The Servo Project Developers
//     matches version 0.1.8:
//         licensed MIT by Simon Sapin
//     memoffset version 0.6.2:
//         licensed MIT by Gilad Naaman
//     minidl version 0.1.1:
//         licensed Apache-2.0 OR MIT by MaulingMonkey
//     miniz_oxide version 0.3.7:
//         licensed MIT by Frommi, oyvindln
//     miniz_oxide version 0.4.4:
//         licensed Apache-2.0 OR MIT OR Zlib by Frommi, oyvindln
//     mmapio version 0.9.1:
//         licensed Apache-2.0 OR MIT by henrylee2cn, dignifiedquire, Dan Burkert
//     num-integer version 0.1.44:
//         licensed Apache-2.0 OR MIT by The Rust Project Developers
//     num-iter version 0.1.42:
//         licensed Apache-2.0 OR MIT by The Rust Project Developers
//     num-rational version 0.3.2:
//         licensed Apache-2.0 OR MIT by The Rust Project Developers
//     num-traits version 0.2.14:
//         licensed Apache-2.0 OR MIT by The Rust Project Developers
//     num_cpus version 1.13.0:
//         licensed Apache-2.0 OR MIT by Sean McArthur
//     once_cell version 1.7.2:
//         licensed Apache-2.0 OR MIT by Aleksey Kladov
//     owned_ttf_parser version 0.6.0:
//         licensed Apache-2.0 by Alex Butler
//     parking_lot version 0.11.1:
//         licensed Apache-2.0 OR MIT by Amanieu d'Antras
//     parking_lot_core version 0.8.3:
//         licensed Apache-2.0 OR MIT by Amanieu d'Antras
//     peek-poke version 0.2.0:
//         licensed Apache-2.0 OR MIT by Dan Glastonbury
//     peek-poke-derive version 0.2.1:
//         licensed Apache-2.0 OR MIT by Dan Glastonbury
//     pico-args version 0.4.0:
//         licensed MIT by Evgeniy Reizner
//     plane-split version 0.17.1:
//         licensed MPL-2.0 by Dzmitry Malyshau
//     png version 0.16.8:
//         licensed Apache-2.0 OR MIT by nwin
//     proc-macro2 version 1.0.24:
//         licensed Apache-2.0 OR MIT by Alex Crichton, David Tolnay
//     quote version 1.0.9:
//         licensed Apache-2.0 OR MIT by David Tolnay
//     raw-window-handle version 0.3.3:
//         licensed MIT by Osspial
//     rayon version 1.5.0:
//         licensed Apache-2.0 OR MIT by Niko Matsakis, Josh Stone
//     rayon-core version 1.9.0:
//         licensed Apache-2.0 OR MIT by Niko Matsakis, Josh Stone
//     rctree version 0.3.3:
//         licensed MIT by Simon Sapin, Evgeniy Reizner
//     rental-impl version 0.5.5:
//         licensed Apache-2.0 OR MIT by Jameson Ernst
//     resvg version 0.14.0:
//         licensed MPL-2.0 by Reizner Evgeniy
//     rgb version 0.8.25:
//         licensed MIT by Kornel Lesiński
//     roxmltree version 0.14.0:
//         licensed Apache-2.0 OR MIT by Evgeniy Reizner
//     rust-fontconfig version 0.1.3:
//         licensed MIT by Felix Schütt
//     rustc-demangle version 0.1.18:
//         licensed Apache-2.0 OR MIT by Alex Crichton
//     rustc-hash version 1.1.0:
//         licensed Apache-2.0 OR MIT by The Rust Project Developers
//     safe_arch version 0.5.2:
//         licensed Apache-2.0 OR MIT OR Zlib by Lokathor
//     scoped_threadpool version 0.1.9:
//         licensed MIT by Marvin Löbel
//     scopeguard version 1.1.0:
//         licensed Apache-2.0 OR MIT by bluss
//     serde version 1.0.125:
//         licensed Apache-2.0 OR MIT by Erick Tryzelaar, David Tolnay
//     serde_bytes version 0.11.5:
//         licensed Apache-2.0 OR MIT by David Tolnay
//     serde_derive version 1.0.125:
//         licensed Apache-2.0 OR MIT by Erick Tryzelaar, David Tolnay
//     sid version 0.6.1:
//         licensed Apache-2.0 OR MIT by Nicolas Silva
//     simplecss version 0.2.0:
//         licensed Apache-2.0 OR MIT by Reizner Evgeniy
//     siphasher version 0.2.3:
//         licensed Apache-2.0 OR MIT by Frank Denis
//     smallvec version 1.6.1:
//         licensed Apache-2.0 OR MIT by The Servo Project Developers
//     stable_deref_trait version 1.2.0:
//         licensed Apache-2.0 OR MIT by Robert Grosse
//     strfmt version 0.1.6:
//         licensed MIT by Garrett Berg
//     svg_fmt version 0.4.1:
//         licensed Apache-2.0 OR MIT by Nicolas Silva
//     svgfilters version 0.3.0:
//         licensed MPL-2.0 by Evgeniy Reizner
//     svgtypes version 0.5.0:
//         licensed Apache-2.0 OR MIT by Evgeniy Reizner
//     syn version 1.0.64:
//         licensed Apache-2.0 OR MIT by David Tolnay
//     synstructure version 0.12.4:
//         licensed MIT by Nika Layzell
//     tiff version 0.6.1:
//         licensed MIT by ccgn, bvssvni, nwin, TyOverby, HeroicKatora,
//         Calum, CensoredUsername, Robzz, birktj
//     time version 0.1.43:
//         licensed Apache-2.0 OR MIT by The Rust Project Developers
//     tiny-skia version 0.5.1:
//         licensed BSD-3-Clause by Evgeniy Reizner
//     tinyfiledialogs version 3.3.10:
//         licensed zlib-acknowledgement by Guillaume Vareille, Josh Matthews
//     tinyvec version 1.1.1:
//         licensed Apache-2.0 OR MIT OR Zlib by Lokathor
//     tinyvec_macros version 0.1.0:
//         licensed Apache-2.0 OR MIT OR Zlib by Soveu
//     tracy-rs version 0.1.2:
//         licensed MPL-2.0 by Glenn Watson
//     ttf-parser version 0.6.2:
//         licensed Apache-2.0 OR MIT by Evgeniy Reizner
//     ttf-parser version 0.11.0:
//         licensed Apache-2.0 OR MIT by Evgeniy Reizner
//     ucd-trie version 0.1.3:
//         licensed Apache-2.0 OR MIT by Andrew Gallant
//     unicode-general-category version 0.3.0:
//         licensed Apache-2.0 by YesLogic Pty. Ltd.
//     unicode-joining-type version 0.5.0:
//         licensed Apache-2.0 by YesLogic Pty. Ltd.
//     unicode-normalization version 0.1.17:
//         licensed Apache-2.0 OR MIT by kwantam, Manish Goregaokar
//     unicode-xid version 0.2.1:
//         licensed Apache-2.0 OR MIT by erick.tryzelaar, kwantam
//     usvg version 0.14.0:
//         licensed MPL-2.0 by Evgeniy Reizner
//     uuid version 0.8.2:
//         licensed Apache-2.0 OR MIT by Ashley Mannix, Christopher Armstrong,
//         Dylan DPC, Hunar Roop Kahlon
//     webrender version 0.61.0:
//         licensed MPL-2.0 by Glenn Watson
//     webrender_api version 0.61.0:
//         licensed MPL-2.0 by Glenn Watson
//     webrender_build version 0.0.1:
//         licensed MPL-2.0 by The Servo Project Developers
//     weezl version 0.1.4:
//         licensed Apache-2.0 OR MIT by HeroicKatora
//     winapi version 0.3.9:
//         licensed Apache-2.0 OR MIT by Peter Atashian
//     winit version 0.24.0:
//         licensed Apache-2.0 by The winit contributors, Pierre Krieger
//     wio version 0.2.2:
//         licensed Apache-2.0 OR MIT by Peter Atashian
//     wr_malloc_size_of version 0.0.1:
//         licensed Apache-2.0 OR MIT by The Servo Project Developers
//     xmlparser version 0.13.3:
//         licensed Apache-2.0 OR MIT by Evgeniy Reizner
//     xmlwriter version 0.1.0:
//         licensed MIT by Evgeniy Reizner
// 
// To generate the full text of the license for the license, please visit
// https://spdx.org/licenses/ and replace the license author in the source
// text in any given license with the name of the author.
// 
// ----------------------------------------------------------------------
// 
//                    GNU LESSER GENERAL PUBLIC LICENSE
//                        Version 3, 29 June 2007
// 
//  Copyright (C) 2007 Free Software Foundation, Inc. <https://fsf.org/>
//  Everyone is permitted to copy and distribute verbatim copies of this
//  license document, but changing it is not allowed.
// 
// 
//   This version of the GNU Lesser General Public License incorporates
// the terms and conditions of version 3 of the GNU General Public
// License, supplemented by the additional permissions listed below.
// 
//   0. Additional Definitions.
// 
//   As used herein, "this License" refers to version 3 of the GNU Lesser
// General Public License, and the "GNU GPL" refers to version 3 of the
// GNU General Public License.
// 
//   "The Library" refers to a covered work governed by this License,
// other than an Application or a Combined Work as defined below.
// 
//   An "Application" is any work that makes use of an interface provided
// by the Library, but which is not otherwise based on the Library.
// Defining a subclass of a class defined by the Library is deemed a mode
// of using an interface provided by the Library.
// 
//   A "Combined Work" is a work produced by combining or linking an
// Application with the Library.  The particular version of the Library
// with which the Combined Work was made is also called the "Linked
// Version".
// 
//   The "Minimal Corresponding Source" for a Combined Work means the
// Corresponding Source for the Combined Work, excluding any source code
// for portions of the Combined Work that, considered in isolation, are
// based on the Application, and not on the Linked Version.
// 
//   The "Corresponding Application Code" for a Combined Work means the
// object code and/or source code for the Application, including any data
// and utility programs needed for reproducing the Combined Work from the
// Application, but excluding the System Libraries of the Combined Work.
// 
//   1. Exception to Section 3 of the GNU GPL.
// 
//   You may convey a covered work under sections 3 and 4 of this License
// without being bound by section 3 of the GNU GPL.
// 
//   2. Conveying Modified Versions.
// 
//   If you modify a copy of the Library, and, in your modifications, a
// facility refers to a function or data to be supplied by an Application
// that uses the facility (other than as an argument passed when the
// facility is invoked), then you may convey a copy of the modified
// version:
// 
//    a) under this License, provided that you make a good faith effort
//    to ensure that, in the event an Application does not supply the
//    function or data, the facility still operates, and performs
//    whatever part of its purpose remains meaningful, or
// 
//    b) under the GNU GPL, with none of the additional permissions of
//    this License applicable to that copy.
// 
//   3. Object Code Incorporating Material from Library Header Files.
// 
//   The object code form of an Application may incorporate material from
// a header file that is part of the Library.  You may convey such object
// code under terms of your choice, provided that, if the incorporated
// material is not limited to numerical parameters, data structure
// layouts and accessors, or small macros, inline functions and templates
// (ten or fewer lines in length), you do both of the following:
// 
//    a) Give prominent notice with each copy of the object code that the
//    Library is used in it and that the Library and its use are covered
//    by this License.
// 
//    b) Accompany the object code with a copy of the GNU GPL and this
//    license document.
// 
//   4. Combined Works.
// 
//   You may convey a Combined Work under terms of your choice that,
// taken together, effectively do not restrict modification of the
// portions of the Library contained in the Combined Work and reverse
// engineering for debugging such modifications, if you also do each of
// the following:
// 
//    a) Give prominent notice with each copy of the Combined Work that
//    the Library is used in it and that the Library and its use are
//    covered by this License.
// 
//    b) Accompany the Combined Work with a copy of the GNU GPL and this
//    license document.
// 
//    c) For a Combined Work that displays copyright notices during
//    execution, include the copyright notice for the Library among these
//    notices, as well as a reference directing the user to the copies of
//    the GNU GPL and this license document.
// 
//    d) Do one of the following:
// 
//        0) Convey the Minimal Corresponding Source under the terms of
//        this License, and the Corresponding Application Code in a form
//        suitable for, and under terms that permit, the user to
//        recombine or relink the Application with a modified version of
//        the Linked Version to produce a modified Combined Work, in the
//        manner specified by section 6 of the GNU GPL for conveying
//        Corresponding Source.
// 
//        1) Use a suitable shared library mechanism for linking with the
//        Library.  A suitable mechanism is one that (a) uses at run time
//        a copy of the Library already present on the user's computer
//        system, and (b) will operate properly with a modified version
//        of the Library that is interface-compatible with the Linked
//        Version.
// 
//    e) Provide Installation Information, but only if you would
//    otherwise be required to provide such information under section 6
//    of the GNU GPL, and only to the extent that such information is
//    necessary to install and execute a modified version of the Combined
//    Work produced by recombining or relinking the Application with a
//    modified version of the Linked Version. (If you use option 4d0, the
//    Installation Information must accompany the Minimal Corresponding
//    Source and Corresponding Application Code. If you use option 4d1,
//    you must provide the Installation Information in the manner
//    specified by section 6 of the GNU GPL for conveying Corresponding
//    Source.)
// 
//   5. Combined Libraries.
// 
//   You may place library facilities that are a work based on the
// Library side by side in a single library together with other library
// facilities that are not Applications and are not covered by this
// License, and convey such a combined library under terms of your
// choice, if you do both of the following:
// 
//    a) Accompany the combined library with a copy of the same work
//    based on the Library, uncombined with any other library facilities,
//    conveyed under the terms of this License.
// 
//    b) Give prominent notice with the combined library that part of it
//    is a work based on the Library, and explaining where to find the
//    accompanying uncombined form of the same work.
// 
//   6. Revised Versions of the GNU Lesser General Public License.
// 
//   The Free Software Foundation may publish revised and/or new versions
// of the GNU Lesser General Public License from time to time. Such new
// versions will be similar in spirit to the present version, but may
// differ in detail to address new problems or concerns.
// 
//   Each version is given a distinguishing version number. If the
// Library as you received it specifies that a certain numbered version
// of the GNU Lesser General Public License "or any later version"
// applies to it, you have the option of following the terms and
// conditions either of that published version or of any later version
// published by the Free Software Foundation. If the Library as you
// received it does not specify a version number of the GNU Lesser
// General Public License, you may choose any version of the GNU Lesser
// General Public License ever published by the Free Software Foundation.
// 
//   If the Library as you received it specifies that a proxy can decide
// whether future versions of the GNU Lesser General Public License shall
// apply, that proxy's public statement of acceptance of any version is
// permanent authorization for you to choose that version for the
// Library.
// #![no_std]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico",
)]

//! Auto-generated public Rust API for the Azul GUI toolkit version " + version + "

extern crate alloc;

/// Module to re-export common structs (`App`, `AppConfig`, `Css`, `Dom`, `WindowCreateOptions`, `RefAny`, `LayoutInfo`)
pub mod prelude {
    pub use crate::app::*;
    pub use crate::window::*;
    pub use crate::callbacks::*;
    pub use crate::menu::*;
    pub use crate::dom::*;
    pub use crate::css::*;
    pub use crate::style::*;
    pub use crate::gl::*;
    pub use crate::image::*;
    pub use crate::font::*;
    pub use crate::svg::*;
    pub use crate::xml::*;
    pub use crate::fs::*;
    pub use crate::dialog::*;
    pub use crate::clipboard::*;
    pub use crate::time::*;
    pub use crate::task::*;
    pub use crate::str::*;
    pub use crate::vec::*;
    pub use crate::option::*;
    pub use crate::error::*;
}

mod dll {
    impl AzString {
        #[inline]
        pub fn as_str(&self) -> &str {
            unsafe { core::str::from_utf8_unchecked(self.as_bytes()) }
        }
        #[inline]
        pub fn as_bytes(&self) -> &[u8] {
            unsafe { core::slice::from_raw_parts(self.vec.ptr, self.vec.len) }
        }
    }

    impl ::core::fmt::Debug for AzCallback                   { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzLayoutCallbackInner             { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzMarshaledLayoutCallbackInner             { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzRenderImageCallback        { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzIFrameCallback             { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzTimerCallback              { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzWriteBackCallback          { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzThreadDestructorFn         { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzLibraryReceiveThreadMsgFn  { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzLibrarySendThreadMsgFn     { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzCheckThreadFinishedFn      { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzGetSystemTimeFn            { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzCreateThreadFn             { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzThreadRecvFn               { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzThreadReceiverDestructorFn { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzThreadSenderDestructorFn   { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzInstantPtrDestructorFn     { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzInstantPtrCloneFn          { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzThreadSendFn               { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}

    impl PartialEq for AzCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzLayoutCallbackInner { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzMarshaledLayoutCallbackInner { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzRenderImageCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzIFrameCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzTimerCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzWriteBackCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzThreadDestructorFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzLibraryReceiveThreadMsgFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzLibrarySendThreadMsgFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzCheckThreadFinishedFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzGetSystemTimeFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzCreateThreadFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzThreadRecvFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzThreadReceiverDestructorFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzThreadSenderDestructorFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzInstantPtrDestructorFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzInstantPtrCloneFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzThreadSendFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }

    impl PartialOrd for AzCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzLayoutCallbackInner { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzMarshaledLayoutCallbackInner { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzRenderImageCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzIFrameCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzTimerCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzWriteBackCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzThreadDestructorFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzLibraryReceiveThreadMsgFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzLibrarySendThreadMsgFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzCheckThreadFinishedFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzGetSystemTimeFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzCreateThreadFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzThreadRecvFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzThreadReceiverDestructorFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzThreadSenderDestructorFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzInstantPtrDestructorFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzInstantPtrCloneFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzThreadSendFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }

    #[cfg(not(feature = "link_static"))]
    mod dynamic_link {
    use core::ffi::c_void;

    /// Main application class
    #[repr(C)]
    #[derive(Debug)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzApp {
        pub(crate) ptr: *const c_void,
    }

    /// Configuration to set which messages should be logged.
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzAppLogLevel {
        Off,
        Error,
        Warn,
        Info,
        Debug,
        Trace,
    }

    /// Version of the layout solver to use - future binary versions of azul may have more fields here, necessary so that old compiled applications don't break with newer releases of azul. Newer layout versions are opt-in only.
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutSolver {
        Default,
    }

    /// Whether the renderer has VSync enabled
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzVsync {
        Enabled,
        Disabled,
    }

    /// Does the renderer render in SRGB color space? By default, azul tries to set it to `Enabled` and falls back to `Disabled` if the OpenGL context can't be initialized properly
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzSrgb {
        Enabled,
        Disabled,
    }

    /// Does the renderer render using hardware acceleration? By default, azul tries to set it to `Enabled` and falls back to `Disabled` if the OpenGL context can't be initialized properly
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzHwAcceleration {
        Enabled,
        Disabled,
    }

    /// Offset in physical pixels (integer units)
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutPoint {
        pub x: isize,
        pub y: isize,
    }

    /// Size in physical pixels (integer units)
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutSize {
        pub width: isize,
        pub height: isize,
    }

    /// Re-export of rust-allocated (stack based) `IOSHandle` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzIOSHandle {
        pub ui_window: *mut c_void,
        pub ui_view: *mut c_void,
        pub ui_view_controller: *mut c_void,
    }

    /// Re-export of rust-allocated (stack based) `MacOSHandle` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzMacOSHandle {
        pub ns_window: *mut c_void,
        pub ns_view: *mut c_void,
    }

    /// Re-export of rust-allocated (stack based) `XlibHandle` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzXlibHandle {
        pub window: u64,
        pub display: *mut c_void,
    }

    /// Re-export of rust-allocated (stack based) `XcbHandle` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzXcbHandle {
        pub window: u32,
        pub connection: *mut c_void,
    }

    /// Re-export of rust-allocated (stack based) `WaylandHandle` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzWaylandHandle {
        pub surface: *mut c_void,
        pub display: *mut c_void,
    }

    /// Re-export of rust-allocated (stack based) `WindowsHandle` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzWindowsHandle {
        pub hwnd: *mut c_void,
        pub hinstance: *mut c_void,
    }

    /// Re-export of rust-allocated (stack based) `WebHandle` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzWebHandle {
        pub id: u32,
    }

    /// Re-export of rust-allocated (stack based) `AndroidHandle` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzAndroidHandle {
        pub a_native_window: *mut c_void,
    }

    /// X11 window hint: Type of window
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzXWindowType {
        Desktop,
        Dock,
        Toolbar,
        Menu,
        Utility,
        Splash,
        Dialog,
        DropdownMenu,
        PopupMenu,
        Tooltip,
        Notification,
        Combo,
        Dnd,
        Normal,
    }

    /// Same as `LayoutPoint`, but uses `i32` instead of `isize`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzPhysicalPositionI32 {
        pub x: i32,
        pub y: i32,
    }

    /// Same as `LayoutPoint`, but uses `u32` instead of `isize`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzPhysicalSizeU32 {
        pub width: u32,
        pub height: u32,
    }

    /// Logical position (can differ based on HiDPI settings). Usually this is what you'd want for hit-testing and positioning elements.
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLogicalPosition {
        pub x: f32,
        pub y: f32,
    }

    /// A size in "logical" (non-HiDPI-adjusted) pixels in floating-point units
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLogicalSize {
        pub width: f32,
        pub height: f32,
    }

    /// Unique hash of a window icon, so that azul does not have to compare the actual bytes to see wether the window icon has changed.
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzIconKey {
        pub id: usize,
    }

    /// Symbolic name for a keyboard key, does **not** take the keyboard locale into account
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzVirtualKeyCode {
        Key1,
        Key2,
        Key3,
        Key4,
        Key5,
        Key6,
        Key7,
        Key8,
        Key9,
        Key0,
        A,
        B,
        C,
        D,
        E,
        F,
        G,
        H,
        I,
        J,
        K,
        L,
        M,
        N,
        O,
        P,
        Q,
        R,
        S,
        T,
        U,
        V,
        W,
        X,
        Y,
        Z,
        Escape,
        F1,
        F2,
        F3,
        F4,
        F5,
        F6,
        F7,
        F8,
        F9,
        F10,
        F11,
        F12,
        F13,
        F14,
        F15,
        F16,
        F17,
        F18,
        F19,
        F20,
        F21,
        F22,
        F23,
        F24,
        Snapshot,
        Scroll,
        Pause,
        Insert,
        Home,
        Delete,
        End,
        PageDown,
        PageUp,
        Left,
        Up,
        Right,
        Down,
        Back,
        Return,
        Space,
        Compose,
        Caret,
        Numlock,
        Numpad0,
        Numpad1,
        Numpad2,
        Numpad3,
        Numpad4,
        Numpad5,
        Numpad6,
        Numpad7,
        Numpad8,
        Numpad9,
        NumpadAdd,
        NumpadDivide,
        NumpadDecimal,
        NumpadComma,
        NumpadEnter,
        NumpadEquals,
        NumpadMultiply,
        NumpadSubtract,
        AbntC1,
        AbntC2,
        Apostrophe,
        Apps,
        Asterisk,
        At,
        Ax,
        Backslash,
        Calculator,
        Capital,
        Colon,
        Comma,
        Convert,
        Equals,
        Grave,
        Kana,
        Kanji,
        LAlt,
        LBracket,
        LControl,
        LShift,
        LWin,
        Mail,
        MediaSelect,
        MediaStop,
        Minus,
        Mute,
        MyComputer,
        NavigateForward,
        NavigateBackward,
        NextTrack,
        NoConvert,
        OEM102,
        Period,
        PlayPause,
        Plus,
        Power,
        PrevTrack,
        RAlt,
        RBracket,
        RControl,
        RShift,
        RWin,
        Semicolon,
        Slash,
        Sleep,
        Stop,
        Sysrq,
        Tab,
        Underline,
        Unlabeled,
        VolumeDown,
        VolumeUp,
        Wake,
        WebBack,
        WebFavorites,
        WebForward,
        WebHome,
        WebRefresh,
        WebSearch,
        WebStop,
        Yen,
        Copy,
        Paste,
        Cut,
    }

    /// Boolean flags relating to the current window state
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzWindowFlags {
        pub is_maximized: bool,
        pub is_minimized: bool,
        pub is_about_to_close: bool,
        pub is_fullscreen: bool,
        pub has_decorations: bool,
        pub is_visible: bool,
        pub is_always_on_top: bool,
        pub is_resizable: bool,
        pub has_focus: bool,
        pub has_extended_window_frame: bool,
        pub has_blur_behind_window: bool,
        pub smooth_scroll_enabled: bool,
        pub autotab_enabled: bool,
    }

    /// Debugging information, will be rendered as an overlay on top of the UI
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzDebugState {
        pub profiler_dbg: bool,
        pub render_target_dbg: bool,
        pub texture_cache_dbg: bool,
        pub gpu_time_queries: bool,
        pub gpu_sample_queries: bool,
        pub disable_batching: bool,
        pub epochs: bool,
        pub echo_driver_messages: bool,
        pub show_overdraw: bool,
        pub gpu_cache_dbg: bool,
        pub texture_cache_dbg_clear_evicted: bool,
        pub picture_caching_dbg: bool,
        pub primitive_dbg: bool,
        pub zoom_dbg: bool,
        pub small_screen: bool,
        pub disable_opaque_pass: bool,
        pub disable_alpha_pass: bool,
        pub disable_clip_masks: bool,
        pub disable_text_prims: bool,
        pub disable_gradient_prims: bool,
        pub obscure_images: bool,
        pub glyph_flashing: bool,
        pub smart_profiler: bool,
        pub invalidation_dbg: bool,
        pub tile_cache_logging_dbg: bool,
        pub profiler_capture: bool,
        pub force_picture_invalidation: bool,
    }

    /// Current icon of the mouse cursor
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzMouseCursorType {
        Default,
        Crosshair,
        Hand,
        Arrow,
        Move,
        Text,
        Wait,
        Help,
        Progress,
        NotAllowed,
        ContextMenu,
        Cell,
        VerticalText,
        Alias,
        Copy,
        NoDrop,
        Grab,
        Grabbing,
        AllScroll,
        ZoomIn,
        ZoomOut,
        EResize,
        NResize,
        NeResize,
        NwResize,
        SResize,
        SeResize,
        SwResize,
        WResize,
        EwResize,
        NsResize,
        NeswResize,
        NwseResize,
        ColResize,
        RowResize,
    }

    /// Renderer type of the current windows OpenGL context
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzRendererType {
        Hardware,
        Software,
    }

    /// Re-export of rust-allocated (stack based) `MacWindowOptions` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzMacWindowOptions {
        pub _reserved: u8,
    }

    /// Re-export of rust-allocated (stack based) `WasmWindowOptions` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzWasmWindowOptions {
        pub _reserved: u8,
    }

    /// Re-export of rust-allocated (stack based) `FullScreenMode` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzFullScreenMode {
        SlowFullScreen,
        FastFullScreen,
        SlowWindowed,
        FastWindowed,
    }

    /// Window theme, set by the operating system or `WindowCreateOptions.theme` on startup
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzWindowTheme {
        DarkMode,
        LightMode,
    }

    /// Current state of touch devices / touch inputs
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzTouchState {
        pub unused: u8,
    }

    /// C-ABI stable wrapper over a `MarshaledLayoutCallbackInner`
    #[repr(C)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub struct AzMarshaledLayoutCallbackInner {
        pub cb: AzMarshaledLayoutCallbackType,
    }

    /// `AzMarshaledLayoutCallbackType` struct
    pub type AzMarshaledLayoutCallbackType = extern "C" fn(&mut AzRefAny, &mut AzRefAny, AzLayoutCallbackInfo) -> AzStyledDom;

    /// C-ABI stable wrapper over a `LayoutCallbackType`
    #[repr(C)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub struct AzLayoutCallbackInner {
        pub cb: AzLayoutCallbackType,
    }

    /// `AzLayoutCallbackType` struct
    pub type AzLayoutCallbackType = extern "C" fn(&mut AzRefAny, AzLayoutCallbackInfo) -> AzStyledDom;

    /// C-ABI stable wrapper over a `CallbackType`
    #[repr(C)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub struct AzCallback {
        pub cb: AzCallbackType,
    }

    /// `AzCallbackType` struct
    pub type AzCallbackType = extern "C" fn(&mut AzRefAny, AzCallbackInfo) -> AzUpdate;

    /// Specifies if the screen should be updated after the callback function has returned
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzUpdate {
        DoNothing,
        RefreshDom,
        RefreshDomAllWindows,
    }

    /// Index of a Node in the internal `NodeDataContainer`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzNodeId {
        pub inner: usize,
    }

    /// ID of a DOM - one window can contain multiple, nested DOMs (such as iframes)
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzDomId {
        pub inner: usize,
    }

    /// Re-export of rust-allocated (stack based) `PositionInfoInner` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzPositionInfoInner {
        pub x_offset: f32,
        pub y_offset: f32,
        pub static_x_offset: f32,
        pub static_y_offset: f32,
    }

    /// How should an animation repeat (loop, ping-pong, etc.)
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzAnimationRepeat {
        NoRepeat,
        Loop,
        PingPong,
    }

    /// How many times should an animation repeat
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzAnimationRepeatCount {
        Times(usize),
        Infinite,
    }

    /// C-ABI wrapper over an `IFrameCallbackType`
    #[repr(C)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub struct AzIFrameCallback {
        pub cb: AzIFrameCallbackType,
    }

    /// `AzIFrameCallbackType` struct
    pub type AzIFrameCallbackType = extern "C" fn(&mut AzRefAny, AzIFrameCallbackInfo) -> AzIFrameCallbackReturn;

    /// Re-export of rust-allocated (stack based) `RenderImageCallback` struct
    #[repr(C)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub struct AzRenderImageCallback {
        pub cb: AzRenderImageCallbackType,
    }

    /// `AzRenderImageCallbackType` struct
    pub type AzRenderImageCallbackType = extern "C" fn(&mut AzRefAny, AzRenderImageCallbackInfo) -> AzImageRef;

    /// Re-export of rust-allocated (stack based) `TimerCallback` struct
    #[repr(C)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub struct AzTimerCallback {
        pub cb: AzTimerCallbackType,
    }

    /// `AzTimerCallbackType` struct
    pub type AzTimerCallbackType = extern "C" fn(&mut AzRefAny, &mut AzRefAny, AzTimerCallbackInfo) -> AzTimerCallbackReturn;

    /// `AzWriteBackCallbackType` struct
    pub type AzWriteBackCallbackType = extern "C" fn(&mut AzRefAny, AzRefAny, AzCallbackInfo) -> AzUpdate;

    /// Re-export of rust-allocated (stack based) `WriteBackCallback` struct
    #[repr(C)]
    #[derive(Clone)]
    pub struct AzWriteBackCallback {
        pub cb: AzWriteBackCallbackType,
    }

    /// Re-export of rust-allocated (stack based) `ThreadCallback` struct
    #[repr(C)]
    #[derive(Clone)]
    pub struct AzThreadCallback {
        pub cb: AzThreadCallbackType,
    }

    /// `AzThreadCallbackType` struct
    pub type AzThreadCallbackType = extern "C" fn(AzRefAny, AzThreadSender, AzThreadReceiver);

    /// `AzRefAnyDestructorType` struct
    pub type AzRefAnyDestructorType = extern "C" fn(&mut c_void);

    /// Re-export of rust-allocated (stack based) `RefCount` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzRefCount {
        pub(crate) ptr: *const c_void,
    }

    /// When to call a callback action - `On::MouseOver`, `On::MouseOut`, etc.
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOn {
        MouseOver,
        MouseDown,
        LeftMouseDown,
        MiddleMouseDown,
        RightMouseDown,
        MouseUp,
        LeftMouseUp,
        MiddleMouseUp,
        RightMouseUp,
        MouseEnter,
        MouseLeave,
        Scroll,
        TextInput,
        VirtualKeyDown,
        VirtualKeyUp,
        HoveredFile,
        DroppedFile,
        HoveredFileCancelled,
        FocusReceived,
        FocusLost,
    }

    /// Re-export of rust-allocated (stack based) `HoverEventFilter` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzHoverEventFilter {
        MouseOver,
        MouseDown,
        LeftMouseDown,
        RightMouseDown,
        MiddleMouseDown,
        MouseUp,
        LeftMouseUp,
        RightMouseUp,
        MiddleMouseUp,
        MouseEnter,
        MouseLeave,
        Scroll,
        ScrollStart,
        ScrollEnd,
        TextInput,
        VirtualKeyDown,
        VirtualKeyUp,
        HoveredFile,
        DroppedFile,
        HoveredFileCancelled,
        TouchStart,
        TouchMove,
        TouchEnd,
        TouchCancel,
    }

    /// Re-export of rust-allocated (stack based) `FocusEventFilter` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzFocusEventFilter {
        MouseOver,
        MouseDown,
        LeftMouseDown,
        RightMouseDown,
        MiddleMouseDown,
        MouseUp,
        LeftMouseUp,
        RightMouseUp,
        MiddleMouseUp,
        MouseEnter,
        MouseLeave,
        Scroll,
        ScrollStart,
        ScrollEnd,
        TextInput,
        VirtualKeyDown,
        VirtualKeyUp,
        FocusReceived,
        FocusLost,
    }

    /// Re-export of rust-allocated (stack based) `WindowEventFilter` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzWindowEventFilter {
        MouseOver,
        MouseDown,
        LeftMouseDown,
        RightMouseDown,
        MiddleMouseDown,
        MouseUp,
        LeftMouseUp,
        RightMouseUp,
        MiddleMouseUp,
        MouseEnter,
        MouseLeave,
        Scroll,
        ScrollStart,
        ScrollEnd,
        TextInput,
        VirtualKeyDown,
        VirtualKeyUp,
        HoveredFile,
        DroppedFile,
        HoveredFileCancelled,
        Resized,
        Moved,
        TouchStart,
        TouchMove,
        TouchEnd,
        TouchCancel,
        FocusReceived,
        FocusLost,
        CloseRequested,
        ThemeChanged,
    }

    /// Re-export of rust-allocated (stack based) `ComponentEventFilter` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzComponentEventFilter {
        AfterMount,
        BeforeUnmount,
        NodeResized,
        DefaultAction,
        Selected,
    }

    /// Re-export of rust-allocated (stack based) `ApplicationEventFilter` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzApplicationEventFilter {
        DeviceConnected,
        DeviceDisconnected,
    }

    /// MSAA Accessibility role constants. For information on what each role does, see the <a href="https://docs.microsoft.com/en-us/windows/win32/winauto/object-roles">MSDN Role Constants page</a>
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzAccessibilityRole {
        TitleBar,
        MenuBar,
        ScrollBar,
        Grip,
        Sound,
        Cursor,
        Caret,
        Alert,
        Window,
        Client,
        MenuPopup,
        MenuItem,
        Tooltip,
        Application,
        Document,
        Pane,
        Chart,
        Dialog,
        Border,
        Grouping,
        Separator,
        Toolbar,
        StatusBar,
        Table,
        ColumnHeader,
        RowHeader,
        Column,
        Row,
        Cell,
        Link,
        HelpBalloon,
        Character,
        List,
        ListItem,
        Outline,
        OutlineItem,
        Pagetab,
        PropertyPage,
        Indicator,
        Graphic,
        StaticText,
        Text,
        PushButton,
        CheckButton,
        RadioButton,
        ComboBox,
        DropList,
        ProgressBar,
        Dial,
        HotkeyField,
        Slider,
        SpinButton,
        Diagram,
        Animation,
        Equation,
        ButtonDropdown,
        ButtonMenu,
        ButtonDropdownGrid,
        Whitespace,
        PageTabList,
        Clock,
        SplitButton,
        IpAddress,
        Nothing,
    }

    /// MSAA accessibility state. For information on what each state does, see the <a href="https://docs.microsoft.com/en-us/windows/win32/winauto/object-state-constants">MSDN State Constants page</a>.
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzAccessibilityState {
        Unavailable,
        Selected,
        Focused,
        Checked,
        Readonly,
        Default,
        Expanded,
        Collapsed,
        Busy,
        Offscreen,
        Focusable,
        Selectable,
        Linked,
        Traversed,
        Multiselectable,
        Protected,
    }

    /// Re-export of rust-allocated (stack based) `TabIndex` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzTabIndex {
        Auto,
        OverrideInParent(u32),
        NoKeyboardFocus,
    }

    /// Describes the state of a menu item
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzMenuItemState {
        Normal,
        Greyed,
        Disabled,
    }

    /// Re-export of rust-allocated (stack based) `NodeTypeKey` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzNodeTypeKey {
        Body,
        Div,
        Br,
        P,
        Img,
        IFrame,
    }

    /// Re-export of rust-allocated (stack based) `CssNthChildPattern` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzCssNthChildPattern {
        pub repeat: u32,
        pub offset: u32,
    }

    /// Re-export of rust-allocated (stack based) `CssPropertyType` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzCssPropertyType {
        TextColor,
        FontSize,
        FontFamily,
        TextAlign,
        LetterSpacing,
        LineHeight,
        WordSpacing,
        TabWidth,
        Cursor,
        Display,
        Float,
        BoxSizing,
        Width,
        Height,
        MinWidth,
        MinHeight,
        MaxWidth,
        MaxHeight,
        Position,
        Top,
        Right,
        Left,
        Bottom,
        FlexWrap,
        FlexDirection,
        FlexGrow,
        FlexShrink,
        JustifyContent,
        AlignItems,
        AlignContent,
        BackgroundContent,
        BackgroundPosition,
        BackgroundSize,
        BackgroundRepeat,
        OverflowX,
        OverflowY,
        PaddingTop,
        PaddingLeft,
        PaddingRight,
        PaddingBottom,
        MarginTop,
        MarginLeft,
        MarginRight,
        MarginBottom,
        BorderTopLeftRadius,
        BorderTopRightRadius,
        BorderBottomLeftRadius,
        BorderBottomRightRadius,
        BorderTopColor,
        BorderRightColor,
        BorderLeftColor,
        BorderBottomColor,
        BorderTopStyle,
        BorderRightStyle,
        BorderLeftStyle,
        BorderBottomStyle,
        BorderTopWidth,
        BorderRightWidth,
        BorderLeftWidth,
        BorderBottomWidth,
        BoxShadowLeft,
        BoxShadowRight,
        BoxShadowTop,
        BoxShadowBottom,
        ScrollbarStyle,
        Opacity,
        Transform,
        TransformOrigin,
        PerspectiveOrigin,
        BackfaceVisibility,
    }

    /// Re-export of rust-allocated (stack based) `ColorU` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzColorU {
        pub r: u8,
        pub g: u8,
        pub b: u8,
        pub a: u8,
    }

    /// Re-export of rust-allocated (stack based) `SizeMetric` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzSizeMetric {
        Px,
        Pt,
        Em,
        Percent,
    }

    /// Re-export of rust-allocated (stack based) `FloatValue` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzFloatValue {
        pub number: isize,
    }

    /// Re-export of rust-allocated (stack based) `BoxShadowClipMode` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzBoxShadowClipMode {
        Outset,
        Inset,
    }

    /// Re-export of rust-allocated (stack based) `LayoutAlignContent` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutAlignContent {
        Stretch,
        Center,
        Start,
        End,
        SpaceBetween,
        SpaceAround,
    }

    /// Re-export of rust-allocated (stack based) `LayoutAlignItems` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutAlignItems {
        Stretch,
        Center,
        FlexStart,
        FlexEnd,
    }

    /// Re-export of rust-allocated (stack based) `LayoutBoxSizing` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutBoxSizing {
        ContentBox,
        BorderBox,
    }

    /// Re-export of rust-allocated (stack based) `LayoutFlexDirection` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutFlexDirection {
        Row,
        RowReverse,
        Column,
        ColumnReverse,
    }

    /// Re-export of rust-allocated (stack based) `LayoutDisplay` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutDisplay {
        None,
        Flex,
        Block,
        InlineBlock,
    }

    /// Re-export of rust-allocated (stack based) `LayoutFloat` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutFloat {
        Left,
        Right,
    }

    /// Re-export of rust-allocated (stack based) `LayoutJustifyContent` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutJustifyContent {
        Start,
        End,
        Center,
        SpaceBetween,
        SpaceAround,
        SpaceEvenly,
    }

    /// Re-export of rust-allocated (stack based) `LayoutPosition` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutPosition {
        Static,
        Relative,
        Absolute,
        Fixed,
    }

    /// Re-export of rust-allocated (stack based) `LayoutFlexWrap` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutFlexWrap {
        Wrap,
        NoWrap,
    }

    /// Re-export of rust-allocated (stack based) `LayoutOverflow` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutOverflow {
        Scroll,
        Auto,
        Hidden,
        Visible,
    }

    /// Re-export of rust-allocated (stack based) `AngleMetric` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzAngleMetric {
        Degree,
        Radians,
        Grad,
        Turn,
        Percent,
    }

    /// Re-export of rust-allocated (stack based) `DirectionCorner` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzDirectionCorner {
        Right,
        Left,
        Top,
        Bottom,
        TopRight,
        TopLeft,
        BottomRight,
        BottomLeft,
    }

    /// Re-export of rust-allocated (stack based) `ExtendMode` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzExtendMode {
        Clamp,
        Repeat,
    }

    /// Re-export of rust-allocated (stack based) `Shape` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzShape {
        Ellipse,
        Circle,
    }

    /// Re-export of rust-allocated (stack based) `RadialGradientSize` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzRadialGradientSize {
        ClosestSide,
        ClosestCorner,
        FarthestSide,
        FarthestCorner,
    }

    /// Re-export of rust-allocated (stack based) `StyleBackgroundRepeat` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleBackgroundRepeat {
        NoRepeat,
        Repeat,
        RepeatX,
        RepeatY,
    }

    /// Re-export of rust-allocated (stack based) `BorderStyle` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzBorderStyle {
        None,
        Solid,
        Double,
        Dotted,
        Dashed,
        Hidden,
        Groove,
        Ridge,
        Inset,
        Outset,
    }

    /// Re-export of rust-allocated (stack based) `StyleCursor` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleCursor {
        Alias,
        AllScroll,
        Cell,
        ColResize,
        ContextMenu,
        Copy,
        Crosshair,
        Default,
        EResize,
        EwResize,
        Grab,
        Grabbing,
        Help,
        Move,
        NResize,
        NsResize,
        NeswResize,
        NwseResize,
        Pointer,
        Progress,
        RowResize,
        SResize,
        SeResize,
        Text,
        Unset,
        VerticalText,
        WResize,
        Wait,
        ZoomIn,
        ZoomOut,
    }

    /// Re-export of rust-allocated (stack based) `StyleBackfaceVisibility` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleBackfaceVisibility {
        Hidden,
        Visible,
    }

    /// Re-export of rust-allocated (stack based) `StyleTextAlign` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleTextAlign {
        Left,
        Center,
        Right,
    }

    /// Re-export of rust-allocated (stack based) `CheckBoxOnToggleCallback` struct
    #[repr(C)]
    #[derive(Clone)]
    pub struct AzCheckBoxOnToggleCallback {
        pub cb: AzCheckBoxOnToggleCallbackType,
    }

    /// `AzCheckBoxOnToggleCallbackType` struct
    pub type AzCheckBoxOnToggleCallbackType = extern "C" fn(&mut AzRefAny, &AzCheckBoxState, &mut AzCallbackInfo) -> AzUpdate;

    /// Re-export of rust-allocated (stack based) `CheckBoxState` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzCheckBoxState {
        pub checked: bool,
    }

    /// Re-export of rust-allocated (stack based) `ColorInputOnValueChangeCallback` struct
    #[repr(C)]
    #[derive(Clone)]
    pub struct AzColorInputOnValueChangeCallback {
        pub cb: AzColorInputOnValueChangeCallbackType,
    }

    /// `AzColorInputOnValueChangeCallbackType` struct
    pub type AzColorInputOnValueChangeCallbackType = extern "C" fn(&mut AzRefAny, &AzColorInputState, &mut AzCallbackInfo) -> AzUpdate;

    /// Re-export of rust-allocated (stack based) `TextInputSelectionRange` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzTextInputSelectionRange {
        pub from: usize,
        pub to: usize,
    }

    /// Re-export of rust-allocated (stack based) `TextInputOnTextInputCallback` struct
    #[repr(C)]
    #[derive(Clone)]
    pub struct AzTextInputOnTextInputCallback {
        pub cb: AzTextInputOnTextInputCallbackType,
    }

    /// `AzTextInputOnTextInputCallbackType` struct
    pub type AzTextInputOnTextInputCallbackType = extern "C" fn(&mut AzRefAny, &AzTextInputState, &mut AzCallbackInfo) -> AzOnTextInputReturn;

    /// Re-export of rust-allocated (stack based) `TextInputOnVirtualKeyDownCallback` struct
    #[repr(C)]
    #[derive(Clone)]
    pub struct AzTextInputOnVirtualKeyDownCallback {
        pub cb: AzTextInputOnVirtualKeyDownCallbackType,
    }

    /// `AzTextInputOnVirtualKeyDownCallbackType` struct
    pub type AzTextInputOnVirtualKeyDownCallbackType = extern "C" fn(&mut AzRefAny, &AzTextInputState, &mut AzCallbackInfo) -> AzOnTextInputReturn;

    /// Re-export of rust-allocated (stack based) `TextInputOnFocusLostCallback` struct
    #[repr(C)]
    #[derive(Clone)]
    pub struct AzTextInputOnFocusLostCallback {
        pub cb: AzTextInputOnFocusLostCallbackType,
    }

    /// `AzTextInputOnFocusLostCallbackType` struct
    pub type AzTextInputOnFocusLostCallbackType = extern "C" fn(&mut AzRefAny, &AzTextInputState, &mut AzCallbackInfo) -> AzUpdate;

    /// Re-export of rust-allocated (stack based) `TextInputValid` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzTextInputValid {
        Yes,
        No,
    }

    /// Re-export of rust-allocated (stack based) `NumberInputState` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzNumberInputState {
        pub previous: f32,
        pub number: f32,
        pub min: f32,
        pub max: f32,
    }

    /// Re-export of rust-allocated (stack based) `NumberInputOnValueChangeCallback` struct
    #[repr(C)]
    #[derive(Clone)]
    pub struct AzNumberInputOnValueChangeCallback {
        pub cb: AzNumberInputOnValueChangeCallbackType,
    }

    /// `AzNumberInputOnValueChangeCallbackType` struct
    pub type AzNumberInputOnValueChangeCallbackType = extern "C" fn(&mut AzRefAny, &AzNumberInputState, &mut AzCallbackInfo) -> AzUpdate;

    /// Re-export of rust-allocated (stack based) `Node` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzNode {
        pub parent: usize,
        pub previous_sibling: usize,
        pub next_sibling: usize,
        pub last_child: usize,
    }

    /// Re-export of rust-allocated (stack based) `CascadeInfo` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzCascadeInfo {
        pub index_in_parent: u32,
        pub is_last_child: bool,
    }

    /// Re-export of rust-allocated (stack based) `StyledNodeState` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyledNodeState {
        pub normal: bool,
        pub hover: bool,
        pub active: bool,
        pub focused: bool,
    }

    /// Re-export of rust-allocated (stack based) `TagId` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzTagId {
        pub inner: u64,
    }

    /// Re-export of rust-allocated (stack based) `CssPropertyCache` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzCssPropertyCache {
        pub(crate) ptr: *mut c_void,
    }

    /// Re-export of rust-allocated (stack based) `GlVoidPtrConst` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzGlVoidPtrConst {
        pub(crate) ptr: *const c_void,
    }

    /// Re-export of rust-allocated (stack based) `GlVoidPtrMut` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzGlVoidPtrMut {
        pub(crate) ptr: *mut c_void,
    }

    /// Re-export of rust-allocated (stack based) `GlShaderPrecisionFormatReturn` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzGlShaderPrecisionFormatReturn {
        pub _0: i32,
        pub _1: i32,
        pub _2: i32,
    }

    /// Re-export of rust-allocated (stack based) `VertexAttributeType` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzVertexAttributeType {
        Float,
        Double,
        UnsignedByte,
        UnsignedShort,
        UnsignedInt,
    }

    /// Re-export of rust-allocated (stack based) `IndexBufferFormat` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzIndexBufferFormat {
        Points,
        Lines,
        LineStrip,
        Triangles,
        TriangleStrip,
        TriangleFan,
    }

    /// Re-export of rust-allocated (stack based) `GlType` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzGlType {
        Gl,
        Gles,
    }

    /// C-ABI stable reexport of `&[u8]`
    #[repr(C)]
    pub struct AzU8VecRef {
        pub(crate) ptr: *const u8,
        pub len: usize,
    }

    /// C-ABI stable reexport of `&mut [u8]`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzU8VecRefMut {
        pub(crate) ptr: *mut u8,
        pub len: usize,
    }

    /// C-ABI stable reexport of `&[f32]`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzF32VecRef {
        pub(crate) ptr: *const f32,
        pub len: usize,
    }

    /// C-ABI stable reexport of `&[i32]`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzI32VecRef {
        pub(crate) ptr: *const i32,
        pub len: usize,
    }

    /// C-ABI stable reexport of `&[GLuint]` aka `&[u32]`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzGLuintVecRef {
        pub(crate) ptr: *const u32,
        pub len: usize,
    }

    /// C-ABI stable reexport of `&[GLenum]` aka `&[u32]`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzGLenumVecRef {
        pub(crate) ptr: *const u32,
        pub len: usize,
    }

    /// C-ABI stable reexport of `&mut [GLint]` aka `&mut [i32]`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzGLintVecRefMut {
        pub(crate) ptr: *mut i32,
        pub len: usize,
    }

    /// C-ABI stable reexport of `&mut [GLint64]` aka `&mut [i64]`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzGLint64VecRefMut {
        pub(crate) ptr: *mut i64,
        pub len: usize,
    }

    /// C-ABI stable reexport of `&mut [GLboolean]` aka `&mut [u8]`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzGLbooleanVecRefMut {
        pub(crate) ptr: *mut u8,
        pub len: usize,
    }

    /// C-ABI stable reexport of `&mut [GLfloat]` aka `&mut [f32]`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzGLfloatVecRefMut {
        pub(crate) ptr: *mut f32,
        pub len: usize,
    }

    /// C-ABI stable reexport of `&str`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzRefstr {
        pub(crate) ptr: *const u8,
        pub len: usize,
    }

    /// C-ABI stable reexport of `*const gleam::gl::GLsync`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzGLsyncPtr {
        pub(crate) ptr: *const c_void,
    }

    /// Re-export of rust-allocated (stack based) `TextureFlags` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzTextureFlags {
        pub is_opaque: bool,
        pub is_video_texture: bool,
    }

    /// Re-export of rust-allocated (stack based) `ImageRef` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzImageRef {
        pub data: *const c_void,
        pub copies: *const c_void,
    }

    /// Re-export of rust-allocated (stack based) `RawImageFormat` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzRawImageFormat {
        R8,
        R16,
        RG16,
        BGRA8,
        RGBAF32,
        RG8,
        RGBAI32,
        RGBA8,
    }

    /// Re-export of rust-allocated (stack based) `EncodeImageError` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzEncodeImageError {
        InsufficientMemory,
        DimensionError,
        InvalidData,
        Unknown,
    }

    /// Re-export of rust-allocated (stack based) `DecodeImageError` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzDecodeImageError {
        InsufficientMemory,
        DimensionError,
        UnsupportedImageFormat,
        Unknown,
    }

    /// `AzParsedFontDestructorFnType` struct
    pub type AzParsedFontDestructorFnType = extern "C" fn(&mut c_void);

    /// Atomically reference-counted parsed font data
    #[repr(C)]
    #[derive(Debug)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzFontRef {
        pub data: *const c_void,
        pub copies: *const c_void,
    }

    /// Re-export of rust-allocated (stack based) `Svg` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzSvg {
        pub(crate) ptr: *mut c_void,
    }

    /// Re-export of rust-allocated (stack based) `SvgXmlNode` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzSvgXmlNode {
        pub(crate) ptr: *mut c_void,
    }

    /// Re-export of rust-allocated (stack based) `SvgCircle` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzSvgCircle {
        pub center_x: f32,
        pub center_y: f32,
        pub radius: f32,
    }

    /// Re-export of rust-allocated (stack based) `SvgPoint` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzSvgPoint {
        pub x: f32,
        pub y: f32,
    }

    /// Re-export of rust-allocated (stack based) `SvgRect` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzSvgRect {
        pub width: f32,
        pub height: f32,
        pub x: f32,
        pub y: f32,
        pub radius_top_left: f32,
        pub radius_top_right: f32,
        pub radius_bottom_left: f32,
        pub radius_bottom_right: f32,
    }

    /// Re-export of rust-allocated (stack based) `SvgVertex` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzSvgVertex {
        pub x: f32,
        pub y: f32,
    }

    /// Re-export of rust-allocated (stack based) `ShapeRendering` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzShapeRendering {
        OptimizeSpeed,
        CrispEdges,
        GeometricPrecision,
    }

    /// Re-export of rust-allocated (stack based) `TextRendering` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzTextRendering {
        OptimizeSpeed,
        OptimizeLegibility,
        GeometricPrecision,
    }

    /// Re-export of rust-allocated (stack based) `ImageRendering` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzImageRendering {
        OptimizeQuality,
        OptimizeSpeed,
    }

    /// Re-export of rust-allocated (stack based) `FontDatabase` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzFontDatabase {
        Empty,
        System,
    }

    /// Re-export of rust-allocated (stack based) `Indent` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzIndent {
        None,
        Spaces(u8),
        Tabs,
    }

    /// Re-export of rust-allocated (stack based) `SvgFitTo` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzSvgFitTo {
        Original,
        Width(u32),
        Height(u32),
        Zoom(f32),
    }

    /// Re-export of rust-allocated (stack based) `SvgFillRule` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzSvgFillRule {
        Winding,
        EvenOdd,
    }

    /// Re-export of rust-allocated (stack based) `SvgTransform` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzSvgTransform {
        pub sx: f32,
        pub kx: f32,
        pub ky: f32,
        pub sy: f32,
        pub tx: f32,
        pub ty: f32,
    }

    /// Re-export of rust-allocated (stack based) `SvgLineJoin` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzSvgLineJoin {
        Miter,
        MiterClip,
        Round,
        Bevel,
    }

    /// Re-export of rust-allocated (stack based) `SvgLineCap` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzSvgLineCap {
        Butt,
        Square,
        Round,
    }

    /// Re-export of rust-allocated (stack based) `SvgDashPattern` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzSvgDashPattern {
        pub offset: f32,
        pub length_1: f32,
        pub gap_1: f32,
        pub length_2: f32,
        pub gap_2: f32,
        pub length_3: f32,
        pub gap_3: f32,
    }

    /// Re-export of rust-allocated (stack based) `MsgBox` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzMsgBox {
        pub _reserved: usize,
    }

    /// Type of message box icon
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzMsgBoxIcon {
        Info,
        Warning,
        Error,
        Question,
    }

    /// Value returned from a yes / no message box
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzMsgBoxYesNo {
        Yes,
        No,
    }

    /// Value returned from an ok / cancel message box
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzMsgBoxOkCancel {
        Ok,
        Cancel,
    }

    /// File picker dialog
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzFileDialog {
        pub _reserved: usize,
    }

    /// Re-export of rust-allocated (stack based) `ColorPickerDialog` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzColorPickerDialog {
        pub _reserved: usize,
    }

    /// Connection to the system clipboard, on some systems this connection can be cached
    #[repr(C)]
    #[derive(Debug)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzSystemClipboard {
        pub _native: *const c_void,
    }

    /// `AzInstantPtrCloneFnType` struct
    pub type AzInstantPtrCloneFnType = extern "C" fn(&AzInstantPtr) -> AzInstantPtr;

    /// Re-export of rust-allocated (stack based) `InstantPtrCloneFn` struct
    #[repr(C)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub struct AzInstantPtrCloneFn {
        pub cb: AzInstantPtrCloneFnType,
    }

    /// `AzInstantPtrDestructorFnType` struct
    pub type AzInstantPtrDestructorFnType = extern "C" fn(&mut AzInstantPtr);

    /// Re-export of rust-allocated (stack based) `InstantPtrDestructorFn` struct
    #[repr(C)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub struct AzInstantPtrDestructorFn {
        pub cb: AzInstantPtrDestructorFnType,
    }

    /// Re-export of rust-allocated (stack based) `SystemTick` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzSystemTick {
        pub tick_counter: u64,
    }

    /// Re-export of rust-allocated (stack based) `SystemTimeDiff` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzSystemTimeDiff {
        pub secs: u64,
        pub nanos: u32,
    }

    /// Re-export of rust-allocated (stack based) `SystemTickDiff` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzSystemTickDiff {
        pub tick_diff: u64,
    }

    /// Re-export of rust-allocated (stack based) `TimerId` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzTimerId {
        pub id: usize,
    }

    /// Should a timer terminate or not - used to remove active timers
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzTerminateTimer {
        Terminate,
        Continue,
    }

    /// Re-export of rust-allocated (stack based) `ThreadId` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzThreadId {
        pub id: usize,
    }

    /// Re-export of rust-allocated (stack based) `Thread` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzThread {
        pub(crate) ptr: *const c_void,
    }

    /// Re-export of rust-allocated (stack based) `ThreadSender` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzThreadSender {
        pub(crate) ptr: *const c_void,
    }

    /// Re-export of rust-allocated (stack based) `ThreadReceiver` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzThreadReceiver {
        pub(crate) ptr: *const c_void,
    }

    /// `AzCreateThreadFnType` struct
    pub type AzCreateThreadFnType = extern "C" fn(AzRefAny, AzRefAny, AzThreadCallback) -> AzThread;

    /// Re-export of rust-allocated (stack based) `CreateThreadFn` struct
    #[repr(C)]
    #[derive(Clone)]
    pub struct AzCreateThreadFn {
        pub cb: AzCreateThreadFnType,
    }

    /// `AzGetSystemTimeFnType` struct
    pub type AzGetSystemTimeFnType = extern "C" fn() -> AzInstant;

    /// Get the current system time, equivalent to `std::time::Instant::now()`, except it also works on systems that work with "ticks" instead of timers
    #[repr(C)]
    #[derive(Clone)]
    pub struct AzGetSystemTimeFn {
        pub cb: AzGetSystemTimeFnType,
    }

    /// `AzCheckThreadFinishedFnType` struct
    pub type AzCheckThreadFinishedFnType = extern "C" fn(&c_void) -> bool;

    /// Function called to check if the thread has finished
    #[repr(C)]
    #[derive(Clone)]
    pub struct AzCheckThreadFinishedFn {
        pub cb: AzCheckThreadFinishedFnType,
    }

    /// `AzLibrarySendThreadMsgFnType` struct
    pub type AzLibrarySendThreadMsgFnType = extern "C" fn(&c_void, AzThreadSendMsg) -> bool;

    /// Function to send a message to the thread
    #[repr(C)]
    #[derive(Clone)]
    pub struct AzLibrarySendThreadMsgFn {
        pub cb: AzLibrarySendThreadMsgFnType,
    }

    /// `AzLibraryReceiveThreadMsgFnType` struct
    pub type AzLibraryReceiveThreadMsgFnType = extern "C" fn(&c_void) -> AzOptionThreadReceiveMsg;

    /// Function to receive a message from the thread
    #[repr(C)]
    #[derive(Clone)]
    pub struct AzLibraryReceiveThreadMsgFn {
        pub cb: AzLibraryReceiveThreadMsgFnType,
    }

    /// `AzThreadRecvFnType` struct
    pub type AzThreadRecvFnType = extern "C" fn(&c_void) -> AzOptionThreadSendMsg;

    /// Function that the running `Thread` can call to receive messages from the main UI thread
    #[repr(C)]
    #[derive(Clone)]
    pub struct AzThreadRecvFn {
        pub cb: AzThreadRecvFnType,
    }

    /// `AzThreadSendFnType` struct
    pub type AzThreadSendFnType = extern "C" fn(&c_void, AzThreadReceiveMsg) -> bool;

    /// Function that the running `Thread` can call to receive messages from the main UI thread
    #[repr(C)]
    #[derive(Clone)]
    pub struct AzThreadSendFn {
        pub cb: AzThreadSendFnType,
    }

    /// `AzThreadDestructorFnType` struct
    pub type AzThreadDestructorFnType = extern "C" fn(&mut AzThread);

    /// Destructor of the `Thread`
    #[repr(C)]
    #[derive(Clone)]
    pub struct AzThreadDestructorFn {
        pub cb: AzThreadDestructorFnType,
    }

    /// `AzThreadReceiverDestructorFnType` struct
    pub type AzThreadReceiverDestructorFnType = extern "C" fn(&mut AzThreadReceiver);

    /// Destructor of the `ThreadReceiver`
    #[repr(C)]
    #[derive(Clone)]
    pub struct AzThreadReceiverDestructorFn {
        pub cb: AzThreadReceiverDestructorFnType,
    }

    /// `AzThreadSenderDestructorFnType` struct
    pub type AzThreadSenderDestructorFnType = extern "C" fn(&mut AzThreadSender);

    /// Destructor of the `ThreadSender`
    #[repr(C)]
    #[derive(Clone)]
    pub struct AzThreadSenderDestructorFn {
        pub cb: AzThreadSenderDestructorFnType,
    }

    /// Re-export of rust-allocated (stack based) `StyleFontFamilyVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzStyleFontFamilyVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzStyleFontFamilyVecDestructorType),
    }

    /// `AzStyleFontFamilyVecDestructorType` struct
    pub type AzStyleFontFamilyVecDestructorType = extern "C" fn(&mut AzStyleFontFamilyVec);

    /// Re-export of rust-allocated (stack based) `AccessibilityStateVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzAccessibilityStateVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzAccessibilityStateVecDestructorType),
    }

    /// `AzAccessibilityStateVecDestructorType` struct
    pub type AzAccessibilityStateVecDestructorType = extern "C" fn(&mut AzAccessibilityStateVec);

    /// Re-export of rust-allocated (stack based) `MenuItemVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzMenuItemVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzMenuItemVecDestructorType),
    }

    /// `AzMenuItemVecDestructorType` struct
    pub type AzMenuItemVecDestructorType = extern "C" fn(&mut AzMenuItemVec);

    /// Re-export of rust-allocated (stack based) `TesselatedSvgNodeVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzTesselatedSvgNodeVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzTesselatedSvgNodeVecDestructorType),
    }

    /// `AzTesselatedSvgNodeVecDestructorType` struct
    pub type AzTesselatedSvgNodeVecDestructorType = extern "C" fn(&mut AzTesselatedSvgNodeVec);

    /// Re-export of rust-allocated (stack based) `XmlNodeVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzXmlNodeVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzXmlNodeVecDestructorType),
    }

    /// `AzXmlNodeVecDestructorType` struct
    pub type AzXmlNodeVecDestructorType = extern "C" fn(&mut AzXmlNodeVec);

    /// Re-export of rust-allocated (stack based) `FmtArgVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzFmtArgVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzFmtArgVecDestructorType),
    }

    /// `AzFmtArgVecDestructorType` struct
    pub type AzFmtArgVecDestructorType = extern "C" fn(&mut AzFmtArgVec);

    /// Re-export of rust-allocated (stack based) `InlineLineVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzInlineLineVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzInlineLineVecDestructorType),
    }

    /// `AzInlineLineVecDestructorType` struct
    pub type AzInlineLineVecDestructorType = extern "C" fn(&mut AzInlineLineVec);

    /// Re-export of rust-allocated (stack based) `InlineWordVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzInlineWordVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzInlineWordVecDestructorType),
    }

    /// `AzInlineWordVecDestructorType` struct
    pub type AzInlineWordVecDestructorType = extern "C" fn(&mut AzInlineWordVec);

    /// Re-export of rust-allocated (stack based) `InlineGlyphVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzInlineGlyphVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzInlineGlyphVecDestructorType),
    }

    /// `AzInlineGlyphVecDestructorType` struct
    pub type AzInlineGlyphVecDestructorType = extern "C" fn(&mut AzInlineGlyphVec);

    /// Re-export of rust-allocated (stack based) `InlineTextHitVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzInlineTextHitVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzInlineTextHitVecDestructorType),
    }

    /// `AzInlineTextHitVecDestructorType` struct
    pub type AzInlineTextHitVecDestructorType = extern "C" fn(&mut AzInlineTextHitVec);

    /// Re-export of rust-allocated (stack based) `MonitorVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzMonitorVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzMonitorVecDestructorType),
    }

    /// `AzMonitorVecDestructorType` struct
    pub type AzMonitorVecDestructorType = extern "C" fn(&mut AzMonitorVec);

    /// Re-export of rust-allocated (stack based) `VideoModeVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzVideoModeVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzVideoModeVecDestructorType),
    }

    /// `AzVideoModeVecDestructorType` struct
    pub type AzVideoModeVecDestructorType = extern "C" fn(&mut AzVideoModeVec);

    /// Re-export of rust-allocated (stack based) `DomVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzDomVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzDomVecDestructorType),
    }

    /// `AzDomVecDestructorType` struct
    pub type AzDomVecDestructorType = extern "C" fn(&mut AzDomVec);

    /// Re-export of rust-allocated (stack based) `IdOrClassVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzIdOrClassVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzIdOrClassVecDestructorType),
    }

    /// `AzIdOrClassVecDestructorType` struct
    pub type AzIdOrClassVecDestructorType = extern "C" fn(&mut AzIdOrClassVec);

    /// Re-export of rust-allocated (stack based) `NodeDataInlineCssPropertyVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzNodeDataInlineCssPropertyVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzNodeDataInlineCssPropertyVecDestructorType),
    }

    /// `AzNodeDataInlineCssPropertyVecDestructorType` struct
    pub type AzNodeDataInlineCssPropertyVecDestructorType = extern "C" fn(&mut AzNodeDataInlineCssPropertyVec);

    /// Re-export of rust-allocated (stack based) `StyleBackgroundContentVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzStyleBackgroundContentVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzStyleBackgroundContentVecDestructorType),
    }

    /// `AzStyleBackgroundContentVecDestructorType` struct
    pub type AzStyleBackgroundContentVecDestructorType = extern "C" fn(&mut AzStyleBackgroundContentVec);

    /// Re-export of rust-allocated (stack based) `StyleBackgroundPositionVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzStyleBackgroundPositionVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzStyleBackgroundPositionVecDestructorType),
    }

    /// `AzStyleBackgroundPositionVecDestructorType` struct
    pub type AzStyleBackgroundPositionVecDestructorType = extern "C" fn(&mut AzStyleBackgroundPositionVec);

    /// Re-export of rust-allocated (stack based) `StyleBackgroundRepeatVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzStyleBackgroundRepeatVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzStyleBackgroundRepeatVecDestructorType),
    }

    /// `AzStyleBackgroundRepeatVecDestructorType` struct
    pub type AzStyleBackgroundRepeatVecDestructorType = extern "C" fn(&mut AzStyleBackgroundRepeatVec);

    /// Re-export of rust-allocated (stack based) `StyleBackgroundSizeVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzStyleBackgroundSizeVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzStyleBackgroundSizeVecDestructorType),
    }

    /// `AzStyleBackgroundSizeVecDestructorType` struct
    pub type AzStyleBackgroundSizeVecDestructorType = extern "C" fn(&mut AzStyleBackgroundSizeVec);

    /// Re-export of rust-allocated (stack based) `StyleTransformVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzStyleTransformVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzStyleTransformVecDestructorType),
    }

    /// `AzStyleTransformVecDestructorType` struct
    pub type AzStyleTransformVecDestructorType = extern "C" fn(&mut AzStyleTransformVec);

    /// Re-export of rust-allocated (stack based) `CssPropertyVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzCssPropertyVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzCssPropertyVecDestructorType),
    }

    /// `AzCssPropertyVecDestructorType` struct
    pub type AzCssPropertyVecDestructorType = extern "C" fn(&mut AzCssPropertyVec);

    /// Re-export of rust-allocated (stack based) `SvgMultiPolygonVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzSvgMultiPolygonVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzSvgMultiPolygonVecDestructorType),
    }

    /// `AzSvgMultiPolygonVecDestructorType` struct
    pub type AzSvgMultiPolygonVecDestructorType = extern "C" fn(&mut AzSvgMultiPolygonVec);

    /// Re-export of rust-allocated (stack based) `SvgPathVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzSvgPathVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzSvgPathVecDestructorType),
    }

    /// `AzSvgPathVecDestructorType` struct
    pub type AzSvgPathVecDestructorType = extern "C" fn(&mut AzSvgPathVec);

    /// Re-export of rust-allocated (stack based) `VertexAttributeVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzVertexAttributeVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzVertexAttributeVecDestructorType),
    }

    /// `AzVertexAttributeVecDestructorType` struct
    pub type AzVertexAttributeVecDestructorType = extern "C" fn(&mut AzVertexAttributeVec);

    /// Re-export of rust-allocated (stack based) `SvgPathElementVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzSvgPathElementVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzSvgPathElementVecDestructorType),
    }

    /// `AzSvgPathElementVecDestructorType` struct
    pub type AzSvgPathElementVecDestructorType = extern "C" fn(&mut AzSvgPathElementVec);

    /// Re-export of rust-allocated (stack based) `SvgVertexVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzSvgVertexVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzSvgVertexVecDestructorType),
    }

    /// `AzSvgVertexVecDestructorType` struct
    pub type AzSvgVertexVecDestructorType = extern "C" fn(&mut AzSvgVertexVec);

    /// Re-export of rust-allocated (stack based) `U32VecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzU32VecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzU32VecDestructorType),
    }

    /// `AzU32VecDestructorType` struct
    pub type AzU32VecDestructorType = extern "C" fn(&mut AzU32Vec);

    /// Re-export of rust-allocated (stack based) `XWindowTypeVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzXWindowTypeVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzXWindowTypeVecDestructorType),
    }

    /// `AzXWindowTypeVecDestructorType` struct
    pub type AzXWindowTypeVecDestructorType = extern "C" fn(&mut AzXWindowTypeVec);

    /// Re-export of rust-allocated (stack based) `VirtualKeyCodeVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzVirtualKeyCodeVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzVirtualKeyCodeVecDestructorType),
    }

    /// `AzVirtualKeyCodeVecDestructorType` struct
    pub type AzVirtualKeyCodeVecDestructorType = extern "C" fn(&mut AzVirtualKeyCodeVec);

    /// Re-export of rust-allocated (stack based) `CascadeInfoVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzCascadeInfoVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzCascadeInfoVecDestructorType),
    }

    /// `AzCascadeInfoVecDestructorType` struct
    pub type AzCascadeInfoVecDestructorType = extern "C" fn(&mut AzCascadeInfoVec);

    /// Re-export of rust-allocated (stack based) `ScanCodeVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzScanCodeVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzScanCodeVecDestructorType),
    }

    /// `AzScanCodeVecDestructorType` struct
    pub type AzScanCodeVecDestructorType = extern "C" fn(&mut AzScanCodeVec);

    /// Re-export of rust-allocated (stack based) `CssDeclarationVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzCssDeclarationVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzCssDeclarationVecDestructorType),
    }

    /// `AzCssDeclarationVecDestructorType` struct
    pub type AzCssDeclarationVecDestructorType = extern "C" fn(&mut AzCssDeclarationVec);

    /// Re-export of rust-allocated (stack based) `CssPathSelectorVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzCssPathSelectorVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzCssPathSelectorVecDestructorType),
    }

    /// `AzCssPathSelectorVecDestructorType` struct
    pub type AzCssPathSelectorVecDestructorType = extern "C" fn(&mut AzCssPathSelectorVec);

    /// Re-export of rust-allocated (stack based) `StylesheetVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzStylesheetVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzStylesheetVecDestructorType),
    }

    /// `AzStylesheetVecDestructorType` struct
    pub type AzStylesheetVecDestructorType = extern "C" fn(&mut AzStylesheetVec);

    /// Re-export of rust-allocated (stack based) `CssRuleBlockVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzCssRuleBlockVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzCssRuleBlockVecDestructorType),
    }

    /// `AzCssRuleBlockVecDestructorType` struct
    pub type AzCssRuleBlockVecDestructorType = extern "C" fn(&mut AzCssRuleBlockVec);

    /// Re-export of rust-allocated (stack based) `F32VecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzF32VecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzF32VecDestructorType),
    }

    /// `AzF32VecDestructorType` struct
    pub type AzF32VecDestructorType = extern "C" fn(&mut AzF32Vec);

    /// Re-export of rust-allocated (stack based) `U16VecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzU16VecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzU16VecDestructorType),
    }

    /// `AzU16VecDestructorType` struct
    pub type AzU16VecDestructorType = extern "C" fn(&mut AzU16Vec);

    /// Re-export of rust-allocated (stack based) `U8VecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzU8VecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzU8VecDestructorType),
    }

    /// `AzU8VecDestructorType` struct
    pub type AzU8VecDestructorType = extern "C" fn(&mut AzU8Vec);

    /// Re-export of rust-allocated (stack based) `CallbackDataVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzCallbackDataVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzCallbackDataVecDestructorType),
    }

    /// `AzCallbackDataVecDestructorType` struct
    pub type AzCallbackDataVecDestructorType = extern "C" fn(&mut AzCallbackDataVec);

    /// Re-export of rust-allocated (stack based) `DebugMessageVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzDebugMessageVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzDebugMessageVecDestructorType),
    }

    /// `AzDebugMessageVecDestructorType` struct
    pub type AzDebugMessageVecDestructorType = extern "C" fn(&mut AzDebugMessageVec);

    /// Re-export of rust-allocated (stack based) `GLuintVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzGLuintVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzGLuintVecDestructorType),
    }

    /// `AzGLuintVecDestructorType` struct
    pub type AzGLuintVecDestructorType = extern "C" fn(&mut AzGLuintVec);

    /// Re-export of rust-allocated (stack based) `GLintVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzGLintVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzGLintVecDestructorType),
    }

    /// `AzGLintVecDestructorType` struct
    pub type AzGLintVecDestructorType = extern "C" fn(&mut AzGLintVec);

    /// Re-export of rust-allocated (stack based) `StringVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzStringVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzStringVecDestructorType),
    }

    /// `AzStringVecDestructorType` struct
    pub type AzStringVecDestructorType = extern "C" fn(&mut AzStringVec);

    /// Re-export of rust-allocated (stack based) `StringPairVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzStringPairVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzStringPairVecDestructorType),
    }

    /// `AzStringPairVecDestructorType` struct
    pub type AzStringPairVecDestructorType = extern "C" fn(&mut AzStringPairVec);

    /// Re-export of rust-allocated (stack based) `NormalizedLinearColorStopVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzNormalizedLinearColorStopVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzNormalizedLinearColorStopVecDestructorType),
    }

    /// `AzNormalizedLinearColorStopVecDestructorType` struct
    pub type AzNormalizedLinearColorStopVecDestructorType = extern "C" fn(&mut AzNormalizedLinearColorStopVec);

    /// Re-export of rust-allocated (stack based) `NormalizedRadialColorStopVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzNormalizedRadialColorStopVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzNormalizedRadialColorStopVecDestructorType),
    }

    /// `AzNormalizedRadialColorStopVecDestructorType` struct
    pub type AzNormalizedRadialColorStopVecDestructorType = extern "C" fn(&mut AzNormalizedRadialColorStopVec);

    /// Re-export of rust-allocated (stack based) `NodeIdVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzNodeIdVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzNodeIdVecDestructorType),
    }

    /// `AzNodeIdVecDestructorType` struct
    pub type AzNodeIdVecDestructorType = extern "C" fn(&mut AzNodeIdVec);

    /// Re-export of rust-allocated (stack based) `NodeVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzNodeVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzNodeVecDestructorType),
    }

    /// `AzNodeVecDestructorType` struct
    pub type AzNodeVecDestructorType = extern "C" fn(&mut AzNodeVec);

    /// Re-export of rust-allocated (stack based) `StyledNodeVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzStyledNodeVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzStyledNodeVecDestructorType),
    }

    /// `AzStyledNodeVecDestructorType` struct
    pub type AzStyledNodeVecDestructorType = extern "C" fn(&mut AzStyledNodeVec);

    /// Re-export of rust-allocated (stack based) `TagIdToNodeIdMappingVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzTagIdToNodeIdMappingVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzTagIdToNodeIdMappingVecDestructorType),
    }

    /// `AzTagIdToNodeIdMappingVecDestructorType` struct
    pub type AzTagIdToNodeIdMappingVecDestructorType = extern "C" fn(&mut AzTagIdToNodeIdMappingVec);

    /// Re-export of rust-allocated (stack based) `ParentWithNodeDepthVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzParentWithNodeDepthVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzParentWithNodeDepthVecDestructorType),
    }

    /// `AzParentWithNodeDepthVecDestructorType` struct
    pub type AzParentWithNodeDepthVecDestructorType = extern "C" fn(&mut AzParentWithNodeDepthVec);

    /// Re-export of rust-allocated (stack based) `NodeDataVecDestructor` struct
    #[repr(C, u8)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub enum AzNodeDataVecDestructor {
        DefaultRust,
        NoDestructor,
        External(AzNodeDataVecDestructorType),
    }

    /// `AzNodeDataVecDestructorType` struct
    pub type AzNodeDataVecDestructorType = extern "C" fn(&mut AzNodeDataVec);

    /// Re-export of rust-allocated (stack based) `OptionI16` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionI16 {
        None,
        Some(i16),
    }

    /// Re-export of rust-allocated (stack based) `OptionU16` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionU16 {
        None,
        Some(u16),
    }

    /// Re-export of rust-allocated (stack based) `OptionU32` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionU32 {
        None,
        Some(u32),
    }

    /// Re-export of rust-allocated (stack based) `OptionHwndHandle` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionHwndHandle {
        None,
        Some(*mut c_void),
    }

    /// Re-export of rust-allocated (stack based) `OptionX11Visual` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionX11Visual {
        None,
        Some(*const c_void),
    }

    /// Re-export of rust-allocated (stack based) `OptionI32` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionI32 {
        None,
        Some(i32),
    }

    /// Re-export of rust-allocated (stack based) `OptionF32` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionF32 {
        None,
        Some(f32),
    }

    /// Option<char> but the char is a u32, for C FFI stability reasons
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionChar {
        None,
        Some(u32),
    }

    /// Re-export of rust-allocated (stack based) `OptionUsize` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionUsize {
        None,
        Some(usize),
    }

    /// Re-export of rust-allocated (stack based) `SvgParseErrorPosition` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzSvgParseErrorPosition {
        pub row: u32,
        pub col: u32,
    }

    /// External system callbacks to get the system time or create / manage threads
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzSystemCallbacks {
        pub create_thread_fn: AzCreateThreadFn,
        pub get_system_time_fn: AzGetSystemTimeFn,
    }

    /// Force a specific renderer: note that azul will **crash** on startup if the `RendererOptions` are not satisfied.
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzRendererOptions {
        pub vsync: AzVsync,
        pub srgb: AzSrgb,
        pub hw_accel: AzHwAcceleration,
    }

    /// Represents a rectangle in physical pixels (integer units)
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutRect {
        pub origin: AzLayoutPoint,
        pub size: AzLayoutSize,
    }

    /// Raw platform handle, for integration in / with other toolkits and custom non-azul window extensions
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzRawWindowHandle {
        IOS(AzIOSHandle),
        MacOS(AzMacOSHandle),
        Xlib(AzXlibHandle),
        Xcb(AzXcbHandle),
        Wayland(AzWaylandHandle),
        Windows(AzWindowsHandle),
        Web(AzWebHandle),
        Android(AzAndroidHandle),
        Unsupported,
    }

    /// Logical rectangle area (can differ based on HiDPI settings). Usually this is what you'd want for hit-testing and positioning elements.
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLogicalRect {
        pub origin: AzLogicalPosition,
        pub size: AzLogicalSize,
    }

    /// Symbolic accelerator key (ctrl, alt, shift)
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzAcceleratorKey {
        Ctrl,
        Alt,
        Shift,
        Key(AzVirtualKeyCode),
    }

    /// Current position of the mouse cursor, relative to the window. Set to `Uninitialized` on startup (gets initialized on the first frame).
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzCursorPosition {
        OutOfWindow,
        Uninitialized,
        InWindow(AzLogicalPosition),
    }

    /// Position of the top left corner of the window relative to the top left of the monitor
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzWindowPosition {
        Uninitialized,
        Initialized(AzPhysicalPositionI32),
    }

    /// Position of the virtual keyboard necessary to insert CJK characters
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzImePosition {
        Uninitialized,
        Initialized(AzLogicalPosition),
    }

    /// Describes a rendering configuration for a monitor
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzVideoMode {
        pub size: AzLayoutSize,
        pub bit_depth: u16,
        pub refresh_rate: u16,
    }

    /// Combination of node ID + DOM ID, both together can identify a node
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzDomNodeId {
        pub dom: AzDomId,
        pub node: AzNodeId,
    }

    /// Re-export of rust-allocated (stack based) `PositionInfo` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzPositionInfo {
        Static(AzPositionInfoInner),
        Fixed(AzPositionInfoInner),
        Absolute(AzPositionInfoInner),
        Relative(AzPositionInfoInner),
    }

    /// Re-export of rust-allocated (stack based) `HidpiAdjustedBounds` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzHidpiAdjustedBounds {
        pub logical_size: AzLogicalSize,
        pub hidpi_factor: f32,
    }

    /// Re-export of rust-allocated (stack based) `InlineGlyph` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzInlineGlyph {
        pub bounds: AzLogicalRect,
        pub unicode_codepoint: AzOptionChar,
        pub glyph_index: u32,
    }

    /// Re-export of rust-allocated (stack based) `InlineTextHit` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzInlineTextHit {
        pub unicode_codepoint: AzOptionChar,
        pub hit_relative_to_inline_text: AzLogicalPosition,
        pub hit_relative_to_line: AzLogicalPosition,
        pub hit_relative_to_text_content: AzLogicalPosition,
        pub hit_relative_to_glyph: AzLogicalPosition,
        pub line_index_relative_to_text: usize,
        pub word_index_relative_to_text: usize,
        pub text_content_index_relative_to_text: usize,
        pub glyph_index_relative_to_text: usize,
        pub char_index_relative_to_text: usize,
        pub word_index_relative_to_line: usize,
        pub text_content_index_relative_to_line: usize,
        pub glyph_index_relative_to_line: usize,
        pub char_index_relative_to_line: usize,
        pub glyph_index_relative_to_word: usize,
        pub char_index_relative_to_word: usize,
    }

    /// Re-export of rust-allocated (stack based) `IFrameCallbackInfo` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzIFrameCallbackInfo {
        pub system_fonts: *const c_void,
        pub image_cache: *const c_void,
        pub window_theme: AzWindowTheme,
        pub bounds: AzHidpiAdjustedBounds,
        pub scroll_size: AzLogicalSize,
        pub scroll_offset: AzLogicalPosition,
        pub virtual_scroll_size: AzLogicalSize,
        pub virtual_scroll_offset: AzLogicalPosition,
        pub _reserved_ref: *const c_void,
        pub _reserved_mut: *mut c_void,
    }

    /// Re-export of rust-allocated (stack based) `TimerCallbackReturn` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzTimerCallbackReturn {
        pub should_update: AzUpdate,
        pub should_terminate: AzTerminateTimer,
    }

    /// RefAny is a reference-counted, opaque pointer, which stores a reference to a struct. `RefAny` can be up- and downcasted (this usually done via generics and can't be expressed in the Rust API)
    #[repr(C)]
    #[derive(Debug)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzRefAny {
        pub _internal_ptr: *const c_void,
        pub sharing_info: AzRefCount,
    }

    /// Re-export of rust-allocated (stack based) `IFrameNode` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzIFrameNode {
        pub callback: AzIFrameCallback,
        pub data: AzRefAny,
    }

    /// Re-export of rust-allocated (stack based) `NotEventFilter` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzNotEventFilter {
        Hover(AzHoverEventFilter),
        Focus(AzFocusEventFilter),
    }

    /// Similar to `dom.CallbackData`, stores some data + a callback to call when the menu is activated
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzMenuCallback {
        pub callback: AzCallback,
        pub data: AzRefAny,
    }

    /// Icon of a menu entry
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzMenuItemIcon {
        Checkbox(bool),
        Image(AzImageRef),
    }

    /// Re-export of rust-allocated (stack based) `CssNthChildSelector` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzCssNthChildSelector {
        Number(u32),
        Even,
        Odd,
        Pattern(AzCssNthChildPattern),
    }

    /// Re-export of rust-allocated (stack based) `PixelValue` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzPixelValue {
        pub metric: AzSizeMetric,
        pub number: AzFloatValue,
    }

    /// Re-export of rust-allocated (stack based) `PixelValueNoPercent` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzPixelValueNoPercent {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleBoxShadow` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleBoxShadow {
        pub offset: [AzPixelValueNoPercent;2],
        pub color: AzColorU,
        pub blur_radius: AzPixelValueNoPercent,
        pub spread_radius: AzPixelValueNoPercent,
        pub clip_mode: AzBoxShadowClipMode,
    }

    /// Re-export of rust-allocated (stack based) `LayoutBottom` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutBottom {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `LayoutFlexGrow` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutFlexGrow {
        pub inner: AzFloatValue,
    }

    /// Re-export of rust-allocated (stack based) `LayoutFlexShrink` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutFlexShrink {
        pub inner: AzFloatValue,
    }

    /// Re-export of rust-allocated (stack based) `LayoutHeight` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutHeight {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `LayoutLeft` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutLeft {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `LayoutMarginBottom` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutMarginBottom {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `LayoutMarginLeft` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutMarginLeft {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `LayoutMarginRight` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutMarginRight {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `LayoutMarginTop` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutMarginTop {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `LayoutMaxHeight` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutMaxHeight {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `LayoutMaxWidth` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutMaxWidth {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `LayoutMinHeight` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutMinHeight {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `LayoutMinWidth` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutMinWidth {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `LayoutPaddingBottom` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutPaddingBottom {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `LayoutPaddingLeft` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutPaddingLeft {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `LayoutPaddingRight` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutPaddingRight {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `LayoutPaddingTop` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutPaddingTop {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `LayoutRight` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutRight {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `LayoutTop` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutTop {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `LayoutWidth` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutWidth {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `PercentageValue` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzPercentageValue {
        pub number: AzFloatValue,
    }

    /// Re-export of rust-allocated (stack based) `AngleValue` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzAngleValue {
        pub metric: AzAngleMetric,
        pub number: AzFloatValue,
    }

    /// Re-export of rust-allocated (stack based) `NormalizedLinearColorStop` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzNormalizedLinearColorStop {
        pub offset: AzPercentageValue,
        pub color: AzColorU,
    }

    /// Re-export of rust-allocated (stack based) `NormalizedRadialColorStop` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzNormalizedRadialColorStop {
        pub offset: AzAngleValue,
        pub color: AzColorU,
    }

    /// Re-export of rust-allocated (stack based) `DirectionCorners` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzDirectionCorners {
        pub from: AzDirectionCorner,
        pub to: AzDirectionCorner,
    }

    /// Re-export of rust-allocated (stack based) `Direction` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzDirection {
        Angle(AzAngleValue),
        FromTo(AzDirectionCorners),
    }

    /// Re-export of rust-allocated (stack based) `BackgroundPositionHorizontal` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzBackgroundPositionHorizontal {
        Left,
        Center,
        Right,
        Exact(AzPixelValue),
    }

    /// Re-export of rust-allocated (stack based) `BackgroundPositionVertical` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzBackgroundPositionVertical {
        Top,
        Center,
        Bottom,
        Exact(AzPixelValue),
    }

    /// Re-export of rust-allocated (stack based) `StyleBackgroundPosition` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleBackgroundPosition {
        pub horizontal: AzBackgroundPositionHorizontal,
        pub vertical: AzBackgroundPositionVertical,
    }

    /// Re-export of rust-allocated (stack based) `StyleBackgroundSize` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleBackgroundSize {
        ExactSize([AzPixelValue;2]),
        Contain,
        Cover,
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderBottomColor` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleBorderBottomColor {
        pub inner: AzColorU,
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderBottomLeftRadius` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleBorderBottomLeftRadius {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderBottomRightRadius` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleBorderBottomRightRadius {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderBottomStyle` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleBorderBottomStyle {
        pub inner: AzBorderStyle,
    }

    /// Re-export of rust-allocated (stack based) `LayoutBorderBottomWidth` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutBorderBottomWidth {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderLeftColor` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleBorderLeftColor {
        pub inner: AzColorU,
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderLeftStyle` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleBorderLeftStyle {
        pub inner: AzBorderStyle,
    }

    /// Re-export of rust-allocated (stack based) `LayoutBorderLeftWidth` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutBorderLeftWidth {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderRightColor` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleBorderRightColor {
        pub inner: AzColorU,
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderRightStyle` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleBorderRightStyle {
        pub inner: AzBorderStyle,
    }

    /// Re-export of rust-allocated (stack based) `LayoutBorderRightWidth` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutBorderRightWidth {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderTopColor` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleBorderTopColor {
        pub inner: AzColorU,
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderTopLeftRadius` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleBorderTopLeftRadius {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderTopRightRadius` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleBorderTopRightRadius {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderTopStyle` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleBorderTopStyle {
        pub inner: AzBorderStyle,
    }

    /// Re-export of rust-allocated (stack based) `LayoutBorderTopWidth` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzLayoutBorderTopWidth {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleFontSize` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleFontSize {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleLetterSpacing` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleLetterSpacing {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleLineHeight` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleLineHeight {
        pub inner: AzPercentageValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleTabWidth` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleTabWidth {
        pub inner: AzPercentageValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleOpacity` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleOpacity {
        pub inner: AzPercentageValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleTransformOrigin` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleTransformOrigin {
        pub x: AzPixelValue,
        pub y: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `StylePerspectiveOrigin` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStylePerspectiveOrigin {
        pub x: AzPixelValue,
        pub y: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleTransformMatrix2D` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleTransformMatrix2D {
        pub a: AzPixelValue,
        pub b: AzPixelValue,
        pub c: AzPixelValue,
        pub d: AzPixelValue,
        pub tx: AzPixelValue,
        pub ty: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleTransformMatrix3D` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleTransformMatrix3D {
        pub m11: AzPixelValue,
        pub m12: AzPixelValue,
        pub m13: AzPixelValue,
        pub m14: AzPixelValue,
        pub m21: AzPixelValue,
        pub m22: AzPixelValue,
        pub m23: AzPixelValue,
        pub m24: AzPixelValue,
        pub m31: AzPixelValue,
        pub m32: AzPixelValue,
        pub m33: AzPixelValue,
        pub m34: AzPixelValue,
        pub m41: AzPixelValue,
        pub m42: AzPixelValue,
        pub m43: AzPixelValue,
        pub m44: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleTransformTranslate2D` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleTransformTranslate2D {
        pub x: AzPixelValue,
        pub y: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleTransformTranslate3D` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleTransformTranslate3D {
        pub x: AzPixelValue,
        pub y: AzPixelValue,
        pub z: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleTransformRotate3D` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleTransformRotate3D {
        pub x: AzPercentageValue,
        pub y: AzPercentageValue,
        pub z: AzPercentageValue,
        pub angle: AzAngleValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleTransformScale2D` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleTransformScale2D {
        pub x: AzPercentageValue,
        pub y: AzPercentageValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleTransformScale3D` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleTransformScale3D {
        pub x: AzPercentageValue,
        pub y: AzPercentageValue,
        pub z: AzPercentageValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleTransformSkew2D` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleTransformSkew2D {
        pub x: AzPercentageValue,
        pub y: AzPercentageValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleTextColor` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleTextColor {
        pub inner: AzColorU,
    }

    /// Re-export of rust-allocated (stack based) `StyleWordSpacing` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzStyleWordSpacing {
        pub inner: AzPixelValue,
    }

    /// Re-export of rust-allocated (stack based) `StyleBoxShadowValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleBoxShadowValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBoxShadow),
    }

    /// Re-export of rust-allocated (stack based) `LayoutAlignContentValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutAlignContentValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutAlignContent),
    }

    /// Re-export of rust-allocated (stack based) `LayoutAlignItemsValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutAlignItemsValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutAlignItems),
    }

    /// Re-export of rust-allocated (stack based) `LayoutBottomValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutBottomValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutBottom),
    }

    /// Re-export of rust-allocated (stack based) `LayoutBoxSizingValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutBoxSizingValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutBoxSizing),
    }

    /// Re-export of rust-allocated (stack based) `LayoutFlexDirectionValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutFlexDirectionValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutFlexDirection),
    }

    /// Re-export of rust-allocated (stack based) `LayoutDisplayValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutDisplayValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutDisplay),
    }

    /// Re-export of rust-allocated (stack based) `LayoutFlexGrowValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutFlexGrowValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutFlexGrow),
    }

    /// Re-export of rust-allocated (stack based) `LayoutFlexShrinkValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutFlexShrinkValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutFlexShrink),
    }

    /// Re-export of rust-allocated (stack based) `LayoutFloatValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutFloatValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutFloat),
    }

    /// Re-export of rust-allocated (stack based) `LayoutHeightValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutHeightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutHeight),
    }

    /// Re-export of rust-allocated (stack based) `LayoutJustifyContentValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutJustifyContentValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutJustifyContent),
    }

    /// Re-export of rust-allocated (stack based) `LayoutLeftValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutLeftValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutLeft),
    }

    /// Re-export of rust-allocated (stack based) `LayoutMarginBottomValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutMarginBottomValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMarginBottom),
    }

    /// Re-export of rust-allocated (stack based) `LayoutMarginLeftValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutMarginLeftValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMarginLeft),
    }

    /// Re-export of rust-allocated (stack based) `LayoutMarginRightValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutMarginRightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMarginRight),
    }

    /// Re-export of rust-allocated (stack based) `LayoutMarginTopValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutMarginTopValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMarginTop),
    }

    /// Re-export of rust-allocated (stack based) `LayoutMaxHeightValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutMaxHeightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMaxHeight),
    }

    /// Re-export of rust-allocated (stack based) `LayoutMaxWidthValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutMaxWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMaxWidth),
    }

    /// Re-export of rust-allocated (stack based) `LayoutMinHeightValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutMinHeightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMinHeight),
    }

    /// Re-export of rust-allocated (stack based) `LayoutMinWidthValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutMinWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMinWidth),
    }

    /// Re-export of rust-allocated (stack based) `LayoutPaddingBottomValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutPaddingBottomValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPaddingBottom),
    }

    /// Re-export of rust-allocated (stack based) `LayoutPaddingLeftValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutPaddingLeftValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPaddingLeft),
    }

    /// Re-export of rust-allocated (stack based) `LayoutPaddingRightValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutPaddingRightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPaddingRight),
    }

    /// Re-export of rust-allocated (stack based) `LayoutPaddingTopValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutPaddingTopValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPaddingTop),
    }

    /// Re-export of rust-allocated (stack based) `LayoutPositionValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutPositionValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPosition),
    }

    /// Re-export of rust-allocated (stack based) `LayoutRightValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutRightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutRight),
    }

    /// Re-export of rust-allocated (stack based) `LayoutTopValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutTopValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutTop),
    }

    /// Re-export of rust-allocated (stack based) `LayoutWidthValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutWidth),
    }

    /// Re-export of rust-allocated (stack based) `LayoutFlexWrapValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutFlexWrapValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutFlexWrap),
    }

    /// Re-export of rust-allocated (stack based) `LayoutOverflowValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutOverflowValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutOverflow),
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderBottomColorValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleBorderBottomColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomColor),
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderBottomLeftRadiusValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleBorderBottomLeftRadiusValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomLeftRadius),
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderBottomRightRadiusValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleBorderBottomRightRadiusValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomRightRadius),
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderBottomStyleValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleBorderBottomStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomStyle),
    }

    /// Re-export of rust-allocated (stack based) `LayoutBorderBottomWidthValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutBorderBottomWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutBorderBottomWidth),
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderLeftColorValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleBorderLeftColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderLeftColor),
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderLeftStyleValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleBorderLeftStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderLeftStyle),
    }

    /// Re-export of rust-allocated (stack based) `LayoutBorderLeftWidthValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutBorderLeftWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutBorderLeftWidth),
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderRightColorValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleBorderRightColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderRightColor),
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderRightStyleValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleBorderRightStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderRightStyle),
    }

    /// Re-export of rust-allocated (stack based) `LayoutBorderRightWidthValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutBorderRightWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutBorderRightWidth),
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderTopColorValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleBorderTopColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopColor),
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderTopLeftRadiusValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleBorderTopLeftRadiusValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopLeftRadius),
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderTopRightRadiusValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleBorderTopRightRadiusValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopRightRadius),
    }

    /// Re-export of rust-allocated (stack based) `StyleBorderTopStyleValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleBorderTopStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopStyle),
    }

    /// Re-export of rust-allocated (stack based) `LayoutBorderTopWidthValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzLayoutBorderTopWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutBorderTopWidth),
    }

    /// Re-export of rust-allocated (stack based) `StyleCursorValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleCursorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleCursor),
    }

    /// Re-export of rust-allocated (stack based) `StyleFontSizeValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleFontSizeValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleFontSize),
    }

    /// Re-export of rust-allocated (stack based) `StyleLetterSpacingValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleLetterSpacingValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleLetterSpacing),
    }

    /// Re-export of rust-allocated (stack based) `StyleLineHeightValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleLineHeightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleLineHeight),
    }

    /// Re-export of rust-allocated (stack based) `StyleTabWidthValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleTabWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleTabWidth),
    }

    /// Re-export of rust-allocated (stack based) `StyleTextAlignValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleTextAlignValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleTextAlign),
    }

    /// Re-export of rust-allocated (stack based) `StyleTextColorValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleTextColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleTextColor),
    }

    /// Re-export of rust-allocated (stack based) `StyleWordSpacingValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleWordSpacingValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleWordSpacing),
    }

    /// Re-export of rust-allocated (stack based) `StyleOpacityValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleOpacityValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleOpacity),
    }

    /// Re-export of rust-allocated (stack based) `StyleTransformOriginValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleTransformOriginValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleTransformOrigin),
    }

    /// Re-export of rust-allocated (stack based) `StylePerspectiveOriginValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStylePerspectiveOriginValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStylePerspectiveOrigin),
    }

    /// Re-export of rust-allocated (stack based) `StyleBackfaceVisibilityValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleBackfaceVisibilityValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackfaceVisibility),
    }

    /// Re-export of rust-allocated (stack based) `ButtonOnClick` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzButtonOnClick {
        pub data: AzRefAny,
        pub callback: AzCallback,
    }

    /// Re-export of rust-allocated (stack based) `CheckBoxOnToggle` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzCheckBoxOnToggle {
        pub data: AzRefAny,
        pub callback: AzCheckBoxOnToggleCallback,
    }

    /// Re-export of rust-allocated (stack based) `ColorInputState` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzColorInputState {
        pub color: AzColorU,
    }

    /// Re-export of rust-allocated (stack based) `ColorInputOnValueChange` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzColorInputOnValueChange {
        pub data: AzRefAny,
        pub callback: AzColorInputOnValueChangeCallback,
    }

    /// Re-export of rust-allocated (stack based) `TextInputSelection` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzTextInputSelection {
        All,
        FromTo(AzTextInputSelectionRange),
    }

    /// Re-export of rust-allocated (stack based) `TextInputOnTextInput` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzTextInputOnTextInput {
        pub data: AzRefAny,
        pub callback: AzTextInputOnTextInputCallback,
    }

    /// Re-export of rust-allocated (stack based) `TextInputOnVirtualKeyDown` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzTextInputOnVirtualKeyDown {
        pub data: AzRefAny,
        pub callback: AzTextInputOnVirtualKeyDownCallback,
    }

    /// Re-export of rust-allocated (stack based) `TextInputOnFocusLost` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzTextInputOnFocusLost {
        pub data: AzRefAny,
        pub callback: AzTextInputOnFocusLostCallback,
    }

    /// Re-export of rust-allocated (stack based) `OnTextInputReturn` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzOnTextInputReturn {
        pub update: AzUpdate,
        pub valid: AzTextInputValid,
    }

    /// Re-export of rust-allocated (stack based) `NumberInputOnValueChange` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzNumberInputOnValueChange {
        pub data: AzRefAny,
        pub callback: AzNumberInputOnValueChangeCallback,
    }

    /// Re-export of rust-allocated (stack based) `ParentWithNodeDepth` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzParentWithNodeDepth {
        pub depth: usize,
        pub node_id: AzNodeId,
    }

    /// Re-export of rust-allocated (stack based) `Gl` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzGl {
        pub(crate) ptr: *const c_void,
        pub renderer_type: AzRendererType,
    }

    /// C-ABI stable reexport of `&[Refstr]` aka `&mut [&str]`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzRefstrVecRef {
        pub(crate) ptr: *const AzRefstr,
        pub len: usize,
    }

    /// Re-export of rust-allocated (stack based) `ImageMask` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzImageMask {
        pub image: AzImageRef,
        pub rect: AzLogicalRect,
        pub repeat: bool,
    }

    /// Re-export of rust-allocated (stack based) `FontMetrics` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzFontMetrics {
        pub units_per_em: u16,
        pub font_flags: u16,
        pub x_min: i16,
        pub y_min: i16,
        pub x_max: i16,
        pub y_max: i16,
        pub ascender: i16,
        pub descender: i16,
        pub line_gap: i16,
        pub advance_width_max: u16,
        pub min_left_side_bearing: i16,
        pub min_right_side_bearing: i16,
        pub x_max_extent: i16,
        pub caret_slope_rise: i16,
        pub caret_slope_run: i16,
        pub caret_offset: i16,
        pub num_h_metrics: u16,
        pub x_avg_char_width: i16,
        pub us_weight_class: u16,
        pub us_width_class: u16,
        pub fs_type: u16,
        pub y_subscript_x_size: i16,
        pub y_subscript_y_size: i16,
        pub y_subscript_x_offset: i16,
        pub y_subscript_y_offset: i16,
        pub y_superscript_x_size: i16,
        pub y_superscript_y_size: i16,
        pub y_superscript_x_offset: i16,
        pub y_superscript_y_offset: i16,
        pub y_strikeout_size: i16,
        pub y_strikeout_position: i16,
        pub s_family_class: i16,
        pub panose: [u8; 10],
        pub ul_unicode_range1: u32,
        pub ul_unicode_range2: u32,
        pub ul_unicode_range3: u32,
        pub ul_unicode_range4: u32,
        pub ach_vend_id: u32,
        pub fs_selection: u16,
        pub us_first_char_index: u16,
        pub us_last_char_index: u16,
        pub s_typo_ascender: AzOptionI16,
        pub s_typo_descender: AzOptionI16,
        pub s_typo_line_gap: AzOptionI16,
        pub us_win_ascent: AzOptionU16,
        pub us_win_descent: AzOptionU16,
        pub ul_code_page_range1: AzOptionU32,
        pub ul_code_page_range2: AzOptionU32,
        pub sx_height: AzOptionI16,
        pub s_cap_height: AzOptionI16,
        pub us_default_char: AzOptionU16,
        pub us_break_char: AzOptionU16,
        pub us_max_context: AzOptionU16,
        pub us_lower_optical_point_size: AzOptionU16,
        pub us_upper_optical_point_size: AzOptionU16,
    }

    /// Re-export of rust-allocated (stack based) `SvgLine` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzSvgLine {
        pub start: AzSvgPoint,
        pub end: AzSvgPoint,
    }

    /// Re-export of rust-allocated (stack based) `SvgQuadraticCurve` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzSvgQuadraticCurve {
        pub start: AzSvgPoint,
        pub ctrl: AzSvgPoint,
        pub end: AzSvgPoint,
    }

    /// Re-export of rust-allocated (stack based) `SvgCubicCurve` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzSvgCubicCurve {
        pub start: AzSvgPoint,
        pub ctrl_1: AzSvgPoint,
        pub ctrl_2: AzSvgPoint,
        pub end: AzSvgPoint,
    }

    /// Re-export of rust-allocated (stack based) `SvgStringFormatOptions` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzSvgStringFormatOptions {
        pub use_single_quote: bool,
        pub indent: AzIndent,
        pub attributes_indent: AzIndent,
    }

    /// Re-export of rust-allocated (stack based) `SvgFillStyle` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzSvgFillStyle {
        pub line_join: AzSvgLineJoin,
        pub miter_limit: f32,
        pub tolerance: f32,
        pub fill_rule: AzSvgFillRule,
        pub transform: AzSvgTransform,
        pub anti_alias: bool,
        pub high_quality_aa: bool,
    }

    /// Re-export of rust-allocated (stack based) `InstantPtr` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzInstantPtr {
        pub(crate) ptr: *const c_void,
        pub clone_fn: AzInstantPtrCloneFn,
        pub destructor: AzInstantPtrDestructorFn,
    }

    /// Re-export of rust-allocated (stack based) `Duration` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzDuration {
        System(AzSystemTimeDiff),
        Tick(AzSystemTickDiff),
    }

    /// Re-export of rust-allocated (stack based) `ThreadSendMsg` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzThreadSendMsg {
        TerminateThread,
        Tick,
        Custom(AzRefAny),
    }

    /// Re-export of rust-allocated (stack based) `ThreadWriteBackMsg` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzThreadWriteBackMsg {
        pub data: AzRefAny,
        pub callback: AzWriteBackCallback,
    }

    /// Wrapper over a Rust-allocated `Vec<AccessibilityState>`
    #[repr(C)]
    pub struct AzAccessibilityStateVec {
        pub(crate) ptr: *const AzAccessibilityState,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzAccessibilityStateVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<MenuItem>`
    #[repr(C)]
    pub struct AzMenuItemVec {
        pub(crate) ptr: *const AzMenuItem,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzMenuItemVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<XmlNode>`
    #[repr(C)]
    pub struct AzXmlNodeVec {
        pub(crate) ptr: *const AzXmlNode,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzXmlNodeVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<InlineGlyph>`
    #[repr(C)]
    pub struct AzInlineGlyphVec {
        pub(crate) ptr: *const AzInlineGlyph,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzInlineGlyphVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<InlineTextHit>`
    #[repr(C)]
    pub struct AzInlineTextHitVec {
        pub(crate) ptr: *const AzInlineTextHit,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzInlineTextHitVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<VideoMode>`
    #[repr(C)]
    pub struct AzVideoModeVec {
        pub(crate) ptr: *const AzVideoMode,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzVideoModeVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<Dom>`
    #[repr(C)]
    pub struct AzDomVec {
        pub(crate) ptr: *const AzDom,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzDomVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<StyleBackgroundPosition>`
    #[repr(C)]
    pub struct AzStyleBackgroundPositionVec {
        pub(crate) ptr: *const AzStyleBackgroundPosition,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzStyleBackgroundPositionVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<StyleBackgroundRepeat>`
    #[repr(C)]
    pub struct AzStyleBackgroundRepeatVec {
        pub(crate) ptr: *const AzStyleBackgroundRepeat,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzStyleBackgroundRepeatVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<StyleBackgroundSize>`
    #[repr(C)]
    pub struct AzStyleBackgroundSizeVec {
        pub(crate) ptr: *const AzStyleBackgroundSize,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzStyleBackgroundSizeVecDestructor,
    }

    /// Wrapper over a Rust-allocated `SvgVertex`
    #[repr(C)]
    pub struct AzSvgVertexVec {
        pub(crate) ptr: *const AzSvgVertex,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzSvgVertexVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<u32>`
    #[repr(C)]
    pub struct AzU32Vec {
        pub(crate) ptr: *const u32,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzU32VecDestructor,
    }

    /// Wrapper over a Rust-allocated `XWindowType`
    #[repr(C)]
    pub struct AzXWindowTypeVec {
        pub(crate) ptr: *const AzXWindowType,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzXWindowTypeVecDestructor,
    }

    /// Wrapper over a Rust-allocated `VirtualKeyCode`
    #[repr(C)]
    pub struct AzVirtualKeyCodeVec {
        pub(crate) ptr: *const AzVirtualKeyCode,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzVirtualKeyCodeVecDestructor,
    }

    /// Wrapper over a Rust-allocated `CascadeInfo`
    #[repr(C)]
    pub struct AzCascadeInfoVec {
        pub(crate) ptr: *const AzCascadeInfo,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzCascadeInfoVecDestructor,
    }

    /// Wrapper over a Rust-allocated `ScanCode`
    #[repr(C)]
    pub struct AzScanCodeVec {
        pub(crate) ptr: *const u32,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzScanCodeVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<u16>`
    #[repr(C)]
    pub struct AzU16Vec {
        pub(crate) ptr: *const u16,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzU16VecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<f32>`
    #[repr(C)]
    pub struct AzF32Vec {
        pub(crate) ptr: *const f32,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzF32VecDestructor,
    }

    /// Wrapper over a Rust-allocated `U8Vec`
    #[repr(C)]
    pub struct AzU8Vec {
        pub(crate) ptr: *const u8,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzU8VecDestructor,
    }

    /// Wrapper over a Rust-allocated `U32Vec`
    #[repr(C)]
    pub struct AzGLuintVec {
        pub(crate) ptr: *const u32,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzGLuintVecDestructor,
    }

    /// Wrapper over a Rust-allocated `GLintVec`
    #[repr(C)]
    pub struct AzGLintVec {
        pub(crate) ptr: *const i32,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzGLintVecDestructor,
    }

    /// Wrapper over a Rust-allocated `NormalizedLinearColorStopVec`
    #[repr(C)]
    pub struct AzNormalizedLinearColorStopVec {
        pub(crate) ptr: *const AzNormalizedLinearColorStop,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzNormalizedLinearColorStopVecDestructor,
    }

    /// Wrapper over a Rust-allocated `NormalizedRadialColorStopVec`
    #[repr(C)]
    pub struct AzNormalizedRadialColorStopVec {
        pub(crate) ptr: *const AzNormalizedRadialColorStop,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzNormalizedRadialColorStopVecDestructor,
    }

    /// Wrapper over a Rust-allocated `NodeIdVec`
    #[repr(C)]
    pub struct AzNodeIdVec {
        pub(crate) ptr: *const AzNodeId,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzNodeIdVecDestructor,
    }

    /// Wrapper over a Rust-allocated `NodeVec`
    #[repr(C)]
    pub struct AzNodeVec {
        pub(crate) ptr: *const AzNode,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzNodeVecDestructor,
    }

    /// Wrapper over a Rust-allocated `ParentWithNodeDepthVec`
    #[repr(C)]
    pub struct AzParentWithNodeDepthVec {
        pub(crate) ptr: *const AzParentWithNodeDepth,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzParentWithNodeDepthVecDestructor,
    }

    /// Re-export of rust-allocated (stack based) `OptionColorInputOnValueChange` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionColorInputOnValueChange {
        None,
        Some(AzColorInputOnValueChange),
    }

    /// Re-export of rust-allocated (stack based) `OptionButtonOnClick` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionButtonOnClick {
        None,
        Some(AzButtonOnClick),
    }

    /// Re-export of rust-allocated (stack based) `OptionCheckBoxOnToggle` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionCheckBoxOnToggle {
        None,
        Some(AzCheckBoxOnToggle),
    }

    /// Re-export of rust-allocated (stack based) `OptionTextInputOnTextInput` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionTextInputOnTextInput {
        None,
        Some(AzTextInputOnTextInput),
    }

    /// Re-export of rust-allocated (stack based) `OptionTextInputOnVirtualKeyDown` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionTextInputOnVirtualKeyDown {
        None,
        Some(AzTextInputOnVirtualKeyDown),
    }

    /// Re-export of rust-allocated (stack based) `OptionTextInputOnFocusLost` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionTextInputOnFocusLost {
        None,
        Some(AzTextInputOnFocusLost),
    }

    /// Re-export of rust-allocated (stack based) `OptionTextInputSelection` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionTextInputSelection {
        None,
        Some(AzTextInputSelection),
    }

    /// Re-export of rust-allocated (stack based) `OptionNumberInputOnValueChange` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionNumberInputOnValueChange {
        None,
        Some(AzNumberInputOnValueChange),
    }

    /// Re-export of rust-allocated (stack based) `OptionMenuItemIcon` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionMenuItemIcon {
        None,
        Some(AzMenuItemIcon),
    }

    /// Re-export of rust-allocated (stack based) `OptionMenuCallback` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionMenuCallback {
        None,
        Some(AzMenuCallback),
    }

    /// Re-export of rust-allocated (stack based) `OptionPositionInfo` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionPositionInfo {
        None,
        Some(AzPositionInfo),
    }

    /// Re-export of rust-allocated (stack based) `OptionTimerId` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionTimerId {
        None,
        Some(AzTimerId),
    }

    /// Re-export of rust-allocated (stack based) `OptionThreadId` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionThreadId {
        None,
        Some(AzThreadId),
    }

    /// Re-export of rust-allocated (stack based) `OptionImageRef` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionImageRef {
        None,
        Some(AzImageRef),
    }

    /// Re-export of rust-allocated (stack based) `OptionFontRef` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionFontRef {
        None,
        Some(AzFontRef),
    }

    /// Re-export of rust-allocated (stack based) `OptionSystemClipboard` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionSystemClipboard {
        None,
        Some(AzSystemClipboard),
    }

    /// Re-export of rust-allocated (stack based) `OptionGl` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionGl {
        None,
        Some(AzGl),
    }

    /// Re-export of rust-allocated (stack based) `OptionPercentageValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionPercentageValue {
        None,
        Some(AzPercentageValue),
    }

    /// Re-export of rust-allocated (stack based) `OptionAngleValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionAngleValue {
        None,
        Some(AzAngleValue),
    }

    /// Re-export of rust-allocated (stack based) `OptionRendererOptions` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionRendererOptions {
        None,
        Some(AzRendererOptions),
    }

    /// Re-export of rust-allocated (stack based) `OptionCallback` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionCallback {
        None,
        Some(AzCallback),
    }

    /// Re-export of rust-allocated (stack based) `OptionThreadSendMsg` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionThreadSendMsg {
        None,
        Some(AzThreadSendMsg),
    }

    /// Re-export of rust-allocated (stack based) `OptionLayoutRect` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionLayoutRect {
        None,
        Some(AzLayoutRect),
    }

    /// Re-export of rust-allocated (stack based) `OptionRefAny` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionRefAny {
        None,
        Some(AzRefAny),
    }

    /// Re-export of rust-allocated (stack based) `OptionLayoutPoint` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionLayoutPoint {
        None,
        Some(AzLayoutPoint),
    }

    /// Re-export of rust-allocated (stack based) `OptionLayoutSize` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionLayoutSize {
        None,
        Some(AzLayoutSize),
    }

    /// Re-export of rust-allocated (stack based) `OptionWindowTheme` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionWindowTheme {
        None,
        Some(AzWindowTheme),
    }

    /// Re-export of rust-allocated (stack based) `OptionNodeId` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionNodeId {
        None,
        Some(AzNodeId),
    }

    /// Re-export of rust-allocated (stack based) `OptionDomNodeId` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionDomNodeId {
        None,
        Some(AzDomNodeId),
    }

    /// Re-export of rust-allocated (stack based) `OptionColorU` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionColorU {
        None,
        Some(AzColorU),
    }

    /// Re-export of rust-allocated (stack based) `OptionSvgDashPattern` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionSvgDashPattern {
        None,
        Some(AzSvgDashPattern),
    }

    /// Re-export of rust-allocated (stack based) `OptionLogicalPosition` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionLogicalPosition {
        None,
        Some(AzLogicalPosition),
    }

    /// Re-export of rust-allocated (stack based) `OptionPhysicalPositionI32` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionPhysicalPositionI32 {
        None,
        Some(AzPhysicalPositionI32),
    }

    /// Re-export of rust-allocated (stack based) `OptionMouseCursorType` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionMouseCursorType {
        None,
        Some(AzMouseCursorType),
    }

    /// Re-export of rust-allocated (stack based) `OptionLogicalSize` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionLogicalSize {
        None,
        Some(AzLogicalSize),
    }

    /// Re-export of rust-allocated (stack based) `OptionVirtualKeyCode` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionVirtualKeyCode {
        None,
        Some(AzVirtualKeyCode),
    }

    /// Re-export of rust-allocated (stack based) `OptionImageMask` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionImageMask {
        None,
        Some(AzImageMask),
    }

    /// Re-export of rust-allocated (stack based) `OptionTabIndex` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionTabIndex {
        None,
        Some(AzTabIndex),
    }

    /// Re-export of rust-allocated (stack based) `OptionTagId` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionTagId {
        None,
        Some(AzTagId),
    }

    /// Re-export of rust-allocated (stack based) `OptionDuration` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzOptionDuration {
        None,
        Some(AzDuration),
    }

    /// Re-export of rust-allocated (stack based) `OptionU8Vec` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionU8Vec {
        None,
        Some(AzU8Vec),
    }

    /// Re-export of rust-allocated (stack based) `OptionU8VecRef` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionU8VecRef {
        None,
        Some(AzU8VecRef),
    }

    /// Re-export of rust-allocated (stack based) `ResultU8VecEncodeImageError` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzResultU8VecEncodeImageError {
        Ok(AzU8Vec),
        Err(AzEncodeImageError),
    }

    /// Re-export of rust-allocated (stack based) `NonXmlCharError` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzNonXmlCharError {
        pub ch: u32,
        pub pos: AzSvgParseErrorPosition,
    }

    /// Re-export of rust-allocated (stack based) `InvalidCharError` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzInvalidCharError {
        pub expected: u8,
        pub got: u8,
        pub pos: AzSvgParseErrorPosition,
    }

    /// Re-export of rust-allocated (stack based) `InvalidCharMultipleError` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzInvalidCharMultipleError {
        pub expected: u8,
        pub got: AzU8Vec,
        pub pos: AzSvgParseErrorPosition,
    }

    /// Re-export of rust-allocated (stack based) `InvalidQuoteError` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzInvalidQuoteError {
        pub got: u8,
        pub pos: AzSvgParseErrorPosition,
    }

    /// Re-export of rust-allocated (stack based) `InvalidSpaceError` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzInvalidSpaceError {
        pub got: u8,
        pub pos: AzSvgParseErrorPosition,
    }

    /// Configuration for optional features, such as whether to enable logging or panic hooks
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzAppConfig {
        pub layout_solver: AzLayoutSolver,
        pub log_level: AzAppLogLevel,
        pub enable_visual_panic_hook: bool,
        pub enable_logging_on_panic: bool,
        pub enable_tab_navigation: bool,
        pub system_callbacks: AzSystemCallbacks,
    }

    /// Small (16x16x4) window icon, usually shown in the window titlebar
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzSmallWindowIconBytes {
        pub key: AzIconKey,
        pub rgba_bytes: AzU8Vec,
    }

    /// Large (32x32x4) window icon, usually used on high-resolution displays (instead of `SmallWindowIcon`)
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzLargeWindowIconBytes {
        pub key: AzIconKey,
        pub rgba_bytes: AzU8Vec,
    }

    /// Window "favicon", usually shown in the top left of the window on Windows
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzWindowIcon {
        Small(AzSmallWindowIconBytes),
        Large(AzLargeWindowIconBytes),
    }

    /// Application taskbar icon, 256x256x4 bytes in size
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzTaskBarIcon {
        pub key: AzIconKey,
        pub rgba_bytes: AzU8Vec,
    }

    /// Minimum / maximum / current size of the window in logical dimensions
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzWindowSize {
        pub dimensions: AzLogicalSize,
        pub hidpi_factor: f32,
        pub system_hidpi_factor: f32,
        pub min_dimensions: AzOptionLogicalSize,
        pub max_dimensions: AzOptionLogicalSize,
    }

    /// Current keyboard state, stores what keys / characters have been pressed
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzKeyboardState {
        pub shift_down: bool,
        pub ctrl_down: bool,
        pub alt_down: bool,
        pub super_down: bool,
        pub current_char: AzOptionChar,
        pub current_virtual_keycode: AzOptionVirtualKeyCode,
        pub pressed_virtual_keycodes: AzVirtualKeyCodeVec,
        pub pressed_scancodes: AzScanCodeVec,
    }

    /// Current mouse / cursor state
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzMouseState {
        pub mouse_cursor_type: AzOptionMouseCursorType,
        pub cursor_position: AzCursorPosition,
        pub is_cursor_locked: bool,
        pub left_down: bool,
        pub right_down: bool,
        pub middle_down: bool,
        pub scroll_x: AzOptionF32,
        pub scroll_y: AzOptionF32,
    }

    /// C-ABI stable wrapper over a `MarshaledLayoutCallback`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzMarshaledLayoutCallback {
        pub marshal_data: AzRefAny,
        pub cb: AzMarshaledLayoutCallbackInner,
    }

    /// Re-export of rust-allocated (stack based) `InlineTextContents` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzInlineTextContents {
        pub glyphs: AzInlineGlyphVec,
        pub bounds: AzLogicalRect,
    }

    /// Easing function of the animation (ease-in, ease-out, ease-in-out, custom)
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzAnimationEasing {
        Ease,
        Linear,
        EaseIn,
        EaseOut,
        EaseInOut,
        CubicBezier(AzSvgCubicCurve),
    }

    /// Re-export of rust-allocated (stack based) `RenderImageCallbackInfo` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzRenderImageCallbackInfo {
        pub callback_node_id: AzDomNodeId,
        pub bounds: AzHidpiAdjustedBounds,
        pub gl_context: *const AzOptionGl,
        pub image_cache: *const c_void,
        pub system_fonts: *const c_void,
        pub node_hierarchy: *const AzNodeVec,
        pub words_cache: *const c_void,
        pub shaped_words_cache: *const c_void,
        pub positioned_words_cache: *const c_void,
        pub positioned_rects: *const c_void,
        pub _reserved_ref: *const c_void,
        pub _reserved_mut: *mut c_void,
    }

    /// Re-export of rust-allocated (stack based) `LayoutCallbackInfo` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzLayoutCallbackInfo {
        pub window_size: AzWindowSize,
        pub theme: AzWindowTheme,
        pub image_cache: *const c_void,
        pub gl_context: *const AzOptionGl,
        pub system_fonts: *const c_void,
        pub _reserved_ref: *const c_void,
        pub _reserved_mut: *mut c_void,
    }

    /// Re-export of rust-allocated (stack based) `EventFilter` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzEventFilter {
        Hover(AzHoverEventFilter),
        Not(AzNotEventFilter),
        Focus(AzFocusEventFilter),
        Window(AzWindowEventFilter),
        Component(AzComponentEventFilter),
        Application(AzApplicationEventFilter),
    }

    /// Menu struct (application / window menu, dropdown menu, context menu). Modeled after the Windows API
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzMenu {
        pub items: AzMenuItemVec,
    }

    /// Combination of virtual key codes that have to be pressed together
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzVirtualKeyCodeCombo {
        pub keys: AzVirtualKeyCodeVec,
    }

    /// Re-export of rust-allocated (stack based) `CssPathPseudoSelector` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzCssPathPseudoSelector {
        First,
        Last,
        NthChild(AzCssNthChildSelector),
        Hover,
        Active,
        Focus,
    }

    /// Re-export of rust-allocated (stack based) `AnimationInterpolationFunction` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzAnimationInterpolationFunction {
        Ease,
        Linear,
        EaseIn,
        EaseOut,
        EaseInOut,
        CubicBezier(AzSvgCubicCurve),
    }

    /// Re-export of rust-allocated (stack based) `InterpolateContext` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzInterpolateContext {
        pub animation_func: AzAnimationInterpolationFunction,
        pub parent_rect_width: f32,
        pub parent_rect_height: f32,
        pub current_rect_width: f32,
        pub current_rect_height: f32,
    }

    /// Re-export of rust-allocated (stack based) `LinearGradient` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzLinearGradient {
        pub direction: AzDirection,
        pub extend_mode: AzExtendMode,
        pub stops: AzNormalizedLinearColorStopVec,
    }

    /// Re-export of rust-allocated (stack based) `RadialGradient` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzRadialGradient {
        pub shape: AzShape,
        pub size: AzRadialGradientSize,
        pub position: AzStyleBackgroundPosition,
        pub extend_mode: AzExtendMode,
        pub stops: AzNormalizedLinearColorStopVec,
    }

    /// Re-export of rust-allocated (stack based) `ConicGradient` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzConicGradient {
        pub extend_mode: AzExtendMode,
        pub center: AzStyleBackgroundPosition,
        pub angle: AzAngleValue,
        pub stops: AzNormalizedRadialColorStopVec,
    }

    /// Re-export of rust-allocated (stack based) `StyleTransform` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzStyleTransform {
        Matrix(AzStyleTransformMatrix2D),
        Matrix3D(AzStyleTransformMatrix3D),
        Translate(AzStyleTransformTranslate2D),
        Translate3D(AzStyleTransformTranslate3D),
        TranslateX(AzPixelValue),
        TranslateY(AzPixelValue),
        TranslateZ(AzPixelValue),
        Rotate(AzAngleValue),
        Rotate3D(AzStyleTransformRotate3D),
        RotateX(AzAngleValue),
        RotateY(AzAngleValue),
        RotateZ(AzAngleValue),
        Scale(AzStyleTransformScale2D),
        Scale3D(AzStyleTransformScale3D),
        ScaleX(AzPercentageValue),
        ScaleY(AzPercentageValue),
        ScaleZ(AzPercentageValue),
        Skew(AzStyleTransformSkew2D),
        SkewX(AzPercentageValue),
        SkewY(AzPercentageValue),
        Perspective(AzPixelValue),
    }

    /// Re-export of rust-allocated (stack based) `StyleBackgroundPositionVecValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzStyleBackgroundPositionVecValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundPositionVec),
    }

    /// Re-export of rust-allocated (stack based) `StyleBackgroundRepeatVecValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzStyleBackgroundRepeatVecValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundRepeatVec),
    }

    /// Re-export of rust-allocated (stack based) `StyleBackgroundSizeVecValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzStyleBackgroundSizeVecValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundSizeVec),
    }

    /// Re-export of rust-allocated (stack based) `CheckBoxStateWrapper` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzCheckBoxStateWrapper {
        pub inner: AzCheckBoxState,
        pub on_toggle: AzOptionCheckBoxOnToggle,
    }

    /// Re-export of rust-allocated (stack based) `NumberInputStateWrapper` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzNumberInputStateWrapper {
        pub inner: AzNumberInputState,
        pub on_value_change: AzOptionNumberInputOnValueChange,
    }

    /// Re-export of rust-allocated (stack based) `StyledNode` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzStyledNode {
        pub state: AzStyledNodeState,
        pub tag_id: AzOptionTagId,
    }

    /// Re-export of rust-allocated (stack based) `TagIdToNodeIdMapping` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzTagIdToNodeIdMapping {
        pub tag_id: AzTagId,
        pub node_id: AzNodeId,
        pub tab_index: AzOptionTabIndex,
        pub parents: AzNodeIdVec,
    }

    /// Re-export of rust-allocated (stack based) `Texture` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzTexture {
        pub texture_id: u32,
        pub format: AzRawImageFormat,
        pub flags: AzTextureFlags,
        pub size: AzPhysicalSizeU32,
        pub gl_context: AzGl,
    }

    /// C-ABI stable reexport of `(U8Vec, u32)`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzGetProgramBinaryReturn {
        pub _0: AzU8Vec,
        pub _1: u32,
    }

    /// Re-export of rust-allocated (stack based) `RawImageData` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzRawImageData {
        U8(AzU8Vec),
        U16(AzU16Vec),
        F32(AzF32Vec),
    }

    /// Source data of a font file (bytes)
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzFontSource {
        pub data: AzU8Vec,
        pub font_index: u32,
        pub parse_glyph_outlines: bool,
    }

    /// Re-export of rust-allocated (stack based) `SvgPathElement` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzSvgPathElement {
        Line(AzSvgLine),
        QuadraticCurve(AzSvgQuadraticCurve),
        CubicCurve(AzSvgCubicCurve),
    }

    /// Re-export of rust-allocated (stack based) `TesselatedSvgNode` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzTesselatedSvgNode {
        pub vertices: AzSvgVertexVec,
        pub indices: AzU32Vec,
    }

    /// Rust wrapper over a `&[TesselatedSvgNode]` or `&Vec<TesselatedSvgNode>`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzTesselatedSvgNodeVecRef {
        pub(crate) ptr: *const AzTesselatedSvgNode,
        pub len: usize,
    }

    /// Re-export of rust-allocated (stack based) `SvgRenderOptions` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzSvgRenderOptions {
        pub target_size: AzOptionLayoutSize,
        pub background_color: AzOptionColorU,
        pub fit: AzSvgFitTo,
    }

    /// Re-export of rust-allocated (stack based) `SvgStrokeStyle` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub struct AzSvgStrokeStyle {
        pub start_cap: AzSvgLineCap,
        pub end_cap: AzSvgLineCap,
        pub line_join: AzSvgLineJoin,
        pub dash_pattern: AzOptionSvgDashPattern,
        pub line_width: f32,
        pub miter_limit: f32,
        pub tolerance: f32,
        pub apply_line_width: bool,
        pub transform: AzSvgTransform,
        pub anti_alias: bool,
        pub high_quality_aa: bool,
    }

    /// Re-export of rust-allocated (stack based) `Xml` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzXml {
        pub root: AzXmlNodeVec,
    }

    /// Re-export of rust-allocated (stack based) `Instant` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzInstant {
        System(AzInstantPtr),
        Tick(AzSystemTick),
    }

    /// Re-export of rust-allocated (stack based) `ThreadReceiveMsg` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzThreadReceiveMsg {
        WriteBack(AzThreadWriteBackMsg),
        Update(AzUpdate),
    }

    /// Re-export of rust-allocated (stack based) `String` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzString {
        pub vec: AzU8Vec,
    }

    /// Wrapper over a Rust-allocated `Vec<TesselatedSvgNode>`
    #[repr(C)]
    pub struct AzTesselatedSvgNodeVec {
        pub(crate) ptr: *const AzTesselatedSvgNode,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzTesselatedSvgNodeVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<StyleTransform>`
    #[repr(C)]
    pub struct AzStyleTransformVec {
        pub(crate) ptr: *const AzStyleTransform,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzStyleTransformVecDestructor,
    }

    /// Wrapper over a Rust-allocated `VertexAttribute`
    #[repr(C)]
    pub struct AzSvgPathElementVec {
        pub(crate) ptr: *const AzSvgPathElement,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzSvgPathElementVecDestructor,
    }

    /// Wrapper over a Rust-allocated `StringVec`
    #[repr(C)]
    pub struct AzStringVec {
        pub(crate) ptr: *const AzString,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzStringVecDestructor,
    }

    /// Wrapper over a Rust-allocated `StyledNodeVec`
    #[repr(C)]
    pub struct AzStyledNodeVec {
        pub(crate) ptr: *const AzStyledNode,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzStyledNodeVecDestructor,
    }

    /// Wrapper over a Rust-allocated `TagIdToNodeIdMappingVec`
    #[repr(C)]
    pub struct AzTagIdToNodeIdMappingVec {
        pub(crate) ptr: *const AzTagIdToNodeIdMapping,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzTagIdToNodeIdMappingVecDestructor,
    }

    /// Re-export of rust-allocated (stack based) `OptionVirtualKeyCodeCombo` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionVirtualKeyCodeCombo {
        None,
        Some(AzVirtualKeyCodeCombo),
    }

    /// Re-export of rust-allocated (stack based) `OptionMouseState` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionMouseState {
        None,
        Some(AzMouseState),
    }

    /// Re-export of rust-allocated (stack based) `OptionKeyboardState` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionKeyboardState {
        None,
        Some(AzKeyboardState),
    }

    /// Re-export of rust-allocated (stack based) `OptionStringVec` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionStringVec {
        None,
        Some(AzStringVec),
    }

    /// Re-export of rust-allocated (stack based) `OptionThreadReceiveMsg` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionThreadReceiveMsg {
        None,
        Some(AzThreadReceiveMsg),
    }

    /// Re-export of rust-allocated (stack based) `OptionTaskBarIcon` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionTaskBarIcon {
        None,
        Some(AzTaskBarIcon),
    }

    /// Re-export of rust-allocated (stack based) `OptionWindowIcon` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionWindowIcon {
        None,
        Some(AzWindowIcon),
    }

    /// Re-export of rust-allocated (stack based) `OptionString` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionString {
        None,
        Some(AzString),
    }

    /// Re-export of rust-allocated (stack based) `OptionTexture` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionTexture {
        None,
        Some(AzTexture),
    }

    /// Re-export of rust-allocated (stack based) `OptionInstant` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionInstant {
        None,
        Some(AzInstant),
    }

    /// Re-export of rust-allocated (stack based) `DuplicatedNamespaceError` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzDuplicatedNamespaceError {
        pub ns: AzString,
        pub pos: AzSvgParseErrorPosition,
    }

    /// Re-export of rust-allocated (stack based) `UnknownNamespaceError` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzUnknownNamespaceError {
        pub ns: AzString,
        pub pos: AzSvgParseErrorPosition,
    }

    /// Re-export of rust-allocated (stack based) `UnexpectedCloseTagError` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzUnexpectedCloseTagError {
        pub expected: AzString,
        pub actual: AzString,
        pub pos: AzSvgParseErrorPosition,
    }

    /// Re-export of rust-allocated (stack based) `UnknownEntityReferenceError` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzUnknownEntityReferenceError {
        pub entity: AzString,
        pub pos: AzSvgParseErrorPosition,
    }

    /// Re-export of rust-allocated (stack based) `DuplicatedAttributeError` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzDuplicatedAttributeError {
        pub attribute: AzString,
        pub pos: AzSvgParseErrorPosition,
    }

    /// Re-export of rust-allocated (stack based) `InvalidStringError` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzInvalidStringError {
        pub got: AzString,
        pub pos: AzSvgParseErrorPosition,
    }

    /// Window configuration specific to Win32
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzWindowsWindowOptions {
        pub allow_drag_drop: bool,
        pub no_redirection_bitmap: bool,
        pub window_icon: AzOptionWindowIcon,
        pub taskbar_icon: AzOptionTaskBarIcon,
        pub parent_window: AzOptionHwndHandle,
    }

    /// CSD theme of the window title / button controls
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzWaylandTheme {
        pub title_bar_active_background_color: [u8;4],
        pub title_bar_active_separator_color: [u8;4],
        pub title_bar_active_text_color: [u8;4],
        pub title_bar_inactive_background_color: [u8;4],
        pub title_bar_inactive_separator_color: [u8;4],
        pub title_bar_inactive_text_color: [u8;4],
        pub maximize_idle_foreground_inactive_color: [u8;4],
        pub minimize_idle_foreground_inactive_color: [u8;4],
        pub close_idle_foreground_inactive_color: [u8;4],
        pub maximize_hovered_foreground_inactive_color: [u8;4],
        pub minimize_hovered_foreground_inactive_color: [u8;4],
        pub close_hovered_foreground_inactive_color: [u8;4],
        pub maximize_disabled_foreground_inactive_color: [u8;4],
        pub minimize_disabled_foreground_inactive_color: [u8;4],
        pub close_disabled_foreground_inactive_color: [u8;4],
        pub maximize_idle_background_inactive_color: [u8;4],
        pub minimize_idle_background_inactive_color: [u8;4],
        pub close_idle_background_inactive_color: [u8;4],
        pub maximize_hovered_background_inactive_color: [u8;4],
        pub minimize_hovered_background_inactive_color: [u8;4],
        pub close_hovered_background_inactive_color: [u8;4],
        pub maximize_disabled_background_inactive_color: [u8;4],
        pub minimize_disabled_background_inactive_color: [u8;4],
        pub close_disabled_background_inactive_color: [u8;4],
        pub maximize_idle_foreground_active_color: [u8;4],
        pub minimize_idle_foreground_active_color: [u8;4],
        pub close_idle_foreground_active_color: [u8;4],
        pub maximize_hovered_foreground_active_color: [u8;4],
        pub minimize_hovered_foreground_active_color: [u8;4],
        pub close_hovered_foreground_active_color: [u8;4],
        pub maximize_disabled_foreground_active_color: [u8;4],
        pub minimize_disabled_foreground_active_color: [u8;4],
        pub close_disabled_foreground_active_color: [u8;4],
        pub maximize_idle_background_active_color: [u8;4],
        pub minimize_idle_background_active_color: [u8;4],
        pub close_idle_background_active_color: [u8;4],
        pub maximize_hovered_background_active_color: [u8;4],
        pub minimize_hovered_background_active_color: [u8;4],
        pub close_hovered_background_active_color: [u8;4],
        pub maximize_disabled_background_active_color: [u8;4],
        pub minimize_disabled_background_active_color: [u8;4],
        pub close_disabled_background_active_color: [u8;4],
        pub title_bar_font: AzString,
        pub title_bar_font_size: f32,
    }

    /// Key-value pair, used for setting WM hints values specific to GNOME
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzStringPair {
        pub key: AzString,
        pub value: AzString,
    }

    /// Information about a single (or many) monitors, useful for dock widgets
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzMonitor {
        pub id: usize,
        pub name: AzOptionString,
        pub size: AzLayoutSize,
        pub position: AzLayoutPoint,
        pub scale_factor: f64,
        pub video_modes: AzVideoModeVec,
        pub is_primary_monitor: bool,
    }

    /// Re-export of rust-allocated (stack based) `LayoutCallback` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzLayoutCallback {
        Raw(AzLayoutCallbackInner),
        Marshaled(AzMarshaledLayoutCallback),
    }

    /// Re-export of rust-allocated (stack based) `InlineWord` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzInlineWord {
        Tab,
        Return,
        Space,
        Word(AzInlineTextContents),
    }

    /// Re-export of rust-allocated (stack based) `CallbackData` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzCallbackData {
        pub event: AzEventFilter,
        pub callback: AzCallback,
        pub data: AzRefAny,
    }

    /// List of core DOM node types built-into by `azul`
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzNodeType {
        Body,
        Div,
        Br,
        Text(AzString),
        Image(AzImageRef),
        IFrame(AzIFrameNode),
    }

    /// Accessibility information (MSAA wrapper). See `NodeData.set_accessibility_info()`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzAccessibilityInfo {
        pub name: AzOptionString,
        pub value: AzOptionString,
        pub role: AzAccessibilityRole,
        pub states: AzAccessibilityStateVec,
        pub accelerator: AzOptionVirtualKeyCodeCombo,
        pub default_action: AzOptionString,
    }

    /// Re-export of rust-allocated (stack based) `IdOrClass` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzIdOrClass {
        Id(AzString),
        Class(AzString),
    }

    /// Regular labeled menu item
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzStringMenuItem {
        pub label: AzString,
        pub accelerator: AzOptionVirtualKeyCodeCombo,
        pub callback: AzOptionMenuCallback,
        pub state: AzMenuItemState,
        pub icon: AzOptionMenuItemIcon,
        pub children: AzMenuItemVec,
    }

    /// Re-export of rust-allocated (stack based) `CssPathSelector` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzCssPathSelector {
        Global,
        Type(AzNodeTypeKey),
        Class(AzString),
        Id(AzString),
        PseudoSelector(AzCssPathPseudoSelector),
        DirectChildren,
        Children,
    }

    /// Re-export of rust-allocated (stack based) `StyleBackgroundContent` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzStyleBackgroundContent {
        LinearGradient(AzLinearGradient),
        RadialGradient(AzRadialGradient),
        ConicGradient(AzConicGradient),
        Image(AzString),
        Color(AzColorU),
    }

    /// Re-export of rust-allocated (stack based) `ScrollbarInfo` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzScrollbarInfo {
        pub width: AzLayoutWidth,
        pub padding_left: AzLayoutPaddingLeft,
        pub padding_right: AzLayoutPaddingRight,
        pub track: AzStyleBackgroundContent,
        pub thumb: AzStyleBackgroundContent,
        pub button: AzStyleBackgroundContent,
        pub corner: AzStyleBackgroundContent,
        pub resizer: AzStyleBackgroundContent,
    }

    /// Re-export of rust-allocated (stack based) `ScrollbarStyle` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzScrollbarStyle {
        pub horizontal: AzScrollbarInfo,
        pub vertical: AzScrollbarInfo,
    }

    /// Re-export of rust-allocated (stack based) `StyleFontFamily` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzStyleFontFamily {
        System(AzString),
        File(AzString),
        Ref(AzFontRef),
    }

    /// Re-export of rust-allocated (stack based) `ScrollbarStyleValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzScrollbarStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzScrollbarStyle),
    }

    /// Re-export of rust-allocated (stack based) `StyleTransformVecValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzStyleTransformVecValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleTransformVec),
    }

    /// Re-export of rust-allocated (stack based) `ColorInputStateWrapper` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzColorInputStateWrapper {
        pub inner: AzColorInputState,
        pub title: AzString,
        pub on_value_change: AzOptionColorInputOnValueChange,
    }

    /// Re-export of rust-allocated (stack based) `TextInputState` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzTextInputState {
        pub text: AzU32Vec,
        pub placeholder: AzOptionString,
        pub max_len: usize,
        pub selection: AzOptionTextInputSelection,
        pub cursor_pos: usize,
    }

    /// Re-export of rust-allocated (stack based) `VertexAttribute` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzVertexAttribute {
        pub name: AzString,
        pub layout_location: AzOptionUsize,
        pub attribute_type: AzVertexAttributeType,
        pub item_count: usize,
    }

    /// Re-export of rust-allocated (stack based) `DebugMessage` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzDebugMessage {
        pub message: AzString,
        pub source: u32,
        pub ty: u32,
        pub id: u32,
        pub severity: u32,
    }

    /// C-ABI stable reexport of `(i32, u32, AzString)`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzGetActiveAttribReturn {
        pub _0: i32,
        pub _1: u32,
        pub _2: AzString,
    }

    /// C-ABI stable reexport of `(i32, u32, AzString)`
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzGetActiveUniformReturn {
        pub _0: i32,
        pub _1: u32,
        pub _2: AzString,
    }

    /// Re-export of rust-allocated (stack based) `RawImage` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzRawImage {
        pub pixels: AzRawImageData,
        pub width: usize,
        pub height: usize,
        pub alpha_premultiplied: bool,
        pub data_format: AzRawImageFormat,
    }

    /// Re-export of rust-allocated (stack based) `SvgPath` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzSvgPath {
        pub items: AzSvgPathElementVec,
    }

    /// Re-export of rust-allocated (stack based) `SvgParseOptions` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzSvgParseOptions {
        pub relative_image_path: AzOptionString,
        pub dpi: f32,
        pub default_font_family: AzString,
        pub font_size: f32,
        pub languages: AzStringVec,
        pub shape_rendering: AzShapeRendering,
        pub text_rendering: AzTextRendering,
        pub image_rendering: AzImageRendering,
        pub keep_named_groups: bool,
        pub fontdb: AzFontDatabase,
    }

    /// Re-export of rust-allocated (stack based) `SvgStyle` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    #[derive(Copy)]
    pub enum AzSvgStyle {
        Fill(AzSvgFillStyle),
        Stroke(AzSvgStrokeStyle),
    }

    /// **Reference-counted** file handle
    #[repr(C)]
    #[derive(Debug)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzFile {
        pub(crate) ptr: *const c_void,
        pub path: AzString,
    }

    /// Re-export of rust-allocated (stack based) `FileTypeList` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzFileTypeList {
        pub document_types: AzStringVec,
        pub document_descriptor: AzString,
    }

    /// Re-export of rust-allocated (stack based) `Timer` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzTimer {
        pub data: AzRefAny,
        pub node_id: AzOptionDomNodeId,
        pub created: AzInstant,
        pub last_run: AzOptionInstant,
        pub run_count: usize,
        pub delay: AzOptionDuration,
        pub interval: AzOptionDuration,
        pub timeout: AzOptionDuration,
        pub callback: AzTimerCallback,
    }

    /// Re-export of rust-allocated (stack based) `FmtValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzFmtValue {
        Bool(bool),
        Uchar(u8),
        Schar(i8),
        Ushort(u16),
        Sshort(i16),
        Uint(u32),
        Sint(i32),
        Ulong(u64),
        Slong(i64),
        Isize(isize),
        Usize(usize),
        Float(f32),
        Double(f64),
        Str(AzString),
        StrVec(AzStringVec),
    }

    /// Re-export of rust-allocated (stack based) `FmtArg` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzFmtArg {
        pub key: AzString,
        pub value: AzFmtValue,
    }

    /// Wrapper over a Rust-allocated `Vec<StyleFontFamily>`
    #[repr(C)]
    pub struct AzStyleFontFamilyVec {
        pub(crate) ptr: *const AzStyleFontFamily,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzStyleFontFamilyVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<FmtArg>`
    #[repr(C)]
    pub struct AzFmtArgVec {
        pub(crate) ptr: *const AzFmtArg,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzFmtArgVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<InlineWord>`
    #[repr(C)]
    pub struct AzInlineWordVec {
        pub(crate) ptr: *const AzInlineWord,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzInlineWordVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<Monitor>`
    #[repr(C)]
    pub struct AzMonitorVec {
        pub(crate) ptr: *const AzMonitor,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzMonitorVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<IdOrClass>`
    #[repr(C)]
    pub struct AzIdOrClassVec {
        pub(crate) ptr: *const AzIdOrClass,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzIdOrClassVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<StyleBackgroundContent>`
    #[repr(C)]
    pub struct AzStyleBackgroundContentVec {
        pub(crate) ptr: *const AzStyleBackgroundContent,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzStyleBackgroundContentVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<SvgPath>`
    #[repr(C)]
    pub struct AzSvgPathVec {
        pub(crate) ptr: *const AzSvgPath,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzSvgPathVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<VertexAttribute>`
    #[repr(C)]
    pub struct AzVertexAttributeVec {
        pub(crate) ptr: *const AzVertexAttribute,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzVertexAttributeVecDestructor,
    }

    /// Wrapper over a Rust-allocated `CssPathSelector`
    #[repr(C)]
    pub struct AzCssPathSelectorVec {
        pub(crate) ptr: *const AzCssPathSelector,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzCssPathSelectorVecDestructor,
    }

    /// Wrapper over a Rust-allocated `CallbackData`
    #[repr(C)]
    pub struct AzCallbackDataVec {
        pub(crate) ptr: *const AzCallbackData,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzCallbackDataVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<DebugMessage>`
    #[repr(C)]
    pub struct AzDebugMessageVec {
        pub(crate) ptr: *const AzDebugMessage,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzDebugMessageVecDestructor,
    }

    /// Wrapper over a Rust-allocated `StringPairVec`
    #[repr(C)]
    pub struct AzStringPairVec {
        pub(crate) ptr: *const AzStringPair,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzStringPairVecDestructor,
    }

    /// Re-export of rust-allocated (stack based) `OptionFileTypeList` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionFileTypeList {
        None,
        Some(AzFileTypeList),
    }

    /// Re-export of rust-allocated (stack based) `OptionFile` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionFile {
        None,
        Some(AzFile),
    }

    /// Re-export of rust-allocated (stack based) `OptionRawImage` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionRawImage {
        None,
        Some(AzRawImage),
    }

    /// Re-export of rust-allocated (stack based) `OptionWaylandTheme` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionWaylandTheme {
        None,
        Some(AzWaylandTheme),
    }

    /// Re-export of rust-allocated (stack based) `ResultRawImageDecodeImageError` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzResultRawImageDecodeImageError {
        Ok(AzRawImage),
        Err(AzDecodeImageError),
    }

    /// Re-export of rust-allocated (stack based) `XmlStreamError` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzXmlStreamError {
        UnexpectedEndOfStream,
        InvalidName,
        NonXmlChar(AzNonXmlCharError),
        InvalidChar(AzInvalidCharError),
        InvalidCharMultiple(AzInvalidCharMultipleError),
        InvalidQuote(AzInvalidQuoteError),
        InvalidSpace(AzInvalidSpaceError),
        InvalidString(AzInvalidStringError),
        InvalidReference,
        InvalidExternalID,
        InvalidCommentData,
        InvalidCommentEnd,
        InvalidCharacterData,
    }

    /// Re-export of rust-allocated (stack based) `LinuxWindowOptions` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzLinuxWindowOptions {
        pub x11_visual: AzOptionX11Visual,
        pub x11_screen: AzOptionI32,
        pub x11_wm_classes: AzStringPairVec,
        pub x11_override_redirect: bool,
        pub x11_window_types: AzXWindowTypeVec,
        pub x11_gtk_theme_variant: AzOptionString,
        pub x11_resize_increments: AzOptionLogicalSize,
        pub x11_base_size: AzOptionLogicalSize,
        pub wayland_app_id: AzOptionString,
        pub wayland_theme: AzOptionWaylandTheme,
        pub request_user_attention: bool,
        pub window_icon: AzOptionWindowIcon,
    }

    /// Re-export of rust-allocated (stack based) `InlineLine` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzInlineLine {
        pub words: AzInlineWordVec,
        pub bounds: AzLogicalRect,
    }

    /// Item entry in a menu or menu bar
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzMenuItem {
        Label(AzStringMenuItem),
        Separator,
        BreakLine,
    }

    /// Re-export of rust-allocated (stack based) `CssPath` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzCssPath {
        pub selectors: AzCssPathSelectorVec,
    }

    /// Re-export of rust-allocated (stack based) `StyleBackgroundContentVecValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzStyleBackgroundContentVecValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundContentVec),
    }

    /// Re-export of rust-allocated (stack based) `StyleFontFamilyVecValue` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzStyleFontFamilyVecValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleFontFamilyVec),
    }

    /// Parsed CSS key-value pair
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzCssProperty {
        TextColor(AzStyleTextColorValue),
        FontSize(AzStyleFontSizeValue),
        FontFamily(AzStyleFontFamilyVecValue),
        TextAlign(AzStyleTextAlignValue),
        LetterSpacing(AzStyleLetterSpacingValue),
        LineHeight(AzStyleLineHeightValue),
        WordSpacing(AzStyleWordSpacingValue),
        TabWidth(AzStyleTabWidthValue),
        Cursor(AzStyleCursorValue),
        Display(AzLayoutDisplayValue),
        Float(AzLayoutFloatValue),
        BoxSizing(AzLayoutBoxSizingValue),
        Width(AzLayoutWidthValue),
        Height(AzLayoutHeightValue),
        MinWidth(AzLayoutMinWidthValue),
        MinHeight(AzLayoutMinHeightValue),
        MaxWidth(AzLayoutMaxWidthValue),
        MaxHeight(AzLayoutMaxHeightValue),
        Position(AzLayoutPositionValue),
        Top(AzLayoutTopValue),
        Right(AzLayoutRightValue),
        Left(AzLayoutLeftValue),
        Bottom(AzLayoutBottomValue),
        FlexWrap(AzLayoutFlexWrapValue),
        FlexDirection(AzLayoutFlexDirectionValue),
        FlexGrow(AzLayoutFlexGrowValue),
        FlexShrink(AzLayoutFlexShrinkValue),
        JustifyContent(AzLayoutJustifyContentValue),
        AlignItems(AzLayoutAlignItemsValue),
        AlignContent(AzLayoutAlignContentValue),
        BackgroundContent(AzStyleBackgroundContentVecValue),
        BackgroundPosition(AzStyleBackgroundPositionVecValue),
        BackgroundSize(AzStyleBackgroundSizeVecValue),
        BackgroundRepeat(AzStyleBackgroundRepeatVecValue),
        OverflowX(AzLayoutOverflowValue),
        OverflowY(AzLayoutOverflowValue),
        PaddingTop(AzLayoutPaddingTopValue),
        PaddingLeft(AzLayoutPaddingLeftValue),
        PaddingRight(AzLayoutPaddingRightValue),
        PaddingBottom(AzLayoutPaddingBottomValue),
        MarginTop(AzLayoutMarginTopValue),
        MarginLeft(AzLayoutMarginLeftValue),
        MarginRight(AzLayoutMarginRightValue),
        MarginBottom(AzLayoutMarginBottomValue),
        BorderTopLeftRadius(AzStyleBorderTopLeftRadiusValue),
        BorderTopRightRadius(AzStyleBorderTopRightRadiusValue),
        BorderBottomLeftRadius(AzStyleBorderBottomLeftRadiusValue),
        BorderBottomRightRadius(AzStyleBorderBottomRightRadiusValue),
        BorderTopColor(AzStyleBorderTopColorValue),
        BorderRightColor(AzStyleBorderRightColorValue),
        BorderLeftColor(AzStyleBorderLeftColorValue),
        BorderBottomColor(AzStyleBorderBottomColorValue),
        BorderTopStyle(AzStyleBorderTopStyleValue),
        BorderRightStyle(AzStyleBorderRightStyleValue),
        BorderLeftStyle(AzStyleBorderLeftStyleValue),
        BorderBottomStyle(AzStyleBorderBottomStyleValue),
        BorderTopWidth(AzLayoutBorderTopWidthValue),
        BorderRightWidth(AzLayoutBorderRightWidthValue),
        BorderLeftWidth(AzLayoutBorderLeftWidthValue),
        BorderBottomWidth(AzLayoutBorderBottomWidthValue),
        BoxShadowLeft(AzStyleBoxShadowValue),
        BoxShadowRight(AzStyleBoxShadowValue),
        BoxShadowTop(AzStyleBoxShadowValue),
        BoxShadowBottom(AzStyleBoxShadowValue),
        ScrollbarStyle(AzScrollbarStyleValue),
        Opacity(AzStyleOpacityValue),
        Transform(AzStyleTransformVecValue),
        TransformOrigin(AzStyleTransformOriginValue),
        PerspectiveOrigin(AzStylePerspectiveOriginValue),
        BackfaceVisibility(AzStyleBackfaceVisibilityValue),
    }

    /// Re-export of rust-allocated (stack based) `TextInputStateWrapper` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzTextInputStateWrapper {
        pub inner: AzTextInputState,
        pub on_text_input: AzOptionTextInputOnTextInput,
        pub on_virtual_key_down: AzOptionTextInputOnVirtualKeyDown,
        pub on_focus_lost: AzOptionTextInputOnFocusLost,
        pub update_text_input_before_calling_focus_lost_fn: bool,
        pub update_text_input_before_calling_vk_down_fn: bool,
    }

    /// Re-export of rust-allocated (stack based) `CssPropertySource` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzCssPropertySource {
        Css(AzCssPath),
        Inline,
    }

    /// Re-export of rust-allocated (stack based) `VertexLayout` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzVertexLayout {
        pub fields: AzVertexAttributeVec,
    }

    /// Re-export of rust-allocated (stack based) `VertexArrayObject` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzVertexArrayObject {
        pub vertex_layout: AzVertexLayout,
        pub vao_id: u32,
        pub gl_context: AzGl,
    }

    /// Re-export of rust-allocated (stack based) `VertexBuffer` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzVertexBuffer {
        pub vertex_buffer_id: u32,
        pub vertex_buffer_len: usize,
        pub vao: AzVertexArrayObject,
        pub index_buffer_id: u32,
        pub index_buffer_len: usize,
        pub index_buffer_format: AzIndexBufferFormat,
    }

    /// Re-export of rust-allocated (stack based) `SvgMultiPolygon` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzSvgMultiPolygon {
        pub rings: AzSvgPathVec,
    }

    /// Re-export of rust-allocated (stack based) `XmlNode` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzXmlNode {
        pub tag: AzString,
        pub attributes: AzStringPairVec,
        pub children: AzXmlNodeVec,
        pub text: AzOptionString,
    }

    /// Wrapper over a Rust-allocated `Vec<InlineLine>`
    #[repr(C)]
    pub struct AzInlineLineVec {
        pub(crate) ptr: *const AzInlineLine,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzInlineLineVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<CssProperty>`
    #[repr(C)]
    pub struct AzCssPropertyVec {
        pub(crate) ptr: *const AzCssProperty,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzCssPropertyVecDestructor,
    }

    /// Wrapper over a Rust-allocated `Vec<SvgMultiPolygon>`
    #[repr(C)]
    pub struct AzSvgMultiPolygonVec {
        pub(crate) ptr: *const AzSvgMultiPolygon,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzSvgMultiPolygonVecDestructor,
    }

    /// Re-export of rust-allocated (stack based) `OptionCssProperty` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionCssProperty {
        None,
        Some(AzCssProperty),
    }

    /// Re-export of rust-allocated (stack based) `XmlTextError` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzXmlTextError {
        pub stream_error: AzXmlStreamError,
        pub pos: AzSvgParseErrorPosition,
    }

    /// Platform-specific window configuration, i.e. WM options that are not cross-platform
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzPlatformSpecificOptions {
        pub windows_options: AzWindowsWindowOptions,
        pub linux_options: AzLinuxWindowOptions,
        pub mac_options: AzMacWindowOptions,
        pub wasm_options: AzWasmWindowOptions,
    }

    /// Re-export of rust-allocated (stack based) `WindowState` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzWindowState {
        pub title: AzString,
        pub theme: AzWindowTheme,
        pub size: AzWindowSize,
        pub position: AzWindowPosition,
        pub flags: AzWindowFlags,
        pub debug_state: AzDebugState,
        pub keyboard_state: AzKeyboardState,
        pub mouse_state: AzMouseState,
        pub touch_state: AzTouchState,
        pub ime_position: AzImePosition,
        pub monitor: AzMonitor,
        pub platform_specific_options: AzPlatformSpecificOptions,
        pub renderer_options: AzRendererOptions,
        pub background_color: AzColorU,
        pub layout_callback: AzLayoutCallback,
        pub close_callback: AzOptionCallback,
    }

    /// Re-export of rust-allocated (stack based) `CallbackInfo` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzCallbackInfo {
        pub css_property_cache: *const c_void,
        pub styled_node_states: *const c_void,
        pub previous_window_state: *const c_void,
        pub current_window_state: *const c_void,
        pub modifiable_window_state: *mut AzWindowState,
        pub gl_context: *const AzOptionGl,
        pub image_cache: *mut c_void,
        pub system_fonts: *mut c_void,
        pub timers: *mut c_void,
        pub threads: *mut c_void,
        pub new_windows: *mut c_void,
        pub current_window_handle: *const AzRawWindowHandle,
        pub node_hierarchy: *const c_void,
        pub system_callbacks: *const AzSystemCallbacks,
        pub datasets: *mut c_void,
        pub stop_propagation: *mut bool,
        pub focus_target: *mut c_void,
        pub words_cache: *const c_void,
        pub shaped_words_cache: *const c_void,
        pub positioned_words_cache: *const c_void,
        pub positioned_rects: *const c_void,
        pub words_changed_in_callbacks: *mut c_void,
        pub images_changed_in_callbacks: *mut c_void,
        pub image_masks_changed_in_callbacks: *mut c_void,
        pub css_properties_changed_in_callbacks: *mut c_void,
        pub current_scroll_states: *const c_void,
        pub nodes_scrolled_in_callback: *mut c_void,
        pub hit_dom_node: AzDomNodeId,
        pub cursor_relative_to_item: AzOptionLogicalPosition,
        pub cursor_in_viewport: AzOptionLogicalPosition,
        pub _reserved_ref: *const c_void,
        pub _reserved_mut: *mut c_void,
    }

    /// Re-export of rust-allocated (stack based) `InlineText` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzInlineText {
        pub lines: AzInlineLineVec,
        pub content_size: AzLogicalSize,
        pub font_size_px: f32,
        pub last_word_index: usize,
        pub baseline_descender_px: f32,
    }

    /// CSS path to set the keyboard input focus
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzFocusTargetPath {
        pub dom: AzDomId,
        pub css_path: AzCssPath,
    }

    /// Animation struct to start a new animation
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzAnimation {
        pub from: AzCssProperty,
        pub to: AzCssProperty,
        pub duration: AzDuration,
        pub repeat: AzAnimationRepeat,
        pub repeat_count: AzAnimationRepeatCount,
        pub easing: AzAnimationEasing,
        pub relayout_on_finish: bool,
    }

    /// Re-export of rust-allocated (stack based) `TimerCallbackInfo` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzTimerCallbackInfo {
        pub callback_info: AzCallbackInfo,
        pub node_id: AzOptionDomNodeId,
        pub frame_start: AzInstant,
        pub call_count: usize,
        pub is_about_to_finish: bool,
        pub _reserved_ref: *const c_void,
        pub _reserved_mut: *mut c_void,
    }

    /// Re-export of rust-allocated (stack based) `NodeDataInlineCssProperty` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzNodeDataInlineCssProperty {
        Normal(AzCssProperty),
        Active(AzCssProperty),
        Focus(AzCssProperty),
        Hover(AzCssProperty),
    }

    /// Re-export of rust-allocated (stack based) `DynamicCssProperty` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzDynamicCssProperty {
        pub dynamic_id: AzString,
        pub default_value: AzCssProperty,
    }

    /// Re-export of rust-allocated (stack based) `SvgNode` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzSvgNode {
        MultiPolygonCollection(AzSvgMultiPolygonVec),
        MultiPolygon(AzSvgMultiPolygon),
        Path(AzSvgPath),
        Circle(AzSvgCircle),
        Rect(AzSvgRect),
    }

    /// Re-export of rust-allocated (stack based) `SvgStyledNode` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzSvgStyledNode {
        pub geometry: AzSvgNode,
        pub style: AzSvgStyle,
    }

    /// Wrapper over a Rust-allocated `Vec<NodeDataInlineCssProperty>`
    #[repr(C)]
    pub struct AzNodeDataInlineCssPropertyVec {
        pub(crate) ptr: *const AzNodeDataInlineCssProperty,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzNodeDataInlineCssPropertyVecDestructor,
    }

    /// Re-export of rust-allocated (stack based) `OptionWindowState` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionWindowState {
        None,
        Some(AzWindowState),
    }

    /// Re-export of rust-allocated (stack based) `OptionInlineText` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionInlineText {
        None,
        Some(AzInlineText),
    }

    /// Re-export of rust-allocated (stack based) `XmlParseError` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzXmlParseError {
        InvalidDeclaration(AzXmlTextError),
        InvalidComment(AzXmlTextError),
        InvalidPI(AzXmlTextError),
        InvalidDoctype(AzXmlTextError),
        InvalidEntity(AzXmlTextError),
        InvalidElement(AzXmlTextError),
        InvalidAttribute(AzXmlTextError),
        InvalidCdata(AzXmlTextError),
        InvalidCharData(AzXmlTextError),
        UnknownToken(AzSvgParseErrorPosition),
    }

    /// Options on how to initially create the window
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzWindowCreateOptions {
        pub state: AzWindowState,
        pub renderer_type: AzOptionRendererOptions,
        pub theme: AzOptionWindowTheme,
        pub create_callback: AzOptionCallback,
        pub hot_reload: bool,
    }

    /// Defines the keyboard input focus target
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzFocusTarget {
        Id(AzDomNodeId),
        Path(AzFocusTargetPath),
        Previous,
        Next,
        First,
        Last,
        NoFocus,
    }

    /// Represents one single DOM node (node type, classes, ids and callbacks are stored here)
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzNodeData {
        pub node_type: AzNodeType,
        pub dataset: AzOptionRefAny,
        pub ids_and_classes: AzIdOrClassVec,
        pub callbacks: AzCallbackDataVec,
        pub inline_css_props: AzNodeDataInlineCssPropertyVec,
        pub extra: *const c_void,
    }

    /// Re-export of rust-allocated (stack based) `CssDeclaration` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzCssDeclaration {
        Static(AzCssProperty),
        Dynamic(AzDynamicCssProperty),
    }

    /// Re-export of rust-allocated (stack based) `Button` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzButton {
        pub label: AzString,
        pub image: AzOptionImageRef,
        pub container_style: AzNodeDataInlineCssPropertyVec,
        pub label_style: AzNodeDataInlineCssPropertyVec,
        pub image_style: AzNodeDataInlineCssPropertyVec,
        pub on_click: AzOptionButtonOnClick,
    }

    /// Re-export of rust-allocated (stack based) `CheckBox` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzCheckBox {
        pub state: AzCheckBoxStateWrapper,
        pub container_style: AzNodeDataInlineCssPropertyVec,
        pub content_style: AzNodeDataInlineCssPropertyVec,
    }

    /// Re-export of rust-allocated (stack based) `Label` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzLabel {
        pub text: AzString,
        pub style: AzNodeDataInlineCssPropertyVec,
    }

    /// Re-export of rust-allocated (stack based) `ColorInput` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzColorInput {
        pub state: AzColorInputStateWrapper,
        pub style: AzNodeDataInlineCssPropertyVec,
    }

    /// Re-export of rust-allocated (stack based) `TextInput` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzTextInput {
        pub state: AzTextInputStateWrapper,
        pub placeholder_style: AzNodeDataInlineCssPropertyVec,
        pub container_style: AzNodeDataInlineCssPropertyVec,
        pub label_style: AzNodeDataInlineCssPropertyVec,
    }

    /// Re-export of rust-allocated (stack based) `NumberInput` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzNumberInput {
        pub text_input: AzTextInput,
        pub state: AzNumberInputStateWrapper,
    }

    /// Wrapper over a Rust-allocated `CssDeclaration`
    #[repr(C)]
    pub struct AzCssDeclarationVec {
        pub(crate) ptr: *const AzCssDeclaration,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzCssDeclarationVecDestructor,
    }

    /// Wrapper over a Rust-allocated `NodeDataVec`
    #[repr(C)]
    pub struct AzNodeDataVec {
        pub(crate) ptr: *const AzNodeData,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzNodeDataVecDestructor,
    }

    /// Re-export of rust-allocated (stack based) `XmlError` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzXmlError {
        InvalidXmlPrefixUri(AzSvgParseErrorPosition),
        UnexpectedXmlUri(AzSvgParseErrorPosition),
        UnexpectedXmlnsUri(AzSvgParseErrorPosition),
        InvalidElementNamePrefix(AzSvgParseErrorPosition),
        DuplicatedNamespace(AzDuplicatedNamespaceError),
        UnknownNamespace(AzUnknownNamespaceError),
        UnexpectedCloseTag(AzUnexpectedCloseTagError),
        UnexpectedEntityCloseTag(AzSvgParseErrorPosition),
        UnknownEntityReference(AzUnknownEntityReferenceError),
        MalformedEntityReference(AzSvgParseErrorPosition),
        EntityReferenceLoop(AzSvgParseErrorPosition),
        InvalidAttributeValue(AzSvgParseErrorPosition),
        DuplicatedAttribute(AzDuplicatedAttributeError),
        NoRootNode,
        SizeLimit,
        ParserError(AzXmlParseError),
    }

    /// Re-export of rust-allocated (stack based) `Dom` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzDom {
        pub root: AzNodeData,
        pub children: AzDomVec,
        pub total_children: usize,
    }

    /// Re-export of rust-allocated (stack based) `CssRuleBlock` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzCssRuleBlock {
        pub path: AzCssPath,
        pub declarations: AzCssDeclarationVec,
    }

    /// Re-export of rust-allocated (stack based) `StyledDom` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzStyledDom {
        pub root: AzNodeId,
        pub node_hierarchy: AzNodeVec,
        pub node_data: AzNodeDataVec,
        pub styled_nodes: AzStyledNodeVec,
        pub cascade_info: AzCascadeInfoVec,
        pub nodes_with_window_callbacks: AzNodeIdVec,
        pub nodes_with_not_callbacks: AzNodeIdVec,
        pub nodes_with_datasets_and_callbacks: AzNodeIdVec,
        pub tag_ids_to_node_ids: AzTagIdToNodeIdMappingVec,
        pub non_leaf_nodes: AzParentWithNodeDepthVec,
        pub css_property_cache: AzCssPropertyCache,
    }

    /// Wrapper over a Rust-allocated `CssRuleBlock`
    #[repr(C)]
    pub struct AzCssRuleBlockVec {
        pub(crate) ptr: *const AzCssRuleBlock,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzCssRuleBlockVecDestructor,
    }

    /// Re-export of rust-allocated (stack based) `OptionDom` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzOptionDom {
        None,
        Some(AzDom),
    }

    /// Re-export of rust-allocated (stack based) `ResultXmlXmlError` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzResultXmlXmlError {
        Ok(AzXml),
        Err(AzXmlError),
    }

    /// Re-export of rust-allocated (stack based) `SvgParseError` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzSvgParseError {
        InvalidFileSuffix,
        FileOpenFailed,
        NotAnUtf8Str,
        MalformedGZip,
        InvalidSize,
        ParsingFailed(AzXmlError),
    }

    /// <img src="../images/scrollbounds.png"/>
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzIFrameCallbackReturn {
        pub dom: AzStyledDom,
        pub scroll_size: AzLogicalSize,
        pub scroll_offset: AzLogicalPosition,
        pub virtual_scroll_size: AzLogicalSize,
        pub virtual_scroll_offset: AzLogicalPosition,
    }

    /// Re-export of rust-allocated (stack based) `Stylesheet` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzStylesheet {
        pub rules: AzCssRuleBlockVec,
    }

    /// Wrapper over a Rust-allocated `Stylesheet`
    #[repr(C)]
    pub struct AzStylesheetVec {
        pub(crate) ptr: *const AzStylesheet,
        pub len: usize,
        pub cap: usize,
        pub destructor: AzStylesheetVecDestructor,
    }

    /// Re-export of rust-allocated (stack based) `ResultSvgXmlNodeSvgParseError` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzResultSvgXmlNodeSvgParseError {
        Ok(AzSvgXmlNode),
        Err(AzSvgParseError),
    }

    /// Re-export of rust-allocated (stack based) `ResultSvgSvgParseError` struct
    #[repr(C, u8)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub enum AzResultSvgSvgParseError {
        Ok(AzSvg),
        Err(AzSvgParseError),
    }

    /// Re-export of rust-allocated (stack based) `Css` struct
    #[repr(C)]
    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(PartialEq, PartialOrd)]
    pub struct AzCss {
        pub stylesheets: AzStylesheetVec,
    }

    #[cfg_attr(target_os = "windows", link(name="azul.dll"))] // https://github.com/rust-lang/cargo/issues/9082
    #[cfg_attr(not(target_os = "windows"), link(name="azul"))] // https://github.com/rust-lang/cargo/issues/9082
    extern "C" {
        pub(crate) fn AzApp_new(_:  AzRefAny, _:  AzAppConfig) -> AzApp;
        pub(crate) fn AzApp_addWindow(_:  &mut AzApp, _:  AzWindowCreateOptions);
        pub(crate) fn AzApp_addImage(_:  &mut AzApp, _:  AzString, _:  AzImageRef);
        pub(crate) fn AzApp_getMonitors(_:  &AzApp) -> AzMonitorVec;
        pub(crate) fn AzApp_run(_:  &AzApp, _:  AzWindowCreateOptions);
        pub(crate) fn AzApp_delete(_:  &mut AzApp);
        pub(crate) fn AzApp_deepCopy(_:  &AzApp) -> AzApp;
        pub(crate) fn AzAppConfig_new(_:  AzLayoutSolver) -> AzAppConfig;
        pub(crate) fn AzSystemCallbacks_libraryInternal() -> AzSystemCallbacks;
        pub(crate) fn AzWindowCreateOptions_new(_:  AzLayoutCallbackType) -> AzWindowCreateOptions;
        pub(crate) fn AzWindowState_new(_:  AzLayoutCallbackType) -> AzWindowState;
        pub(crate) fn AzWindowState_default() -> AzWindowState;
        pub(crate) fn AzCallbackInfo_getHitNode(_:  &AzCallbackInfo) -> AzDomNodeId;
        pub(crate) fn AzCallbackInfo_getSystemTimeFn(_:  &AzCallbackInfo) -> AzGetSystemTimeFn;
        pub(crate) fn AzCallbackInfo_getCursorRelativeToViewport(_:  &AzCallbackInfo) -> AzOptionLogicalPosition;
        pub(crate) fn AzCallbackInfo_getCursorRelativeToNode(_:  &AzCallbackInfo) -> AzOptionLogicalPosition;
        pub(crate) fn AzCallbackInfo_getCurrentWindowState(_:  &AzCallbackInfo) -> AzWindowState;
        pub(crate) fn AzCallbackInfo_getCurrentKeyboardState(_:  &AzCallbackInfo) -> AzKeyboardState;
        pub(crate) fn AzCallbackInfo_getCurrentMouseState(_:  &AzCallbackInfo) -> AzMouseState;
        pub(crate) fn AzCallbackInfo_getPreviousWindowState(_:  &AzCallbackInfo) -> AzOptionWindowState;
        pub(crate) fn AzCallbackInfo_getPreviousKeyboardState(_:  &AzCallbackInfo) -> AzOptionKeyboardState;
        pub(crate) fn AzCallbackInfo_getPreviousMouseState(_:  &AzCallbackInfo) -> AzOptionMouseState;
        pub(crate) fn AzCallbackInfo_getCurrentWindowHandle(_:  &AzCallbackInfo) -> AzRawWindowHandle;
        pub(crate) fn AzCallbackInfo_getGlContext(_:  &AzCallbackInfo) -> AzOptionGl;
        pub(crate) fn AzCallbackInfo_getScrollPosition(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionLogicalPosition;
        pub(crate) fn AzCallbackInfo_getDataset(_:  &mut AzCallbackInfo, _:  AzDomNodeId) -> AzOptionRefAny;
        pub(crate) fn AzCallbackInfo_getStringContents(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionString;
        pub(crate) fn AzCallbackInfo_getInlineText(_:  &AzCallbackInfo, _:  AzDomNodeId) -> AzOptionInlineText;
        pub(crate) fn AzCallbackInfo_getIndexInParent(_:  &mut AzCallbackInfo, _:  AzDomNodeId) -> usize;
        pub(crate) fn AzCallbackInfo_getParent(_:  &mut AzCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId;
        pub(crate) fn AzCallbackInfo_getPreviousSibling(_:  &mut AzCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId;
        pub(crate) fn AzCallbackInfo_getNextSibling(_:  &mut AzCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId;
        pub(crate) fn AzCallbackInfo_getFirstChild(_:  &mut AzCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId;
        pub(crate) fn AzCallbackInfo_getLastChild(_:  &mut AzCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId;
        pub(crate) fn AzCallbackInfo_getNodePosition(_:  &mut AzCallbackInfo, _:  AzDomNodeId) -> AzOptionPositionInfo;
        pub(crate) fn AzCallbackInfo_getNodeSize(_:  &mut AzCallbackInfo, _:  AzDomNodeId) -> AzOptionLogicalSize;
        pub(crate) fn AzCallbackInfo_getComputedCssProperty(_:  &mut AzCallbackInfo, _:  AzDomNodeId, _:  AzCssPropertyType) -> AzOptionCssProperty;
        pub(crate) fn AzCallbackInfo_setWindowState(_:  &mut AzCallbackInfo, _:  AzWindowState);
        pub(crate) fn AzCallbackInfo_setFocus(_:  &mut AzCallbackInfo, _:  AzFocusTarget);
        pub(crate) fn AzCallbackInfo_setCssProperty(_:  &mut AzCallbackInfo, _:  AzDomNodeId, _:  AzCssProperty);
        pub(crate) fn AzCallbackInfo_setScrollPosition(_:  &mut AzCallbackInfo, _:  AzDomNodeId, _:  AzLogicalPosition);
        pub(crate) fn AzCallbackInfo_setStringContents(_:  &mut AzCallbackInfo, _:  AzDomNodeId, _:  AzString);
        pub(crate) fn AzCallbackInfo_addImage(_:  &mut AzCallbackInfo, _:  AzString, _:  AzImageRef);
        pub(crate) fn AzCallbackInfo_hasImage(_:  &AzCallbackInfo, _:  AzString) -> bool;
        pub(crate) fn AzCallbackInfo_getImage(_:  &AzCallbackInfo, _:  AzString) -> AzOptionImageRef;
        pub(crate) fn AzCallbackInfo_updateImage(_:  &mut AzCallbackInfo, _:  AzDomNodeId, _:  AzImageRef);
        pub(crate) fn AzCallbackInfo_deleteImage(_:  &mut AzCallbackInfo, _:  AzString);
        pub(crate) fn AzCallbackInfo_updateImageMask(_:  &mut AzCallbackInfo, _:  AzDomNodeId, _:  AzImageMask);
        pub(crate) fn AzCallbackInfo_stopPropagation(_:  &mut AzCallbackInfo);
        pub(crate) fn AzCallbackInfo_createWindow(_:  &mut AzCallbackInfo, _:  AzWindowCreateOptions);
        pub(crate) fn AzCallbackInfo_startTimer(_:  &mut AzCallbackInfo, _:  AzTimer) -> AzOptionTimerId;
        pub(crate) fn AzCallbackInfo_startAnimation(_:  &mut AzCallbackInfo, _:  AzDomNodeId, _:  AzAnimation) -> AzOptionTimerId;
        pub(crate) fn AzCallbackInfo_stopTimer(_:  &mut AzCallbackInfo, _:  AzTimerId) -> bool;
        pub(crate) fn AzCallbackInfo_startThread(_:  &mut AzCallbackInfo, _:  AzRefAny, _:  AzRefAny, _:  AzThreadCallback) -> AzOptionThreadId;
        pub(crate) fn AzCallbackInfo_sendThreadMsg(_:  &mut AzCallbackInfo, _:  AzThreadId, _:  AzThreadSendMsg) -> bool;
        pub(crate) fn AzCallbackInfo_stopThread(_:  &mut AzCallbackInfo, _:  AzThreadId) -> bool;
        pub(crate) fn AzHidpiAdjustedBounds_getLogicalSize(_:  &AzHidpiAdjustedBounds) -> AzLogicalSize;
        pub(crate) fn AzHidpiAdjustedBounds_getPhysicalSize(_:  &AzHidpiAdjustedBounds) -> AzPhysicalSizeU32;
        pub(crate) fn AzHidpiAdjustedBounds_getHidpiFactor(_:  &AzHidpiAdjustedBounds) -> f32;
        pub(crate) fn AzInlineText_hitTest(_:  &AzInlineText, _:  AzLogicalPosition) -> AzInlineTextHitVec;
        pub(crate) fn AzRenderImageCallbackInfo_getGlContext(_:  &AzRenderImageCallbackInfo) -> AzOptionGl;
        pub(crate) fn AzRenderImageCallbackInfo_getBounds(_:  &AzRenderImageCallbackInfo) -> AzHidpiAdjustedBounds;
        pub(crate) fn AzRenderImageCallbackInfo_getCallbackNodeId(_:  &AzRenderImageCallbackInfo) -> AzDomNodeId;
        pub(crate) fn AzRenderImageCallbackInfo_getInlineText(_:  &AzRenderImageCallbackInfo, _:  AzDomNodeId) -> AzOptionInlineText;
        pub(crate) fn AzRenderImageCallbackInfo_getIndexInParent(_:  &mut AzRenderImageCallbackInfo, _:  AzDomNodeId) -> usize;
        pub(crate) fn AzRenderImageCallbackInfo_getParent(_:  &mut AzRenderImageCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId;
        pub(crate) fn AzRenderImageCallbackInfo_getPreviousSibling(_:  &mut AzRenderImageCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId;
        pub(crate) fn AzRenderImageCallbackInfo_getNextSibling(_:  &mut AzRenderImageCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId;
        pub(crate) fn AzRenderImageCallbackInfo_getFirstChild(_:  &mut AzRenderImageCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId;
        pub(crate) fn AzRenderImageCallbackInfo_getLastChild(_:  &mut AzRenderImageCallbackInfo, _:  AzDomNodeId) -> AzOptionDomNodeId;
        pub(crate) fn AzRefCount_canBeShared(_:  &AzRefCount) -> bool;
        pub(crate) fn AzRefCount_canBeSharedMut(_:  &AzRefCount) -> bool;
        pub(crate) fn AzRefCount_increaseRef(_:  &mut AzRefCount);
        pub(crate) fn AzRefCount_decreaseRef(_:  &mut AzRefCount);
        pub(crate) fn AzRefCount_increaseRefmut(_:  &mut AzRefCount);
        pub(crate) fn AzRefCount_decreaseRefmut(_:  &mut AzRefCount);
        pub(crate) fn AzRefCount_delete(_:  &mut AzRefCount);
        pub(crate) fn AzRefCount_deepCopy(_:  &AzRefCount) -> AzRefCount;
        pub(crate) fn AzRefAny_newC(_:  *const c_void, _:  usize, _:  u64, _:  AzString, _:  AzRefAnyDestructorType) -> AzRefAny;
        pub(crate) fn AzRefAny_getTypeId(_:  &AzRefAny) -> u64;
        pub(crate) fn AzRefAny_getTypeName(_:  &AzRefAny) -> AzString;
        pub(crate) fn AzRefAny_delete(_:  &mut AzRefAny);
        pub(crate) fn AzRefAny_deepCopy(_:  &AzRefAny) -> AzRefAny;
        pub(crate) fn AzLayoutCallbackInfo_getGlContext(_:  &AzLayoutCallbackInfo) -> AzOptionGl;
        pub(crate) fn AzLayoutCallbackInfo_getSystemFonts(_:  &AzLayoutCallbackInfo) -> AzStringPairVec;
        pub(crate) fn AzLayoutCallbackInfo_getImage(_:  &AzLayoutCallbackInfo, _:  AzString) -> AzOptionImageRef;
        pub(crate) fn AzDom_new(_:  AzNodeType) -> AzDom;
        pub(crate) fn AzDom_body() -> AzDom;
        pub(crate) fn AzDom_div() -> AzDom;
        pub(crate) fn AzDom_br() -> AzDom;
        pub(crate) fn AzDom_text(_:  AzString) -> AzDom;
        pub(crate) fn AzDom_image(_:  AzImageRef) -> AzDom;
        pub(crate) fn AzDom_iframe(_:  AzRefAny, _:  AzIFrameCallbackType) -> AzDom;
        pub(crate) fn AzDom_setNodeType(_:  &mut AzDom, _:  AzNodeType);
        pub(crate) fn AzDom_withNodeType(_:  &mut AzDom, _:  AzNodeType) -> AzDom;
        pub(crate) fn AzDom_setDataset(_:  &mut AzDom, _:  AzRefAny);
        pub(crate) fn AzDom_withDataset(_:  &mut AzDom, _:  AzRefAny) -> AzDom;
        pub(crate) fn AzDom_setIdsAndClasses(_:  &mut AzDom, _:  AzIdOrClassVec);
        pub(crate) fn AzDom_withIdsAndClasses(_:  &mut AzDom, _:  AzIdOrClassVec) -> AzDom;
        pub(crate) fn AzDom_setCallbacks(_:  &mut AzDom, _:  AzCallbackDataVec);
        pub(crate) fn AzDom_withCallbacks(_:  &mut AzDom, _:  AzCallbackDataVec) -> AzDom;
        pub(crate) fn AzDom_setInlineCssProps(_:  &mut AzDom, _:  AzNodeDataInlineCssPropertyVec);
        pub(crate) fn AzDom_withInlineCssProps(_:  &mut AzDom, _:  AzNodeDataInlineCssPropertyVec) -> AzDom;
        pub(crate) fn AzDom_addChild(_:  &mut AzDom, _:  AzDom);
        pub(crate) fn AzDom_withChild(_:  &mut AzDom, _:  AzDom) -> AzDom;
        pub(crate) fn AzDom_setChildren(_:  &mut AzDom, _:  AzDomVec);
        pub(crate) fn AzDom_withChildren(_:  &mut AzDom, _:  AzDomVec) -> AzDom;
        pub(crate) fn AzDom_addId(_:  &mut AzDom, _:  AzString);
        pub(crate) fn AzDom_withId(_:  &mut AzDom, _:  AzString) -> AzDom;
        pub(crate) fn AzDom_addClass(_:  &mut AzDom, _:  AzString);
        pub(crate) fn AzDom_withClass(_:  &mut AzDom, _:  AzString) -> AzDom;
        pub(crate) fn AzDom_addCssProperty(_:  &mut AzDom, _:  AzCssProperty);
        pub(crate) fn AzDom_withCssProperty(_:  &mut AzDom, _:  AzCssProperty) -> AzDom;
        pub(crate) fn AzDom_addHoverCssProperty(_:  &mut AzDom, _:  AzCssProperty);
        pub(crate) fn AzDom_withHoverCssProperty(_:  &mut AzDom, _:  AzCssProperty) -> AzDom;
        pub(crate) fn AzDom_addActiveCssProperty(_:  &mut AzDom, _:  AzCssProperty);
        pub(crate) fn AzDom_withActiveCssProperty(_:  &mut AzDom, _:  AzCssProperty) -> AzDom;
        pub(crate) fn AzDom_addFocusCssProperty(_:  &mut AzDom, _:  AzCssProperty);
        pub(crate) fn AzDom_withFocusCssProperty(_:  &mut AzDom, _:  AzCssProperty) -> AzDom;
        pub(crate) fn AzDom_setClipMask(_:  &mut AzDom, _:  AzImageMask);
        pub(crate) fn AzDom_withClipMask(_:  &mut AzDom, _:  AzImageMask) -> AzDom;
        pub(crate) fn AzDom_setTabIndex(_:  &mut AzDom, _:  AzTabIndex);
        pub(crate) fn AzDom_withTabIndex(_:  &mut AzDom, _:  AzTabIndex) -> AzDom;
        pub(crate) fn AzDom_setAccessibilityInfo(_:  &mut AzDom, _:  AzAccessibilityInfo);
        pub(crate) fn AzDom_withAccessibilityInfo(_:  &mut AzDom, _:  AzAccessibilityInfo) -> AzDom;
        pub(crate) fn AzDom_setMenuBar(_:  &mut AzDom, _:  AzMenu);
        pub(crate) fn AzDom_withMenuBar(_:  &mut AzDom, _:  AzMenu) -> AzDom;
        pub(crate) fn AzDom_setContextMenu(_:  &mut AzDom, _:  AzMenu);
        pub(crate) fn AzDom_withContextMenu(_:  &mut AzDom, _:  AzMenu) -> AzDom;
        pub(crate) fn AzDom_hash(_:  &AzDom) -> u64;
        pub(crate) fn AzDom_nodeCount(_:  &AzDom) -> usize;
        pub(crate) fn AzDom_getHtmlStringTest(_:  &mut AzDom) -> AzString;
        pub(crate) fn AzDom_getHtmlStringDebug(_:  &mut AzDom) -> AzString;
        pub(crate) fn AzDom_style(_:  &mut AzDom, _:  AzCss) -> AzStyledDom;
        pub(crate) fn AzNodeData_new(_:  AzNodeType) -> AzNodeData;
        pub(crate) fn AzNodeData_body() -> AzNodeData;
        pub(crate) fn AzNodeData_div() -> AzNodeData;
        pub(crate) fn AzNodeData_br() -> AzNodeData;
        pub(crate) fn AzNodeData_text(_:  AzString) -> AzNodeData;
        pub(crate) fn AzNodeData_image(_:  AzImageRef) -> AzNodeData;
        pub(crate) fn AzNodeData_iframe(_:  AzRefAny, _:  AzIFrameCallbackType) -> AzNodeData;
        pub(crate) fn AzNodeData_setNodeType(_:  &mut AzNodeData, _:  AzNodeType);
        pub(crate) fn AzNodeData_withNodeType(_:  &mut AzNodeData, _:  AzNodeType) -> AzNodeData;
        pub(crate) fn AzNodeData_setDataset(_:  &mut AzNodeData, _:  AzRefAny);
        pub(crate) fn AzNodeData_withDataset(_:  &mut AzNodeData, _:  AzRefAny) -> AzNodeData;
        pub(crate) fn AzNodeData_setIdsAndClasses(_:  &mut AzNodeData, _:  AzIdOrClassVec);
        pub(crate) fn AzNodeData_withIdsAndClasses(_:  &mut AzNodeData, _:  AzIdOrClassVec) -> AzNodeData;
        pub(crate) fn AzNodeData_setCallbacks(_:  &mut AzNodeData, _:  AzCallbackDataVec);
        pub(crate) fn AzNodeData_withCallbacks(_:  &mut AzNodeData, _:  AzCallbackDataVec) -> AzNodeData;
        pub(crate) fn AzNodeData_setInlineCssProps(_:  &mut AzNodeData, _:  AzNodeDataInlineCssPropertyVec);
        pub(crate) fn AzNodeData_withInlineCssProps(_:  &mut AzNodeData, _:  AzNodeDataInlineCssPropertyVec) -> AzNodeData;
        pub(crate) fn AzNodeData_setClipMask(_:  &mut AzNodeData, _:  AzImageMask);
        pub(crate) fn AzNodeData_setTabIndex(_:  &mut AzNodeData, _:  AzTabIndex);
        pub(crate) fn AzNodeData_setAccessibilityInfo(_:  &mut AzNodeData, _:  AzAccessibilityInfo);
        pub(crate) fn AzNodeData_setMenuBar(_:  &mut AzNodeData, _:  AzMenu);
        pub(crate) fn AzNodeData_setContextMenu(_:  &mut AzNodeData, _:  AzMenu);
        pub(crate) fn AzNodeData_hash(_:  &AzNodeData) -> u64;
        pub(crate) fn AzOn_intoEventFilter(_:  AzOn) -> AzEventFilter;
        pub(crate) fn AzCss_empty() -> AzCss;
        pub(crate) fn AzCss_fromString(_:  AzString) -> AzCss;
        pub(crate) fn AzColorU_fromStr(_:  AzString) -> AzColorU;
        pub(crate) fn AzColorU_toHash(_:  &AzColorU) -> AzString;
        pub(crate) fn AzCssProperty_getKeyString(_:  &AzCssProperty) -> AzString;
        pub(crate) fn AzCssProperty_getValueString(_:  &AzCssProperty) -> AzString;
        pub(crate) fn AzCssProperty_getKeyValueString(_:  &AzCssProperty) -> AzString;
        pub(crate) fn AzCssProperty_interpolate(_:  &AzCssProperty, _:  AzCssProperty, _:  f32, _:  AzInterpolateContext) -> AzCssProperty;
        pub(crate) fn AzButton_new(_:  AzString) -> AzButton;
        pub(crate) fn AzButton_setOnClick(_:  &mut AzButton, _:  AzRefAny, _:  AzCallbackType);
        pub(crate) fn AzButton_withOnClick(_:  &mut AzButton, _:  AzRefAny, _:  AzCallbackType) -> AzButton;
        pub(crate) fn AzButton_dom(_:  &mut AzButton) -> AzDom;
        pub(crate) fn AzCheckBox_new(_:  bool) -> AzCheckBox;
        pub(crate) fn AzCheckBox_setOnToggle(_:  &mut AzCheckBox, _:  AzRefAny, _:  AzCheckBoxOnToggleCallbackType);
        pub(crate) fn AzCheckBox_withOnToggle(_:  &mut AzCheckBox, _:  AzRefAny, _:  AzCheckBoxOnToggleCallbackType) -> AzCheckBox;
        pub(crate) fn AzCheckBox_dom(_:  &mut AzCheckBox) -> AzDom;
        pub(crate) fn AzLabel_new(_:  AzString) -> AzLabel;
        pub(crate) fn AzLabel_dom(_:  &mut AzLabel) -> AzDom;
        pub(crate) fn AzColorInput_new(_:  AzColorU) -> AzColorInput;
        pub(crate) fn AzColorInput_setOnValueChange(_:  &mut AzColorInput, _:  AzRefAny, _:  AzColorInputOnValueChangeCallbackType);
        pub(crate) fn AzColorInput_withOnValueChange(_:  &mut AzColorInput, _:  AzRefAny, _:  AzColorInputOnValueChangeCallbackType) -> AzColorInput;
        pub(crate) fn AzColorInput_dom(_:  &mut AzColorInput) -> AzDom;
        pub(crate) fn AzTextInput_new(_:  AzString) -> AzTextInput;
        pub(crate) fn AzTextInput_setOnTextInput(_:  &mut AzTextInput, _:  AzRefAny, _:  AzTextInputOnTextInputCallbackType);
        pub(crate) fn AzTextInput_withOnTextInput(_:  &mut AzTextInput, _:  AzRefAny, _:  AzTextInputOnTextInputCallbackType) -> AzTextInput;
        pub(crate) fn AzTextInput_setOnVirtualKeyDown(_:  &mut AzTextInput, _:  AzRefAny, _:  AzTextInputOnVirtualKeyDownCallbackType);
        pub(crate) fn AzTextInput_withOnVirtualKeyDown(_:  &mut AzTextInput, _:  AzRefAny, _:  AzTextInputOnVirtualKeyDownCallbackType) -> AzTextInput;
        pub(crate) fn AzTextInput_setOnFocusLost(_:  &mut AzTextInput, _:  AzRefAny, _:  AzTextInputOnFocusLostCallbackType);
        pub(crate) fn AzTextInput_withOnFocusLost(_:  &mut AzTextInput, _:  AzRefAny, _:  AzTextInputOnFocusLostCallbackType) -> AzTextInput;
        pub(crate) fn AzTextInput_setPlaceholderStyle(_:  &mut AzTextInput, _:  AzNodeDataInlineCssPropertyVec);
        pub(crate) fn AzTextInput_withPlaceholderStyle(_:  &mut AzTextInput, _:  AzNodeDataInlineCssPropertyVec) -> AzTextInput;
        pub(crate) fn AzTextInput_setContainerStyle(_:  &mut AzTextInput, _:  AzNodeDataInlineCssPropertyVec);
        pub(crate) fn AzTextInput_withContainerStyle(_:  &mut AzTextInput, _:  AzNodeDataInlineCssPropertyVec) -> AzTextInput;
        pub(crate) fn AzTextInput_setLabelStyle(_:  &mut AzTextInput, _:  AzNodeDataInlineCssPropertyVec);
        pub(crate) fn AzTextInput_withLabelStyle(_:  &mut AzTextInput, _:  AzNodeDataInlineCssPropertyVec) -> AzTextInput;
        pub(crate) fn AzTextInput_dom(_:  &mut AzTextInput) -> AzDom;
        pub(crate) fn AzNumberInput_new(_:  f32) -> AzNumberInput;
        pub(crate) fn AzCssPropertyCache_delete(_:  &mut AzCssPropertyCache);
        pub(crate) fn AzCssPropertyCache_deepCopy(_:  &AzCssPropertyCache) -> AzCssPropertyCache;
        pub(crate) fn AzStyledDom_new(_:  AzDom, _:  AzCss) -> AzStyledDom;
        pub(crate) fn AzStyledDom_default() -> AzStyledDom;
        pub(crate) fn AzStyledDom_fromXml(_:  AzString) -> AzStyledDom;
        pub(crate) fn AzStyledDom_fromFile(_:  AzString) -> AzStyledDom;
        pub(crate) fn AzStyledDom_appendChild(_:  &mut AzStyledDom, _:  AzStyledDom);
        pub(crate) fn AzStyledDom_restyle(_:  &mut AzStyledDom, _:  AzCss);
        pub(crate) fn AzStyledDom_nodeCount(_:  &AzStyledDom) -> usize;
        pub(crate) fn AzStyledDom_getHtmlStringTest(_:  &AzStyledDom) -> AzString;
        pub(crate) fn AzStyledDom_getHtmlStringDebug(_:  &AzStyledDom) -> AzString;
        pub(crate) fn AzTexture_allocateClipMask(_:  AzGl, _:  AzLayoutSize) -> AzTexture;
        pub(crate) fn AzTexture_drawClipMask(_:  &mut AzTexture, _:  AzTesselatedSvgNode) -> bool;
        pub(crate) fn AzTexture_applyFxaa(_:  &mut AzTexture) -> bool;
        pub(crate) fn AzTexture_delete(_:  &mut AzTexture);
        pub(crate) fn AzTexture_deepCopy(_:  &AzTexture) -> AzTexture;
        pub(crate) fn AzGlVoidPtrConst_delete(_:  &mut AzGlVoidPtrConst);
        pub(crate) fn AzGlVoidPtrConst_deepCopy(_:  &AzGlVoidPtrConst) -> AzGlVoidPtrConst;
        pub(crate) fn AzGl_getType(_:  &AzGl) -> AzGlType;
        pub(crate) fn AzGl_bufferDataUntyped(_:  &AzGl, _:  u32, _:  isize, _:  AzGlVoidPtrConst, _:  u32);
        pub(crate) fn AzGl_bufferSubDataUntyped(_:  &AzGl, _:  u32, _:  isize, _:  isize, _:  AzGlVoidPtrConst);
        pub(crate) fn AzGl_mapBuffer(_:  &AzGl, _:  u32, _:  u32) -> AzGlVoidPtrMut;
        pub(crate) fn AzGl_mapBufferRange(_:  &AzGl, _:  u32, _:  isize, _:  isize, _:  u32) -> AzGlVoidPtrMut;
        pub(crate) fn AzGl_unmapBuffer(_:  &AzGl, _:  u32) -> u8;
        pub(crate) fn AzGl_texBuffer(_:  &AzGl, _:  u32, _:  u32, _:  u32);
        pub(crate) fn AzGl_shaderSource(_:  &AzGl, _:  u32, _:  AzStringVec);
        pub(crate) fn AzGl_readBuffer(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_readPixelsIntoBuffer(_:  &AzGl, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRefMut);
        pub(crate) fn AzGl_readPixels(_:  &AzGl, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32) -> AzU8Vec;
        pub(crate) fn AzGl_readPixelsIntoPbo(_:  &AzGl, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32);
        pub(crate) fn AzGl_sampleCoverage(_:  &AzGl, _:  f32, _:  bool);
        pub(crate) fn AzGl_polygonOffset(_:  &AzGl, _:  f32, _:  f32);
        pub(crate) fn AzGl_pixelStoreI(_:  &AzGl, _:  u32, _:  i32);
        pub(crate) fn AzGl_genBuffers(_:  &AzGl, _:  i32) -> AzGLuintVec;
        pub(crate) fn AzGl_genRenderbuffers(_:  &AzGl, _:  i32) -> AzGLuintVec;
        pub(crate) fn AzGl_genFramebuffers(_:  &AzGl, _:  i32) -> AzGLuintVec;
        pub(crate) fn AzGl_genTextures(_:  &AzGl, _:  i32) -> AzGLuintVec;
        pub(crate) fn AzGl_genVertexArrays(_:  &AzGl, _:  i32) -> AzGLuintVec;
        pub(crate) fn AzGl_genQueries(_:  &AzGl, _:  i32) -> AzGLuintVec;
        pub(crate) fn AzGl_beginQuery(_:  &AzGl, _:  u32, _:  u32);
        pub(crate) fn AzGl_endQuery(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_queryCounter(_:  &AzGl, _:  u32, _:  u32);
        pub(crate) fn AzGl_getQueryObjectIv(_:  &AzGl, _:  u32, _:  u32) -> i32;
        pub(crate) fn AzGl_getQueryObjectUiv(_:  &AzGl, _:  u32, _:  u32) -> u32;
        pub(crate) fn AzGl_getQueryObjectI64V(_:  &AzGl, _:  u32, _:  u32) -> i64;
        pub(crate) fn AzGl_getQueryObjectUi64V(_:  &AzGl, _:  u32, _:  u32) -> u64;
        pub(crate) fn AzGl_deleteQueries(_:  &AzGl, _:  AzGLuintVecRef);
        pub(crate) fn AzGl_deleteVertexArrays(_:  &AzGl, _:  AzGLuintVecRef);
        pub(crate) fn AzGl_deleteBuffers(_:  &AzGl, _:  AzGLuintVecRef);
        pub(crate) fn AzGl_deleteRenderbuffers(_:  &AzGl, _:  AzGLuintVecRef);
        pub(crate) fn AzGl_deleteFramebuffers(_:  &AzGl, _:  AzGLuintVecRef);
        pub(crate) fn AzGl_deleteTextures(_:  &AzGl, _:  AzGLuintVecRef);
        pub(crate) fn AzGl_framebufferRenderbuffer(_:  &AzGl, _:  u32, _:  u32, _:  u32, _:  u32);
        pub(crate) fn AzGl_renderbufferStorage(_:  &AzGl, _:  u32, _:  u32, _:  i32, _:  i32);
        pub(crate) fn AzGl_depthFunc(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_activeTexture(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_attachShader(_:  &AzGl, _:  u32, _:  u32);
        pub(crate) fn AzGl_bindAttribLocation(_:  &AzGl, _:  u32, _:  u32, _:  AzRefstr);
        pub(crate) fn AzGl_getUniformIv(_:  &AzGl, _:  u32, _:  i32, _:  AzGLintVecRefMut);
        pub(crate) fn AzGl_getUniformFv(_:  &AzGl, _:  u32, _:  i32, _:  AzGLfloatVecRefMut);
        pub(crate) fn AzGl_getUniformBlockIndex(_:  &AzGl, _:  u32, _:  AzRefstr) -> u32;
        pub(crate) fn AzGl_getUniformIndices(_:  &AzGl, _:  u32, _:  AzRefstrVecRef) -> AzGLuintVec;
        pub(crate) fn AzGl_bindBufferBase(_:  &AzGl, _:  u32, _:  u32, _:  u32);
        pub(crate) fn AzGl_bindBufferRange(_:  &AzGl, _:  u32, _:  u32, _:  u32, _:  isize, _:  isize);
        pub(crate) fn AzGl_uniformBlockBinding(_:  &AzGl, _:  u32, _:  u32, _:  u32);
        pub(crate) fn AzGl_bindBuffer(_:  &AzGl, _:  u32, _:  u32);
        pub(crate) fn AzGl_bindVertexArray(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_bindRenderbuffer(_:  &AzGl, _:  u32, _:  u32);
        pub(crate) fn AzGl_bindFramebuffer(_:  &AzGl, _:  u32, _:  u32);
        pub(crate) fn AzGl_bindTexture(_:  &AzGl, _:  u32, _:  u32);
        pub(crate) fn AzGl_drawBuffers(_:  &AzGl, _:  AzGLenumVecRef);
        pub(crate) fn AzGl_texImage2D(_:  &AzGl, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzOptionU8VecRef);
        pub(crate) fn AzGl_compressedTexImage2D(_:  &AzGl, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32, _:  AzU8VecRef);
        pub(crate) fn AzGl_compressedTexSubImage2D(_:  &AzGl, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  AzU8VecRef);
        pub(crate) fn AzGl_texImage3D(_:  &AzGl, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzOptionU8VecRef);
        pub(crate) fn AzGl_copyTexImage2D(_:  &AzGl, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn AzGl_copyTexSubImage2D(_:  &AzGl, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn AzGl_copyTexSubImage3D(_:  &AzGl, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn AzGl_texSubImage2D(_:  &AzGl, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRef);
        pub(crate) fn AzGl_texSubImage2DPbo(_:  &AzGl, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  usize);
        pub(crate) fn AzGl_texSubImage3D(_:  &AzGl, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRef);
        pub(crate) fn AzGl_texSubImage3DPbo(_:  &AzGl, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  usize);
        pub(crate) fn AzGl_texStorage2D(_:  &AzGl, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32);
        pub(crate) fn AzGl_texStorage3D(_:  &AzGl, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn AzGl_getTexImageIntoBuffer(_:  &AzGl, _:  u32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRefMut);
        pub(crate) fn AzGl_copyImageSubData(_:  &AzGl, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn AzGl_invalidateFramebuffer(_:  &AzGl, _:  u32, _:  AzGLenumVecRef);
        pub(crate) fn AzGl_invalidateSubFramebuffer(_:  &AzGl, _:  u32, _:  AzGLenumVecRef, _:  i32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn AzGl_getIntegerV(_:  &AzGl, _:  u32, _:  AzGLintVecRefMut);
        pub(crate) fn AzGl_getInteger64V(_:  &AzGl, _:  u32, _:  AzGLint64VecRefMut);
        pub(crate) fn AzGl_getIntegerIv(_:  &AzGl, _:  u32, _:  u32, _:  AzGLintVecRefMut);
        pub(crate) fn AzGl_getInteger64Iv(_:  &AzGl, _:  u32, _:  u32, _:  AzGLint64VecRefMut);
        pub(crate) fn AzGl_getBooleanV(_:  &AzGl, _:  u32, _:  AzGLbooleanVecRefMut);
        pub(crate) fn AzGl_getFloatV(_:  &AzGl, _:  u32, _:  AzGLfloatVecRefMut);
        pub(crate) fn AzGl_getFramebufferAttachmentParameterIv(_:  &AzGl, _:  u32, _:  u32, _:  u32) -> i32;
        pub(crate) fn AzGl_getRenderbufferParameterIv(_:  &AzGl, _:  u32, _:  u32) -> i32;
        pub(crate) fn AzGl_getTexParameterIv(_:  &AzGl, _:  u32, _:  u32) -> i32;
        pub(crate) fn AzGl_getTexParameterFv(_:  &AzGl, _:  u32, _:  u32) -> f32;
        pub(crate) fn AzGl_texParameterI(_:  &AzGl, _:  u32, _:  u32, _:  i32);
        pub(crate) fn AzGl_texParameterF(_:  &AzGl, _:  u32, _:  u32, _:  f32);
        pub(crate) fn AzGl_framebufferTexture2D(_:  &AzGl, _:  u32, _:  u32, _:  u32, _:  u32, _:  i32);
        pub(crate) fn AzGl_framebufferTextureLayer(_:  &AzGl, _:  u32, _:  u32, _:  u32, _:  i32, _:  i32);
        pub(crate) fn AzGl_blitFramebuffer(_:  &AzGl, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32);
        pub(crate) fn AzGl_vertexAttrib4F(_:  &AzGl, _:  u32, _:  f32, _:  f32, _:  f32, _:  f32);
        pub(crate) fn AzGl_vertexAttribPointerF32(_:  &AzGl, _:  u32, _:  i32, _:  bool, _:  i32, _:  u32);
        pub(crate) fn AzGl_vertexAttribPointer(_:  &AzGl, _:  u32, _:  i32, _:  u32, _:  bool, _:  i32, _:  u32);
        pub(crate) fn AzGl_vertexAttribIPointer(_:  &AzGl, _:  u32, _:  i32, _:  u32, _:  i32, _:  u32);
        pub(crate) fn AzGl_vertexAttribDivisor(_:  &AzGl, _:  u32, _:  u32);
        pub(crate) fn AzGl_viewport(_:  &AzGl, _:  i32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn AzGl_scissor(_:  &AzGl, _:  i32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn AzGl_lineWidth(_:  &AzGl, _:  f32);
        pub(crate) fn AzGl_useProgram(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_validateProgram(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_drawArrays(_:  &AzGl, _:  u32, _:  i32, _:  i32);
        pub(crate) fn AzGl_drawArraysInstanced(_:  &AzGl, _:  u32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn AzGl_drawElements(_:  &AzGl, _:  u32, _:  i32, _:  u32, _:  u32);
        pub(crate) fn AzGl_drawElementsInstanced(_:  &AzGl, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32);
        pub(crate) fn AzGl_blendColor(_:  &AzGl, _:  f32, _:  f32, _:  f32, _:  f32);
        pub(crate) fn AzGl_blendFunc(_:  &AzGl, _:  u32, _:  u32);
        pub(crate) fn AzGl_blendFuncSeparate(_:  &AzGl, _:  u32, _:  u32, _:  u32, _:  u32);
        pub(crate) fn AzGl_blendEquation(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_blendEquationSeparate(_:  &AzGl, _:  u32, _:  u32);
        pub(crate) fn AzGl_colorMask(_:  &AzGl, _:  bool, _:  bool, _:  bool, _:  bool);
        pub(crate) fn AzGl_cullFace(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_frontFace(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_enable(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_disable(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_hint(_:  &AzGl, _:  u32, _:  u32);
        pub(crate) fn AzGl_isEnabled(_:  &AzGl, _:  u32) -> u8;
        pub(crate) fn AzGl_isShader(_:  &AzGl, _:  u32) -> u8;
        pub(crate) fn AzGl_isTexture(_:  &AzGl, _:  u32) -> u8;
        pub(crate) fn AzGl_isFramebuffer(_:  &AzGl, _:  u32) -> u8;
        pub(crate) fn AzGl_isRenderbuffer(_:  &AzGl, _:  u32) -> u8;
        pub(crate) fn AzGl_checkFrameBufferStatus(_:  &AzGl, _:  u32) -> u32;
        pub(crate) fn AzGl_enableVertexAttribArray(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_disableVertexAttribArray(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_uniform1F(_:  &AzGl, _:  i32, _:  f32);
        pub(crate) fn AzGl_uniform1Fv(_:  &AzGl, _:  i32, _:  AzF32VecRef);
        pub(crate) fn AzGl_uniform1I(_:  &AzGl, _:  i32, _:  i32);
        pub(crate) fn AzGl_uniform1Iv(_:  &AzGl, _:  i32, _:  AzI32VecRef);
        pub(crate) fn AzGl_uniform1Ui(_:  &AzGl, _:  i32, _:  u32);
        pub(crate) fn AzGl_uniform2F(_:  &AzGl, _:  i32, _:  f32, _:  f32);
        pub(crate) fn AzGl_uniform2Fv(_:  &AzGl, _:  i32, _:  AzF32VecRef);
        pub(crate) fn AzGl_uniform2I(_:  &AzGl, _:  i32, _:  i32, _:  i32);
        pub(crate) fn AzGl_uniform2Iv(_:  &AzGl, _:  i32, _:  AzI32VecRef);
        pub(crate) fn AzGl_uniform2Ui(_:  &AzGl, _:  i32, _:  u32, _:  u32);
        pub(crate) fn AzGl_uniform3F(_:  &AzGl, _:  i32, _:  f32, _:  f32, _:  f32);
        pub(crate) fn AzGl_uniform3Fv(_:  &AzGl, _:  i32, _:  AzF32VecRef);
        pub(crate) fn AzGl_uniform3I(_:  &AzGl, _:  i32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn AzGl_uniform3Iv(_:  &AzGl, _:  i32, _:  AzI32VecRef);
        pub(crate) fn AzGl_uniform3Ui(_:  &AzGl, _:  i32, _:  u32, _:  u32, _:  u32);
        pub(crate) fn AzGl_uniform4F(_:  &AzGl, _:  i32, _:  f32, _:  f32, _:  f32, _:  f32);
        pub(crate) fn AzGl_uniform4I(_:  &AzGl, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32);
        pub(crate) fn AzGl_uniform4Iv(_:  &AzGl, _:  i32, _:  AzI32VecRef);
        pub(crate) fn AzGl_uniform4Ui(_:  &AzGl, _:  i32, _:  u32, _:  u32, _:  u32, _:  u32);
        pub(crate) fn AzGl_uniform4Fv(_:  &AzGl, _:  i32, _:  AzF32VecRef);
        pub(crate) fn AzGl_uniformMatrix2Fv(_:  &AzGl, _:  i32, _:  bool, _:  AzF32VecRef);
        pub(crate) fn AzGl_uniformMatrix3Fv(_:  &AzGl, _:  i32, _:  bool, _:  AzF32VecRef);
        pub(crate) fn AzGl_uniformMatrix4Fv(_:  &AzGl, _:  i32, _:  bool, _:  AzF32VecRef);
        pub(crate) fn AzGl_depthMask(_:  &AzGl, _:  bool);
        pub(crate) fn AzGl_depthRange(_:  &AzGl, _:  f64, _:  f64);
        pub(crate) fn AzGl_getActiveAttrib(_:  &AzGl, _:  u32, _:  u32) -> AzGetActiveAttribReturn;
        pub(crate) fn AzGl_getActiveUniform(_:  &AzGl, _:  u32, _:  u32) -> AzGetActiveUniformReturn;
        pub(crate) fn AzGl_getActiveUniformsIv(_:  &AzGl, _:  u32, _:  AzGLuintVec, _:  u32) -> AzGLintVec;
        pub(crate) fn AzGl_getActiveUniformBlockI(_:  &AzGl, _:  u32, _:  u32, _:  u32) -> i32;
        pub(crate) fn AzGl_getActiveUniformBlockIv(_:  &AzGl, _:  u32, _:  u32, _:  u32) -> AzGLintVec;
        pub(crate) fn AzGl_getActiveUniformBlockName(_:  &AzGl, _:  u32, _:  u32) -> AzString;
        pub(crate) fn AzGl_getAttribLocation(_:  &AzGl, _:  u32, _:  AzRefstr) -> i32;
        pub(crate) fn AzGl_getFragDataLocation(_:  &AzGl, _:  u32, _:  AzRefstr) -> i32;
        pub(crate) fn AzGl_getUniformLocation(_:  &AzGl, _:  u32, _:  AzRefstr) -> i32;
        pub(crate) fn AzGl_getProgramInfoLog(_:  &AzGl, _:  u32) -> AzString;
        pub(crate) fn AzGl_getProgramIv(_:  &AzGl, _:  u32, _:  u32, _:  AzGLintVecRefMut);
        pub(crate) fn AzGl_getProgramBinary(_:  &AzGl, _:  u32) -> AzGetProgramBinaryReturn;
        pub(crate) fn AzGl_programBinary(_:  &AzGl, _:  u32, _:  u32, _:  AzU8VecRef);
        pub(crate) fn AzGl_programParameterI(_:  &AzGl, _:  u32, _:  u32, _:  i32);
        pub(crate) fn AzGl_getVertexAttribIv(_:  &AzGl, _:  u32, _:  u32, _:  AzGLintVecRefMut);
        pub(crate) fn AzGl_getVertexAttribFv(_:  &AzGl, _:  u32, _:  u32, _:  AzGLfloatVecRefMut);
        pub(crate) fn AzGl_getVertexAttribPointerV(_:  &AzGl, _:  u32, _:  u32) -> isize;
        pub(crate) fn AzGl_getBufferParameterIv(_:  &AzGl, _:  u32, _:  u32) -> i32;
        pub(crate) fn AzGl_getShaderInfoLog(_:  &AzGl, _:  u32) -> AzString;
        pub(crate) fn AzGl_getString(_:  &AzGl, _:  u32) -> AzString;
        pub(crate) fn AzGl_getStringI(_:  &AzGl, _:  u32, _:  u32) -> AzString;
        pub(crate) fn AzGl_getShaderIv(_:  &AzGl, _:  u32, _:  u32, _:  AzGLintVecRefMut);
        pub(crate) fn AzGl_getShaderPrecisionFormat(_:  &AzGl, _:  u32, _:  u32) -> AzGlShaderPrecisionFormatReturn;
        pub(crate) fn AzGl_compileShader(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_createProgram(_:  &AzGl) -> u32;
        pub(crate) fn AzGl_deleteProgram(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_createShader(_:  &AzGl, _:  u32) -> u32;
        pub(crate) fn AzGl_deleteShader(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_detachShader(_:  &AzGl, _:  u32, _:  u32);
        pub(crate) fn AzGl_linkProgram(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_clearColor(_:  &AzGl, _:  f32, _:  f32, _:  f32, _:  f32);
        pub(crate) fn AzGl_clear(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_clearDepth(_:  &AzGl, _:  f64);
        pub(crate) fn AzGl_clearStencil(_:  &AzGl, _:  i32);
        pub(crate) fn AzGl_flush(_:  &AzGl);
        pub(crate) fn AzGl_finish(_:  &AzGl);
        pub(crate) fn AzGl_getError(_:  &AzGl) -> u32;
        pub(crate) fn AzGl_stencilMask(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_stencilMaskSeparate(_:  &AzGl, _:  u32, _:  u32);
        pub(crate) fn AzGl_stencilFunc(_:  &AzGl, _:  u32, _:  i32, _:  u32);
        pub(crate) fn AzGl_stencilFuncSeparate(_:  &AzGl, _:  u32, _:  u32, _:  i32, _:  u32);
        pub(crate) fn AzGl_stencilOp(_:  &AzGl, _:  u32, _:  u32, _:  u32);
        pub(crate) fn AzGl_stencilOpSeparate(_:  &AzGl, _:  u32, _:  u32, _:  u32, _:  u32);
        pub(crate) fn AzGl_eglImageTargetTexture2DOes(_:  &AzGl, _:  u32, _:  AzGlVoidPtrConst);
        pub(crate) fn AzGl_generateMipmap(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_insertEventMarkerExt(_:  &AzGl, _:  AzRefstr);
        pub(crate) fn AzGl_pushGroupMarkerExt(_:  &AzGl, _:  AzRefstr);
        pub(crate) fn AzGl_popGroupMarkerExt(_:  &AzGl);
        pub(crate) fn AzGl_debugMessageInsertKhr(_:  &AzGl, _:  u32, _:  u32, _:  u32, _:  u32, _:  AzRefstr);
        pub(crate) fn AzGl_pushDebugGroupKhr(_:  &AzGl, _:  u32, _:  u32, _:  AzRefstr);
        pub(crate) fn AzGl_popDebugGroupKhr(_:  &AzGl);
        pub(crate) fn AzGl_fenceSync(_:  &AzGl, _:  u32, _:  u32) -> AzGLsyncPtr;
        pub(crate) fn AzGl_clientWaitSync(_:  &AzGl, _:  AzGLsyncPtr, _:  u32, _:  u64) -> u32;
        pub(crate) fn AzGl_waitSync(_:  &AzGl, _:  AzGLsyncPtr, _:  u32, _:  u64);
        pub(crate) fn AzGl_deleteSync(_:  &AzGl, _:  AzGLsyncPtr);
        pub(crate) fn AzGl_textureRangeApple(_:  &AzGl, _:  u32, _:  AzU8VecRef);
        pub(crate) fn AzGl_genFencesApple(_:  &AzGl, _:  i32) -> AzGLuintVec;
        pub(crate) fn AzGl_deleteFencesApple(_:  &AzGl, _:  AzGLuintVecRef);
        pub(crate) fn AzGl_setFenceApple(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_finishFenceApple(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_testFenceApple(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_testObjectApple(_:  &AzGl, _:  u32, _:  u32) -> u8;
        pub(crate) fn AzGl_finishObjectApple(_:  &AzGl, _:  u32, _:  u32);
        pub(crate) fn AzGl_getFragDataIndex(_:  &AzGl, _:  u32, _:  AzRefstr) -> i32;
        pub(crate) fn AzGl_blendBarrierKhr(_:  &AzGl);
        pub(crate) fn AzGl_bindFragDataLocationIndexed(_:  &AzGl, _:  u32, _:  u32, _:  u32, _:  AzRefstr);
        pub(crate) fn AzGl_getDebugMessages(_:  &AzGl) -> AzDebugMessageVec;
        pub(crate) fn AzGl_provokingVertexAngle(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_genVertexArraysApple(_:  &AzGl, _:  i32) -> AzGLuintVec;
        pub(crate) fn AzGl_bindVertexArrayApple(_:  &AzGl, _:  u32);
        pub(crate) fn AzGl_deleteVertexArraysApple(_:  &AzGl, _:  AzGLuintVecRef);
        pub(crate) fn AzGl_copyTextureChromium(_:  &AzGl, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  u32, _:  u8, _:  u8, _:  u8);
        pub(crate) fn AzGl_copySubTextureChromium(_:  &AzGl, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u8, _:  u8, _:  u8);
        pub(crate) fn AzGl_eglImageTargetRenderbufferStorageOes(_:  &AzGl, _:  u32, _:  AzGlVoidPtrConst);
        pub(crate) fn AzGl_copyTexture3DAngle(_:  &AzGl, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  u32, _:  u8, _:  u8, _:  u8);
        pub(crate) fn AzGl_copySubTexture3DAngle(_:  &AzGl, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u8, _:  u8, _:  u8);
        pub(crate) fn AzGl_bufferStorage(_:  &AzGl, _:  u32, _:  isize, _:  AzGlVoidPtrConst, _:  u32);
        pub(crate) fn AzGl_flushMappedBufferRange(_:  &AzGl, _:  u32, _:  isize, _:  isize);
        pub(crate) fn AzGl_delete(_:  &mut AzGl);
        pub(crate) fn AzGl_deepCopy(_:  &AzGl) -> AzGl;
        pub(crate) fn AzGLsyncPtr_delete(_:  &mut AzGLsyncPtr);
        pub(crate) fn AzGLsyncPtr_deepCopy(_:  &AzGLsyncPtr) -> AzGLsyncPtr;
        pub(crate) fn AzTextureFlags_default() -> AzTextureFlags;
        pub(crate) fn AzImageRef_invalid(_:  usize, _:  usize, _:  AzRawImageFormat) -> AzImageRef;
        pub(crate) fn AzImageRef_rawImage(_:  AzRawImage) -> AzOptionImageRef;
        pub(crate) fn AzImageRef_glTexture(_:  AzTexture) -> AzImageRef;
        pub(crate) fn AzImageRef_callback(_:  AzRenderImageCallback, _:  AzRefAny) -> AzImageRef;
        pub(crate) fn AzImageRef_cloneBytes(_:  &AzImageRef) -> AzImageRef;
        pub(crate) fn AzImageRef_isInvalid(_:  &AzImageRef) -> bool;
        pub(crate) fn AzImageRef_isGlTexture(_:  &AzImageRef) -> bool;
        pub(crate) fn AzImageRef_isRawImage(_:  &AzImageRef) -> bool;
        pub(crate) fn AzImageRef_isCallback(_:  &AzImageRef) -> bool;
        pub(crate) fn AzImageRef_delete(_:  &mut AzImageRef);
        pub(crate) fn AzImageRef_deepCopy(_:  &AzImageRef) -> AzImageRef;
        pub(crate) fn AzRawImage_empty() -> AzRawImage;
        pub(crate) fn AzRawImage_allocateClipMask(_:  AzLayoutSize) -> AzRawImage;
        pub(crate) fn AzRawImage_decodeImageBytesAny(_:  AzU8VecRef) -> AzResultRawImageDecodeImageError;
        pub(crate) fn AzRawImage_drawClipMask(_:  &mut AzRawImage, _:  AzSvgNode, _:  AzSvgStyle) -> bool;
        pub(crate) fn AzRawImage_encodeBmp(_:  &AzRawImage) -> AzResultU8VecEncodeImageError;
        pub(crate) fn AzRawImage_encodePng(_:  &AzRawImage) -> AzResultU8VecEncodeImageError;
        pub(crate) fn AzRawImage_encodeJpeg(_:  &AzRawImage) -> AzResultU8VecEncodeImageError;
        pub(crate) fn AzRawImage_encodeTga(_:  &AzRawImage) -> AzResultU8VecEncodeImageError;
        pub(crate) fn AzRawImage_encodePnm(_:  &AzRawImage) -> AzResultU8VecEncodeImageError;
        pub(crate) fn AzRawImage_encodeGif(_:  &AzRawImage) -> AzResultU8VecEncodeImageError;
        pub(crate) fn AzRawImage_encodeTiff(_:  &AzRawImage) -> AzResultU8VecEncodeImageError;
        pub(crate) fn AzFontRef_parse(_:  AzFontSource) -> AzOptionFontRef;
        pub(crate) fn AzFontRef_getFontMetrics(_:  &AzFontRef) -> AzFontMetrics;
        pub(crate) fn AzFontRef_delete(_:  &mut AzFontRef);
        pub(crate) fn AzFontRef_deepCopy(_:  &AzFontRef) -> AzFontRef;
        pub(crate) fn AzSvg_fromString(_:  AzString, _:  AzSvgParseOptions) -> AzResultSvgSvgParseError;
        pub(crate) fn AzSvg_fromBytes(_:  AzU8VecRef, _:  AzSvgParseOptions) -> AzResultSvgSvgParseError;
        pub(crate) fn AzSvg_getRoot(_:  &AzSvg) -> AzSvgXmlNode;
        pub(crate) fn AzSvg_render(_:  &AzSvg, _:  AzSvgRenderOptions) -> AzOptionRawImage;
        pub(crate) fn AzSvg_toString(_:  &AzSvg, _:  AzSvgStringFormatOptions) -> AzString;
        pub(crate) fn AzSvg_delete(_:  &mut AzSvg);
        pub(crate) fn AzSvg_deepCopy(_:  &AzSvg) -> AzSvg;
        pub(crate) fn AzSvgXmlNode_parseFrom(_:  AzU8VecRef, _:  AzSvgParseOptions) -> AzResultSvgXmlNodeSvgParseError;
        pub(crate) fn AzSvgXmlNode_render(_:  &AzSvgXmlNode, _:  AzSvgRenderOptions) -> AzOptionRawImage;
        pub(crate) fn AzSvgXmlNode_toString(_:  &AzSvgXmlNode, _:  AzSvgStringFormatOptions) -> AzString;
        pub(crate) fn AzSvgXmlNode_delete(_:  &mut AzSvgXmlNode);
        pub(crate) fn AzSvgXmlNode_deepCopy(_:  &AzSvgXmlNode) -> AzSvgXmlNode;
        pub(crate) fn AzSvgMultiPolygon_tesselateFill(_:  &AzSvgMultiPolygon, _:  AzSvgFillStyle) -> AzTesselatedSvgNode;
        pub(crate) fn AzSvgMultiPolygon_tesselateStroke(_:  &AzSvgMultiPolygon, _:  AzSvgStrokeStyle) -> AzTesselatedSvgNode;
        pub(crate) fn AzSvgNode_tesselateFill(_:  &AzSvgNode, _:  AzSvgFillStyle) -> AzTesselatedSvgNode;
        pub(crate) fn AzSvgNode_tesselateStroke(_:  &AzSvgNode, _:  AzSvgStrokeStyle) -> AzTesselatedSvgNode;
        pub(crate) fn AzSvgStyledNode_tesselate(_:  &AzSvgStyledNode) -> AzTesselatedSvgNode;
        pub(crate) fn AzSvgCircle_tesselateFill(_:  &AzSvgCircle, _:  AzSvgFillStyle) -> AzTesselatedSvgNode;
        pub(crate) fn AzSvgCircle_tesselateStroke(_:  &AzSvgCircle, _:  AzSvgStrokeStyle) -> AzTesselatedSvgNode;
        pub(crate) fn AzSvgPath_tesselateFill(_:  &AzSvgPath, _:  AzSvgFillStyle) -> AzTesselatedSvgNode;
        pub(crate) fn AzSvgPath_tesselateStroke(_:  &AzSvgPath, _:  AzSvgStrokeStyle) -> AzTesselatedSvgNode;
        pub(crate) fn AzSvgRect_tesselateFill(_:  &AzSvgRect, _:  AzSvgFillStyle) -> AzTesselatedSvgNode;
        pub(crate) fn AzSvgRect_tesselateStroke(_:  &AzSvgRect, _:  AzSvgStrokeStyle) -> AzTesselatedSvgNode;
        pub(crate) fn AzTesselatedSvgNode_empty() -> AzTesselatedSvgNode;
        pub(crate) fn AzTesselatedSvgNode_fromNodes(_:  AzTesselatedSvgNodeVecRef) -> AzTesselatedSvgNode;
        pub(crate) fn AzSvgParseOptions_default() -> AzSvgParseOptions;
        pub(crate) fn AzSvgRenderOptions_default() -> AzSvgRenderOptions;
        pub(crate) fn AzXml_fromStr(_:  AzRefstr) -> AzResultXmlXmlError;
        pub(crate) fn AzFile_open(_:  AzString) -> AzOptionFile;
        pub(crate) fn AzFile_create(_:  AzString) -> AzOptionFile;
        pub(crate) fn AzFile_readToString(_:  &mut AzFile) -> AzOptionString;
        pub(crate) fn AzFile_readToBytes(_:  &mut AzFile) -> AzOptionU8Vec;
        pub(crate) fn AzFile_writeString(_:  &mut AzFile, _:  AzRefstr) -> bool;
        pub(crate) fn AzFile_writeBytes(_:  &mut AzFile, _:  AzU8VecRef) -> bool;
        pub(crate) fn AzFile_close(_:  &mut AzFile);
        pub(crate) fn AzFile_delete(_:  &mut AzFile);
        pub(crate) fn AzFile_deepCopy(_:  &AzFile) -> AzFile;
        pub(crate) fn AzMsgBox_ok(_:  AzMsgBoxIcon, _:  AzString, _:  AzString) -> bool;
        pub(crate) fn AzMsgBox_okCancel(_:  AzMsgBoxIcon, _:  AzString, _:  AzString, _:  AzMsgBoxOkCancel) -> AzMsgBoxOkCancel;
        pub(crate) fn AzMsgBox_yesNo(_:  AzMsgBoxIcon, _:  AzString, _:  AzString, _:  AzMsgBoxYesNo) -> AzMsgBoxYesNo;
        pub(crate) fn AzFileDialog_selectFile(_:  AzString, _:  AzOptionString, _:  AzOptionFileTypeList) -> AzOptionString;
        pub(crate) fn AzFileDialog_selectMultipleFiles(_:  AzString, _:  AzOptionString, _:  AzOptionFileTypeList) -> AzOptionStringVec;
        pub(crate) fn AzFileDialog_selectFolder(_:  AzString, _:  AzOptionString) -> AzOptionString;
        pub(crate) fn AzFileDialog_saveFile(_:  AzString, _:  AzOptionString) -> AzOptionString;
        pub(crate) fn AzColorPickerDialog_open(_:  AzString, _:  AzOptionColorU) -> AzOptionColorU;
        pub(crate) fn AzSystemClipboard_new() -> AzOptionSystemClipboard;
        pub(crate) fn AzSystemClipboard_getStringContents(_:  &AzSystemClipboard) -> AzOptionString;
        pub(crate) fn AzSystemClipboard_setStringContents(_:  &mut AzSystemClipboard, _:  AzString) -> bool;
        pub(crate) fn AzSystemClipboard_delete(_:  &mut AzSystemClipboard);
        pub(crate) fn AzSystemClipboard_deepCopy(_:  &AzSystemClipboard) -> AzSystemClipboard;
        pub(crate) fn AzInstant_durationSince(_:  &AzInstant, _:  AzInstant) -> AzOptionDuration;
        pub(crate) fn AzInstant_addDuration(_:  &mut AzInstant, _:  AzDuration) -> AzInstant;
        pub(crate) fn AzInstant_linearInterpolate(_:  &AzInstant, _:  AzInstant, _:  AzInstant) -> f32;
        pub(crate) fn AzInstantPtr_delete(_:  &mut AzInstantPtr);
        pub(crate) fn AzInstantPtr_deepCopy(_:  &AzInstantPtr) -> AzInstantPtr;
        pub(crate) fn AzTimer_new(_:  AzRefAny, _:  AzTimerCallbackType, _:  AzGetSystemTimeFn) -> AzTimer;
        pub(crate) fn AzTimer_withDelay(_:  &AzTimer, _:  AzDuration) -> AzTimer;
        pub(crate) fn AzTimer_withInterval(_:  &AzTimer, _:  AzDuration) -> AzTimer;
        pub(crate) fn AzTimer_withTimeout(_:  &AzTimer, _:  AzDuration) -> AzTimer;
        pub(crate) fn AzThread_delete(_:  &mut AzThread);
        pub(crate) fn AzThread_deepCopy(_:  &AzThread) -> AzThread;
        pub(crate) fn AzThreadSender_send(_:  &mut AzThreadSender, _:  AzThreadReceiveMsg) -> bool;
        pub(crate) fn AzThreadSender_delete(_:  &mut AzThreadSender);
        pub(crate) fn AzThreadSender_deepCopy(_:  &AzThreadSender) -> AzThreadSender;
        pub(crate) fn AzThreadReceiver_receive(_:  &mut AzThreadReceiver) -> AzOptionThreadSendMsg;
        pub(crate) fn AzThreadReceiver_delete(_:  &mut AzThreadReceiver);
        pub(crate) fn AzThreadReceiver_deepCopy(_:  &AzThreadReceiver) -> AzThreadReceiver;
        pub(crate) fn AzString_format(_:  AzString, _:  AzFmtArgVec) -> AzString;
        pub(crate) fn AzString_copyFromBytes(_:  *const u8, _:  usize, _:  usize) -> AzString;
        pub(crate) fn AzString_trim(_:  &AzString) -> AzString;
        pub(crate) fn AzString_asRefstr(_:  &AzString) -> AzRefstr;
        pub(crate) fn AzAccessibilityStateVec_delete(_:  &mut AzAccessibilityStateVec);
        pub(crate) fn AzMenuItemVec_delete(_:  &mut AzMenuItemVec);
        pub(crate) fn AzTesselatedSvgNodeVec_asRefVec(_:  &AzTesselatedSvgNodeVec) -> AzTesselatedSvgNodeVecRef;
        pub(crate) fn AzTesselatedSvgNodeVec_delete(_:  &mut AzTesselatedSvgNodeVec);
        pub(crate) fn AzStyleFontFamilyVec_delete(_:  &mut AzStyleFontFamilyVec);
        pub(crate) fn AzXmlNodeVec_delete(_:  &mut AzXmlNodeVec);
        pub(crate) fn AzFmtArgVec_delete(_:  &mut AzFmtArgVec);
        pub(crate) fn AzInlineLineVec_delete(_:  &mut AzInlineLineVec);
        pub(crate) fn AzInlineWordVec_delete(_:  &mut AzInlineWordVec);
        pub(crate) fn AzInlineGlyphVec_delete(_:  &mut AzInlineGlyphVec);
        pub(crate) fn AzInlineTextHitVec_delete(_:  &mut AzInlineTextHitVec);
        pub(crate) fn AzMonitorVec_delete(_:  &mut AzMonitorVec);
        pub(crate) fn AzVideoModeVec_delete(_:  &mut AzVideoModeVec);
        pub(crate) fn AzDomVec_delete(_:  &mut AzDomVec);
        pub(crate) fn AzIdOrClassVec_delete(_:  &mut AzIdOrClassVec);
        pub(crate) fn AzNodeDataInlineCssPropertyVec_delete(_:  &mut AzNodeDataInlineCssPropertyVec);
        pub(crate) fn AzStyleBackgroundContentVec_delete(_:  &mut AzStyleBackgroundContentVec);
        pub(crate) fn AzStyleBackgroundPositionVec_delete(_:  &mut AzStyleBackgroundPositionVec);
        pub(crate) fn AzStyleBackgroundRepeatVec_delete(_:  &mut AzStyleBackgroundRepeatVec);
        pub(crate) fn AzStyleBackgroundSizeVec_delete(_:  &mut AzStyleBackgroundSizeVec);
        pub(crate) fn AzStyleTransformVec_delete(_:  &mut AzStyleTransformVec);
        pub(crate) fn AzCssPropertyVec_delete(_:  &mut AzCssPropertyVec);
        pub(crate) fn AzSvgMultiPolygonVec_delete(_:  &mut AzSvgMultiPolygonVec);
        pub(crate) fn AzSvgPathVec_delete(_:  &mut AzSvgPathVec);
        pub(crate) fn AzVertexAttributeVec_delete(_:  &mut AzVertexAttributeVec);
        pub(crate) fn AzSvgPathElementVec_delete(_:  &mut AzSvgPathElementVec);
        pub(crate) fn AzSvgVertexVec_delete(_:  &mut AzSvgVertexVec);
        pub(crate) fn AzU32Vec_delete(_:  &mut AzU32Vec);
        pub(crate) fn AzXWindowTypeVec_delete(_:  &mut AzXWindowTypeVec);
        pub(crate) fn AzVirtualKeyCodeVec_delete(_:  &mut AzVirtualKeyCodeVec);
        pub(crate) fn AzCascadeInfoVec_delete(_:  &mut AzCascadeInfoVec);
        pub(crate) fn AzScanCodeVec_delete(_:  &mut AzScanCodeVec);
        pub(crate) fn AzCssDeclarationVec_delete(_:  &mut AzCssDeclarationVec);
        pub(crate) fn AzCssPathSelectorVec_delete(_:  &mut AzCssPathSelectorVec);
        pub(crate) fn AzStylesheetVec_delete(_:  &mut AzStylesheetVec);
        pub(crate) fn AzCssRuleBlockVec_delete(_:  &mut AzCssRuleBlockVec);
        pub(crate) fn AzU16Vec_delete(_:  &mut AzU16Vec);
        pub(crate) fn AzF32Vec_delete(_:  &mut AzF32Vec);
        pub(crate) fn AzU8Vec_copyFromBytes(_:  *const u8, _:  usize, _:  usize) -> AzU8Vec;
        pub(crate) fn AzU8Vec_asRefVec(_:  &AzU8Vec) -> AzU8VecRef;
        pub(crate) fn AzU8Vec_delete(_:  &mut AzU8Vec);
        pub(crate) fn AzCallbackDataVec_delete(_:  &mut AzCallbackDataVec);
        pub(crate) fn AzDebugMessageVec_delete(_:  &mut AzDebugMessageVec);
        pub(crate) fn AzGLuintVec_delete(_:  &mut AzGLuintVec);
        pub(crate) fn AzGLintVec_delete(_:  &mut AzGLintVec);
        pub(crate) fn AzStringVec_delete(_:  &mut AzStringVec);
        pub(crate) fn AzStringPairVec_delete(_:  &mut AzStringPairVec);
        pub(crate) fn AzNormalizedLinearColorStopVec_delete(_:  &mut AzNormalizedLinearColorStopVec);
        pub(crate) fn AzNormalizedRadialColorStopVec_delete(_:  &mut AzNormalizedRadialColorStopVec);
        pub(crate) fn AzNodeIdVec_delete(_:  &mut AzNodeIdVec);
        pub(crate) fn AzNodeVec_delete(_:  &mut AzNodeVec);
        pub(crate) fn AzStyledNodeVec_delete(_:  &mut AzStyledNodeVec);
        pub(crate) fn AzTagIdToNodeIdMappingVec_delete(_:  &mut AzTagIdToNodeIdMappingVec);
        pub(crate) fn AzParentWithNodeDepthVec_delete(_:  &mut AzParentWithNodeDepthVec);
        pub(crate) fn AzNodeDataVec_delete(_:  &mut AzNodeDataVec);
    }

    }

    #[cfg(not(feature = "link_static"))]
    pub use self::dynamic_link::*;


    #[cfg(feature = "link_static")]
    mod static_link {
       #[cfg(feature = "link_static")]
        extern crate azul; // the azul_dll package, confusingly it has to also be named "azul"
       #[cfg(feature = "link_static")]
        use azul::*;
    }

    #[cfg(feature = "link_static")]
    pub use self::static_link::*;
}

pub mod app {
    #![allow(dead_code, unused_imports)]
    //! `App` construction and configuration
    use crate::dll::*;
    use core::ffi::c_void;
    use crate::callbacks::RefAny;
    use crate::window::WindowCreateOptions;
    use crate::str::String;
    use crate::image::ImageRef;
    /// Main application class
    
#[doc(inline)] pub use crate::dll::AzApp as App;
    impl App {
        /// Creates a new App instance from the given `AppConfig`
        pub fn new(data: RefAny, config: AppConfig) -> Self { unsafe { crate::dll::AzApp_new(data, config) } }
        /// Spawn a new window on the screen when the app is run.
        pub fn add_window(&mut self, window: WindowCreateOptions)  { unsafe { crate::dll::AzApp_addWindow(self, window) } }
        /// Adds a new image identified by an ID to the image cache
        pub fn add_image(&mut self, id: String, image: ImageRef)  { unsafe { crate::dll::AzApp_addImage(self, id, image) } }
        /// Returns a list of monitors - useful for setting the monitor that a window should spawn on.
        pub fn get_monitors(&self)  -> crate::vec::MonitorVec { unsafe { crate::dll::AzApp_getMonitors(self) } }
        /// Runs the application. Due to platform restrictions (specifically `WinMain` on Windows), this function never returns.
        pub fn run(&self, window: WindowCreateOptions)  { unsafe { crate::dll::AzApp_run(self, window) } }
    }

    impl Clone for App { fn clone(&self) -> Self { unsafe { crate::dll::AzApp_deepCopy(self) } } }
    impl Drop for App { fn drop(&mut self) { unsafe { crate::dll::AzApp_delete(self) } } }
    /// Configuration for optional features, such as whether to enable logging or panic hooks
    
#[doc(inline)] pub use crate::dll::AzAppConfig as AppConfig;
    impl AppConfig {
        /// Constructs a default `AppConfig`, uses the layout solver currently available
        pub fn new(layout_solver: LayoutSolver) -> Self { unsafe { crate::dll::AzAppConfig_new(layout_solver) } }
    }

    /// Configuration to set which messages should be logged.
    
#[doc(inline)] pub use crate::dll::AzAppLogLevel as AppLogLevel;
    /// Version of the layout solver to use - future binary versions of azul may have more fields here, necessary so that old compiled applications don't break with newer releases of azul. Newer layout versions are opt-in only.
    
#[doc(inline)] pub use crate::dll::AzLayoutSolver as LayoutSolver;
    /// External system callbacks to get the system time or create / manage threads
    
#[doc(inline)] pub use crate::dll::AzSystemCallbacks as SystemCallbacks;
    impl SystemCallbacks {
        /// Use the default, library-internal callbacks instead of providing your own
        pub fn library_internal() -> Self { unsafe { crate::dll::AzSystemCallbacks_libraryInternal() } }
    }

}

pub mod window {
    #![allow(dead_code, unused_imports)]
    //! Window creation / startup configuration
    use crate::dll::*;
    use core::ffi::c_void;

    impl LayoutSize {
        #[inline(always)]
        pub const fn new(width: isize, height: isize) -> Self { Self { width, height } }
        #[inline(always)]
        pub const fn zero() -> Self { Self::new(0, 0) }
    }

    impl LayoutPoint {
        #[inline(always)]
        pub const fn new(x: isize, y: isize) -> Self { Self { x, y } }
        #[inline(always)]
        pub const fn zero() -> Self { Self::new(0, 0) }
    }

    impl LayoutRect {
        #[inline(always)]
        pub const fn new(origin: LayoutPoint, size: LayoutSize) -> Self { Self { origin, size } }
        #[inline(always)]
        pub const fn zero() -> Self { Self::new(LayoutPoint::zero(), LayoutSize::zero()) }
        #[inline(always)]
        pub const fn max_x(&self) -> isize { self.origin.x + self.size.width }
        #[inline(always)]
        pub const fn min_x(&self) -> isize { self.origin.x }
        #[inline(always)]
        pub const fn max_y(&self) -> isize { self.origin.y + self.size.height }
        #[inline(always)]
        pub const fn min_y(&self) -> isize { self.origin.y }

        pub const fn contains(&self, other: &LayoutPoint) -> bool {
            self.min_x() <= other.x && other.x < self.max_x() &&
            self.min_y() <= other.y && other.y < self.max_y()
        }

        pub fn contains_f32(&self, other_x: f32, other_y: f32) -> bool {
            self.min_x() as f32 <= other_x && other_x < self.max_x() as f32 &&
            self.min_y() as f32 <= other_y && other_y < self.max_y() as f32
        }

        /// Same as `contains()`, but returns the (x, y) offset of the hit point
        ///
        /// On a regular computer this function takes ~3.2ns to run
        #[inline]
        pub const fn hit_test(&self, other: &LayoutPoint) -> Option<LayoutPoint> {
            let dx_left_edge = other.x - self.min_x();
            let dx_right_edge = self.max_x() - other.x;
            let dy_top_edge = other.y - self.min_y();
            let dy_bottom_edge = self.max_y() - other.y;
            if dx_left_edge > 0 &&
               dx_right_edge > 0 &&
               dy_top_edge > 0 &&
               dy_bottom_edge > 0
            {
                Some(LayoutPoint::new(dx_left_edge, dy_top_edge))
            } else {
                None
            }
        }

        // Returns if b overlaps a
        #[inline(always)]
        pub const fn contains_rect(&self, b: &LayoutRect) -> bool {

            let a = self;

            let a_x         = a.origin.x;
            let a_y         = a.origin.y;
            let a_width     = a.size.width;
            let a_height    = a.size.height;

            let b_x         = b.origin.x;
            let b_y         = b.origin.y;
            let b_width     = b.size.width;
            let b_height    = b.size.height;

            b_x >= a_x &&
            b_y >= a_y &&
            b_x + b_width <= a_x + a_width &&
            b_y + b_height <= a_y + a_height
        }
    }    use crate::callbacks::LayoutCallbackType;
    /// Options on how to initially create the window
    
#[doc(inline)] pub use crate::dll::AzWindowCreateOptions as WindowCreateOptions;
    impl WindowCreateOptions {
        /// Creates a new window configuration with a custom layout callback
        pub fn new(layout_callback: LayoutCallbackType) -> Self { unsafe { crate::dll::AzWindowCreateOptions_new(layout_callback) } }
    }

    /// Force a specific renderer: note that azul will **crash** on startup if the `RendererOptions` are not satisfied.
    
#[doc(inline)] pub use crate::dll::AzRendererOptions as RendererOptions;
    /// Whether the renderer has VSync enabled
    
#[doc(inline)] pub use crate::dll::AzVsync as Vsync;
    /// Does the renderer render in SRGB color space? By default, azul tries to set it to `Enabled` and falls back to `Disabled` if the OpenGL context can't be initialized properly
    
#[doc(inline)] pub use crate::dll::AzSrgb as Srgb;
    /// Does the renderer render using hardware acceleration? By default, azul tries to set it to `Enabled` and falls back to `Disabled` if the OpenGL context can't be initialized properly
    
#[doc(inline)] pub use crate::dll::AzHwAcceleration as HwAcceleration;
    /// Offset in physical pixels (integer units)
    
#[doc(inline)] pub use crate::dll::AzLayoutPoint as LayoutPoint;
    /// Size in physical pixels (integer units)
    
#[doc(inline)] pub use crate::dll::AzLayoutSize as LayoutSize;
    /// Represents a rectangle in physical pixels (integer units)
    
#[doc(inline)] pub use crate::dll::AzLayoutRect as LayoutRect;
    /// Raw platform handle, for integration in / with other toolkits and custom non-azul window extensions
    
#[doc(inline)] pub use crate::dll::AzRawWindowHandle as RawWindowHandle;
    /// `IOSHandle` struct
    
#[doc(inline)] pub use crate::dll::AzIOSHandle as IOSHandle;
    /// `MacOSHandle` struct
    
#[doc(inline)] pub use crate::dll::AzMacOSHandle as MacOSHandle;
    /// `XlibHandle` struct
    
#[doc(inline)] pub use crate::dll::AzXlibHandle as XlibHandle;
    /// `XcbHandle` struct
    
#[doc(inline)] pub use crate::dll::AzXcbHandle as XcbHandle;
    /// `WaylandHandle` struct
    
#[doc(inline)] pub use crate::dll::AzWaylandHandle as WaylandHandle;
    /// `WindowsHandle` struct
    
#[doc(inline)] pub use crate::dll::AzWindowsHandle as WindowsHandle;
    /// `WebHandle` struct
    
#[doc(inline)] pub use crate::dll::AzWebHandle as WebHandle;
    /// `AndroidHandle` struct
    
#[doc(inline)] pub use crate::dll::AzAndroidHandle as AndroidHandle;
    /// X11 window hint: Type of window
    
#[doc(inline)] pub use crate::dll::AzXWindowType as XWindowType;
    /// Same as `LayoutPoint`, but uses `i32` instead of `isize`
    
#[doc(inline)] pub use crate::dll::AzPhysicalPositionI32 as PhysicalPositionI32;
    /// Same as `LayoutPoint`, but uses `u32` instead of `isize`
    
#[doc(inline)] pub use crate::dll::AzPhysicalSizeU32 as PhysicalSizeU32;
    /// Logical rectangle area (can differ based on HiDPI settings). Usually this is what you'd want for hit-testing and positioning elements.
    
#[doc(inline)] pub use crate::dll::AzLogicalRect as LogicalRect;
    /// Logical position (can differ based on HiDPI settings). Usually this is what you'd want for hit-testing and positioning elements.
    
#[doc(inline)] pub use crate::dll::AzLogicalPosition as LogicalPosition;
    /// A size in "logical" (non-HiDPI-adjusted) pixels in floating-point units
    
#[doc(inline)] pub use crate::dll::AzLogicalSize as LogicalSize;
    /// Unique hash of a window icon, so that azul does not have to compare the actual bytes to see wether the window icon has changed.
    
#[doc(inline)] pub use crate::dll::AzIconKey as IconKey;
    /// Small (16x16x4) window icon, usually shown in the window titlebar
    
#[doc(inline)] pub use crate::dll::AzSmallWindowIconBytes as SmallWindowIconBytes;
    /// Large (32x32x4) window icon, usually used on high-resolution displays (instead of `SmallWindowIcon`)
    
#[doc(inline)] pub use crate::dll::AzLargeWindowIconBytes as LargeWindowIconBytes;
    /// Window "favicon", usually shown in the top left of the window on Windows
    
#[doc(inline)] pub use crate::dll::AzWindowIcon as WindowIcon;
    /// Application taskbar icon, 256x256x4 bytes in size
    
#[doc(inline)] pub use crate::dll::AzTaskBarIcon as TaskBarIcon;
    /// Symbolic name for a keyboard key, does **not** take the keyboard locale into account
    
#[doc(inline)] pub use crate::dll::AzVirtualKeyCode as VirtualKeyCode;
    /// Symbolic accelerator key (ctrl, alt, shift)
    
#[doc(inline)] pub use crate::dll::AzAcceleratorKey as AcceleratorKey;
    /// Minimum / maximum / current size of the window in logical dimensions
    
#[doc(inline)] pub use crate::dll::AzWindowSize as WindowSize;
    /// Boolean flags relating to the current window state
    
#[doc(inline)] pub use crate::dll::AzWindowFlags as WindowFlags;
    /// Debugging information, will be rendered as an overlay on top of the UI
    
#[doc(inline)] pub use crate::dll::AzDebugState as DebugState;
    /// Current keyboard state, stores what keys / characters have been pressed
    
#[doc(inline)] pub use crate::dll::AzKeyboardState as KeyboardState;
    /// Current icon of the mouse cursor
    
#[doc(inline)] pub use crate::dll::AzMouseCursorType as MouseCursorType;
    /// Current position of the mouse cursor, relative to the window. Set to `Uninitialized` on startup (gets initialized on the first frame).
    
#[doc(inline)] pub use crate::dll::AzCursorPosition as CursorPosition;
    /// Current mouse / cursor state
    
#[doc(inline)] pub use crate::dll::AzMouseState as MouseState;
    /// Platform-specific window configuration, i.e. WM options that are not cross-platform
    
#[doc(inline)] pub use crate::dll::AzPlatformSpecificOptions as PlatformSpecificOptions;
    /// Window configuration specific to Win32
    
#[doc(inline)] pub use crate::dll::AzWindowsWindowOptions as WindowsWindowOptions;
    /// CSD theme of the window title / button controls
    
#[doc(inline)] pub use crate::dll::AzWaylandTheme as WaylandTheme;
    /// Renderer type of the current windows OpenGL context
    
#[doc(inline)] pub use crate::dll::AzRendererType as RendererType;
    /// Key-value pair, used for setting WM hints values specific to GNOME
    
#[doc(inline)] pub use crate::dll::AzStringPair as StringPair;
    /// `LinuxWindowOptions` struct
    
#[doc(inline)] pub use crate::dll::AzLinuxWindowOptions as LinuxWindowOptions;
    /// `MacWindowOptions` struct
    
#[doc(inline)] pub use crate::dll::AzMacWindowOptions as MacWindowOptions;
    /// `WasmWindowOptions` struct
    
#[doc(inline)] pub use crate::dll::AzWasmWindowOptions as WasmWindowOptions;
    /// `FullScreenMode` struct
    
#[doc(inline)] pub use crate::dll::AzFullScreenMode as FullScreenMode;
    /// Window theme, set by the operating system or `WindowCreateOptions.theme` on startup
    
#[doc(inline)] pub use crate::dll::AzWindowTheme as WindowTheme;
    /// Position of the top left corner of the window relative to the top left of the monitor
    
#[doc(inline)] pub use crate::dll::AzWindowPosition as WindowPosition;
    /// Position of the virtual keyboard necessary to insert CJK characters
    
#[doc(inline)] pub use crate::dll::AzImePosition as ImePosition;
    /// Current state of touch devices / touch inputs
    
#[doc(inline)] pub use crate::dll::AzTouchState as TouchState;
    /// Information about a single (or many) monitors, useful for dock widgets
    
#[doc(inline)] pub use crate::dll::AzMonitor as Monitor;
    /// Describes a rendering configuration for a monitor
    
#[doc(inline)] pub use crate::dll::AzVideoMode as VideoMode;
    /// `WindowState` struct
    
#[doc(inline)] pub use crate::dll::AzWindowState as WindowState;
    impl WindowState {
        /// Creates a new WindowState with default settings and a custom layout callback
        pub fn new(layout_callback: LayoutCallbackType) -> Self { unsafe { crate::dll::AzWindowState_new(layout_callback) } }
        /// Creates a default WindowState with an empty layout callback - useful only if you use the Rust `WindowState { .. WindowState::default() }` intialization syntax.
        pub fn default() -> Self { unsafe { crate::dll::AzWindowState_default() } }
    }

}

pub mod callbacks {
    #![allow(dead_code, unused_imports)]
    //! Callback type definitions + struct definitions of `CallbackInfo`s
    use crate::dll::*;
    use core::ffi::c_void;

    #[derive(Debug)]
    #[repr(C)]
    pub struct Ref<'a, T> {
        ptr: &'a T,
        sharing_info: RefCount,
    }

    impl<'a, T> Drop for Ref<'a, T> {
        fn drop(&mut self) {
            self.sharing_info.decrease_ref();
        }
    }

    impl<'a, T> core::ops::Deref for Ref<'a, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            self.ptr
        }
    }

    #[derive(Debug)]
    #[repr(C)]
    pub struct RefMut<'a, T> {
        ptr: &'a mut T,
        sharing_info: RefCount,
    }

    impl<'a, T> Drop for RefMut<'a, T> {
        fn drop(&mut self) {
            self.sharing_info.decrease_refmut();
        }
    }

    impl<'a, T> core::ops::Deref for RefMut<'a, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &*self.ptr
        }
    }

    impl<'a, T> core::ops::DerefMut for RefMut<'a, T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            self.ptr
        }
    }

    impl RefAny {

        /// Creates a new, type-erased pointer by casting the `T` value into a `Vec<u8>` and saving the length + type ID
        pub fn new<T: 'static>(value: T) -> Self {
            use crate::dll::*;

            extern "C" fn default_custom_destructor<U: 'static>(ptr: &mut c_void) {
                use core::{mem, ptr};

                // note: in the default constructor, we do not need to check whether U == T

                unsafe {
                    // copy the struct from the heap to the stack and
                    // call mem::drop on U to run the destructor
                    let mut stack_mem = mem::MaybeUninit::<U>::uninit();
                    ptr::copy_nonoverlapping((ptr as *mut c_void) as *const U, stack_mem.as_mut_ptr(), mem::size_of::<U>());
                    let stack_mem = stack_mem.assume_init();
                    mem::drop(stack_mem);
                }
            }

            let type_name_str = ::core::any::type_name::<T>();
            let st = crate::str::String::from_const_str(type_name_str);
            let s = unsafe { crate::dll::AzRefAny_newC(
                (&value as *const T) as *const c_void,
                ::core::mem::size_of::<T>(),
                Self::type_id::<T>(),
                st,
                default_custom_destructor::<T>,
            ) };
            ::core::mem::forget(value); // do not run the destructor of T here!
            s
        }

        /// Downcasts the type-erased pointer to a type `&U`, returns `None` if the types don't match
        #[inline]
        pub fn downcast_ref<'a, U: 'static>(&'a mut self) -> Option<Ref<'a, U>> {
            let is_same_type = self.get_type_id() == Self::type_id::<U>();
            if !is_same_type { return None; }

            let can_be_shared = self.sharing_info.can_be_shared();
            if !can_be_shared { return None; }

            self.sharing_info.increase_ref();
            Some(Ref {
                ptr: unsafe { &*(self._internal_ptr as *const U) },
                sharing_info: self.sharing_info.clone(),
            })
        }

        /// Downcasts the type-erased pointer to a type `&mut U`, returns `None` if the types don't match
        #[inline]
        pub fn downcast_mut<'a, U: 'static>(&'a mut self) -> Option<RefMut<'a, U>> {
            let is_same_type = self.get_type_id() == Self::type_id::<U>();
            if !is_same_type { return None; }

            let can_be_shared_mut = self.sharing_info.can_be_shared_mut();
            if !can_be_shared_mut { return None; }

            self.sharing_info.increase_refmut();

            Some(RefMut {
                ptr: unsafe { &mut *(self._internal_ptr as *mut U) },
                sharing_info: self.sharing_info.clone(),
            })
        }

        // Returns the typeid of `T` as a u64 (necessary because `core::any::TypeId` is not C-ABI compatible)
        #[inline]
        pub fn type_id<T: 'static>() -> u64 {
            use core::any::TypeId;
            use core::mem;

            // fast method to serialize the type id into a u64
            let t_id = TypeId::of::<T>();
            let struct_as_bytes = unsafe { ::core::slice::from_raw_parts((&t_id as *const TypeId) as *const u8, mem::size_of::<TypeId>()) };
            struct_as_bytes.into_iter().enumerate().map(|(s_pos, s)| ((*s as u64) << s_pos)).sum()
        }
    }    use crate::css::{CssProperty, CssPropertyType};
    use crate::window::{LogicalPosition, WindowCreateOptions, WindowState};
    use crate::str::String;
    use crate::image::{ImageMask, ImageRef};
    use crate::task::{ThreadId, ThreadSendMsg, Timer, TimerId};
    /// `LayoutCallback` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutCallback as LayoutCallback;
    /// C-ABI stable wrapper over a `MarshaledLayoutCallback`
    
#[doc(inline)] pub use crate::dll::AzMarshaledLayoutCallback as MarshaledLayoutCallback;
    /// C-ABI stable wrapper over a `MarshaledLayoutCallbackInner`
    
#[doc(inline)] pub use crate::dll::AzMarshaledLayoutCallbackInner as MarshaledLayoutCallbackInner;
    /// Marshaled version of LayoutCallback, carrys an extra "marshal_data" containing the (usually external) function object
    
#[doc(inline)] pub use crate::dll::AzMarshaledLayoutCallbackType as MarshaledLayoutCallbackType;
    /// C-ABI stable wrapper over a `LayoutCallbackType`
    
#[doc(inline)] pub use crate::dll::AzLayoutCallbackInner as LayoutCallbackInner;
    /// Main callback to layout the UI. azul will only call this callback when necessary (usually when one of the callback or timer returns `RegenerateStyledDomForCurrentWindow`), however azul may also call this callback at any given time, so it should be performant. This is the main entry point for your app UI.
    
#[doc(inline)] pub use crate::dll::AzLayoutCallbackType as LayoutCallbackType;
    /// C-ABI stable wrapper over a `CallbackType`
    
#[doc(inline)] pub use crate::dll::AzCallback as Callback;
    /// Generic UI callback function pointer: called when the `EventFilter` is active
    
#[doc(inline)] pub use crate::dll::AzCallbackType as CallbackType;
    /// `CallbackInfo` struct
    
#[doc(inline)] pub use crate::dll::AzCallbackInfo as CallbackInfo;
    impl CallbackInfo {
        /// Returns the `DomNodeId` of the element that the callback was attached to.
        pub fn get_hit_node(&self)  -> crate::callbacks::DomNodeId { unsafe { crate::dll::AzCallbackInfo_getHitNode(self) } }
        /// Returns the function pointer necessary to query the current time.
        pub fn get_system_time_fn(&self)  -> crate::task::GetSystemTimeFn { unsafe { crate::dll::AzCallbackInfo_getSystemTimeFn(self) } }
        /// Returns the `LayoutPoint` of the cursor in the viewport (relative to the origin of the `Dom`). Set to `None` if the cursor is not in the current window.
        pub fn get_cursor_relative_to_viewport(&self)  -> crate::option::OptionLogicalPosition { unsafe { crate::dll::AzCallbackInfo_getCursorRelativeToViewport(self) } }
        /// Returns the `LayoutPoint` of the cursor in the viewport (relative to the origin of the `Dom`). Set to `None` if the cursor is not hovering over the current node.
        pub fn get_cursor_relative_to_node(&self)  -> crate::option::OptionLogicalPosition { unsafe { crate::dll::AzCallbackInfo_getCursorRelativeToNode(self) } }
        /// Returns a copy of the current windows `WindowState`.
        pub fn get_current_window_state(&self)  -> crate::window::WindowState { unsafe { crate::dll::AzCallbackInfo_getCurrentWindowState(self) } }
        /// Returns a copy of the internal `KeyboardState`. Same as `self.get_window_state().keyboard_state`
        pub fn get_current_keyboard_state(&self)  -> crate::window::KeyboardState { unsafe { crate::dll::AzCallbackInfo_getCurrentKeyboardState(self) } }
        /// Returns a copy of the internal `MouseState`. Same as `self.get_window_state().mouse_state`
        pub fn get_current_mouse_state(&self)  -> crate::window::MouseState { unsafe { crate::dll::AzCallbackInfo_getCurrentMouseState(self) } }
        /// Returns a copy of the current windows `WindowState`.
        pub fn get_previous_window_state(&self)  -> crate::option::OptionWindowState { unsafe { crate::dll::AzCallbackInfo_getPreviousWindowState(self) } }
        /// Returns a copy of the internal `KeyboardState`. Same as `self.get_window_state().keyboard_state`
        pub fn get_previous_keyboard_state(&self)  -> crate::option::OptionKeyboardState { unsafe { crate::dll::AzCallbackInfo_getPreviousKeyboardState(self) } }
        /// Returns a copy of the internal `MouseState`. Same as `self.get_window_state().mouse_state`
        pub fn get_previous_mouse_state(&self)  -> crate::option::OptionMouseState { unsafe { crate::dll::AzCallbackInfo_getPreviousMouseState(self) } }
        /// Returns a copy of the current windows `RawWindowHandle`.
        pub fn get_current_window_handle(&self)  -> crate::window::RawWindowHandle { unsafe { crate::dll::AzCallbackInfo_getCurrentWindowHandle(self) } }
        /// Returns a **reference-counted copy** of the current windows' `Gl` (context). You can use this to render OpenGL textures.
        pub fn get_gl_context(&self)  -> crate::option::OptionGl { unsafe { crate::dll::AzCallbackInfo_getGlContext(self) } }
        /// Returns the x / y offset that this node has been scrolled to by the user or `None` if the node has not been scrolled.
        pub fn get_scroll_position(&self, node_id: DomNodeId)  -> crate::option::OptionLogicalPosition { unsafe { crate::dll::AzCallbackInfo_getScrollPosition(self, node_id) } }
        /// Returns the `dataset` property of the given Node or `None` if the node doesn't have a `dataset` property.
        pub fn get_dataset(&mut self, node_id: DomNodeId)  -> crate::option::OptionRefAny { unsafe { crate::dll::AzCallbackInfo_getDataset(self, node_id) } }
        /// If the node is a `Text` node, returns a copy of the internal string contents.
        pub fn get_string_contents(&self, node_id: DomNodeId)  -> crate::option::OptionString { unsafe { crate::dll::AzCallbackInfo_getStringContents(self, node_id) } }
        /// If the node is a `Text` node, returns the layouted inline glyphs
        pub fn get_inline_text(&self, node_id: DomNodeId)  -> crate::option::OptionInlineText { unsafe { crate::dll::AzCallbackInfo_getInlineText(self, node_id) } }
        /// Returns the index of the node relative to the parent node.
        pub fn get_index_in_parent(&mut self, node_id: DomNodeId)  -> usize { unsafe { crate::dll::AzCallbackInfo_getIndexInParent(self, node_id) } }
        /// Returns the parent `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_parent(&mut self, node_id: DomNodeId)  -> crate::option::OptionDomNodeId { unsafe { crate::dll::AzCallbackInfo_getParent(self, node_id) } }
        /// Returns the previous siblings `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_previous_sibling(&mut self, node_id: DomNodeId)  -> crate::option::OptionDomNodeId { unsafe { crate::dll::AzCallbackInfo_getPreviousSibling(self, node_id) } }
        /// Returns the next siblings `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_next_sibling(&mut self, node_id: DomNodeId)  -> crate::option::OptionDomNodeId { unsafe { crate::dll::AzCallbackInfo_getNextSibling(self, node_id) } }
        /// Returns the next siblings `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_first_child(&mut self, node_id: DomNodeId)  -> crate::option::OptionDomNodeId { unsafe { crate::dll::AzCallbackInfo_getFirstChild(self, node_id) } }
        /// Returns the next siblings `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_last_child(&mut self, node_id: DomNodeId)  -> crate::option::OptionDomNodeId { unsafe { crate::dll::AzCallbackInfo_getLastChild(self, node_id) } }
        /// Returns the position of a given DOM node in the UI
        pub fn get_node_position(&mut self, node_id: DomNodeId)  -> crate::option::OptionPositionInfo { unsafe { crate::dll::AzCallbackInfo_getNodePosition(self, node_id) } }
        /// Returns the size of a given DOM node in the UI
        pub fn get_node_size(&mut self, node_id: DomNodeId)  -> crate::option::OptionLogicalSize { unsafe { crate::dll::AzCallbackInfo_getNodeSize(self, node_id) } }
        /// Returns the current computed CSS property of a given DOM node in the UI
        pub fn get_computed_css_property(&mut self, node_id: DomNodeId, property_type: CssPropertyType)  -> crate::option::OptionCssProperty { unsafe { crate::dll::AzCallbackInfo_getComputedCssProperty(self, node_id, property_type) } }
        /// Sets the new `WindowState` for the next frame. The window is updated after all callbacks are run.
        pub fn set_window_state(&mut self, new_state: WindowState)  { unsafe { crate::dll::AzCallbackInfo_setWindowState(self, new_state) } }
        /// Sets the new `FocusTarget` for the next frame. Note that this will emit a `On::FocusLost` and `On::FocusReceived` event, if the focused node has changed.
        pub fn set_focus(&mut self, target: FocusTarget)  { unsafe { crate::dll::AzCallbackInfo_setFocus(self, target) } }
        /// Sets a `CssProperty` on a given node to its new value. If this property change affects the layout, this will automatically trigger a relayout and redraw of the screen.
        pub fn set_css_property(&mut self, node_id: DomNodeId, new_property: CssProperty)  { unsafe { crate::dll::AzCallbackInfo_setCssProperty(self, node_id, new_property) } }
        /// Sets the scroll position of the node
        pub fn set_scroll_position(&mut self, node_id: DomNodeId, scroll_position: LogicalPosition)  { unsafe { crate::dll::AzCallbackInfo_setScrollPosition(self, node_id, scroll_position) } }
        /// If the node is a `Text` node, overwrites the `Text` content with the new string, without requiring the entire UI to be rebuilt.
        pub fn set_string_contents(&mut self, node_id: DomNodeId, string: String)  { unsafe { crate::dll::AzCallbackInfo_setStringContents(self, node_id, string) } }
        /// Adds a new image identified by an ID to the image cache
        pub fn add_image(&mut self, id: String, image: ImageRef)  { unsafe { crate::dll::AzCallbackInfo_addImage(self, id, image) } }
        /// Returns whether an image with a given CSS ID already exists
        pub fn has_image(&self, id: String)  -> bool { unsafe { crate::dll::AzCallbackInfo_hasImage(self, id) } }
        /// Returns the image with a given CSS ID
        pub fn get_image(&self, id: String)  -> crate::option::OptionImageRef { unsafe { crate::dll::AzCallbackInfo_getImage(self, id) } }
        /// If the node is an `Image`, exchanges the current image with a new source
        pub fn update_image(&mut self, node_id: DomNodeId, new_image: ImageRef)  { unsafe { crate::dll::AzCallbackInfo_updateImage(self, node_id, new_image) } }
        /// Deletes an image identified by a CSS ID from the image cache
        pub fn delete_image(&mut self, id: String)  { unsafe { crate::dll::AzCallbackInfo_deleteImage(self, id) } }
        /// If the node has an `ImageMask`, exchanges the current mask for the new mask
        pub fn update_image_mask(&mut self, node_id: DomNodeId, new_mask: ImageMask)  { unsafe { crate::dll::AzCallbackInfo_updateImageMask(self, node_id, new_mask) } }
        /// Stops the propagation of the current callback event type to the parent. Events are bubbled from the inside out (children first, then parents), this event stops the propagation of the event to the parent.
        pub fn stop_propagation(&mut self)  { unsafe { crate::dll::AzCallbackInfo_stopPropagation(self) } }
        /// Spawns a new window with the given `WindowCreateOptions`.
        pub fn create_window(&mut self, new_window: WindowCreateOptions)  { unsafe { crate::dll::AzCallbackInfo_createWindow(self, new_window) } }
        /// Adds a new `Timer` to the runtime. See the documentation for `Timer` for more information.
        pub fn start_timer(&mut self, timer: Timer)  -> crate::option::OptionTimerId { unsafe { crate::dll::AzCallbackInfo_startTimer(self, timer) } }
        /// Starts an animation timer on a give NodeId - same as a `Timer`, but uses a pre-configured interpolation function to drive the animation timer
        pub fn start_animation(&mut self, node: DomNodeId, animation: Animation)  -> crate::option::OptionTimerId { unsafe { crate::dll::AzCallbackInfo_startAnimation(self, node, animation) } }
        /// Stops / cancels a `Timer`. See the documentation for `Timer` for more information.
        pub fn stop_timer(&mut self, timer_id: TimerId)  -> bool { unsafe { crate::dll::AzCallbackInfo_stopTimer(self, timer_id) } }
        /// Starts a new `Thread` to the runtime. See the documentation for `Thread` for more information.
        pub fn start_thread(&mut self, thread_initialize_data: RefAny, writeback_data: RefAny, callback: ThreadCallback)  -> crate::option::OptionThreadId { unsafe { crate::dll::AzCallbackInfo_startThread(self, thread_initialize_data, writeback_data, callback) } }
        /// Sends a message to a background thread
        pub fn send_thread_msg(&mut self, thread_id: ThreadId, msg: ThreadSendMsg)  -> bool { unsafe { crate::dll::AzCallbackInfo_sendThreadMsg(self, thread_id, msg) } }
        /// Stops a thread at the nearest possible opportunity. Sends a `ThreadSendMsg::TerminateThread` message to the thread and joins the thread.
        pub fn stop_thread(&mut self, thread_id: ThreadId)  -> bool { unsafe { crate::dll::AzCallbackInfo_stopThread(self, thread_id) } }
    }

    /// Specifies if the screen should be updated after the callback function has returned
    
#[doc(inline)] pub use crate::dll::AzUpdate as Update;
    /// Index of a Node in the internal `NodeDataContainer`
    
#[doc(inline)] pub use crate::dll::AzNodeId as NodeId;
    /// ID of a DOM - one window can contain multiple, nested DOMs (such as iframes)
    
#[doc(inline)] pub use crate::dll::AzDomId as DomId;
    /// Combination of node ID + DOM ID, both together can identify a node
    
#[doc(inline)] pub use crate::dll::AzDomNodeId as DomNodeId;
    /// `PositionInfo` struct
    
#[doc(inline)] pub use crate::dll::AzPositionInfo as PositionInfo;
    /// `PositionInfoInner` struct
    
#[doc(inline)] pub use crate::dll::AzPositionInfoInner as PositionInfoInner;
    /// `HidpiAdjustedBounds` struct
    
#[doc(inline)] pub use crate::dll::AzHidpiAdjustedBounds as HidpiAdjustedBounds;
    impl HidpiAdjustedBounds {
        /// Returns the size of the bounds in logical units
        pub fn get_logical_size(&self)  -> crate::window::LogicalSize { unsafe { crate::dll::AzHidpiAdjustedBounds_getLogicalSize(self) } }
        /// Returns the size of the bounds in physical units
        pub fn get_physical_size(&self)  -> crate::window::PhysicalSizeU32 { unsafe { crate::dll::AzHidpiAdjustedBounds_getPhysicalSize(self) } }
        /// Returns the hidpi factor of the bounds
        pub fn get_hidpi_factor(&self)  -> f32 { unsafe { crate::dll::AzHidpiAdjustedBounds_getHidpiFactor(self) } }
    }

    /// `InlineText` struct
    
#[doc(inline)] pub use crate::dll::AzInlineText as InlineText;
    impl InlineText {
        /// Hit-tests the inline text, returns detailed information about which glyph / word / line, etc. the position (usually the mouse cursor) is currently over. Result may be empty (no hits) or contain more than one result (cursor is hovering over multiple overlapping glyphs at once).
        pub fn hit_test(&self, position: LogicalPosition)  -> crate::vec::InlineTextHitVec { unsafe { crate::dll::AzInlineText_hitTest(self, position) } }
    }

    /// `InlineLine` struct
    
#[doc(inline)] pub use crate::dll::AzInlineLine as InlineLine;
    /// `InlineWord` struct
    
#[doc(inline)] pub use crate::dll::AzInlineWord as InlineWord;
    /// `InlineTextContents` struct
    
#[doc(inline)] pub use crate::dll::AzInlineTextContents as InlineTextContents;
    /// `InlineGlyph` struct
    
#[doc(inline)] pub use crate::dll::AzInlineGlyph as InlineGlyph;
    /// `InlineTextHit` struct
    
#[doc(inline)] pub use crate::dll::AzInlineTextHit as InlineTextHit;
    /// Defines the keyboard input focus target
    
#[doc(inline)] pub use crate::dll::AzFocusTarget as FocusTarget;
    /// CSS path to set the keyboard input focus
    
#[doc(inline)] pub use crate::dll::AzFocusTargetPath as FocusTargetPath;
    /// Animation struct to start a new animation
    
#[doc(inline)] pub use crate::dll::AzAnimation as Animation;
    /// How should an animation repeat (loop, ping-pong, etc.)
    
#[doc(inline)] pub use crate::dll::AzAnimationRepeat as AnimationRepeat;
    /// How many times should an animation repeat
    
#[doc(inline)] pub use crate::dll::AzAnimationRepeatCount as AnimationRepeatCount;
    /// Easing function of the animation (ease-in, ease-out, ease-in-out, custom)
    
#[doc(inline)] pub use crate::dll::AzAnimationEasing as AnimationEasing;
    /// C-ABI wrapper over an `IFrameCallbackType`
    
#[doc(inline)] pub use crate::dll::AzIFrameCallback as IFrameCallback;
    /// For rendering large or infinite datasets such as tables or lists, azul uses `IFrameCallbacks` that allow the library user to only render the visible portion of DOM nodes, not the entire set. IFrames are rendered after the screen has been laid out, but before it gets composited. IFrames can be used recursively (i.e. iframes within iframes are possible). IFrames are re-rendered once the user scrolls to the bounds (see `IFrameCallbackReturn` on how to set the bounds) or the parent DOM was recreated.
    
#[doc(inline)] pub use crate::dll::AzIFrameCallbackType as IFrameCallbackType;
    /// `IFrameCallbackInfo` struct
    
#[doc(inline)] pub use crate::dll::AzIFrameCallbackInfo as IFrameCallbackInfo;
    /// <img src="../images/scrollbounds.png"/>
    
#[doc(inline)] pub use crate::dll::AzIFrameCallbackReturn as IFrameCallbackReturn;
    /// `RenderImageCallback` struct
    
#[doc(inline)] pub use crate::dll::AzRenderImageCallback as RenderImageCallback;
    /// `RenderImageCallbackType` struct
    
#[doc(inline)] pub use crate::dll::AzRenderImageCallbackType as RenderImageCallbackType;
    /// `RenderImageCallbackInfo` struct
    
#[doc(inline)] pub use crate::dll::AzRenderImageCallbackInfo as RenderImageCallbackInfo;
    impl RenderImageCallbackInfo {
        /// Returns a copy of the internal `Gl`
        pub fn get_gl_context(&self)  -> crate::option::OptionGl { unsafe { crate::dll::AzRenderImageCallbackInfo_getGlContext(self) } }
        /// Returns a copy of the internal `HidpiAdjustedBounds`
        pub fn get_bounds(&self)  -> crate::callbacks::HidpiAdjustedBounds { unsafe { crate::dll::AzRenderImageCallbackInfo_getBounds(self) } }
        /// Returns the `DomNodeId` that this callback was called on
        pub fn get_callback_node_id(&self)  -> crate::callbacks::DomNodeId { unsafe { crate::dll::AzRenderImageCallbackInfo_getCallbackNodeId(self) } }
        /// If the node is a `Text` node, returns the layouted inline glyphs
        pub fn get_inline_text(&self, node_id: DomNodeId)  -> crate::option::OptionInlineText { unsafe { crate::dll::AzRenderImageCallbackInfo_getInlineText(self, node_id) } }
        /// Returns the index of the node relative to the parent node.
        pub fn get_index_in_parent(&mut self, node_id: DomNodeId)  -> usize { unsafe { crate::dll::AzRenderImageCallbackInfo_getIndexInParent(self, node_id) } }
        /// Returns the parent `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_parent(&mut self, node_id: DomNodeId)  -> crate::option::OptionDomNodeId { unsafe { crate::dll::AzRenderImageCallbackInfo_getParent(self, node_id) } }
        /// Returns the previous siblings `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_previous_sibling(&mut self, node_id: DomNodeId)  -> crate::option::OptionDomNodeId { unsafe { crate::dll::AzRenderImageCallbackInfo_getPreviousSibling(self, node_id) } }
        /// Returns the next siblings `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_next_sibling(&mut self, node_id: DomNodeId)  -> crate::option::OptionDomNodeId { unsafe { crate::dll::AzRenderImageCallbackInfo_getNextSibling(self, node_id) } }
        /// Returns the next siblings `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_first_child(&mut self, node_id: DomNodeId)  -> crate::option::OptionDomNodeId { unsafe { crate::dll::AzRenderImageCallbackInfo_getFirstChild(self, node_id) } }
        /// Returns the next siblings `DomNodeId` of the given `DomNodeId`. Returns `None` on an invalid NodeId.
        pub fn get_last_child(&mut self, node_id: DomNodeId)  -> crate::option::OptionDomNodeId { unsafe { crate::dll::AzRenderImageCallbackInfo_getLastChild(self, node_id) } }
    }

    /// `TimerCallback` struct
    
#[doc(inline)] pub use crate::dll::AzTimerCallback as TimerCallback;
    /// `TimerCallbackType` struct
    
#[doc(inline)] pub use crate::dll::AzTimerCallbackType as TimerCallbackType;
    /// `TimerCallbackInfo` struct
    
#[doc(inline)] pub use crate::dll::AzTimerCallbackInfo as TimerCallbackInfo;
    /// `TimerCallbackReturn` struct
    
#[doc(inline)] pub use crate::dll::AzTimerCallbackReturn as TimerCallbackReturn;
    /// `WriteBackCallbackType` struct
    
#[doc(inline)] pub use crate::dll::AzWriteBackCallbackType as WriteBackCallbackType;
    /// `WriteBackCallback` struct
    
#[doc(inline)] pub use crate::dll::AzWriteBackCallback as WriteBackCallback;
    /// `ThreadCallback` struct
    
#[doc(inline)] pub use crate::dll::AzThreadCallback as ThreadCallback;
    /// `ThreadCallbackType` struct
    
#[doc(inline)] pub use crate::dll::AzThreadCallbackType as ThreadCallbackType;
    /// `RefAnyDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzRefAnyDestructorType as RefAnyDestructorType;
    /// `RefCount` struct
    
#[doc(inline)] pub use crate::dll::AzRefCount as RefCount;
    impl RefCount {
        /// Calls the `RefCount::can_be_shared` function.
        pub fn can_be_shared(&self)  -> bool { unsafe { crate::dll::AzRefCount_canBeShared(self) } }
        /// Calls the `RefCount::can_be_shared_mut` function.
        pub fn can_be_shared_mut(&self)  -> bool { unsafe { crate::dll::AzRefCount_canBeSharedMut(self) } }
        /// Calls the `RefCount::increase_ref` function.
        pub fn increase_ref(&mut self)  { unsafe { crate::dll::AzRefCount_increaseRef(self) } }
        /// Calls the `RefCount::decrease_ref` function.
        pub fn decrease_ref(&mut self)  { unsafe { crate::dll::AzRefCount_decreaseRef(self) } }
        /// Calls the `RefCount::increase_refmut` function.
        pub fn increase_refmut(&mut self)  { unsafe { crate::dll::AzRefCount_increaseRefmut(self) } }
        /// Calls the `RefCount::decrease_refmut` function.
        pub fn decrease_refmut(&mut self)  { unsafe { crate::dll::AzRefCount_decreaseRefmut(self) } }
    }

    impl Clone for RefCount { fn clone(&self) -> Self { unsafe { crate::dll::AzRefCount_deepCopy(self) } } }
    impl Drop for RefCount { fn drop(&mut self) { unsafe { crate::dll::AzRefCount_delete(self) } } }
    /// RefAny is a reference-counted, opaque pointer, which stores a reference to a struct. `RefAny` can be up- and downcasted (this usually done via generics and can't be expressed in the Rust API)
    
#[doc(inline)] pub use crate::dll::AzRefAny as RefAny;
    impl RefAny {
        /// Creates a new `RefAny` instance.
        pub fn new_c(ptr: *const c_void, len: usize, type_id: u64, type_name: String, destructor: RefAnyDestructorType) -> Self { unsafe { crate::dll::AzRefAny_newC(ptr, len, type_id, type_name, destructor) } }
        /// Calls the `RefAny::get_type_id` function.
        pub fn get_type_id(&self)  -> u64 { unsafe { crate::dll::AzRefAny_getTypeId(self) } }
        /// Calls the `RefAny::get_type_name` function.
        pub fn get_type_name(&self)  -> crate::str::String { unsafe { crate::dll::AzRefAny_getTypeName(self) } }
    }

    impl Clone for RefAny { fn clone(&self) -> Self { unsafe { crate::dll::AzRefAny_deepCopy(self) } } }
    impl Drop for RefAny { fn drop(&mut self) { unsafe { crate::dll::AzRefAny_delete(self) } } }
    /// `LayoutCallbackInfo` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutCallbackInfo as LayoutCallbackInfo;
    impl LayoutCallbackInfo {
        /// Returns a copy of the OpenGL context
        pub fn get_gl_context(&self)  -> crate::option::OptionGl { unsafe { crate::dll::AzLayoutCallbackInfo_getGlContext(self) } }
        /// Returns all system-native fonts with their respective file paths as values
        pub fn get_system_fonts(&self)  -> crate::vec::StringPairVec { unsafe { crate::dll::AzLayoutCallbackInfo_getSystemFonts(self) } }
        /// Returns an `ImageRef` referenced by a CSS ID
        pub fn get_image(&self, id: String)  -> crate::option::OptionImageRef { unsafe { crate::dll::AzLayoutCallbackInfo_getImage(self, id) } }
    }

}

pub mod dom {
    #![allow(dead_code, unused_imports)]
    //! `Dom` construction and configuration
    use crate::dll::*;
    use core::ffi::c_void;
    impl Default for Dom {
        fn default() -> Self {
            Dom::div()
        }
    }

    impl Default for NodeData {
        fn default() -> Self {
            NodeData::new(NodeType::Div)
        }
    }

    impl Default for TabIndex {
        fn default() -> Self {
            TabIndex::Auto
        }
    }

    impl core::iter::FromIterator<Dom> for Dom {
        fn from_iter<I: IntoIterator<Item=Dom>>(iter: I) -> Self {
            use crate::vec::DomVec;
            let mut total_children = 0;
            let children = iter.into_iter().map(|c| {
                total_children += c.total_children + 1;
                c
            }).collect::<DomVec>();

            Dom {
                root: NodeData::div(),
                children,
                total_children,
            }
        }
    }

    impl core::iter::FromIterator<NodeData> for Dom {
        fn from_iter<I: IntoIterator<Item=NodeData>>(iter: I) -> Self {
            use crate::vec::DomVec;
            let children = iter.into_iter().map(|c| Dom {
                root: c,
                children: DomVec::from_const_slice(&[]),
                total_children: 0
            }).collect::<DomVec>();
            let total_children = children.len();

            Dom {
                root: NodeData::div(),
                children: children,
                total_children,
            }
        }
    }

    impl core::iter::FromIterator<NodeType> for Dom {
        fn from_iter<I: core::iter::IntoIterator<Item=NodeType>>(iter: I) -> Self {
            iter.into_iter().map(|i| {
                let mut nd = NodeData::default();
                nd.node_type = i;
                nd
            }).collect()
        }
    }

    impl From<On> for AzEventFilter {
        fn from(on: On) -> AzEventFilter {
            on.into_event_filter()
        }
    }    use crate::str::String;
    use crate::image::{ImageMask, ImageRef};
    use crate::callbacks::{IFrameCallbackType, RefAny};
    use crate::vec::{CallbackDataVec, DomVec, IdOrClassVec, NodeDataInlineCssPropertyVec};
    use crate::css::{Css, CssProperty};
    use crate::menu::Menu;
    /// `Dom` struct
    
#[doc(inline)] pub use crate::dll::AzDom as Dom;
    impl Dom {
        /// Creates a new `Dom` instance.
        pub fn new(node_type: NodeType) -> Self { unsafe { crate::dll::AzDom_new(node_type) } }
        /// Creates a new `Dom` instance.
        pub fn body() -> Self { unsafe { crate::dll::AzDom_body() } }
        /// Creates a new `Dom` instance.
        pub fn div() -> Self { unsafe { crate::dll::AzDom_div() } }
        /// Creates a new `Dom` instance.
        pub fn br() -> Self { unsafe { crate::dll::AzDom_br() } }
        /// Creates a new `Dom` instance.
        pub fn text(string: String) -> Self { unsafe { crate::dll::AzDom_text(string) } }
        /// Creates a new `Dom` instance.
        pub fn image(image: ImageRef) -> Self { unsafe { crate::dll::AzDom_image(image) } }
        /// Creates a new `Dom` instance.
        pub fn iframe(data: RefAny, callback: IFrameCallbackType) -> Self { unsafe { crate::dll::AzDom_iframe(data, callback) } }
        /// Calls the `Dom::set_node_type` function.
        pub fn set_node_type(&mut self, node_type: NodeType)  { unsafe { crate::dll::AzDom_setNodeType(self, node_type) } }
        /// Calls the `Dom::with_node_type` function.
        pub fn with_node_type(&mut self, node_type: NodeType)  -> crate::dom::Dom { unsafe { crate::dll::AzDom_withNodeType(self, node_type) } }
        /// Calls the `Dom::set_dataset` function.
        pub fn set_dataset(&mut self, dataset: RefAny)  { unsafe { crate::dll::AzDom_setDataset(self, dataset) } }
        /// Calls the `Dom::with_dataset` function.
        pub fn with_dataset(&mut self, dataset: RefAny)  -> crate::dom::Dom { unsafe { crate::dll::AzDom_withDataset(self, dataset) } }
        /// Calls the `Dom::set_ids_and_classes` function.
        pub fn set_ids_and_classes(&mut self, ids_and_classes: IdOrClassVec)  { unsafe { crate::dll::AzDom_setIdsAndClasses(self, ids_and_classes) } }
        /// Calls the `Dom::with_ids_and_classes` function.
        pub fn with_ids_and_classes(&mut self, ids_and_classes: IdOrClassVec)  -> crate::dom::Dom { unsafe { crate::dll::AzDom_withIdsAndClasses(self, ids_and_classes) } }
        /// Calls the `Dom::set_callbacks` function.
        pub fn set_callbacks(&mut self, callbacks: CallbackDataVec)  { unsafe { crate::dll::AzDom_setCallbacks(self, callbacks) } }
        /// Calls the `Dom::with_callbacks` function.
        pub fn with_callbacks(&mut self, callbacks: CallbackDataVec)  -> crate::dom::Dom { unsafe { crate::dll::AzDom_withCallbacks(self, callbacks) } }
        /// Calls the `Dom::set_inline_css_props` function.
        pub fn set_inline_css_props(&mut self, css_properties: NodeDataInlineCssPropertyVec)  { unsafe { crate::dll::AzDom_setInlineCssProps(self, css_properties) } }
        /// Calls the `Dom::with_inline_css_props` function.
        pub fn with_inline_css_props(&mut self, css_properties: NodeDataInlineCssPropertyVec)  -> crate::dom::Dom { unsafe { crate::dll::AzDom_withInlineCssProps(self, css_properties) } }
        /// Adds a child node to this DOM (potentially heap-allocates in Rust code). Swaps `self` with a default `Dom` in order to prevent accidental copies.
        pub fn add_child(&mut self, child: Dom)  { unsafe { crate::dll::AzDom_addChild(self, child) } }
        /// Same as add_child, but as a builder method.
        pub fn with_child(&mut self, child: Dom)  -> crate::dom::Dom { unsafe { crate::dll::AzDom_withChild(self, child) } }
        /// Adds a child node to this DOM (potentially heap-allocates in Rust code). Swaps `self` with a default `Dom` in order to prevent accidental copies.
        pub fn set_children(&mut self, children: DomVec)  { unsafe { crate::dll::AzDom_setChildren(self, children) } }
        /// Same as set_children, but as a builder method.
        pub fn with_children(&mut self, children: DomVec)  -> crate::dom::Dom { unsafe { crate::dll::AzDom_withChildren(self, children) } }
        /// Adds an CSS ID to the DOM root node.
        pub fn add_id(&mut self, id: String)  { unsafe { crate::dll::AzDom_addId(self, id) } }
        /// Same as add_id, but as a builder method
        pub fn with_id(&mut self, id: String)  -> crate::dom::Dom { unsafe { crate::dll::AzDom_withId(self, id) } }
        /// Adds a CSS class to the DOM root node.
        pub fn add_class(&mut self, class: String)  { unsafe { crate::dll::AzDom_addClass(self, class) } }
        /// Same as add_class, but as a builder method
        pub fn with_class(&mut self, class: String)  -> crate::dom::Dom { unsafe { crate::dll::AzDom_withClass(self, class) } }
        /// Adds an inline (normal) CSS property to the DOM root node.
        pub fn add_css_property(&mut self, prop: CssProperty)  { unsafe { crate::dll::AzDom_addCssProperty(self, prop) } }
        /// Same as add_class, but as a builder method
        pub fn with_css_property(&mut self, prop: CssProperty)  -> crate::dom::Dom { unsafe { crate::dll::AzDom_withCssProperty(self, prop) } }
        /// Adds an inline (hover) CSS property to the DOM root node.
        pub fn add_hover_css_property(&mut self, prop: CssProperty)  { unsafe { crate::dll::AzDom_addHoverCssProperty(self, prop) } }
        /// Same as add_class, but as a builder method
        pub fn with_hover_css_property(&mut self, prop: CssProperty)  -> crate::dom::Dom { unsafe { crate::dll::AzDom_withHoverCssProperty(self, prop) } }
        /// Adds an inline (hover) CSS property to the DOM root node.
        pub fn add_active_css_property(&mut self, prop: CssProperty)  { unsafe { crate::dll::AzDom_addActiveCssProperty(self, prop) } }
        /// Same as add_class, but as a builder method
        pub fn with_active_css_property(&mut self, prop: CssProperty)  -> crate::dom::Dom { unsafe { crate::dll::AzDom_withActiveCssProperty(self, prop) } }
        /// Adds an inline (hover) CSS property to the DOM root node.
        pub fn add_focus_css_property(&mut self, prop: CssProperty)  { unsafe { crate::dll::AzDom_addFocusCssProperty(self, prop) } }
        /// Same as add_class, but as a builder method
        pub fn with_focus_css_property(&mut self, prop: CssProperty)  -> crate::dom::Dom { unsafe { crate::dll::AzDom_withFocusCssProperty(self, prop) } }
        /// Sets the clip mask for the DOM root node.
        pub fn set_clip_mask(&mut self, clip_mask: ImageMask)  { unsafe { crate::dll::AzDom_setClipMask(self, clip_mask) } }
        /// Same as set_clip_mask, but as a builder method
        pub fn with_clip_mask(&mut self, clip_mask: ImageMask)  -> crate::dom::Dom { unsafe { crate::dll::AzDom_withClipMask(self, clip_mask) } }
        /// Sets the tab index for the DOM root node.
        pub fn set_tab_index(&mut self, tab_index: TabIndex)  { unsafe { crate::dll::AzDom_setTabIndex(self, tab_index) } }
        /// Same as set_tab_index, but as a builder method
        pub fn with_tab_index(&mut self, tab_index: TabIndex)  -> crate::dom::Dom { unsafe { crate::dll::AzDom_withTabIndex(self, tab_index) } }
        /// Sets accessibility attributes for the DOM root node.
        pub fn set_accessibility_info(&mut self, accessibility_info: AccessibilityInfo)  { unsafe { crate::dll::AzDom_setAccessibilityInfo(self, accessibility_info) } }
        /// Same as set_accessibility_info, but as a builder method
        pub fn with_accessibility_info(&mut self, accessibility_info: AccessibilityInfo)  -> crate::dom::Dom { unsafe { crate::dll::AzDom_withAccessibilityInfo(self, accessibility_info) } }
        /// Sets the menu bar for the DOM root node. See `NodeData::set_menu_bar` for more information.
        pub fn set_menu_bar(&mut self, menu_bar: Menu)  { unsafe { crate::dll::AzDom_setMenuBar(self, menu_bar) } }
        /// Same as set_accessibility_info, but as a builder method
        pub fn with_menu_bar(&mut self, menu_bar: Menu)  -> crate::dom::Dom { unsafe { crate::dll::AzDom_withMenuBar(self, menu_bar) } }
        /// Sets the context menu for the DOM root node. See `NodeData::set_context_menu` for more information.
        pub fn set_context_menu(&mut self, context_menu: Menu)  { unsafe { crate::dll::AzDom_setContextMenu(self, context_menu) } }
        /// Same as set_context_menu, but as a builder method
        pub fn with_context_menu(&mut self, context_menu: Menu)  -> crate::dom::Dom { unsafe { crate::dll::AzDom_withContextMenu(self, context_menu) } }
        /// Calculates the hash of this node (note: in order to be truly unique, you also have to hash the DOM and Node ID).
        pub fn hash(&self)  -> u64 { unsafe { crate::dll::AzDom_hash(self) } }
        /// Returns the number of nodes in the DOM, including all child DOM trees. Result is equal to `self.total_children + 1` (count of all child trees + the root node)
        pub fn node_count(&self)  -> usize { unsafe { crate::dll::AzDom_nodeCount(self) } }
        /// Returns a HTML for unit testing
        pub fn get_html_string_test(&mut self)  -> crate::str::String { unsafe { crate::dll::AzDom_getHtmlStringTest(self) } }
        /// Returns a HTML string that you can write to a file in order to debug the UI structure and debug potential cascading issues
        pub fn get_html_string_debug(&mut self)  -> crate::str::String { unsafe { crate::dll::AzDom_getHtmlStringDebug(self) } }
        /// Same as `StyledDom::new(dom, css)`: NOTE - replaces self with an empty DOM, in order to prevent cloning the DOM entirely
        pub fn style(&mut self, css: Css)  -> crate::style::StyledDom { unsafe { crate::dll::AzDom_style(self, css) } }
    }

    /// `IFrameNode` struct
    
#[doc(inline)] pub use crate::dll::AzIFrameNode as IFrameNode;
    /// `CallbackData` struct
    
#[doc(inline)] pub use crate::dll::AzCallbackData as CallbackData;
    /// Represents one single DOM node (node type, classes, ids and callbacks are stored here)
    
#[doc(inline)] pub use crate::dll::AzNodeData as NodeData;
    impl NodeData {
        /// Creates an new, empty `NodeData` struct
        pub fn new(node_type: NodeType) -> Self { unsafe { crate::dll::AzNodeData_new(node_type) } }
        /// Creates a new `NodeData` instance.
        pub fn body() -> Self { unsafe { crate::dll::AzNodeData_body() } }
        /// Creates a new `NodeData` instance.
        pub fn div() -> Self { unsafe { crate::dll::AzNodeData_div() } }
        /// Creates a new `NodeData` instance.
        pub fn br() -> Self { unsafe { crate::dll::AzNodeData_br() } }
        /// Creates a new `NodeData` instance.
        pub fn text(string: String) -> Self { unsafe { crate::dll::AzNodeData_text(string) } }
        /// Creates a new `NodeData` instance.
        pub fn image(image: ImageRef) -> Self { unsafe { crate::dll::AzNodeData_image(image) } }
        /// Creates a new `NodeData` instance.
        pub fn iframe(data: RefAny, callback: IFrameCallbackType) -> Self { unsafe { crate::dll::AzNodeData_iframe(data, callback) } }
        /// Calls the `NodeData::set_node_type` function.
        pub fn set_node_type(&mut self, node_type: NodeType)  { unsafe { crate::dll::AzNodeData_setNodeType(self, node_type) } }
        /// Calls the `NodeData::with_node_type` function.
        pub fn with_node_type(&mut self, node_type: NodeType)  -> crate::dom::NodeData { unsafe { crate::dll::AzNodeData_withNodeType(self, node_type) } }
        /// Calls the `NodeData::set_dataset` function.
        pub fn set_dataset(&mut self, dataset: RefAny)  { unsafe { crate::dll::AzNodeData_setDataset(self, dataset) } }
        /// Calls the `NodeData::with_dataset` function.
        pub fn with_dataset(&mut self, dataset: RefAny)  -> crate::dom::NodeData { unsafe { crate::dll::AzNodeData_withDataset(self, dataset) } }
        /// Calls the `NodeData::set_ids_and_classes` function.
        pub fn set_ids_and_classes(&mut self, ids_and_classes: IdOrClassVec)  { unsafe { crate::dll::AzNodeData_setIdsAndClasses(self, ids_and_classes) } }
        /// Calls the `NodeData::with_ids_and_classes` function.
        pub fn with_ids_and_classes(&mut self, ids_and_classes: IdOrClassVec)  -> crate::dom::NodeData { unsafe { crate::dll::AzNodeData_withIdsAndClasses(self, ids_and_classes) } }
        /// Calls the `NodeData::set_callbacks` function.
        pub fn set_callbacks(&mut self, callbacks: CallbackDataVec)  { unsafe { crate::dll::AzNodeData_setCallbacks(self, callbacks) } }
        /// Calls the `NodeData::with_callbacks` function.
        pub fn with_callbacks(&mut self, callbacks: CallbackDataVec)  -> crate::dom::NodeData { unsafe { crate::dll::AzNodeData_withCallbacks(self, callbacks) } }
        /// Calls the `NodeData::set_inline_css_props` function.
        pub fn set_inline_css_props(&mut self, css_properties: NodeDataInlineCssPropertyVec)  { unsafe { crate::dll::AzNodeData_setInlineCssProps(self, css_properties) } }
        /// Calls the `NodeData::with_inline_css_props` function.
        pub fn with_inline_css_props(&mut self, css_properties: NodeDataInlineCssPropertyVec)  -> crate::dom::NodeData { unsafe { crate::dll::AzNodeData_withInlineCssProps(self, css_properties) } }
        /// Sets the `extra.clip_mask` field for this node
        pub fn set_clip_mask(&mut self, image_mask: ImageMask)  { unsafe { crate::dll::AzNodeData_setClipMask(self, image_mask) } }
        /// Sets the tab index for this node
        pub fn set_tab_index(&mut self, tab_index: TabIndex)  { unsafe { crate::dll::AzNodeData_setTabIndex(self, tab_index) } }
        /// Sets accessibility attributes for this node
        pub fn set_accessibility_info(&mut self, accessibility_info: AccessibilityInfo)  { unsafe { crate::dll::AzNodeData_setAccessibilityInfo(self, accessibility_info) } }
        /// Adds a (native) menu bar: If this node is the root node the menu bar will be added to the window, else it will be displayed using the width and position of the bounding rectangle
        pub fn set_menu_bar(&mut self, menu_bar: Menu)  { unsafe { crate::dll::AzNodeData_setMenuBar(self, menu_bar) } }
        /// Signalizes that this node has a (native) context-aware menu. If set, the user can left-click the node to open the menu
        pub fn set_context_menu(&mut self, context_menu: Menu)  { unsafe { crate::dll::AzNodeData_setContextMenu(self, context_menu) } }
        /// Calculates the hash of this node (note: in order to be truly unique, you also have to hash the DOM and Node ID).
        pub fn hash(&self)  -> u64 { unsafe { crate::dll::AzNodeData_hash(self) } }
    }

    /// List of core DOM node types built-into by `azul`
    
#[doc(inline)] pub use crate::dll::AzNodeType as NodeType;
    /// When to call a callback action - `On::MouseOver`, `On::MouseOut`, etc.
    
#[doc(inline)] pub use crate::dll::AzOn as On;
    impl On {
        /// Converts the `On` shorthand into a `EventFilter`
        pub fn into_event_filter(self)  -> crate::dom::EventFilter { unsafe { crate::dll::AzOn_intoEventFilter(self) } }
    }

    /// `EventFilter` struct
    
#[doc(inline)] pub use crate::dll::AzEventFilter as EventFilter;
    /// `HoverEventFilter` struct
    
#[doc(inline)] pub use crate::dll::AzHoverEventFilter as HoverEventFilter;
    /// `FocusEventFilter` struct
    
#[doc(inline)] pub use crate::dll::AzFocusEventFilter as FocusEventFilter;
    /// `NotEventFilter` struct
    
#[doc(inline)] pub use crate::dll::AzNotEventFilter as NotEventFilter;
    /// `WindowEventFilter` struct
    
#[doc(inline)] pub use crate::dll::AzWindowEventFilter as WindowEventFilter;
    /// `ComponentEventFilter` struct
    
#[doc(inline)] pub use crate::dll::AzComponentEventFilter as ComponentEventFilter;
    /// `ApplicationEventFilter` struct
    
#[doc(inline)] pub use crate::dll::AzApplicationEventFilter as ApplicationEventFilter;
    /// Accessibility information (MSAA wrapper). See `NodeData.set_accessibility_info()`
    
#[doc(inline)] pub use crate::dll::AzAccessibilityInfo as AccessibilityInfo;
    /// MSAA Accessibility role constants. For information on what each role does, see the <a href="https://docs.microsoft.com/en-us/windows/win32/winauto/object-roles">MSDN Role Constants page</a>
    
#[doc(inline)] pub use crate::dll::AzAccessibilityRole as AccessibilityRole;
    /// MSAA accessibility state. For information on what each state does, see the <a href="https://docs.microsoft.com/en-us/windows/win32/winauto/object-state-constants">MSDN State Constants page</a>.
    
#[doc(inline)] pub use crate::dll::AzAccessibilityState as AccessibilityState;
    /// `TabIndex` struct
    
#[doc(inline)] pub use crate::dll::AzTabIndex as TabIndex;
    /// `IdOrClass` struct
    
#[doc(inline)] pub use crate::dll::AzIdOrClass as IdOrClass;
    /// `NodeDataInlineCssProperty` struct
    
#[doc(inline)] pub use crate::dll::AzNodeDataInlineCssProperty as NodeDataInlineCssProperty;
}

pub mod menu {
    #![allow(dead_code, unused_imports)]
    use crate::dll::*;
    use core::ffi::c_void;
    /// Menu struct (application / window menu, dropdown menu, context menu). Modeled after the Windows API
    
#[doc(inline)] pub use crate::dll::AzMenu as Menu;
    /// Item entry in a menu or menu bar
    
#[doc(inline)] pub use crate::dll::AzMenuItem as MenuItem;
    /// Regular labeled menu item
    
#[doc(inline)] pub use crate::dll::AzStringMenuItem as StringMenuItem;
    /// Combination of virtual key codes that have to be pressed together
    
#[doc(inline)] pub use crate::dll::AzVirtualKeyCodeCombo as VirtualKeyCodeCombo;
    /// Similar to `dom.CallbackData`, stores some data + a callback to call when the menu is activated
    
#[doc(inline)] pub use crate::dll::AzMenuCallback as MenuCallback;
    /// Icon of a menu entry
    
#[doc(inline)] pub use crate::dll::AzMenuItemIcon as MenuItemIcon;
    /// Describes the state of a menu item
    
#[doc(inline)] pub use crate::dll::AzMenuItemState as MenuItemState;
}

pub mod css {
    #![allow(dead_code, unused_imports)]
    //! `Css` parsing module
    use crate::dll::*;
    use core::ffi::c_void;
    use crate::vec::{
        StyleBackgroundPositionVec,
        StyleBackgroundContentVec,
        StyleBackgroundSizeVec,
        StyleBackgroundRepeatVec,
        StyleTransformVec,
        StyleFontFamilyVec,
    };

    macro_rules! css_property_from_type {($prop_type:expr, $content_type:ident) => ({
        match $prop_type {
            CssPropertyType::TextColor => CssProperty::TextColor(StyleTextColorValue::$content_type),
            CssPropertyType::FontSize => CssProperty::FontSize(StyleFontSizeValue::$content_type),
            CssPropertyType::FontFamily => CssProperty::FontFamily(StyleFontFamilyVecValue::$content_type),
            CssPropertyType::TextAlign => CssProperty::TextAlign(StyleTextAlignValue::$content_type),
            CssPropertyType::LetterSpacing => CssProperty::LetterSpacing(StyleLetterSpacingValue::$content_type),
            CssPropertyType::LineHeight => CssProperty::LineHeight(StyleLineHeightValue::$content_type),
            CssPropertyType::WordSpacing => CssProperty::WordSpacing(StyleWordSpacingValue::$content_type),
            CssPropertyType::TabWidth => CssProperty::TabWidth(StyleTabWidthValue::$content_type),
            CssPropertyType::Cursor => CssProperty::Cursor(StyleCursorValue::$content_type),
            CssPropertyType::Display => CssProperty::Display(LayoutDisplayValue::$content_type),
            CssPropertyType::Float => CssProperty::Float(LayoutFloatValue::$content_type),
            CssPropertyType::BoxSizing => CssProperty::BoxSizing(LayoutBoxSizingValue::$content_type),
            CssPropertyType::Width => CssProperty::Width(LayoutWidthValue::$content_type),
            CssPropertyType::Height => CssProperty::Height(LayoutHeightValue::$content_type),
            CssPropertyType::MinWidth => CssProperty::MinWidth(LayoutMinWidthValue::$content_type),
            CssPropertyType::MinHeight => CssProperty::MinHeight(LayoutMinHeightValue::$content_type),
            CssPropertyType::MaxWidth => CssProperty::MaxWidth(LayoutMaxWidthValue::$content_type),
            CssPropertyType::MaxHeight => CssProperty::MaxHeight(LayoutMaxHeightValue::$content_type),
            CssPropertyType::Position => CssProperty::Position(LayoutPositionValue::$content_type),
            CssPropertyType::Top => CssProperty::Top(LayoutTopValue::$content_type),
            CssPropertyType::Right => CssProperty::Right(LayoutRightValue::$content_type),
            CssPropertyType::Left => CssProperty::Left(LayoutLeftValue::$content_type),
            CssPropertyType::Bottom => CssProperty::Bottom(LayoutBottomValue::$content_type),
            CssPropertyType::FlexWrap => CssProperty::FlexWrap(LayoutFlexWrapValue::$content_type),
            CssPropertyType::FlexDirection => CssProperty::FlexDirection(LayoutFlexDirectionValue::$content_type),
            CssPropertyType::FlexGrow => CssProperty::FlexGrow(LayoutFlexGrowValue::$content_type),
            CssPropertyType::FlexShrink => CssProperty::FlexShrink(LayoutFlexShrinkValue::$content_type),
            CssPropertyType::JustifyContent => CssProperty::JustifyContent(LayoutJustifyContentValue::$content_type),
            CssPropertyType::AlignItems => CssProperty::AlignItems(LayoutAlignItemsValue::$content_type),
            CssPropertyType::AlignContent => CssProperty::AlignContent(LayoutAlignContentValue::$content_type),
            CssPropertyType::Background => CssProperty::BackgroundContent(StyleBackgroundContentVecValue::$content_type),
            CssPropertyType::BackgroundImage => CssProperty::BackgroundContent(StyleBackgroundContentVecValue::$content_type),
            CssPropertyType::BackgroundColor => CssProperty::BackgroundContent(StyleBackgroundContentVecValue::$content_type),
            CssPropertyType::BackgroundPosition => CssProperty::BackgroundPosition(StyleBackgroundPositionVecValue::$content_type),
            CssPropertyType::BackgroundSize => CssProperty::BackgroundSize(StyleBackgroundSizeVecValue::$content_type),
            CssPropertyType::BackgroundRepeat => CssProperty::BackgroundRepeat(StyleBackgroundRepeatVecValue::$content_type),
            CssPropertyType::OverflowX => CssProperty::OverflowX(LayoutOverflowValue::$content_type),
            CssPropertyType::OverflowY => CssProperty::OverflowY(LayoutOverflowValue::$content_type),
            CssPropertyType::PaddingTop => CssProperty::PaddingTop(LayoutPaddingTopValue::$content_type),
            CssPropertyType::PaddingLeft => CssProperty::PaddingLeft(LayoutPaddingLeftValue::$content_type),
            CssPropertyType::PaddingRight => CssProperty::PaddingRight(LayoutPaddingRightValue::$content_type),
            CssPropertyType::PaddingBottom => CssProperty::PaddingBottom(LayoutPaddingBottomValue::$content_type),
            CssPropertyType::MarginTop => CssProperty::MarginTop(LayoutMarginTopValue::$content_type),
            CssPropertyType::MarginLeft => CssProperty::MarginLeft(LayoutMarginLeftValue::$content_type),
            CssPropertyType::MarginRight => CssProperty::MarginRight(LayoutMarginRightValue::$content_type),
            CssPropertyType::MarginBottom => CssProperty::MarginBottom(LayoutMarginBottomValue::$content_type),
            CssPropertyType::BorderTopLeftRadius => CssProperty::BorderTopLeftRadius(StyleBorderTopLeftRadiusValue::$content_type),
            CssPropertyType::BorderTopRightRadius => CssProperty::BorderTopRightRadius(StyleBorderTopRightRadiusValue::$content_type),
            CssPropertyType::BorderBottomLeftRadius => CssProperty::BorderBottomLeftRadius(StyleBorderBottomLeftRadiusValue::$content_type),
            CssPropertyType::BorderBottomRightRadius => CssProperty::BorderBottomRightRadius(StyleBorderBottomRightRadiusValue::$content_type),
            CssPropertyType::BorderTopColor => CssProperty::BorderTopColor(StyleBorderTopColorValue::$content_type),
            CssPropertyType::BorderRightColor => CssProperty::BorderRightColor(StyleBorderRightColorValue::$content_type),
            CssPropertyType::BorderLeftColor => CssProperty::BorderLeftColor(StyleBorderLeftColorValue::$content_type),
            CssPropertyType::BorderBottomColor => CssProperty::BorderBottomColor(StyleBorderBottomColorValue::$content_type),
            CssPropertyType::BorderTopStyle => CssProperty::BorderTopStyle(StyleBorderTopStyleValue::$content_type),
            CssPropertyType::BorderRightStyle => CssProperty::BorderRightStyle(StyleBorderRightStyleValue::$content_type),
            CssPropertyType::BorderLeftStyle => CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::$content_type),
            CssPropertyType::BorderBottomStyle => CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::$content_type),
            CssPropertyType::BorderTopWidth => CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::$content_type),
            CssPropertyType::BorderRightWidth => CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::$content_type),
            CssPropertyType::BorderLeftWidth => CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::$content_type),
            CssPropertyType::BorderBottomWidth => CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::$content_type),
            CssPropertyType::BoxShadowLeft => CssProperty::BoxShadowLeft(StyleBoxShadowValue::$content_type),
            CssPropertyType::BoxShadowRight => CssProperty::BoxShadowRight(StyleBoxShadowValue::$content_type),
            CssPropertyType::BoxShadowTop => CssProperty::BoxShadowTop(StyleBoxShadowValue::$content_type),
            CssPropertyType::BoxShadowBottom => CssProperty::BoxShadowBottom(StyleBoxShadowValue::$content_type),
            CssPropertyType::ScrollbarStyle => CssProperty::ScrollbarStyle(ScrollbarStyleValue::$content_type),
            CssPropertyType::Opacity => CssProperty::Opacity(StyleOpacityValue::$content_type),
            CssPropertyType::Transform => CssProperty::Transform(StyleTransformVecValue::$content_type),
            CssPropertyType::PerspectiveOrigin => CssProperty::PerspectiveOrigin(StylePerspectiveOriginValue::$content_type),
            CssPropertyType::TransformOrigin => CssProperty::TransformOrigin(StyleTransformOriginValue::$content_type),
            CssPropertyType::BackfaceVisibility => CssProperty::BackfaceVisibility(StyleBackfaceVisibilityValue::$content_type),
        }
    })}

    impl CssProperty {

        /// Return the type (key) of this property as a statically typed enum
        pub const fn get_type(&self) -> CssPropertyType {
            match &self {
                CssProperty::TextColor(_) => CssPropertyType::TextColor,
                CssProperty::FontSize(_) => CssPropertyType::FontSize,
                CssProperty::FontFamily(_) => CssPropertyType::FontFamily,
                CssProperty::TextAlign(_) => CssPropertyType::TextAlign,
                CssProperty::LetterSpacing(_) => CssPropertyType::LetterSpacing,
                CssProperty::LineHeight(_) => CssPropertyType::LineHeight,
                CssProperty::WordSpacing(_) => CssPropertyType::WordSpacing,
                CssProperty::TabWidth(_) => CssPropertyType::TabWidth,
                CssProperty::Cursor(_) => CssPropertyType::Cursor,
                CssProperty::Display(_) => CssPropertyType::Display,
                CssProperty::Float(_) => CssPropertyType::Float,
                CssProperty::BoxSizing(_) => CssPropertyType::BoxSizing,
                CssProperty::Width(_) => CssPropertyType::Width,
                CssProperty::Height(_) => CssPropertyType::Height,
                CssProperty::MinWidth(_) => CssPropertyType::MinWidth,
                CssProperty::MinHeight(_) => CssPropertyType::MinHeight,
                CssProperty::MaxWidth(_) => CssPropertyType::MaxWidth,
                CssProperty::MaxHeight(_) => CssPropertyType::MaxHeight,
                CssProperty::Position(_) => CssPropertyType::Position,
                CssProperty::Top(_) => CssPropertyType::Top,
                CssProperty::Right(_) => CssPropertyType::Right,
                CssProperty::Left(_) => CssPropertyType::Left,
                CssProperty::Bottom(_) => CssPropertyType::Bottom,
                CssProperty::FlexWrap(_) => CssPropertyType::FlexWrap,
                CssProperty::FlexDirection(_) => CssPropertyType::FlexDirection,
                CssProperty::FlexGrow(_) => CssPropertyType::FlexGrow,
                CssProperty::FlexShrink(_) => CssPropertyType::FlexShrink,
                CssProperty::JustifyContent(_) => CssPropertyType::JustifyContent,
                CssProperty::AlignItems(_) => CssPropertyType::AlignItems,
                CssProperty::AlignContent(_) => CssPropertyType::AlignContent,
                CssProperty::BackgroundContent(_) => CssPropertyType::BackgroundImage, // TODO: wrong!
                CssProperty::BackgroundPosition(_) => CssPropertyType::BackgroundPosition,
                CssProperty::BackgroundSize(_) => CssPropertyType::BackgroundSize,
                CssProperty::BackgroundRepeat(_) => CssPropertyType::BackgroundRepeat,
                CssProperty::OverflowX(_) => CssPropertyType::OverflowX,
                CssProperty::OverflowY(_) => CssPropertyType::OverflowY,
                CssProperty::PaddingTop(_) => CssPropertyType::PaddingTop,
                CssProperty::PaddingLeft(_) => CssPropertyType::PaddingLeft,
                CssProperty::PaddingRight(_) => CssPropertyType::PaddingRight,
                CssProperty::PaddingBottom(_) => CssPropertyType::PaddingBottom,
                CssProperty::MarginTop(_) => CssPropertyType::MarginTop,
                CssProperty::MarginLeft(_) => CssPropertyType::MarginLeft,
                CssProperty::MarginRight(_) => CssPropertyType::MarginRight,
                CssProperty::MarginBottom(_) => CssPropertyType::MarginBottom,
                CssProperty::BorderTopLeftRadius(_) => CssPropertyType::BorderTopLeftRadius,
                CssProperty::BorderTopRightRadius(_) => CssPropertyType::BorderTopRightRadius,
                CssProperty::BorderBottomLeftRadius(_) => CssPropertyType::BorderBottomLeftRadius,
                CssProperty::BorderBottomRightRadius(_) => CssPropertyType::BorderBottomRightRadius,
                CssProperty::BorderTopColor(_) => CssPropertyType::BorderTopColor,
                CssProperty::BorderRightColor(_) => CssPropertyType::BorderRightColor,
                CssProperty::BorderLeftColor(_) => CssPropertyType::BorderLeftColor,
                CssProperty::BorderBottomColor(_) => CssPropertyType::BorderBottomColor,
                CssProperty::BorderTopStyle(_) => CssPropertyType::BorderTopStyle,
                CssProperty::BorderRightStyle(_) => CssPropertyType::BorderRightStyle,
                CssProperty::BorderLeftStyle(_) => CssPropertyType::BorderLeftStyle,
                CssProperty::BorderBottomStyle(_) => CssPropertyType::BorderBottomStyle,
                CssProperty::BorderTopWidth(_) => CssPropertyType::BorderTopWidth,
                CssProperty::BorderRightWidth(_) => CssPropertyType::BorderRightWidth,
                CssProperty::BorderLeftWidth(_) => CssPropertyType::BorderLeftWidth,
                CssProperty::BorderBottomWidth(_) => CssPropertyType::BorderBottomWidth,
                CssProperty::BoxShadowLeft(_) => CssPropertyType::BoxShadowLeft,
                CssProperty::BoxShadowRight(_) => CssPropertyType::BoxShadowRight,
                CssProperty::BoxShadowTop(_) => CssPropertyType::BoxShadowTop,
                CssProperty::BoxShadowBottom(_) => CssPropertyType::BoxShadowBottom,
                CssProperty::ScrollbarStyle(_) => CssPropertyType::ScrollbarStyle,
                CssProperty::Opacity(_) => CssPropertyType::Opacity,
                CssProperty::Transform(_) => CssPropertyType::Transform,
                CssProperty::PerspectiveOrigin(_) => CssPropertyType::PerspectiveOrigin,
                CssProperty::TransformOrigin(_) => CssPropertyType::TransformOrigin,
                CssProperty::BackfaceVisibility(_) => CssPropertyType::BackfaceVisibility,
            }
        }

        // const constructors for easier API access

        pub const fn const_none(prop_type: CssPropertyType) -> Self { css_property_from_type!(prop_type, None) }
        pub const fn const_auto(prop_type: CssPropertyType) -> Self { css_property_from_type!(prop_type, Auto) }
        pub const fn const_initial(prop_type: CssPropertyType) -> Self { css_property_from_type!(prop_type, Initial) }
        pub const fn const_inherit(prop_type: CssPropertyType) -> Self { css_property_from_type!(prop_type, Inherit) }

        pub const fn const_text_color(input: StyleTextColor) -> Self { CssProperty::TextColor(StyleTextColorValue::Exact(input)) }
        pub const fn const_font_size(input: StyleFontSize) -> Self { CssProperty::FontSize(StyleFontSizeValue::Exact(input)) }
        pub const fn const_font_family(input: StyleFontFamilyVec) -> Self { CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(input)) }
        pub const fn const_text_align(input: StyleTextAlign) -> Self { CssProperty::TextAlign(StyleTextAlignValue::Exact(input)) }
        pub const fn const_letter_spacing(input: StyleLetterSpacing) -> Self { CssProperty::LetterSpacing(StyleLetterSpacingValue::Exact(input)) }
        pub const fn const_line_height(input: StyleLineHeight) -> Self { CssProperty::LineHeight(StyleLineHeightValue::Exact(input)) }
        pub const fn const_word_spacing(input: StyleWordSpacing) -> Self { CssProperty::WordSpacing(StyleWordSpacingValue::Exact(input)) }
        pub const fn const_tab_width(input: StyleTabWidth) -> Self { CssProperty::TabWidth(StyleTabWidthValue::Exact(input)) }
        pub const fn const_cursor(input: StyleCursor) -> Self { CssProperty::Cursor(StyleCursorValue::Exact(input)) }
        pub const fn const_display(input: LayoutDisplay) -> Self { CssProperty::Display(LayoutDisplayValue::Exact(input)) }
        pub const fn const_float(input: LayoutFloat) -> Self { CssProperty::Float(LayoutFloatValue::Exact(input)) }
        pub const fn const_box_sizing(input: LayoutBoxSizing) -> Self { CssProperty::BoxSizing(LayoutBoxSizingValue::Exact(input)) }
        pub const fn const_width(input: LayoutWidth) -> Self { CssProperty::Width(LayoutWidthValue::Exact(input)) }
        pub const fn const_height(input: LayoutHeight) -> Self { CssProperty::Height(LayoutHeightValue::Exact(input)) }
        pub const fn const_min_width(input: LayoutMinWidth) -> Self { CssProperty::MinWidth(LayoutMinWidthValue::Exact(input)) }
        pub const fn const_min_height(input: LayoutMinHeight) -> Self { CssProperty::MinHeight(LayoutMinHeightValue::Exact(input)) }
        pub const fn const_max_width(input: LayoutMaxWidth) -> Self { CssProperty::MaxWidth(LayoutMaxWidthValue::Exact(input)) }
        pub const fn const_max_height(input: LayoutMaxHeight) -> Self { CssProperty::MaxHeight(LayoutMaxHeightValue::Exact(input)) }
        pub const fn const_position(input: LayoutPosition) -> Self { CssProperty::Position(LayoutPositionValue::Exact(input)) }
        pub const fn const_top(input: LayoutTop) -> Self { CssProperty::Top(LayoutTopValue::Exact(input)) }
        pub const fn const_right(input: LayoutRight) -> Self { CssProperty::Right(LayoutRightValue::Exact(input)) }
        pub const fn const_left(input: LayoutLeft) -> Self { CssProperty::Left(LayoutLeftValue::Exact(input)) }
        pub const fn const_bottom(input: LayoutBottom) -> Self { CssProperty::Bottom(LayoutBottomValue::Exact(input)) }
        pub const fn const_flex_wrap(input: LayoutFlexWrap) -> Self { CssProperty::FlexWrap(LayoutFlexWrapValue::Exact(input)) }
        pub const fn const_flex_direction(input: LayoutFlexDirection) -> Self { CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(input)) }
        pub const fn const_flex_grow(input: LayoutFlexGrow) -> Self { CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(input)) }
        pub const fn const_flex_shrink(input: LayoutFlexShrink) -> Self { CssProperty::FlexShrink(LayoutFlexShrinkValue::Exact(input)) }
        pub const fn const_justify_content(input: LayoutJustifyContent) -> Self { CssProperty::JustifyContent(LayoutJustifyContentValue::Exact(input)) }
        pub const fn const_align_items(input: LayoutAlignItems) -> Self { CssProperty::AlignItems(LayoutAlignItemsValue::Exact(input)) }
        pub const fn const_align_content(input: LayoutAlignContent) -> Self { CssProperty::AlignContent(LayoutAlignContentValue::Exact(input)) }
        pub const fn const_background_content(input: StyleBackgroundContentVec) -> Self { CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(input)) }
        pub const fn const_background_position(input: StyleBackgroundPositionVec) -> Self { CssProperty::BackgroundPosition(StyleBackgroundPositionVecValue::Exact(input)) }
        pub const fn const_background_size(input: StyleBackgroundSizeVec) -> Self { CssProperty::BackgroundSize(StyleBackgroundSizeVecValue::Exact(input)) }
        pub const fn const_background_repeat(input: StyleBackgroundRepeatVec) -> Self { CssProperty::BackgroundRepeat(StyleBackgroundRepeatVecValue::Exact(input)) }
        pub const fn const_overflow_x(input: LayoutOverflow) -> Self { CssProperty::OverflowX(LayoutOverflowValue::Exact(input)) }
        pub const fn const_overflow_y(input: LayoutOverflow) -> Self { CssProperty::OverflowY(LayoutOverflowValue::Exact(input)) }
        pub const fn const_padding_top(input: LayoutPaddingTop) -> Self { CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(input)) }
        pub const fn const_padding_left(input: LayoutPaddingLeft) -> Self { CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(input)) }
        pub const fn const_padding_right(input: LayoutPaddingRight) -> Self { CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(input)) }
        pub const fn const_padding_bottom(input: LayoutPaddingBottom) -> Self { CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(input)) }
        pub const fn const_margin_top(input: LayoutMarginTop) -> Self { CssProperty::MarginTop(LayoutMarginTopValue::Exact(input)) }
        pub const fn const_margin_left(input: LayoutMarginLeft) -> Self { CssProperty::MarginLeft(LayoutMarginLeftValue::Exact(input)) }
        pub const fn const_margin_right(input: LayoutMarginRight) -> Self { CssProperty::MarginRight(LayoutMarginRightValue::Exact(input)) }
        pub const fn const_margin_bottom(input: LayoutMarginBottom) -> Self { CssProperty::MarginBottom(LayoutMarginBottomValue::Exact(input)) }
        pub const fn const_border_top_left_radius(input: StyleBorderTopLeftRadius) -> Self { CssProperty::BorderTopLeftRadius(StyleBorderTopLeftRadiusValue::Exact(input)) }
        pub const fn const_border_top_right_radius(input: StyleBorderTopRightRadius) -> Self { CssProperty::BorderTopRightRadius(StyleBorderTopRightRadiusValue::Exact(input)) }
        pub const fn const_border_bottom_left_radius(input: StyleBorderBottomLeftRadius) -> Self { CssProperty::BorderBottomLeftRadius(StyleBorderBottomLeftRadiusValue::Exact(input)) }
        pub const fn const_border_bottom_right_radius(input: StyleBorderBottomRightRadius) -> Self { CssProperty::BorderBottomRightRadius(StyleBorderBottomRightRadiusValue::Exact(input)) }
        pub const fn const_border_top_color(input: StyleBorderTopColor) -> Self { CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(input)) }
        pub const fn const_border_right_color(input: StyleBorderRightColor) -> Self { CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(input)) }
        pub const fn const_border_left_color(input: StyleBorderLeftColor) -> Self { CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(input)) }
        pub const fn const_border_bottom_color(input: StyleBorderBottomColor) -> Self { CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(input)) }
        pub const fn const_border_top_style(input: StyleBorderTopStyle) -> Self { CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(input)) }
        pub const fn const_border_right_style(input: StyleBorderRightStyle) -> Self { CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(input)) }
        pub const fn const_border_left_style(input: StyleBorderLeftStyle) -> Self { CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(input)) }
        pub const fn const_border_bottom_style(input: StyleBorderBottomStyle) -> Self { CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(input)) }
        pub const fn const_border_top_width(input: LayoutBorderTopWidth) -> Self { CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(input)) }
        pub const fn const_border_right_width(input: LayoutBorderRightWidth) -> Self { CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(input)) }
        pub const fn const_border_left_width(input: LayoutBorderLeftWidth) -> Self { CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(input)) }
        pub const fn const_border_bottom_width(input: LayoutBorderBottomWidth) -> Self { CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(input)) }
        pub const fn const_box_shadow_left(input: StyleBoxShadow) -> Self { CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(input)) }
        pub const fn const_box_shadow_right(input: StyleBoxShadow) -> Self { CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(input)) }
        pub const fn const_box_shadow_top(input: StyleBoxShadow) -> Self { CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(input)) }
        pub const fn const_box_shadow_bottom(input: StyleBoxShadow) -> Self { CssProperty::BoxShadowBottom(StyleBoxShadowValue::Exact(input)) }
        pub const fn const_opacity(input: StyleOpacity) -> Self { CssProperty::Opacity(StyleOpacityValue::Exact(input)) }
        pub const fn const_transform(input: StyleTransformVec) -> Self { CssProperty::Transform(StyleTransformVecValue::Exact(input)) }
        pub const fn const_transform_origin(input: StyleTransformOrigin) -> Self { CssProperty::TransformOrigin(StyleTransformOriginValue::Exact(input)) }
        pub const fn const_perspective_origin(input: StylePerspectiveOrigin) -> Self { CssProperty::PerspectiveOrigin(StylePerspectiveOriginValue::Exact(input)) }
        pub const fn const_backface_visiblity(input: StyleBackfaceVisibility) -> Self { CssProperty::BackfaceVisibility(StyleBackfaceVisibilityValue::Exact(input)) }
    }

    const FP_PRECISION_MULTIPLIER: f32 = 1000.0;
    const FP_PRECISION_MULTIPLIER_CONST: isize = FP_PRECISION_MULTIPLIER as isize;

    impl FloatValue {
        /// Same as `FloatValue::new()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        pub const fn const_new(value: isize)  -> Self {
            Self { number: value * FP_PRECISION_MULTIPLIER_CONST }
        }

        pub fn new(value: f32) -> Self {
            Self { number: (value * FP_PRECISION_MULTIPLIER) as isize }
        }

        pub fn get(&self) -> f32 {
            self.number as f32 / FP_PRECISION_MULTIPLIER
        }
    }

    impl From<f32> for FloatValue {
        fn from(val: f32) -> Self {
            Self::new(val)
        }
    }

    impl AngleValue {

        #[inline]
        pub const fn zero() -> Self {
            const ZERO_DEG: AngleValue = AngleValue::const_deg(0);
            ZERO_DEG
        }

        /// Same as `PixelValue::px()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_deg(value: isize) -> Self {
            Self::const_from_metric(AngleMetric::Degree, value)
        }

        /// Same as `PixelValue::em()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_rad(value: isize) -> Self {
            Self::const_from_metric(AngleMetric::Radians, value)
        }

        /// Same as `PixelValue::pt()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_grad(value: isize) -> Self {
            Self::const_from_metric(AngleMetric::Grad, value)
        }

        /// Same as `PixelValue::pt()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_turn(value: isize) -> Self {
            Self::const_from_metric(AngleMetric::Turn, value)
        }

        #[inline]
        pub fn const_percent(value: isize) -> Self {
            Self::const_from_metric(AngleMetric::Percent, value)
        }

        #[inline]
        pub const fn const_from_metric(metric: AngleMetric, value: isize) -> Self {
            Self {
                metric: metric,
                number: FloatValue::const_new(value),
            }
        }

        #[inline]
        pub fn deg(value: f32) -> Self {
            Self::from_metric(AngleMetric::Degree, value)
        }

        #[inline]
        pub fn rad(value: f32) -> Self {
            Self::from_metric(AngleMetric::Radians, value)
        }

        #[inline]
        pub fn grad(value: f32) -> Self {
            Self::from_metric(AngleMetric::Grad, value)
        }

        #[inline]
        pub fn turn(value: f32) -> Self {
            Self::from_metric(AngleMetric::Turn, value)
        }

        #[inline]
        pub fn percent(value: f32) -> Self {
            Self::from_metric(AngleMetric::Percent, value)
        }

        #[inline]
        pub fn from_metric(metric: AngleMetric, value: f32) -> Self {
            Self {
                metric: metric,
                number: FloatValue::new(value),
            }
        }
    }

    impl PixelValue {

        #[inline]
        pub const fn zero() -> Self {
            const ZERO_PX: PixelValue = PixelValue::const_px(0);
            ZERO_PX
        }

        /// Same as `PixelValue::px()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_px(value: isize) -> Self {
            Self::const_from_metric(SizeMetric::Px, value)
        }

        /// Same as `PixelValue::em()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_em(value: isize) -> Self {
            Self::const_from_metric(SizeMetric::Em, value)
        }

        /// Same as `PixelValue::pt()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_pt(value: isize) -> Self {
            Self::const_from_metric(SizeMetric::Pt, value)
        }

        /// Same as `PixelValue::pt()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_percent(value: isize) -> Self {
            Self::const_from_metric(SizeMetric::Percent, value)
        }

        #[inline]
        pub const fn const_from_metric(metric: SizeMetric, value: isize) -> Self {
            Self {
                metric: metric,
                number: FloatValue::const_new(value),
            }
        }

        #[inline]
        pub fn px(value: f32) -> Self {
            Self::from_metric(SizeMetric::Px, value)
        }

        #[inline]
        pub fn em(value: f32) -> Self {
            Self::from_metric(SizeMetric::Em, value)
        }

        #[inline]
        pub fn pt(value: f32) -> Self {
            Self::from_metric(SizeMetric::Pt, value)
        }

        #[inline]
        pub fn percent(value: f32) -> Self {
            Self::from_metric(SizeMetric::Percent, value)
        }

        #[inline]
        pub fn from_metric(metric: SizeMetric, value: f32) -> Self {
            Self {
                metric: metric,
                number: FloatValue::new(value),
            }
        }
    }

    impl PixelValueNoPercent {

        #[inline]
        pub const fn zero() -> Self {
            Self { inner: PixelValue::zero() }
        }

        /// Same as `PixelValueNoPercent::px()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_px(value: isize) -> Self {
            Self { inner: PixelValue::const_px(value) }
        }

        /// Same as `PixelValueNoPercent::em()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_em(value: isize) -> Self {
            Self { inner: PixelValue::const_em(value) }
        }

        /// Same as `PixelValueNoPercent::pt()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_pt(value: isize) -> Self {
            Self { inner: PixelValue::const_pt(value) }
        }

        #[inline]
        const fn const_from_metric(metric: SizeMetric, value: isize) -> Self {
            Self { inner: PixelValue::const_from_metric(metric, value) }
        }

        #[inline]
        pub fn px(value: f32) -> Self {
            Self { inner: PixelValue::px(value) }
        }

        #[inline]
        pub fn em(value: f32) -> Self {
            Self { inner: PixelValue::em(value) }
        }

        #[inline]
        pub fn pt(value: f32) -> Self {
            Self { inner: PixelValue::pt(value) }
        }

        #[inline]
        fn from_metric(metric: SizeMetric, value: f32) -> Self {
            Self { inner: PixelValue::from_metric(metric, value) }
        }
    }

    impl PercentageValue {

        /// Same as `PercentageValue::new()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_new(value: isize) -> Self {
            Self { number: FloatValue::const_new(value) }
        }

        #[inline]
        pub fn new(value: f32) -> Self {
            Self { number: value.into() }
        }

        #[inline]
        pub fn get(&self) -> f32 {
            self.number.get()
        }
    }

    /// Creates `pt`, `px` and `em` constructors for any struct that has a
    /// `PixelValue` as it's self.0 field.
    macro_rules! impl_pixel_value {($struct:ident) => (

        impl $struct {

            #[inline]
            pub const fn zero() -> Self {
                Self { inner: PixelValue::zero() }
            }

            /// Same as `PixelValue::px()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            #[inline]
            pub const fn const_px(value: isize) -> Self {
                Self { inner: PixelValue::const_px(value) }
            }

            /// Same as `PixelValue::em()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            #[inline]
            pub const fn const_em(value: isize) -> Self {
                Self { inner: PixelValue::const_em(value) }
            }

            /// Same as `PixelValue::pt()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            #[inline]
            pub const fn const_pt(value: isize) -> Self {
                Self { inner: PixelValue::const_pt(value) }
            }

            /// Same as `PixelValue::pt()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            #[inline]
            pub const fn const_percent(value: isize) -> Self {
                Self { inner: PixelValue::const_percent(value) }
            }

            #[inline]
            pub const fn const_from_metric(metric: SizeMetric, value: isize) -> Self {
                Self { inner: PixelValue::const_from_metric(metric, value) }
            }

            #[inline]
            pub fn px(value: f32) -> Self {
                Self { inner: PixelValue::px(value) }
            }

            #[inline]
            pub fn em(value: f32) -> Self {
                Self { inner: PixelValue::em(value) }
            }

            #[inline]
            pub fn pt(value: f32) -> Self {
                Self { inner: PixelValue::pt(value) }
            }

            #[inline]
            pub fn percent(value: f32) -> Self {
                Self { inner: PixelValue::percent(value) }
            }

            #[inline]
            pub fn from_metric(metric: SizeMetric, value: f32) -> Self {
                Self { inner: PixelValue::from_metric(metric, value) }
            }
        }
    )}

    impl_pixel_value!(StyleBorderTopLeftRadius);
    impl_pixel_value!(StyleBorderBottomLeftRadius);
    impl_pixel_value!(StyleBorderTopRightRadius);
    impl_pixel_value!(StyleBorderBottomRightRadius);
    impl_pixel_value!(LayoutBorderTopWidth);
    impl_pixel_value!(LayoutBorderLeftWidth);
    impl_pixel_value!(LayoutBorderRightWidth);
    impl_pixel_value!(LayoutBorderBottomWidth);
    impl_pixel_value!(LayoutWidth);
    impl_pixel_value!(LayoutHeight);
    impl_pixel_value!(LayoutMinHeight);
    impl_pixel_value!(LayoutMinWidth);
    impl_pixel_value!(LayoutMaxWidth);
    impl_pixel_value!(LayoutMaxHeight);
    impl_pixel_value!(LayoutTop);
    impl_pixel_value!(LayoutBottom);
    impl_pixel_value!(LayoutRight);
    impl_pixel_value!(LayoutLeft);
    impl_pixel_value!(LayoutPaddingTop);
    impl_pixel_value!(LayoutPaddingBottom);
    impl_pixel_value!(LayoutPaddingRight);
    impl_pixel_value!(LayoutPaddingLeft);
    impl_pixel_value!(LayoutMarginTop);
    impl_pixel_value!(LayoutMarginBottom);
    impl_pixel_value!(LayoutMarginRight);
    impl_pixel_value!(LayoutMarginLeft);
    impl_pixel_value!(StyleLetterSpacing);
    impl_pixel_value!(StyleWordSpacing);
    impl_pixel_value!(StyleFontSize);

    macro_rules! impl_float_value {($struct:ident) => (
        impl $struct {
            /// Same as `FloatValue::new()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            pub const fn const_new(value: isize)  -> Self {
                Self { inner: FloatValue::const_new(value) }
            }

            pub fn new(value: f32) -> Self {
                Self { inner: FloatValue::new(value) }
            }

            pub fn get(&self) -> f32 {
                self.inner.get()
            }
        }

        impl From<f32> for $struct {
            fn from(val: f32) -> Self {
                Self { inner: FloatValue::from(val) }
            }
        }
    )}

    impl_float_value!(LayoutFlexGrow);
    impl_float_value!(LayoutFlexShrink);

    macro_rules! impl_percentage_value{($struct:ident) => (
        impl $struct {
            /// Same as `PercentageValue::new()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            #[inline]
            pub const fn const_new(value: isize) -> Self {
                Self { inner: PercentageValue::const_new(value) }
            }
        }
    )}

    impl_percentage_value!(StyleLineHeight);
    impl_percentage_value!(StyleTabWidth);
    impl_percentage_value!(StyleOpacity);
    use crate::str::String;
    /// `CssRuleBlock` struct
    
#[doc(inline)] pub use crate::dll::AzCssRuleBlock as CssRuleBlock;
    /// `CssDeclaration` struct
    
#[doc(inline)] pub use crate::dll::AzCssDeclaration as CssDeclaration;
    /// `DynamicCssProperty` struct
    
#[doc(inline)] pub use crate::dll::AzDynamicCssProperty as DynamicCssProperty;
    /// `CssPath` struct
    
#[doc(inline)] pub use crate::dll::AzCssPath as CssPath;
    /// `CssPathSelector` struct
    
#[doc(inline)] pub use crate::dll::AzCssPathSelector as CssPathSelector;
    /// `NodeTypeKey` struct
    
#[doc(inline)] pub use crate::dll::AzNodeTypeKey as NodeTypeKey;
    /// `CssPathPseudoSelector` struct
    
#[doc(inline)] pub use crate::dll::AzCssPathPseudoSelector as CssPathPseudoSelector;
    /// `CssNthChildSelector` struct
    
#[doc(inline)] pub use crate::dll::AzCssNthChildSelector as CssNthChildSelector;
    /// `CssNthChildPattern` struct
    
#[doc(inline)] pub use crate::dll::AzCssNthChildPattern as CssNthChildPattern;
    /// `Stylesheet` struct
    
#[doc(inline)] pub use crate::dll::AzStylesheet as Stylesheet;
    /// `Css` struct
    
#[doc(inline)] pub use crate::dll::AzCss as Css;
    impl Css {
        /// Returns an empty CSS style
        pub fn empty() -> Self { unsafe { crate::dll::AzCss_empty() } }
        /// Returns a CSS style parsed from a `String`
        pub fn from_string(s: String) -> Self { unsafe { crate::dll::AzCss_fromString(s) } }
    }

    /// `CssPropertyType` struct
    
#[doc(inline)] pub use crate::dll::AzCssPropertyType as CssPropertyType;
    /// `AnimationInterpolationFunction` struct
    
#[doc(inline)] pub use crate::dll::AzAnimationInterpolationFunction as AnimationInterpolationFunction;
    /// `InterpolateContext` struct
    
#[doc(inline)] pub use crate::dll::AzInterpolateContext as InterpolateContext;
    /// `ColorU` struct
    
#[doc(inline)] pub use crate::dll::AzColorU as ColorU;
    impl ColorU {
        /// Creates a new `ColorU` instance.
        pub fn from_str(string: String) -> Self { unsafe { crate::dll::AzColorU_fromStr(string) } }
        /// Calls the `ColorU::to_hash` function.
        pub fn to_hash(&self)  -> crate::str::String { unsafe { crate::dll::AzColorU_toHash(self) } }
    }

    /// `SizeMetric` struct
    
#[doc(inline)] pub use crate::dll::AzSizeMetric as SizeMetric;
    /// `FloatValue` struct
    
#[doc(inline)] pub use crate::dll::AzFloatValue as FloatValue;
    /// `PixelValue` struct
    
#[doc(inline)] pub use crate::dll::AzPixelValue as PixelValue;
    /// `PixelValueNoPercent` struct
    
#[doc(inline)] pub use crate::dll::AzPixelValueNoPercent as PixelValueNoPercent;
    /// `BoxShadowClipMode` struct
    
#[doc(inline)] pub use crate::dll::AzBoxShadowClipMode as BoxShadowClipMode;
    /// `StyleBoxShadow` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBoxShadow as StyleBoxShadow;
    /// `LayoutAlignContent` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutAlignContent as LayoutAlignContent;
    /// `LayoutAlignItems` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutAlignItems as LayoutAlignItems;
    /// `LayoutBottom` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutBottom as LayoutBottom;
    /// `LayoutBoxSizing` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutBoxSizing as LayoutBoxSizing;
    /// `LayoutFlexDirection` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutFlexDirection as LayoutFlexDirection;
    /// `LayoutDisplay` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutDisplay as LayoutDisplay;
    /// `LayoutFlexGrow` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutFlexGrow as LayoutFlexGrow;
    /// `LayoutFlexShrink` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutFlexShrink as LayoutFlexShrink;
    /// `LayoutFloat` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutFloat as LayoutFloat;
    /// `LayoutHeight` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutHeight as LayoutHeight;
    /// `LayoutJustifyContent` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutJustifyContent as LayoutJustifyContent;
    /// `LayoutLeft` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutLeft as LayoutLeft;
    /// `LayoutMarginBottom` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutMarginBottom as LayoutMarginBottom;
    /// `LayoutMarginLeft` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutMarginLeft as LayoutMarginLeft;
    /// `LayoutMarginRight` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutMarginRight as LayoutMarginRight;
    /// `LayoutMarginTop` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutMarginTop as LayoutMarginTop;
    /// `LayoutMaxHeight` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutMaxHeight as LayoutMaxHeight;
    /// `LayoutMaxWidth` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutMaxWidth as LayoutMaxWidth;
    /// `LayoutMinHeight` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutMinHeight as LayoutMinHeight;
    /// `LayoutMinWidth` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutMinWidth as LayoutMinWidth;
    /// `LayoutPaddingBottom` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutPaddingBottom as LayoutPaddingBottom;
    /// `LayoutPaddingLeft` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutPaddingLeft as LayoutPaddingLeft;
    /// `LayoutPaddingRight` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutPaddingRight as LayoutPaddingRight;
    /// `LayoutPaddingTop` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutPaddingTop as LayoutPaddingTop;
    /// `LayoutPosition` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutPosition as LayoutPosition;
    /// `LayoutRight` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutRight as LayoutRight;
    /// `LayoutTop` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutTop as LayoutTop;
    /// `LayoutWidth` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutWidth as LayoutWidth;
    /// `LayoutFlexWrap` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutFlexWrap as LayoutFlexWrap;
    /// `LayoutOverflow` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutOverflow as LayoutOverflow;
    /// `PercentageValue` struct
    
#[doc(inline)] pub use crate::dll::AzPercentageValue as PercentageValue;
    /// `AngleMetric` struct
    
#[doc(inline)] pub use crate::dll::AzAngleMetric as AngleMetric;
    /// `AngleValue` struct
    
#[doc(inline)] pub use crate::dll::AzAngleValue as AngleValue;
    /// `NormalizedLinearColorStop` struct
    
#[doc(inline)] pub use crate::dll::AzNormalizedLinearColorStop as NormalizedLinearColorStop;
    /// `NormalizedRadialColorStop` struct
    
#[doc(inline)] pub use crate::dll::AzNormalizedRadialColorStop as NormalizedRadialColorStop;
    /// `DirectionCorner` struct
    
#[doc(inline)] pub use crate::dll::AzDirectionCorner as DirectionCorner;
    /// `DirectionCorners` struct
    
#[doc(inline)] pub use crate::dll::AzDirectionCorners as DirectionCorners;
    /// `Direction` struct
    
#[doc(inline)] pub use crate::dll::AzDirection as Direction;
    /// `ExtendMode` struct
    
#[doc(inline)] pub use crate::dll::AzExtendMode as ExtendMode;
    /// `LinearGradient` struct
    
#[doc(inline)] pub use crate::dll::AzLinearGradient as LinearGradient;
    /// `Shape` struct
    
#[doc(inline)] pub use crate::dll::AzShape as Shape;
    /// `RadialGradientSize` struct
    
#[doc(inline)] pub use crate::dll::AzRadialGradientSize as RadialGradientSize;
    /// `RadialGradient` struct
    
#[doc(inline)] pub use crate::dll::AzRadialGradient as RadialGradient;
    /// `ConicGradient` struct
    
#[doc(inline)] pub use crate::dll::AzConicGradient as ConicGradient;
    /// `StyleBackgroundContent` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBackgroundContent as StyleBackgroundContent;
    /// `BackgroundPositionHorizontal` struct
    
#[doc(inline)] pub use crate::dll::AzBackgroundPositionHorizontal as BackgroundPositionHorizontal;
    /// `BackgroundPositionVertical` struct
    
#[doc(inline)] pub use crate::dll::AzBackgroundPositionVertical as BackgroundPositionVertical;
    /// `StyleBackgroundPosition` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBackgroundPosition as StyleBackgroundPosition;
    /// `StyleBackgroundRepeat` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBackgroundRepeat as StyleBackgroundRepeat;
    /// `StyleBackgroundSize` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBackgroundSize as StyleBackgroundSize;
    /// `StyleBorderBottomColor` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderBottomColor as StyleBorderBottomColor;
    /// `StyleBorderBottomLeftRadius` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderBottomLeftRadius as StyleBorderBottomLeftRadius;
    /// `StyleBorderBottomRightRadius` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderBottomRightRadius as StyleBorderBottomRightRadius;
    /// `BorderStyle` struct
    
#[doc(inline)] pub use crate::dll::AzBorderStyle as BorderStyle;
    /// `StyleBorderBottomStyle` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderBottomStyle as StyleBorderBottomStyle;
    /// `LayoutBorderBottomWidth` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutBorderBottomWidth as LayoutBorderBottomWidth;
    /// `StyleBorderLeftColor` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderLeftColor as StyleBorderLeftColor;
    /// `StyleBorderLeftStyle` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderLeftStyle as StyleBorderLeftStyle;
    /// `LayoutBorderLeftWidth` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutBorderLeftWidth as LayoutBorderLeftWidth;
    /// `StyleBorderRightColor` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderRightColor as StyleBorderRightColor;
    /// `StyleBorderRightStyle` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderRightStyle as StyleBorderRightStyle;
    /// `LayoutBorderRightWidth` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutBorderRightWidth as LayoutBorderRightWidth;
    /// `StyleBorderTopColor` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderTopColor as StyleBorderTopColor;
    /// `StyleBorderTopLeftRadius` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderTopLeftRadius as StyleBorderTopLeftRadius;
    /// `StyleBorderTopRightRadius` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderTopRightRadius as StyleBorderTopRightRadius;
    /// `StyleBorderTopStyle` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderTopStyle as StyleBorderTopStyle;
    /// `LayoutBorderTopWidth` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutBorderTopWidth as LayoutBorderTopWidth;
    /// `ScrollbarInfo` struct
    
#[doc(inline)] pub use crate::dll::AzScrollbarInfo as ScrollbarInfo;
    /// `ScrollbarStyle` struct
    
#[doc(inline)] pub use crate::dll::AzScrollbarStyle as ScrollbarStyle;
    /// `StyleCursor` struct
    
#[doc(inline)] pub use crate::dll::AzStyleCursor as StyleCursor;
    /// `StyleFontFamily` struct
    
#[doc(inline)] pub use crate::dll::AzStyleFontFamily as StyleFontFamily;
    /// `StyleFontSize` struct
    
#[doc(inline)] pub use crate::dll::AzStyleFontSize as StyleFontSize;
    /// `StyleLetterSpacing` struct
    
#[doc(inline)] pub use crate::dll::AzStyleLetterSpacing as StyleLetterSpacing;
    /// `StyleLineHeight` struct
    
#[doc(inline)] pub use crate::dll::AzStyleLineHeight as StyleLineHeight;
    /// `StyleTabWidth` struct
    
#[doc(inline)] pub use crate::dll::AzStyleTabWidth as StyleTabWidth;
    /// `StyleOpacity` struct
    
#[doc(inline)] pub use crate::dll::AzStyleOpacity as StyleOpacity;
    /// `StyleTransformOrigin` struct
    
#[doc(inline)] pub use crate::dll::AzStyleTransformOrigin as StyleTransformOrigin;
    /// `StylePerspectiveOrigin` struct
    
#[doc(inline)] pub use crate::dll::AzStylePerspectiveOrigin as StylePerspectiveOrigin;
    /// `StyleBackfaceVisibility` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBackfaceVisibility as StyleBackfaceVisibility;
    /// `StyleTransform` struct
    
#[doc(inline)] pub use crate::dll::AzStyleTransform as StyleTransform;
    /// `StyleTransformMatrix2D` struct
    
#[doc(inline)] pub use crate::dll::AzStyleTransformMatrix2D as StyleTransformMatrix2D;
    /// `StyleTransformMatrix3D` struct
    
#[doc(inline)] pub use crate::dll::AzStyleTransformMatrix3D as StyleTransformMatrix3D;
    /// `StyleTransformTranslate2D` struct
    
#[doc(inline)] pub use crate::dll::AzStyleTransformTranslate2D as StyleTransformTranslate2D;
    /// `StyleTransformTranslate3D` struct
    
#[doc(inline)] pub use crate::dll::AzStyleTransformTranslate3D as StyleTransformTranslate3D;
    /// `StyleTransformRotate3D` struct
    
#[doc(inline)] pub use crate::dll::AzStyleTransformRotate3D as StyleTransformRotate3D;
    /// `StyleTransformScale2D` struct
    
#[doc(inline)] pub use crate::dll::AzStyleTransformScale2D as StyleTransformScale2D;
    /// `StyleTransformScale3D` struct
    
#[doc(inline)] pub use crate::dll::AzStyleTransformScale3D as StyleTransformScale3D;
    /// `StyleTransformSkew2D` struct
    
#[doc(inline)] pub use crate::dll::AzStyleTransformSkew2D as StyleTransformSkew2D;
    /// `StyleTextAlign` struct
    
#[doc(inline)] pub use crate::dll::AzStyleTextAlign as StyleTextAlign;
    /// `StyleTextColor` struct
    
#[doc(inline)] pub use crate::dll::AzStyleTextColor as StyleTextColor;
    /// `StyleWordSpacing` struct
    
#[doc(inline)] pub use crate::dll::AzStyleWordSpacing as StyleWordSpacing;
    /// `StyleBoxShadowValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBoxShadowValue as StyleBoxShadowValue;
    /// `LayoutAlignContentValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutAlignContentValue as LayoutAlignContentValue;
    /// `LayoutAlignItemsValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutAlignItemsValue as LayoutAlignItemsValue;
    /// `LayoutBottomValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutBottomValue as LayoutBottomValue;
    /// `LayoutBoxSizingValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutBoxSizingValue as LayoutBoxSizingValue;
    /// `LayoutFlexDirectionValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutFlexDirectionValue as LayoutFlexDirectionValue;
    /// `LayoutDisplayValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutDisplayValue as LayoutDisplayValue;
    /// `LayoutFlexGrowValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutFlexGrowValue as LayoutFlexGrowValue;
    /// `LayoutFlexShrinkValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutFlexShrinkValue as LayoutFlexShrinkValue;
    /// `LayoutFloatValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutFloatValue as LayoutFloatValue;
    /// `LayoutHeightValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutHeightValue as LayoutHeightValue;
    /// `LayoutJustifyContentValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutJustifyContentValue as LayoutJustifyContentValue;
    /// `LayoutLeftValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutLeftValue as LayoutLeftValue;
    /// `LayoutMarginBottomValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutMarginBottomValue as LayoutMarginBottomValue;
    /// `LayoutMarginLeftValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutMarginLeftValue as LayoutMarginLeftValue;
    /// `LayoutMarginRightValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutMarginRightValue as LayoutMarginRightValue;
    /// `LayoutMarginTopValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutMarginTopValue as LayoutMarginTopValue;
    /// `LayoutMaxHeightValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutMaxHeightValue as LayoutMaxHeightValue;
    /// `LayoutMaxWidthValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutMaxWidthValue as LayoutMaxWidthValue;
    /// `LayoutMinHeightValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutMinHeightValue as LayoutMinHeightValue;
    /// `LayoutMinWidthValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutMinWidthValue as LayoutMinWidthValue;
    /// `LayoutPaddingBottomValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutPaddingBottomValue as LayoutPaddingBottomValue;
    /// `LayoutPaddingLeftValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutPaddingLeftValue as LayoutPaddingLeftValue;
    /// `LayoutPaddingRightValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutPaddingRightValue as LayoutPaddingRightValue;
    /// `LayoutPaddingTopValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutPaddingTopValue as LayoutPaddingTopValue;
    /// `LayoutPositionValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutPositionValue as LayoutPositionValue;
    /// `LayoutRightValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutRightValue as LayoutRightValue;
    /// `LayoutTopValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutTopValue as LayoutTopValue;
    /// `LayoutWidthValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutWidthValue as LayoutWidthValue;
    /// `LayoutFlexWrapValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutFlexWrapValue as LayoutFlexWrapValue;
    /// `LayoutOverflowValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutOverflowValue as LayoutOverflowValue;
    /// `ScrollbarStyleValue` struct
    
#[doc(inline)] pub use crate::dll::AzScrollbarStyleValue as ScrollbarStyleValue;
    /// `StyleBackgroundContentVecValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBackgroundContentVecValue as StyleBackgroundContentVecValue;
    /// `StyleBackgroundPositionVecValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBackgroundPositionVecValue as StyleBackgroundPositionVecValue;
    /// `StyleBackgroundRepeatVecValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBackgroundRepeatVecValue as StyleBackgroundRepeatVecValue;
    /// `StyleBackgroundSizeVecValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBackgroundSizeVecValue as StyleBackgroundSizeVecValue;
    /// `StyleBorderBottomColorValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderBottomColorValue as StyleBorderBottomColorValue;
    /// `StyleBorderBottomLeftRadiusValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderBottomLeftRadiusValue as StyleBorderBottomLeftRadiusValue;
    /// `StyleBorderBottomRightRadiusValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderBottomRightRadiusValue as StyleBorderBottomRightRadiusValue;
    /// `StyleBorderBottomStyleValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderBottomStyleValue as StyleBorderBottomStyleValue;
    /// `LayoutBorderBottomWidthValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutBorderBottomWidthValue as LayoutBorderBottomWidthValue;
    /// `StyleBorderLeftColorValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderLeftColorValue as StyleBorderLeftColorValue;
    /// `StyleBorderLeftStyleValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderLeftStyleValue as StyleBorderLeftStyleValue;
    /// `LayoutBorderLeftWidthValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutBorderLeftWidthValue as LayoutBorderLeftWidthValue;
    /// `StyleBorderRightColorValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderRightColorValue as StyleBorderRightColorValue;
    /// `StyleBorderRightStyleValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderRightStyleValue as StyleBorderRightStyleValue;
    /// `LayoutBorderRightWidthValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutBorderRightWidthValue as LayoutBorderRightWidthValue;
    /// `StyleBorderTopColorValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderTopColorValue as StyleBorderTopColorValue;
    /// `StyleBorderTopLeftRadiusValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderTopLeftRadiusValue as StyleBorderTopLeftRadiusValue;
    /// `StyleBorderTopRightRadiusValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderTopRightRadiusValue as StyleBorderTopRightRadiusValue;
    /// `StyleBorderTopStyleValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBorderTopStyleValue as StyleBorderTopStyleValue;
    /// `LayoutBorderTopWidthValue` struct
    
#[doc(inline)] pub use crate::dll::AzLayoutBorderTopWidthValue as LayoutBorderTopWidthValue;
    /// `StyleCursorValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleCursorValue as StyleCursorValue;
    /// `StyleFontFamilyVecValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleFontFamilyVecValue as StyleFontFamilyVecValue;
    /// `StyleFontSizeValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleFontSizeValue as StyleFontSizeValue;
    /// `StyleLetterSpacingValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleLetterSpacingValue as StyleLetterSpacingValue;
    /// `StyleLineHeightValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleLineHeightValue as StyleLineHeightValue;
    /// `StyleTabWidthValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleTabWidthValue as StyleTabWidthValue;
    /// `StyleTextAlignValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleTextAlignValue as StyleTextAlignValue;
    /// `StyleTextColorValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleTextColorValue as StyleTextColorValue;
    /// `StyleWordSpacingValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleWordSpacingValue as StyleWordSpacingValue;
    /// `StyleOpacityValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleOpacityValue as StyleOpacityValue;
    /// `StyleTransformVecValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleTransformVecValue as StyleTransformVecValue;
    /// `StyleTransformOriginValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleTransformOriginValue as StyleTransformOriginValue;
    /// `StylePerspectiveOriginValue` struct
    
#[doc(inline)] pub use crate::dll::AzStylePerspectiveOriginValue as StylePerspectiveOriginValue;
    /// `StyleBackfaceVisibilityValue` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBackfaceVisibilityValue as StyleBackfaceVisibilityValue;
    /// Parsed CSS key-value pair
    
#[doc(inline)] pub use crate::dll::AzCssProperty as CssProperty;
    impl CssProperty {
        /// Returns the key of the CSS property as a string, i.e. `background`
        pub fn get_key_string(&self)  -> crate::str::String { unsafe { crate::dll::AzCssProperty_getKeyString(self) } }
        /// Returns the value of the CSS property as a string, i.e. `linear-gradient(red, blue)`
        pub fn get_value_string(&self)  -> crate::str::String { unsafe { crate::dll::AzCssProperty_getValueString(self) } }
        /// Returns the CSS key-value pair as a string, i.e. `background: linear-gradient(red, blue)`
        pub fn get_key_value_string(&self)  -> crate::str::String { unsafe { crate::dll::AzCssProperty_getKeyValueString(self) } }
        /// Interpolates two CSS properties given a value `t` ranging from 0.0 to 1.0. The interpolation function can be set on the `context` (`Ease`, `Linear`, etc.).
        pub fn interpolate(&self, other: CssProperty, t: f32, context: InterpolateContext)  -> crate::css::CssProperty { unsafe { crate::dll::AzCssProperty_interpolate(self, other, t, context) } }
    }

}

pub mod widgets {
    #![allow(dead_code, unused_imports)]
    //! Default, built-in widgets (button, label, textinput, etc.)
    use crate::dll::*;
    use core::ffi::c_void;
    use crate::str::String;
    use crate::callbacks::{CallbackType, RefAny};
    use crate::css::ColorU;
    use crate::vec::NodeDataInlineCssPropertyVec;
    /// `Button` struct
    
#[doc(inline)] pub use crate::dll::AzButton as Button;
    impl Button {
        /// Creates a new labeled button
        pub fn new(label: String) -> Self { unsafe { crate::dll::AzButton_new(label) } }
        /// Calls the `Button::set_on_click` function.
        pub fn set_on_click(&mut self, data: RefAny, callback: CallbackType)  { unsafe { crate::dll::AzButton_setOnClick(self, data, callback) } }
        /// Calls the `Button::with_on_click` function.
        pub fn with_on_click(&mut self, data: RefAny, callback: CallbackType)  -> crate::widgets::Button { unsafe { crate::dll::AzButton_withOnClick(self, data, callback) } }
        /// Calls the `Button::dom` function.
        pub fn dom(&mut self)  -> crate::dom::Dom { unsafe { crate::dll::AzButton_dom(self) } }
    }

    /// `ButtonOnClick` struct
    
#[doc(inline)] pub use crate::dll::AzButtonOnClick as ButtonOnClick;
    /// `CheckBox` struct
    
#[doc(inline)] pub use crate::dll::AzCheckBox as CheckBox;
    impl CheckBox {
        /// Creates a new checkbox, disabled or enabled
        pub fn new(checked: bool) -> Self { unsafe { crate::dll::AzCheckBox_new(checked) } }
        /// Calls the `CheckBox::set_on_toggle` function.
        pub fn set_on_toggle(&mut self, data: RefAny, callback: CheckBoxOnToggleCallbackType)  { unsafe { crate::dll::AzCheckBox_setOnToggle(self, data, callback) } }
        /// Calls the `CheckBox::with_on_toggle` function.
        pub fn with_on_toggle(&mut self, data: RefAny, callback: CheckBoxOnToggleCallbackType)  -> crate::widgets::CheckBox { unsafe { crate::dll::AzCheckBox_withOnToggle(self, data, callback) } }
        /// Calls the `CheckBox::dom` function.
        pub fn dom(&mut self)  -> crate::dom::Dom { unsafe { crate::dll::AzCheckBox_dom(self) } }
    }

    /// `CheckBoxStateWrapper` struct
    
#[doc(inline)] pub use crate::dll::AzCheckBoxStateWrapper as CheckBoxStateWrapper;
    /// `CheckBoxOnToggle` struct
    
#[doc(inline)] pub use crate::dll::AzCheckBoxOnToggle as CheckBoxOnToggle;
    /// `CheckBoxOnToggleCallback` struct
    
#[doc(inline)] pub use crate::dll::AzCheckBoxOnToggleCallback as CheckBoxOnToggleCallback;
    /// `CheckBoxOnToggleCallbackType` struct
    
#[doc(inline)] pub use crate::dll::AzCheckBoxOnToggleCallbackType as CheckBoxOnToggleCallbackType;
    /// `CheckBoxState` struct
    
#[doc(inline)] pub use crate::dll::AzCheckBoxState as CheckBoxState;
    /// `Label` struct
    
#[doc(inline)] pub use crate::dll::AzLabel as Label;
    impl Label {
        /// Creates a new `Label` instance.
        pub fn new(text: String) -> Self { unsafe { crate::dll::AzLabel_new(text) } }
        /// Calls the `Label::dom` function.
        pub fn dom(&mut self)  -> crate::dom::Dom { unsafe { crate::dll::AzLabel_dom(self) } }
    }

    /// `ColorInput` struct
    
#[doc(inline)] pub use crate::dll::AzColorInput as ColorInput;
    impl ColorInput {
        /// Creates a new `ColorInput` instance.
        pub fn new(color: ColorU) -> Self { unsafe { crate::dll::AzColorInput_new(color) } }
        /// Calls the `ColorInput::set_on_value_change` function.
        pub fn set_on_value_change(&mut self, data: RefAny, callback: ColorInputOnValueChangeCallbackType)  { unsafe { crate::dll::AzColorInput_setOnValueChange(self, data, callback) } }
        /// Calls the `ColorInput::with_on_value_change` function.
        pub fn with_on_value_change(&mut self, data: RefAny, callback: ColorInputOnValueChangeCallbackType)  -> crate::widgets::ColorInput { unsafe { crate::dll::AzColorInput_withOnValueChange(self, data, callback) } }
        /// Calls the `ColorInput::dom` function.
        pub fn dom(&mut self)  -> crate::dom::Dom { unsafe { crate::dll::AzColorInput_dom(self) } }
    }

    /// `ColorInputStateWrapper` struct
    
#[doc(inline)] pub use crate::dll::AzColorInputStateWrapper as ColorInputStateWrapper;
    /// `ColorInputState` struct
    
#[doc(inline)] pub use crate::dll::AzColorInputState as ColorInputState;
    /// `ColorInputOnValueChange` struct
    
#[doc(inline)] pub use crate::dll::AzColorInputOnValueChange as ColorInputOnValueChange;
    /// `ColorInputOnValueChangeCallback` struct
    
#[doc(inline)] pub use crate::dll::AzColorInputOnValueChangeCallback as ColorInputOnValueChangeCallback;
    /// `ColorInputOnValueChangeCallbackType` struct
    
#[doc(inline)] pub use crate::dll::AzColorInputOnValueChangeCallbackType as ColorInputOnValueChangeCallbackType;
    /// `TextInput` struct
    
#[doc(inline)] pub use crate::dll::AzTextInput as TextInput;
    impl TextInput {
        /// Creates a new `TextInput` instance.
        pub fn new(initial_text: String) -> Self { unsafe { crate::dll::AzTextInput_new(initial_text) } }
        /// Calls the `TextInput::set_on_text_input` function.
        pub fn set_on_text_input(&mut self, data: RefAny, callback: TextInputOnTextInputCallbackType)  { unsafe { crate::dll::AzTextInput_setOnTextInput(self, data, callback) } }
        /// Calls the `TextInput::with_on_text_input` function.
        pub fn with_on_text_input(&mut self, data: RefAny, callback: TextInputOnTextInputCallbackType)  -> crate::widgets::TextInput { unsafe { crate::dll::AzTextInput_withOnTextInput(self, data, callback) } }
        /// Calls the `TextInput::set_on_virtual_key_down` function.
        pub fn set_on_virtual_key_down(&mut self, data: RefAny, callback: TextInputOnVirtualKeyDownCallbackType)  { unsafe { crate::dll::AzTextInput_setOnVirtualKeyDown(self, data, callback) } }
        /// Calls the `TextInput::with_on_virtual_key_down` function.
        pub fn with_on_virtual_key_down(&mut self, data: RefAny, callback: TextInputOnVirtualKeyDownCallbackType)  -> crate::widgets::TextInput { unsafe { crate::dll::AzTextInput_withOnVirtualKeyDown(self, data, callback) } }
        /// Calls the `TextInput::set_on_focus_lost` function.
        pub fn set_on_focus_lost(&mut self, data: RefAny, callback: TextInputOnFocusLostCallbackType)  { unsafe { crate::dll::AzTextInput_setOnFocusLost(self, data, callback) } }
        /// Calls the `TextInput::with_on_focus_lost` function.
        pub fn with_on_focus_lost(&mut self, data: RefAny, callback: TextInputOnFocusLostCallbackType)  -> crate::widgets::TextInput { unsafe { crate::dll::AzTextInput_withOnFocusLost(self, data, callback) } }
        /// Calls the `TextInput::set_placeholder_style` function.
        pub fn set_placeholder_style(&mut self, placeholder_style: NodeDataInlineCssPropertyVec)  { unsafe { crate::dll::AzTextInput_setPlaceholderStyle(self, placeholder_style) } }
        /// Calls the `TextInput::with_placeholder_style` function.
        pub fn with_placeholder_style(&mut self, placeholder_style: NodeDataInlineCssPropertyVec)  -> crate::widgets::TextInput { unsafe { crate::dll::AzTextInput_withPlaceholderStyle(self, placeholder_style) } }
        /// Calls the `TextInput::set_container_style` function.
        pub fn set_container_style(&mut self, container_style: NodeDataInlineCssPropertyVec)  { unsafe { crate::dll::AzTextInput_setContainerStyle(self, container_style) } }
        /// Calls the `TextInput::with_container_style` function.
        pub fn with_container_style(&mut self, container_style: NodeDataInlineCssPropertyVec)  -> crate::widgets::TextInput { unsafe { crate::dll::AzTextInput_withContainerStyle(self, container_style) } }
        /// Calls the `TextInput::set_label_style` function.
        pub fn set_label_style(&mut self, label_style: NodeDataInlineCssPropertyVec)  { unsafe { crate::dll::AzTextInput_setLabelStyle(self, label_style) } }
        /// Calls the `TextInput::with_label_style` function.
        pub fn with_label_style(&mut self, label_style: NodeDataInlineCssPropertyVec)  -> crate::widgets::TextInput { unsafe { crate::dll::AzTextInput_withLabelStyle(self, label_style) } }
        /// Calls the `TextInput::dom` function.
        pub fn dom(&mut self)  -> crate::dom::Dom { unsafe { crate::dll::AzTextInput_dom(self) } }
    }

    /// `TextInputStateWrapper` struct
    
#[doc(inline)] pub use crate::dll::AzTextInputStateWrapper as TextInputStateWrapper;
    /// `TextInputState` struct
    
#[doc(inline)] pub use crate::dll::AzTextInputState as TextInputState;
    /// `TextInputSelection` struct
    
#[doc(inline)] pub use crate::dll::AzTextInputSelection as TextInputSelection;
    /// `TextInputSelectionRange` struct
    
#[doc(inline)] pub use crate::dll::AzTextInputSelectionRange as TextInputSelectionRange;
    /// `TextInputOnTextInput` struct
    
#[doc(inline)] pub use crate::dll::AzTextInputOnTextInput as TextInputOnTextInput;
    /// `TextInputOnTextInputCallback` struct
    
#[doc(inline)] pub use crate::dll::AzTextInputOnTextInputCallback as TextInputOnTextInputCallback;
    /// `TextInputOnTextInputCallbackType` struct
    
#[doc(inline)] pub use crate::dll::AzTextInputOnTextInputCallbackType as TextInputOnTextInputCallbackType;
    /// `TextInputOnVirtualKeyDown` struct
    
#[doc(inline)] pub use crate::dll::AzTextInputOnVirtualKeyDown as TextInputOnVirtualKeyDown;
    /// `TextInputOnVirtualKeyDownCallback` struct
    
#[doc(inline)] pub use crate::dll::AzTextInputOnVirtualKeyDownCallback as TextInputOnVirtualKeyDownCallback;
    /// `TextInputOnVirtualKeyDownCallbackType` struct
    
#[doc(inline)] pub use crate::dll::AzTextInputOnVirtualKeyDownCallbackType as TextInputOnVirtualKeyDownCallbackType;
    /// `TextInputOnFocusLost` struct
    
#[doc(inline)] pub use crate::dll::AzTextInputOnFocusLost as TextInputOnFocusLost;
    /// `TextInputOnFocusLostCallback` struct
    
#[doc(inline)] pub use crate::dll::AzTextInputOnFocusLostCallback as TextInputOnFocusLostCallback;
    /// `TextInputOnFocusLostCallbackType` struct
    
#[doc(inline)] pub use crate::dll::AzTextInputOnFocusLostCallbackType as TextInputOnFocusLostCallbackType;
    /// `OnTextInputReturn` struct
    
#[doc(inline)] pub use crate::dll::AzOnTextInputReturn as OnTextInputReturn;
    /// `TextInputValid` struct
    
#[doc(inline)] pub use crate::dll::AzTextInputValid as TextInputValid;
    /// `NumberInput` struct
    
#[doc(inline)] pub use crate::dll::AzNumberInput as NumberInput;
    impl NumberInput {
        /// Creates a new `NumberInput` instance.
        pub fn new(number: f32) -> Self { unsafe { crate::dll::AzNumberInput_new(number) } }
    }

    /// `NumberInputStateWrapper` struct
    
#[doc(inline)] pub use crate::dll::AzNumberInputStateWrapper as NumberInputStateWrapper;
    /// `NumberInputState` struct
    
#[doc(inline)] pub use crate::dll::AzNumberInputState as NumberInputState;
    /// `NumberInputOnValueChange` struct
    
#[doc(inline)] pub use crate::dll::AzNumberInputOnValueChange as NumberInputOnValueChange;
    /// `NumberInputOnValueChangeCallback` struct
    
#[doc(inline)] pub use crate::dll::AzNumberInputOnValueChangeCallback as NumberInputOnValueChangeCallback;
    /// `NumberInputOnValueChangeCallbackType` struct
    
#[doc(inline)] pub use crate::dll::AzNumberInputOnValueChangeCallbackType as NumberInputOnValueChangeCallbackType;
}

pub mod style {
    #![allow(dead_code, unused_imports)]
    //! DOM to CSS cascading and styling module
    use crate::dll::*;
    use core::ffi::c_void;
    use crate::dom::Dom;
    use crate::css::Css;
    use crate::str::String;
    /// `Node` struct
    
#[doc(inline)] pub use crate::dll::AzNode as Node;
    /// `CascadeInfo` struct
    
#[doc(inline)] pub use crate::dll::AzCascadeInfo as CascadeInfo;
    /// `CssPropertySource` struct
    
#[doc(inline)] pub use crate::dll::AzCssPropertySource as CssPropertySource;
    /// `StyledNodeState` struct
    
#[doc(inline)] pub use crate::dll::AzStyledNodeState as StyledNodeState;
    /// `StyledNode` struct
    
#[doc(inline)] pub use crate::dll::AzStyledNode as StyledNode;
    /// `TagId` struct
    
#[doc(inline)] pub use crate::dll::AzTagId as TagId;
    /// `TagIdToNodeIdMapping` struct
    
#[doc(inline)] pub use crate::dll::AzTagIdToNodeIdMapping as TagIdToNodeIdMapping;
    /// `ParentWithNodeDepth` struct
    
#[doc(inline)] pub use crate::dll::AzParentWithNodeDepth as ParentWithNodeDepth;
    /// `CssPropertyCache` struct
    
#[doc(inline)] pub use crate::dll::AzCssPropertyCache as CssPropertyCache;
    impl Clone for CssPropertyCache { fn clone(&self) -> Self { unsafe { crate::dll::AzCssPropertyCache_deepCopy(self) } } }
    impl Drop for CssPropertyCache { fn drop(&mut self) { unsafe { crate::dll::AzCssPropertyCache_delete(self) } } }
    /// `StyledDom` struct
    
#[doc(inline)] pub use crate::dll::AzStyledDom as StyledDom;
    impl StyledDom {
        /// Styles a `Dom` with the given `Css`, returning the `StyledDom` - complexity `O(count(dom_nodes) * count(css_blocks))`: make sure that the `Dom` and the `Css` are as small as possible, use inline CSS if the performance isn't good enough
        pub fn new(dom: Dom, css: Css) -> Self { unsafe { crate::dll::AzStyledDom_new(dom, css) } }
        /// Returns a default, empty `Dom`, usually returned if you don't want to crash in an error case.
        pub fn default() -> Self { unsafe { crate::dll::AzStyledDom_default() } }
        /// Returns a DOM loaded from an XML file
        pub fn from_xml(xml_string: String) -> Self { unsafe { crate::dll::AzStyledDom_fromXml(xml_string) } }
        /// Same as `from_xml`, but loads the file relative to the current directory
        pub fn from_file(xml_file_path: String) -> Self { unsafe { crate::dll::AzStyledDom_fromFile(xml_file_path) } }
        /// Appends an already styled list of DOM nodes to the current `dom.root` - complexity `O(count(dom.dom_nodes))`
        pub fn append_child(&mut self, dom: StyledDom)  { unsafe { crate::dll::AzStyledDom_appendChild(self, dom) } }
        /// Restyles an already styled DOM with a new CSS - overwrites old styles, but does not replace them, useful for implementing user styles that are applied on top of the existing application style
        pub fn restyle(&mut self, css: Css)  { unsafe { crate::dll::AzStyledDom_restyle(self, css) } }
        /// Returns the number of nodes in the styled DOM
        pub fn node_count(&self)  -> usize { unsafe { crate::dll::AzStyledDom_nodeCount(self) } }
        /// Returns a HTML for unit testing
        pub fn get_html_string_test(&self)  -> crate::str::String { unsafe { crate::dll::AzStyledDom_getHtmlStringTest(self) } }
        /// Returns a HTML string that you can write to a file in order to debug the UI structure and debug potential cascading issues
        pub fn get_html_string_debug(&self)  -> crate::str::String { unsafe { crate::dll::AzStyledDom_getHtmlStringDebug(self) } }
    }

}

pub mod gl {
    #![allow(dead_code, unused_imports)]
    //! OpenGl helper types (`Texture`, `Gl`, etc.)
    use crate::dll::*;
    use core::ffi::c_void;
    impl Refstr {
        fn as_str(&self) -> &str { unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(self.ptr, self.len)) } }
    }

    impl From<&str> for Refstr {
        fn from(s: &str) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl RefstrVecRef {
        fn as_slice(&self) -> &[Refstr] { unsafe { core::slice::from_raw_parts(self.ptr, self.len) } }
    }

    impl From<&[Refstr]> for RefstrVecRef {
        fn from(s: &[Refstr]) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl From<&mut [GLint64]> for GLint64VecRefMut {
        fn from(s: &mut [GLint64]) -> Self {
            Self { ptr: s.as_mut_ptr(), len: s.len() }
        }
    }

    impl GLint64VecRefMut {
        fn as_mut_slice(&mut self) -> &mut [GLint64] { unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) } }
    }

    impl From<&mut [GLfloat]> for GLfloatVecRefMut {
        fn from(s: &mut [GLfloat]) -> Self {
            Self { ptr: s.as_mut_ptr(), len: s.len() }
        }
    }

    impl GLfloatVecRefMut {
        fn as_mut_slice(&mut self) -> &mut [GLfloat] { unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) } }
    }

    impl From<&mut [GLint]> for GLintVecRefMut {
        fn from(s: &mut [GLint]) -> Self {
            Self { ptr: s.as_mut_ptr(), len: s.len() }
        }
    }

    impl GLintVecRefMut {
        fn as_mut_slice(&mut self) -> &mut [GLint] { unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) } }
    }

    impl From<&[GLuint]> for GLuintVecRef {
        fn from(s: &[GLuint]) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl GLuintVecRef {
        fn as_slice(&self) -> &[GLuint] { unsafe { core::slice::from_raw_parts(self.ptr, self.len) } }
    }

    impl From<&[GLenum]> for GLenumVecRef {
        fn from(s: &[GLenum]) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl GLenumVecRef {
        fn as_slice(&self) -> &[GLenum] { unsafe { core::slice::from_raw_parts(self.ptr, self.len) } }
    }

    impl From<&[u8]> for U8VecRef {
        fn from(s: &[u8]) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl U8VecRef {
        fn as_slice(&self) -> &[u8] { unsafe { core::slice::from_raw_parts(self.ptr, self.len) } }
    }

    impl ::core::fmt::Debug for U8VecRef {
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            self.as_slice().fmt(f)
        }
    }

    impl Clone for U8VecRef {
        fn clone(&self) -> Self {
            U8VecRef::from(self.as_slice())
        }
    }

    impl PartialOrd for U8VecRef {
        fn partial_cmp(&self, rhs: &Self) -> Option<core::cmp::Ordering> {
            self.as_slice().partial_cmp(rhs.as_slice())
        }
    }

    impl Ord for U8VecRef {
        fn cmp(&self, rhs: &Self) -> core::cmp::Ordering {
            self.as_slice().cmp(rhs.as_slice())
        }
    }

    impl PartialEq for U8VecRef {
        fn eq(&self, rhs: &Self) -> bool {
            self.as_slice().eq(rhs.as_slice())
        }
    }

    impl Eq for U8VecRef { }

    impl core::hash::Hash for U8VecRef {
        fn hash<H>(&self, state: &mut H) where H: core::hash::Hasher {
            self.as_slice().hash(state)
        }
    }

    impl From<&[f32]> for F32VecRef {
        fn from(s: &[f32]) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl F32VecRef {
        fn as_slice(&self) -> &[f32] { unsafe { core::slice::from_raw_parts(self.ptr, self.len) } }
    }

    impl From<&[i32]> for I32VecRef {
        fn from(s: &[i32]) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl I32VecRef {
        fn as_slice(&self) -> &[i32] { unsafe { core::slice::from_raw_parts(self.ptr, self.len) } }
    }

    impl From<&mut [GLboolean]> for GLbooleanVecRefMut {
        fn from(s: &mut [GLboolean]) -> Self {
            Self { ptr: s.as_mut_ptr(), len: s.len() }
        }
    }

    impl GLbooleanVecRefMut {
        fn as_mut_slice(&mut self) -> &mut [GLboolean] { unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) } }
    }

    impl From<&mut [u8]> for U8VecRefMut {
        fn from(s: &mut [u8]) -> Self {
            Self { ptr: s.as_mut_ptr(), len: s.len() }
        }
    }

    impl U8VecRefMut {
        fn as_mut_slice(&mut self) -> &mut [u8] { unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) } }
    }

    /// Built in primitive types provided by the C language
    #[allow(non_camel_case_types)]
    pub mod ctypes {
        pub enum c_void {}
        pub type c_char = i8;
        pub type c_schar = i8;
        pub type c_uchar = u8;
        pub type c_short = i16;
        pub type c_ushort = u16;
        pub type c_int = i32;
        pub type c_uint = u32;
        pub type c_long = i32;
        pub type c_ulong = u32;
        pub type c_longlong = i64;
        pub type c_ulonglong = u64;
        pub type c_float = f32;
        pub type c_double = f64;
        pub type __int8 = i8;
        pub type __uint8 = u8;
        pub type __int16 = i16;
        pub type __uint16 = u16;
        pub type __int32 = i32;
        pub type __uint32 = u32;
        pub type __int64 = i64;
        pub type __uint64 = u64;
        pub type wchar_t = u16;
    }

    pub use self::ctypes::*;

    pub type GLenum = c_uint;
    pub type GLboolean = c_uchar;
    pub type GLbitfield = c_uint;
    pub type GLvoid = c_void;
    pub type GLbyte = c_char;
    pub type GLshort = c_short;
    pub type GLint = c_int;
    pub type GLclampx = c_int;
    pub type GLubyte = c_uchar;
    pub type GLushort = c_ushort;
    pub type GLuint = c_uint;
    pub type GLsizei = c_int;
    pub type GLfloat = c_float;
    pub type GLclampf = c_float;
    pub type GLdouble = c_double;
    pub type GLclampd = c_double;
    pub type GLeglImageOES = *const c_void;
    pub type GLchar = c_char;
    pub type GLcharARB = c_char;

    #[cfg(target_os = "macos")]
    pub type GLhandleARB = *const c_void;
    #[cfg(not(target_os = "macos"))]
    pub type GLhandleARB = c_uint;

    pub type GLhalfARB = c_ushort;
    pub type GLhalf = c_ushort;

    // Must be 32 bits
    pub type GLfixed = GLint;
    pub type GLintptr = isize;
    pub type GLsizeiptr = isize;
    pub type GLint64 = i64;
    pub type GLuint64 = u64;
    pub type GLintptrARB = isize;
    pub type GLsizeiptrARB = isize;
    pub type GLint64EXT = i64;
    pub type GLuint64EXT = u64;

    pub type GLDEBUGPROC = Option<extern "system" fn(source: GLenum, gltype: GLenum, id: GLuint, severity: GLenum, length: GLsizei, message: *const GLchar, userParam: *mut c_void)>;
    pub type GLDEBUGPROCARB = Option<extern "system" fn(source: GLenum, gltype: GLenum, id: GLuint, severity: GLenum, length: GLsizei, message: *const GLchar, userParam: *mut c_void)>;
    pub type GLDEBUGPROCKHR = Option<extern "system" fn(source: GLenum, gltype: GLenum, id: GLuint, severity: GLenum, length: GLsizei, message: *const GLchar, userParam: *mut c_void)>;

    // Vendor extension types
    pub type GLDEBUGPROCAMD = Option<extern "system" fn(id: GLuint, category: GLenum, severity: GLenum, length: GLsizei, message: *const GLchar, userParam: *mut c_void)>;
    pub type GLhalfNV = c_ushort;
    pub type GLvdpauSurfaceNV = GLintptr;



    use crate::window::LayoutSize;
    use crate::svg::TesselatedSvgNode;
    use crate::vec::{GLuintVec, StringVec};
    use crate::option::OptionU8VecRef;
    /// `Texture` struct
    
#[doc(inline)] pub use crate::dll::AzTexture as Texture;
    impl Texture {
        /// Allocates an OpenGL texture of a given size with a single red channel (used for image masks)
        pub fn allocate_clip_mask(gl: Gl, size: LayoutSize) -> Self { unsafe { crate::dll::AzTexture_allocateClipMask(gl, size) } }
        /// Draws a vertex / index buffer (aka. `&TesselatedSvgNode`) to the texture
        pub fn draw_clip_mask(&mut self, node: TesselatedSvgNode)  -> bool { unsafe { crate::dll::AzTexture_drawClipMask(self, node) } }
        /// Applies an FXAA filter to the texture
        pub fn apply_fxaa(&mut self)  -> bool { unsafe { crate::dll::AzTexture_applyFxaa(self) } }
    }

    impl Clone for Texture { fn clone(&self) -> Self { unsafe { crate::dll::AzTexture_deepCopy(self) } } }
    impl Drop for Texture { fn drop(&mut self) { unsafe { crate::dll::AzTexture_delete(self) } } }
    /// `GlVoidPtrConst` struct
    
#[doc(inline)] pub use crate::dll::AzGlVoidPtrConst as GlVoidPtrConst;
    impl Clone for GlVoidPtrConst { fn clone(&self) -> Self { unsafe { crate::dll::AzGlVoidPtrConst_deepCopy(self) } } }
    impl Drop for GlVoidPtrConst { fn drop(&mut self) { unsafe { crate::dll::AzGlVoidPtrConst_delete(self) } } }
    /// `GlVoidPtrMut` struct
    
#[doc(inline)] pub use crate::dll::AzGlVoidPtrMut as GlVoidPtrMut;
    /// `Gl` struct
    
#[doc(inline)] pub use crate::dll::AzGl as Gl;
    impl Gl {
        /// Calls the `Gl::get_type` function.
        pub fn get_type(&self)  -> crate::gl::GlType { unsafe { crate::dll::AzGl_getType(self) } }
        /// Calls the `Gl::buffer_data_untyped` function.
        pub fn buffer_data_untyped(&self, target: u32, size: isize, data: GlVoidPtrConst, usage: u32)  { unsafe { crate::dll::AzGl_bufferDataUntyped(self, target, size, data, usage) } }
        /// Calls the `Gl::buffer_sub_data_untyped` function.
        pub fn buffer_sub_data_untyped(&self, target: u32, offset: isize, size: isize, data: GlVoidPtrConst)  { unsafe { crate::dll::AzGl_bufferSubDataUntyped(self, target, offset, size, data) } }
        /// Calls the `Gl::map_buffer` function.
        pub fn map_buffer(&self, target: u32, access: u32)  -> crate::gl::GlVoidPtrMut { unsafe { crate::dll::AzGl_mapBuffer(self, target, access) } }
        /// Calls the `Gl::map_buffer_range` function.
        pub fn map_buffer_range(&self, target: u32, offset: isize, length: isize, access: u32)  -> crate::gl::GlVoidPtrMut { unsafe { crate::dll::AzGl_mapBufferRange(self, target, offset, length, access) } }
        /// Calls the `Gl::unmap_buffer` function.
        pub fn unmap_buffer(&self, target: u32)  -> u8 { unsafe { crate::dll::AzGl_unmapBuffer(self, target) } }
        /// Calls the `Gl::tex_buffer` function.
        pub fn tex_buffer(&self, target: u32, internal_format: u32, buffer: u32)  { unsafe { crate::dll::AzGl_texBuffer(self, target, internal_format, buffer) } }
        /// Calls the `Gl::shader_source` function.
        pub fn shader_source(&self, shader: u32, strings: StringVec)  { unsafe { crate::dll::AzGl_shaderSource(self, shader, strings) } }
        /// Calls the `Gl::read_buffer` function.
        pub fn read_buffer(&self, mode: u32)  { unsafe { crate::dll::AzGl_readBuffer(self, mode) } }
        /// Calls the `Gl::read_pixels_into_buffer` function.
        pub fn read_pixels_into_buffer(&self, x: i32, y: i32, width: i32, height: i32, format: u32, pixel_type: u32, dst_buffer: U8VecRefMut)  { unsafe { crate::dll::AzGl_readPixelsIntoBuffer(self, x, y, width, height, format, pixel_type, dst_buffer) } }
        /// Calls the `Gl::read_pixels` function.
        pub fn read_pixels(&self, x: i32, y: i32, width: i32, height: i32, format: u32, pixel_type: u32)  -> crate::vec::U8Vec { unsafe { crate::dll::AzGl_readPixels(self, x, y, width, height, format, pixel_type) } }
        /// Calls the `Gl::read_pixels_into_pbo` function.
        pub fn read_pixels_into_pbo(&self, x: i32, y: i32, width: i32, height: i32, format: u32, pixel_type: u32)  { unsafe { crate::dll::AzGl_readPixelsIntoPbo(self, x, y, width, height, format, pixel_type) } }
        /// Calls the `Gl::sample_coverage` function.
        pub fn sample_coverage(&self, value: f32, invert: bool)  { unsafe { crate::dll::AzGl_sampleCoverage(self, value, invert) } }
        /// Calls the `Gl::polygon_offset` function.
        pub fn polygon_offset(&self, factor: f32, units: f32)  { unsafe { crate::dll::AzGl_polygonOffset(self, factor, units) } }
        /// Calls the `Gl::pixel_store_i` function.
        pub fn pixel_store_i(&self, name: u32, param: i32)  { unsafe { crate::dll::AzGl_pixelStoreI(self, name, param) } }
        /// Calls the `Gl::gen_buffers` function.
        pub fn gen_buffers(&self, n: i32)  -> crate::vec::GLuintVec { unsafe { crate::dll::AzGl_genBuffers(self, n) } }
        /// Calls the `Gl::gen_renderbuffers` function.
        pub fn gen_renderbuffers(&self, n: i32)  -> crate::vec::GLuintVec { unsafe { crate::dll::AzGl_genRenderbuffers(self, n) } }
        /// Calls the `Gl::gen_framebuffers` function.
        pub fn gen_framebuffers(&self, n: i32)  -> crate::vec::GLuintVec { unsafe { crate::dll::AzGl_genFramebuffers(self, n) } }
        /// Calls the `Gl::gen_textures` function.
        pub fn gen_textures(&self, n: i32)  -> crate::vec::GLuintVec { unsafe { crate::dll::AzGl_genTextures(self, n) } }
        /// Calls the `Gl::gen_vertex_arrays` function.
        pub fn gen_vertex_arrays(&self, n: i32)  -> crate::vec::GLuintVec { unsafe { crate::dll::AzGl_genVertexArrays(self, n) } }
        /// Calls the `Gl::gen_queries` function.
        pub fn gen_queries(&self, n: i32)  -> crate::vec::GLuintVec { unsafe { crate::dll::AzGl_genQueries(self, n) } }
        /// Calls the `Gl::begin_query` function.
        pub fn begin_query(&self, target: u32, id: u32)  { unsafe { crate::dll::AzGl_beginQuery(self, target, id) } }
        /// Calls the `Gl::end_query` function.
        pub fn end_query(&self, target: u32)  { unsafe { crate::dll::AzGl_endQuery(self, target) } }
        /// Calls the `Gl::query_counter` function.
        pub fn query_counter(&self, id: u32, target: u32)  { unsafe { crate::dll::AzGl_queryCounter(self, id, target) } }
        /// Calls the `Gl::get_query_object_iv` function.
        pub fn get_query_object_iv(&self, id: u32, pname: u32)  -> i32 { unsafe { crate::dll::AzGl_getQueryObjectIv(self, id, pname) } }
        /// Calls the `Gl::get_query_object_uiv` function.
        pub fn get_query_object_uiv(&self, id: u32, pname: u32)  -> u32 { unsafe { crate::dll::AzGl_getQueryObjectUiv(self, id, pname) } }
        /// Calls the `Gl::get_query_object_i64v` function.
        pub fn get_query_object_i64v(&self, id: u32, pname: u32)  -> i64 { unsafe { crate::dll::AzGl_getQueryObjectI64V(self, id, pname) } }
        /// Calls the `Gl::get_query_object_ui64v` function.
        pub fn get_query_object_ui64v(&self, id: u32, pname: u32)  -> u64 { unsafe { crate::dll::AzGl_getQueryObjectUi64V(self, id, pname) } }
        /// Calls the `Gl::delete_queries` function.
        pub fn delete_queries(&self, queries: GLuintVecRef)  { unsafe { crate::dll::AzGl_deleteQueries(self, queries) } }
        /// Calls the `Gl::delete_vertex_arrays` function.
        pub fn delete_vertex_arrays(&self, vertex_arrays: GLuintVecRef)  { unsafe { crate::dll::AzGl_deleteVertexArrays(self, vertex_arrays) } }
        /// Calls the `Gl::delete_buffers` function.
        pub fn delete_buffers(&self, buffers: GLuintVecRef)  { unsafe { crate::dll::AzGl_deleteBuffers(self, buffers) } }
        /// Calls the `Gl::delete_renderbuffers` function.
        pub fn delete_renderbuffers(&self, renderbuffers: GLuintVecRef)  { unsafe { crate::dll::AzGl_deleteRenderbuffers(self, renderbuffers) } }
        /// Calls the `Gl::delete_framebuffers` function.
        pub fn delete_framebuffers(&self, framebuffers: GLuintVecRef)  { unsafe { crate::dll::AzGl_deleteFramebuffers(self, framebuffers) } }
        /// Calls the `Gl::delete_textures` function.
        pub fn delete_textures(&self, textures: GLuintVecRef)  { unsafe { crate::dll::AzGl_deleteTextures(self, textures) } }
        /// Calls the `Gl::framebuffer_renderbuffer` function.
        pub fn framebuffer_renderbuffer(&self, target: u32, attachment: u32, renderbuffertarget: u32, renderbuffer: u32)  { unsafe { crate::dll::AzGl_framebufferRenderbuffer(self, target, attachment, renderbuffertarget, renderbuffer) } }
        /// Calls the `Gl::renderbuffer_storage` function.
        pub fn renderbuffer_storage(&self, target: u32, internalformat: u32, width: i32, height: i32)  { unsafe { crate::dll::AzGl_renderbufferStorage(self, target, internalformat, width, height) } }
        /// Calls the `Gl::depth_func` function.
        pub fn depth_func(&self, func: u32)  { unsafe { crate::dll::AzGl_depthFunc(self, func) } }
        /// Calls the `Gl::active_texture` function.
        pub fn active_texture(&self, texture: u32)  { unsafe { crate::dll::AzGl_activeTexture(self, texture) } }
        /// Calls the `Gl::attach_shader` function.
        pub fn attach_shader(&self, program: u32, shader: u32)  { unsafe { crate::dll::AzGl_attachShader(self, program, shader) } }
        /// Calls the `Gl::bind_attrib_location` function.
        pub fn bind_attrib_location(&self, program: u32, index: u32, name: Refstr)  { unsafe { crate::dll::AzGl_bindAttribLocation(self, program, index, name) } }
        /// Calls the `Gl::get_uniform_iv` function.
        pub fn get_uniform_iv(&self, program: u32, location: i32, result: GLintVecRefMut)  { unsafe { crate::dll::AzGl_getUniformIv(self, program, location, result) } }
        /// Calls the `Gl::get_uniform_fv` function.
        pub fn get_uniform_fv(&self, program: u32, location: i32, result: GLfloatVecRefMut)  { unsafe { crate::dll::AzGl_getUniformFv(self, program, location, result) } }
        /// Calls the `Gl::get_uniform_block_index` function.
        pub fn get_uniform_block_index(&self, program: u32, name: Refstr)  -> u32 { unsafe { crate::dll::AzGl_getUniformBlockIndex(self, program, name) } }
        /// Calls the `Gl::get_uniform_indices` function.
        pub fn get_uniform_indices(&self, program: u32, names: RefstrVecRef)  -> crate::vec::GLuintVec { unsafe { crate::dll::AzGl_getUniformIndices(self, program, names) } }
        /// Calls the `Gl::bind_buffer_base` function.
        pub fn bind_buffer_base(&self, target: u32, index: u32, buffer: u32)  { unsafe { crate::dll::AzGl_bindBufferBase(self, target, index, buffer) } }
        /// Calls the `Gl::bind_buffer_range` function.
        pub fn bind_buffer_range(&self, target: u32, index: u32, buffer: u32, offset: isize, size: isize)  { unsafe { crate::dll::AzGl_bindBufferRange(self, target, index, buffer, offset, size) } }
        /// Calls the `Gl::uniform_block_binding` function.
        pub fn uniform_block_binding(&self, program: u32, uniform_block_index: u32, uniform_block_binding: u32)  { unsafe { crate::dll::AzGl_uniformBlockBinding(self, program, uniform_block_index, uniform_block_binding) } }
        /// Calls the `Gl::bind_buffer` function.
        pub fn bind_buffer(&self, target: u32, buffer: u32)  { unsafe { crate::dll::AzGl_bindBuffer(self, target, buffer) } }
        /// Calls the `Gl::bind_vertex_array` function.
        pub fn bind_vertex_array(&self, vao: u32)  { unsafe { crate::dll::AzGl_bindVertexArray(self, vao) } }
        /// Calls the `Gl::bind_renderbuffer` function.
        pub fn bind_renderbuffer(&self, target: u32, renderbuffer: u32)  { unsafe { crate::dll::AzGl_bindRenderbuffer(self, target, renderbuffer) } }
        /// Calls the `Gl::bind_framebuffer` function.
        pub fn bind_framebuffer(&self, target: u32, framebuffer: u32)  { unsafe { crate::dll::AzGl_bindFramebuffer(self, target, framebuffer) } }
        /// Calls the `Gl::bind_texture` function.
        pub fn bind_texture(&self, target: u32, texture: u32)  { unsafe { crate::dll::AzGl_bindTexture(self, target, texture) } }
        /// Calls the `Gl::draw_buffers` function.
        pub fn draw_buffers(&self, bufs: GLenumVecRef)  { unsafe { crate::dll::AzGl_drawBuffers(self, bufs) } }
        /// Calls the `Gl::tex_image_2d` function.
        pub fn tex_image_2d(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, border: i32, format: u32, ty: u32, opt_data: OptionU8VecRef)  { unsafe { crate::dll::AzGl_texImage2D(self, target, level, internal_format, width, height, border, format, ty, opt_data) } }
        /// Calls the `Gl::compressed_tex_image_2d` function.
        pub fn compressed_tex_image_2d(&self, target: u32, level: i32, internal_format: u32, width: i32, height: i32, border: i32, data: U8VecRef)  { unsafe { crate::dll::AzGl_compressedTexImage2D(self, target, level, internal_format, width, height, border, data) } }
        /// Calls the `Gl::compressed_tex_sub_image_2d` function.
        pub fn compressed_tex_sub_image_2d(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, width: i32, height: i32, format: u32, data: U8VecRef)  { unsafe { crate::dll::AzGl_compressedTexSubImage2D(self, target, level, xoffset, yoffset, width, height, format, data) } }
        /// Calls the `Gl::tex_image_3d` function.
        pub fn tex_image_3d(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, depth: i32, border: i32, format: u32, ty: u32, opt_data: OptionU8VecRef)  { unsafe { crate::dll::AzGl_texImage3D(self, target, level, internal_format, width, height, depth, border, format, ty, opt_data) } }
        /// Calls the `Gl::copy_tex_image_2d` function.
        pub fn copy_tex_image_2d(&self, target: u32, level: i32, internal_format: u32, x: i32, y: i32, width: i32, height: i32, border: i32)  { unsafe { crate::dll::AzGl_copyTexImage2D(self, target, level, internal_format, x, y, width, height, border) } }
        /// Calls the `Gl::copy_tex_sub_image_2d` function.
        pub fn copy_tex_sub_image_2d(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, x: i32, y: i32, width: i32, height: i32)  { unsafe { crate::dll::AzGl_copyTexSubImage2D(self, target, level, xoffset, yoffset, x, y, width, height) } }
        /// Calls the `Gl::copy_tex_sub_image_3d` function.
        pub fn copy_tex_sub_image_3d(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, zoffset: i32, x: i32, y: i32, width: i32, height: i32)  { unsafe { crate::dll::AzGl_copyTexSubImage3D(self, target, level, xoffset, yoffset, zoffset, x, y, width, height) } }
        /// Calls the `Gl::tex_sub_image_2d` function.
        pub fn tex_sub_image_2d(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, width: i32, height: i32, format: u32, ty: u32, data: U8VecRef)  { unsafe { crate::dll::AzGl_texSubImage2D(self, target, level, xoffset, yoffset, width, height, format, ty, data) } }
        /// Calls the `Gl::tex_sub_image_2d_pbo` function.
        pub fn tex_sub_image_2d_pbo(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, width: i32, height: i32, format: u32, ty: u32, offset: usize)  { unsafe { crate::dll::AzGl_texSubImage2DPbo(self, target, level, xoffset, yoffset, width, height, format, ty, offset) } }
        /// Calls the `Gl::tex_sub_image_3d` function.
        pub fn tex_sub_image_3d(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, zoffset: i32, width: i32, height: i32, depth: i32, format: u32, ty: u32, data: U8VecRef)  { unsafe { crate::dll::AzGl_texSubImage3D(self, target, level, xoffset, yoffset, zoffset, width, height, depth, format, ty, data) } }
        /// Calls the `Gl::tex_sub_image_3d_pbo` function.
        pub fn tex_sub_image_3d_pbo(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, zoffset: i32, width: i32, height: i32, depth: i32, format: u32, ty: u32, offset: usize)  { unsafe { crate::dll::AzGl_texSubImage3DPbo(self, target, level, xoffset, yoffset, zoffset, width, height, depth, format, ty, offset) } }
        /// Calls the `Gl::tex_storage_2d` function.
        pub fn tex_storage_2d(&self, target: u32, levels: i32, internal_format: u32, width: i32, height: i32)  { unsafe { crate::dll::AzGl_texStorage2D(self, target, levels, internal_format, width, height) } }
        /// Calls the `Gl::tex_storage_3d` function.
        pub fn tex_storage_3d(&self, target: u32, levels: i32, internal_format: u32, width: i32, height: i32, depth: i32)  { unsafe { crate::dll::AzGl_texStorage3D(self, target, levels, internal_format, width, height, depth) } }
        /// Calls the `Gl::get_tex_image_into_buffer` function.
        pub fn get_tex_image_into_buffer(&self, target: u32, level: i32, format: u32, ty: u32, output: U8VecRefMut)  { unsafe { crate::dll::AzGl_getTexImageIntoBuffer(self, target, level, format, ty, output) } }
        /// Calls the `Gl::copy_image_sub_data` function.
        pub fn copy_image_sub_data(&self, src_name: u32, src_target: u32, src_level: i32, src_x: i32, src_y: i32, src_z: i32, dst_name: u32, dst_target: u32, dst_level: i32, dst_x: i32, dst_y: i32, dst_z: i32, src_width: i32, src_height: i32, src_depth: i32)  { unsafe { crate::dll::AzGl_copyImageSubData(self, src_name, src_target, src_level, src_x, src_y, src_z, dst_name, dst_target, dst_level, dst_x, dst_y, dst_z, src_width, src_height, src_depth) } }
        /// Calls the `Gl::invalidate_framebuffer` function.
        pub fn invalidate_framebuffer(&self, target: u32, attachments: GLenumVecRef)  { unsafe { crate::dll::AzGl_invalidateFramebuffer(self, target, attachments) } }
        /// Calls the `Gl::invalidate_sub_framebuffer` function.
        pub fn invalidate_sub_framebuffer(&self, target: u32, attachments: GLenumVecRef, xoffset: i32, yoffset: i32, width: i32, height: i32)  { unsafe { crate::dll::AzGl_invalidateSubFramebuffer(self, target, attachments, xoffset, yoffset, width, height) } }
        /// Calls the `Gl::get_integer_v` function.
        pub fn get_integer_v(&self, name: u32, result: GLintVecRefMut)  { unsafe { crate::dll::AzGl_getIntegerV(self, name, result) } }
        /// Calls the `Gl::get_integer_64v` function.
        pub fn get_integer_64v(&self, name: u32, result: GLint64VecRefMut)  { unsafe { crate::dll::AzGl_getInteger64V(self, name, result) } }
        /// Calls the `Gl::get_integer_iv` function.
        pub fn get_integer_iv(&self, name: u32, index: u32, result: GLintVecRefMut)  { unsafe { crate::dll::AzGl_getIntegerIv(self, name, index, result) } }
        /// Calls the `Gl::get_integer_64iv` function.
        pub fn get_integer_64iv(&self, name: u32, index: u32, result: GLint64VecRefMut)  { unsafe { crate::dll::AzGl_getInteger64Iv(self, name, index, result) } }
        /// Calls the `Gl::get_boolean_v` function.
        pub fn get_boolean_v(&self, name: u32, result: GLbooleanVecRefMut)  { unsafe { crate::dll::AzGl_getBooleanV(self, name, result) } }
        /// Calls the `Gl::get_float_v` function.
        pub fn get_float_v(&self, name: u32, result: GLfloatVecRefMut)  { unsafe { crate::dll::AzGl_getFloatV(self, name, result) } }
        /// Calls the `Gl::get_framebuffer_attachment_parameter_iv` function.
        pub fn get_framebuffer_attachment_parameter_iv(&self, target: u32, attachment: u32, pname: u32)  -> i32 { unsafe { crate::dll::AzGl_getFramebufferAttachmentParameterIv(self, target, attachment, pname) } }
        /// Calls the `Gl::get_renderbuffer_parameter_iv` function.
        pub fn get_renderbuffer_parameter_iv(&self, target: u32, pname: u32)  -> i32 { unsafe { crate::dll::AzGl_getRenderbufferParameterIv(self, target, pname) } }
        /// Calls the `Gl::get_tex_parameter_iv` function.
        pub fn get_tex_parameter_iv(&self, target: u32, name: u32)  -> i32 { unsafe { crate::dll::AzGl_getTexParameterIv(self, target, name) } }
        /// Calls the `Gl::get_tex_parameter_fv` function.
        pub fn get_tex_parameter_fv(&self, target: u32, name: u32)  -> f32 { unsafe { crate::dll::AzGl_getTexParameterFv(self, target, name) } }
        /// Calls the `Gl::tex_parameter_i` function.
        pub fn tex_parameter_i(&self, target: u32, pname: u32, param: i32)  { unsafe { crate::dll::AzGl_texParameterI(self, target, pname, param) } }
        /// Calls the `Gl::tex_parameter_f` function.
        pub fn tex_parameter_f(&self, target: u32, pname: u32, param: f32)  { unsafe { crate::dll::AzGl_texParameterF(self, target, pname, param) } }
        /// Calls the `Gl::framebuffer_texture_2d` function.
        pub fn framebuffer_texture_2d(&self, target: u32, attachment: u32, textarget: u32, texture: u32, level: i32)  { unsafe { crate::dll::AzGl_framebufferTexture2D(self, target, attachment, textarget, texture, level) } }
        /// Calls the `Gl::framebuffer_texture_layer` function.
        pub fn framebuffer_texture_layer(&self, target: u32, attachment: u32, texture: u32, level: i32, layer: i32)  { unsafe { crate::dll::AzGl_framebufferTextureLayer(self, target, attachment, texture, level, layer) } }
        /// Calls the `Gl::blit_framebuffer` function.
        pub fn blit_framebuffer(&self, src_x0: i32, src_y0: i32, src_x1: i32, src_y1: i32, dst_x0: i32, dst_y0: i32, dst_x1: i32, dst_y1: i32, mask: u32, filter: u32)  { unsafe { crate::dll::AzGl_blitFramebuffer(self, src_x0, src_y0, src_x1, src_y1, dst_x0, dst_y0, dst_x1, dst_y1, mask, filter) } }
        /// Calls the `Gl::vertex_attrib_4f` function.
        pub fn vertex_attrib_4f(&self, index: u32, x: f32, y: f32, z: f32, w: f32)  { unsafe { crate::dll::AzGl_vertexAttrib4F(self, index, x, y, z, w) } }
        /// Calls the `Gl::vertex_attrib_pointer_f32` function.
        pub fn vertex_attrib_pointer_f32(&self, index: u32, size: i32, normalized: bool, stride: i32, offset: u32)  { unsafe { crate::dll::AzGl_vertexAttribPointerF32(self, index, size, normalized, stride, offset) } }
        /// Calls the `Gl::vertex_attrib_pointer` function.
        pub fn vertex_attrib_pointer(&self, index: u32, size: i32, type_: u32, normalized: bool, stride: i32, offset: u32)  { unsafe { crate::dll::AzGl_vertexAttribPointer(self, index, size, type_, normalized, stride, offset) } }
        /// Calls the `Gl::vertex_attrib_i_pointer` function.
        pub fn vertex_attrib_i_pointer(&self, index: u32, size: i32, type_: u32, stride: i32, offset: u32)  { unsafe { crate::dll::AzGl_vertexAttribIPointer(self, index, size, type_, stride, offset) } }
        /// Calls the `Gl::vertex_attrib_divisor` function.
        pub fn vertex_attrib_divisor(&self, index: u32, divisor: u32)  { unsafe { crate::dll::AzGl_vertexAttribDivisor(self, index, divisor) } }
        /// Calls the `Gl::viewport` function.
        pub fn viewport(&self, x: i32, y: i32, width: i32, height: i32)  { unsafe { crate::dll::AzGl_viewport(self, x, y, width, height) } }
        /// Calls the `Gl::scissor` function.
        pub fn scissor(&self, x: i32, y: i32, width: i32, height: i32)  { unsafe { crate::dll::AzGl_scissor(self, x, y, width, height) } }
        /// Calls the `Gl::line_width` function.
        pub fn line_width(&self, width: f32)  { unsafe { crate::dll::AzGl_lineWidth(self, width) } }
        /// Calls the `Gl::use_program` function.
        pub fn use_program(&self, program: u32)  { unsafe { crate::dll::AzGl_useProgram(self, program) } }
        /// Calls the `Gl::validate_program` function.
        pub fn validate_program(&self, program: u32)  { unsafe { crate::dll::AzGl_validateProgram(self, program) } }
        /// Calls the `Gl::draw_arrays` function.
        pub fn draw_arrays(&self, mode: u32, first: i32, count: i32)  { unsafe { crate::dll::AzGl_drawArrays(self, mode, first, count) } }
        /// Calls the `Gl::draw_arrays_instanced` function.
        pub fn draw_arrays_instanced(&self, mode: u32, first: i32, count: i32, primcount: i32)  { unsafe { crate::dll::AzGl_drawArraysInstanced(self, mode, first, count, primcount) } }
        /// Calls the `Gl::draw_elements` function.
        pub fn draw_elements(&self, mode: u32, count: i32, element_type: u32, indices_offset: u32)  { unsafe { crate::dll::AzGl_drawElements(self, mode, count, element_type, indices_offset) } }
        /// Calls the `Gl::draw_elements_instanced` function.
        pub fn draw_elements_instanced(&self, mode: u32, count: i32, element_type: u32, indices_offset: u32, primcount: i32)  { unsafe { crate::dll::AzGl_drawElementsInstanced(self, mode, count, element_type, indices_offset, primcount) } }
        /// Calls the `Gl::blend_color` function.
        pub fn blend_color(&self, r: f32, g: f32, b: f32, a: f32)  { unsafe { crate::dll::AzGl_blendColor(self, r, g, b, a) } }
        /// Calls the `Gl::blend_func` function.
        pub fn blend_func(&self, sfactor: u32, dfactor: u32)  { unsafe { crate::dll::AzGl_blendFunc(self, sfactor, dfactor) } }
        /// Calls the `Gl::blend_func_separate` function.
        pub fn blend_func_separate(&self, src_rgb: u32, dest_rgb: u32, src_alpha: u32, dest_alpha: u32)  { unsafe { crate::dll::AzGl_blendFuncSeparate(self, src_rgb, dest_rgb, src_alpha, dest_alpha) } }
        /// Calls the `Gl::blend_equation` function.
        pub fn blend_equation(&self, mode: u32)  { unsafe { crate::dll::AzGl_blendEquation(self, mode) } }
        /// Calls the `Gl::blend_equation_separate` function.
        pub fn blend_equation_separate(&self, mode_rgb: u32, mode_alpha: u32)  { unsafe { crate::dll::AzGl_blendEquationSeparate(self, mode_rgb, mode_alpha) } }
        /// Calls the `Gl::color_mask` function.
        pub fn color_mask(&self, r: bool, g: bool, b: bool, a: bool)  { unsafe { crate::dll::AzGl_colorMask(self, r, g, b, a) } }
        /// Calls the `Gl::cull_face` function.
        pub fn cull_face(&self, mode: u32)  { unsafe { crate::dll::AzGl_cullFace(self, mode) } }
        /// Calls the `Gl::front_face` function.
        pub fn front_face(&self, mode: u32)  { unsafe { crate::dll::AzGl_frontFace(self, mode) } }
        /// Calls the `Gl::enable` function.
        pub fn enable(&self, cap: u32)  { unsafe { crate::dll::AzGl_enable(self, cap) } }
        /// Calls the `Gl::disable` function.
        pub fn disable(&self, cap: u32)  { unsafe { crate::dll::AzGl_disable(self, cap) } }
        /// Calls the `Gl::hint` function.
        pub fn hint(&self, param_name: u32, param_val: u32)  { unsafe { crate::dll::AzGl_hint(self, param_name, param_val) } }
        /// Calls the `Gl::is_enabled` function.
        pub fn is_enabled(&self, cap: u32)  -> u8 { unsafe { crate::dll::AzGl_isEnabled(self, cap) } }
        /// Calls the `Gl::is_shader` function.
        pub fn is_shader(&self, shader: u32)  -> u8 { unsafe { crate::dll::AzGl_isShader(self, shader) } }
        /// Calls the `Gl::is_texture` function.
        pub fn is_texture(&self, texture: u32)  -> u8 { unsafe { crate::dll::AzGl_isTexture(self, texture) } }
        /// Calls the `Gl::is_framebuffer` function.
        pub fn is_framebuffer(&self, framebuffer: u32)  -> u8 { unsafe { crate::dll::AzGl_isFramebuffer(self, framebuffer) } }
        /// Calls the `Gl::is_renderbuffer` function.
        pub fn is_renderbuffer(&self, renderbuffer: u32)  -> u8 { unsafe { crate::dll::AzGl_isRenderbuffer(self, renderbuffer) } }
        /// Calls the `Gl::check_frame_buffer_status` function.
        pub fn check_frame_buffer_status(&self, target: u32)  -> u32 { unsafe { crate::dll::AzGl_checkFrameBufferStatus(self, target) } }
        /// Calls the `Gl::enable_vertex_attrib_array` function.
        pub fn enable_vertex_attrib_array(&self, index: u32)  { unsafe { crate::dll::AzGl_enableVertexAttribArray(self, index) } }
        /// Calls the `Gl::disable_vertex_attrib_array` function.
        pub fn disable_vertex_attrib_array(&self, index: u32)  { unsafe { crate::dll::AzGl_disableVertexAttribArray(self, index) } }
        /// Calls the `Gl::uniform_1f` function.
        pub fn uniform_1f(&self, location: i32, v0: f32)  { unsafe { crate::dll::AzGl_uniform1F(self, location, v0) } }
        /// Calls the `Gl::uniform_1fv` function.
        pub fn uniform_1fv(&self, location: i32, values: F32VecRef)  { unsafe { crate::dll::AzGl_uniform1Fv(self, location, values) } }
        /// Calls the `Gl::uniform_1i` function.
        pub fn uniform_1i(&self, location: i32, v0: i32)  { unsafe { crate::dll::AzGl_uniform1I(self, location, v0) } }
        /// Calls the `Gl::uniform_1iv` function.
        pub fn uniform_1iv(&self, location: i32, values: I32VecRef)  { unsafe { crate::dll::AzGl_uniform1Iv(self, location, values) } }
        /// Calls the `Gl::uniform_1ui` function.
        pub fn uniform_1ui(&self, location: i32, v0: u32)  { unsafe { crate::dll::AzGl_uniform1Ui(self, location, v0) } }
        /// Calls the `Gl::uniform_2f` function.
        pub fn uniform_2f(&self, location: i32, v0: f32, v1: f32)  { unsafe { crate::dll::AzGl_uniform2F(self, location, v0, v1) } }
        /// Calls the `Gl::uniform_2fv` function.
        pub fn uniform_2fv(&self, location: i32, values: F32VecRef)  { unsafe { crate::dll::AzGl_uniform2Fv(self, location, values) } }
        /// Calls the `Gl::uniform_2i` function.
        pub fn uniform_2i(&self, location: i32, v0: i32, v1: i32)  { unsafe { crate::dll::AzGl_uniform2I(self, location, v0, v1) } }
        /// Calls the `Gl::uniform_2iv` function.
        pub fn uniform_2iv(&self, location: i32, values: I32VecRef)  { unsafe { crate::dll::AzGl_uniform2Iv(self, location, values) } }
        /// Calls the `Gl::uniform_2ui` function.
        pub fn uniform_2ui(&self, location: i32, v0: u32, v1: u32)  { unsafe { crate::dll::AzGl_uniform2Ui(self, location, v0, v1) } }
        /// Calls the `Gl::uniform_3f` function.
        pub fn uniform_3f(&self, location: i32, v0: f32, v1: f32, v2: f32)  { unsafe { crate::dll::AzGl_uniform3F(self, location, v0, v1, v2) } }
        /// Calls the `Gl::uniform_3fv` function.
        pub fn uniform_3fv(&self, location: i32, values: F32VecRef)  { unsafe { crate::dll::AzGl_uniform3Fv(self, location, values) } }
        /// Calls the `Gl::uniform_3i` function.
        pub fn uniform_3i(&self, location: i32, v0: i32, v1: i32, v2: i32)  { unsafe { crate::dll::AzGl_uniform3I(self, location, v0, v1, v2) } }
        /// Calls the `Gl::uniform_3iv` function.
        pub fn uniform_3iv(&self, location: i32, values: I32VecRef)  { unsafe { crate::dll::AzGl_uniform3Iv(self, location, values) } }
        /// Calls the `Gl::uniform_3ui` function.
        pub fn uniform_3ui(&self, location: i32, v0: u32, v1: u32, v2: u32)  { unsafe { crate::dll::AzGl_uniform3Ui(self, location, v0, v1, v2) } }
        /// Calls the `Gl::uniform_4f` function.
        pub fn uniform_4f(&self, location: i32, x: f32, y: f32, z: f32, w: f32)  { unsafe { crate::dll::AzGl_uniform4F(self, location, x, y, z, w) } }
        /// Calls the `Gl::uniform_4i` function.
        pub fn uniform_4i(&self, location: i32, x: i32, y: i32, z: i32, w: i32)  { unsafe { crate::dll::AzGl_uniform4I(self, location, x, y, z, w) } }
        /// Calls the `Gl::uniform_4iv` function.
        pub fn uniform_4iv(&self, location: i32, values: I32VecRef)  { unsafe { crate::dll::AzGl_uniform4Iv(self, location, values) } }
        /// Calls the `Gl::uniform_4ui` function.
        pub fn uniform_4ui(&self, location: i32, x: u32, y: u32, z: u32, w: u32)  { unsafe { crate::dll::AzGl_uniform4Ui(self, location, x, y, z, w) } }
        /// Calls the `Gl::uniform_4fv` function.
        pub fn uniform_4fv(&self, location: i32, values: F32VecRef)  { unsafe { crate::dll::AzGl_uniform4Fv(self, location, values) } }
        /// Calls the `Gl::uniform_matrix_2fv` function.
        pub fn uniform_matrix_2fv(&self, location: i32, transpose: bool, value: F32VecRef)  { unsafe { crate::dll::AzGl_uniformMatrix2Fv(self, location, transpose, value) } }
        /// Calls the `Gl::uniform_matrix_3fv` function.
        pub fn uniform_matrix_3fv(&self, location: i32, transpose: bool, value: F32VecRef)  { unsafe { crate::dll::AzGl_uniformMatrix3Fv(self, location, transpose, value) } }
        /// Calls the `Gl::uniform_matrix_4fv` function.
        pub fn uniform_matrix_4fv(&self, location: i32, transpose: bool, value: F32VecRef)  { unsafe { crate::dll::AzGl_uniformMatrix4Fv(self, location, transpose, value) } }
        /// Calls the `Gl::depth_mask` function.
        pub fn depth_mask(&self, flag: bool)  { unsafe { crate::dll::AzGl_depthMask(self, flag) } }
        /// Calls the `Gl::depth_range` function.
        pub fn depth_range(&self, near: f64, far: f64)  { unsafe { crate::dll::AzGl_depthRange(self, near, far) } }
        /// Calls the `Gl::get_active_attrib` function.
        pub fn get_active_attrib(&self, program: u32, index: u32)  -> crate::gl::GetActiveAttribReturn { unsafe { crate::dll::AzGl_getActiveAttrib(self, program, index) } }
        /// Calls the `Gl::get_active_uniform` function.
        pub fn get_active_uniform(&self, program: u32, index: u32)  -> crate::gl::GetActiveUniformReturn { unsafe { crate::dll::AzGl_getActiveUniform(self, program, index) } }
        /// Calls the `Gl::get_active_uniforms_iv` function.
        pub fn get_active_uniforms_iv(&self, program: u32, indices: GLuintVec, pname: u32)  -> crate::vec::GLintVec { unsafe { crate::dll::AzGl_getActiveUniformsIv(self, program, indices, pname) } }
        /// Calls the `Gl::get_active_uniform_block_i` function.
        pub fn get_active_uniform_block_i(&self, program: u32, index: u32, pname: u32)  -> i32 { unsafe { crate::dll::AzGl_getActiveUniformBlockI(self, program, index, pname) } }
        /// Calls the `Gl::get_active_uniform_block_iv` function.
        pub fn get_active_uniform_block_iv(&self, program: u32, index: u32, pname: u32)  -> crate::vec::GLintVec { unsafe { crate::dll::AzGl_getActiveUniformBlockIv(self, program, index, pname) } }
        /// Calls the `Gl::get_active_uniform_block_name` function.
        pub fn get_active_uniform_block_name(&self, program: u32, index: u32)  -> crate::str::String { unsafe { crate::dll::AzGl_getActiveUniformBlockName(self, program, index) } }
        /// Calls the `Gl::get_attrib_location` function.
        pub fn get_attrib_location(&self, program: u32, name: Refstr)  -> i32 { unsafe { crate::dll::AzGl_getAttribLocation(self, program, name) } }
        /// Calls the `Gl::get_frag_data_location` function.
        pub fn get_frag_data_location(&self, program: u32, name: Refstr)  -> i32 { unsafe { crate::dll::AzGl_getFragDataLocation(self, program, name) } }
        /// Calls the `Gl::get_uniform_location` function.
        pub fn get_uniform_location(&self, program: u32, name: Refstr)  -> i32 { unsafe { crate::dll::AzGl_getUniformLocation(self, program, name) } }
        /// Calls the `Gl::get_program_info_log` function.
        pub fn get_program_info_log(&self, program: u32)  -> crate::str::String { unsafe { crate::dll::AzGl_getProgramInfoLog(self, program) } }
        /// Calls the `Gl::get_program_iv` function.
        pub fn get_program_iv(&self, program: u32, pname: u32, result: GLintVecRefMut)  { unsafe { crate::dll::AzGl_getProgramIv(self, program, pname, result) } }
        /// Calls the `Gl::get_program_binary` function.
        pub fn get_program_binary(&self, program: u32)  -> crate::gl::GetProgramBinaryReturn { unsafe { crate::dll::AzGl_getProgramBinary(self, program) } }
        /// Calls the `Gl::program_binary` function.
        pub fn program_binary(&self, program: u32, format: u32, binary: U8VecRef)  { unsafe { crate::dll::AzGl_programBinary(self, program, format, binary) } }
        /// Calls the `Gl::program_parameter_i` function.
        pub fn program_parameter_i(&self, program: u32, pname: u32, value: i32)  { unsafe { crate::dll::AzGl_programParameterI(self, program, pname, value) } }
        /// Calls the `Gl::get_vertex_attrib_iv` function.
        pub fn get_vertex_attrib_iv(&self, index: u32, pname: u32, result: GLintVecRefMut)  { unsafe { crate::dll::AzGl_getVertexAttribIv(self, index, pname, result) } }
        /// Calls the `Gl::get_vertex_attrib_fv` function.
        pub fn get_vertex_attrib_fv(&self, index: u32, pname: u32, result: GLfloatVecRefMut)  { unsafe { crate::dll::AzGl_getVertexAttribFv(self, index, pname, result) } }
        /// Calls the `Gl::get_vertex_attrib_pointer_v` function.
        pub fn get_vertex_attrib_pointer_v(&self, index: u32, pname: u32)  -> isize { unsafe { crate::dll::AzGl_getVertexAttribPointerV(self, index, pname) } }
        /// Calls the `Gl::get_buffer_parameter_iv` function.
        pub fn get_buffer_parameter_iv(&self, target: u32, pname: u32)  -> i32 { unsafe { crate::dll::AzGl_getBufferParameterIv(self, target, pname) } }
        /// Calls the `Gl::get_shader_info_log` function.
        pub fn get_shader_info_log(&self, shader: u32)  -> crate::str::String { unsafe { crate::dll::AzGl_getShaderInfoLog(self, shader) } }
        /// Calls the `Gl::get_string` function.
        pub fn get_string(&self, which: u32)  -> crate::str::String { unsafe { crate::dll::AzGl_getString(self, which) } }
        /// Calls the `Gl::get_string_i` function.
        pub fn get_string_i(&self, which: u32, index: u32)  -> crate::str::String { unsafe { crate::dll::AzGl_getStringI(self, which, index) } }
        /// Calls the `Gl::get_shader_iv` function.
        pub fn get_shader_iv(&self, shader: u32, pname: u32, result: GLintVecRefMut)  { unsafe { crate::dll::AzGl_getShaderIv(self, shader, pname, result) } }
        /// Calls the `Gl::get_shader_precision_format` function.
        pub fn get_shader_precision_format(&self, shader_type: u32, precision_type: u32)  -> crate::gl::GlShaderPrecisionFormatReturn { unsafe { crate::dll::AzGl_getShaderPrecisionFormat(self, shader_type, precision_type) } }
        /// Calls the `Gl::compile_shader` function.
        pub fn compile_shader(&self, shader: u32)  { unsafe { crate::dll::AzGl_compileShader(self, shader) } }
        /// Calls the `Gl::create_program` function.
        pub fn create_program(&self)  -> u32 { unsafe { crate::dll::AzGl_createProgram(self) } }
        /// Calls the `Gl::delete_program` function.
        pub fn delete_program(&self, program: u32)  { unsafe { crate::dll::AzGl_deleteProgram(self, program) } }
        /// Calls the `Gl::create_shader` function.
        pub fn create_shader(&self, shader_type: u32)  -> u32 { unsafe { crate::dll::AzGl_createShader(self, shader_type) } }
        /// Calls the `Gl::delete_shader` function.
        pub fn delete_shader(&self, shader: u32)  { unsafe { crate::dll::AzGl_deleteShader(self, shader) } }
        /// Calls the `Gl::detach_shader` function.
        pub fn detach_shader(&self, program: u32, shader: u32)  { unsafe { crate::dll::AzGl_detachShader(self, program, shader) } }
        /// Calls the `Gl::link_program` function.
        pub fn link_program(&self, program: u32)  { unsafe { crate::dll::AzGl_linkProgram(self, program) } }
        /// Calls the `Gl::clear_color` function.
        pub fn clear_color(&self, r: f32, g: f32, b: f32, a: f32)  { unsafe { crate::dll::AzGl_clearColor(self, r, g, b, a) } }
        /// Calls the `Gl::clear` function.
        pub fn clear(&self, buffer_mask: u32)  { unsafe { crate::dll::AzGl_clear(self, buffer_mask) } }
        /// Calls the `Gl::clear_depth` function.
        pub fn clear_depth(&self, depth: f64)  { unsafe { crate::dll::AzGl_clearDepth(self, depth) } }
        /// Calls the `Gl::clear_stencil` function.
        pub fn clear_stencil(&self, s: i32)  { unsafe { crate::dll::AzGl_clearStencil(self, s) } }
        /// Calls the `Gl::flush` function.
        pub fn flush(&self)  { unsafe { crate::dll::AzGl_flush(self) } }
        /// Calls the `Gl::finish` function.
        pub fn finish(&self)  { unsafe { crate::dll::AzGl_finish(self) } }
        /// Calls the `Gl::get_error` function.
        pub fn get_error(&self)  -> u32 { unsafe { crate::dll::AzGl_getError(self) } }
        /// Calls the `Gl::stencil_mask` function.
        pub fn stencil_mask(&self, mask: u32)  { unsafe { crate::dll::AzGl_stencilMask(self, mask) } }
        /// Calls the `Gl::stencil_mask_separate` function.
        pub fn stencil_mask_separate(&self, face: u32, mask: u32)  { unsafe { crate::dll::AzGl_stencilMaskSeparate(self, face, mask) } }
        /// Calls the `Gl::stencil_func` function.
        pub fn stencil_func(&self, func: u32, ref_: i32, mask: u32)  { unsafe { crate::dll::AzGl_stencilFunc(self, func, ref_, mask) } }
        /// Calls the `Gl::stencil_func_separate` function.
        pub fn stencil_func_separate(&self, face: u32, func: u32, ref_: i32, mask: u32)  { unsafe { crate::dll::AzGl_stencilFuncSeparate(self, face, func, ref_, mask) } }
        /// Calls the `Gl::stencil_op` function.
        pub fn stencil_op(&self, sfail: u32, dpfail: u32, dppass: u32)  { unsafe { crate::dll::AzGl_stencilOp(self, sfail, dpfail, dppass) } }
        /// Calls the `Gl::stencil_op_separate` function.
        pub fn stencil_op_separate(&self, face: u32, sfail: u32, dpfail: u32, dppass: u32)  { unsafe { crate::dll::AzGl_stencilOpSeparate(self, face, sfail, dpfail, dppass) } }
        /// Calls the `Gl::egl_image_target_texture2d_oes` function.
        pub fn egl_image_target_texture2d_oes(&self, target: u32, image: GlVoidPtrConst)  { unsafe { crate::dll::AzGl_eglImageTargetTexture2DOes(self, target, image) } }
        /// Calls the `Gl::generate_mipmap` function.
        pub fn generate_mipmap(&self, target: u32)  { unsafe { crate::dll::AzGl_generateMipmap(self, target) } }
        /// Calls the `Gl::insert_event_marker_ext` function.
        pub fn insert_event_marker_ext(&self, message: Refstr)  { unsafe { crate::dll::AzGl_insertEventMarkerExt(self, message) } }
        /// Calls the `Gl::push_group_marker_ext` function.
        pub fn push_group_marker_ext(&self, message: Refstr)  { unsafe { crate::dll::AzGl_pushGroupMarkerExt(self, message) } }
        /// Calls the `Gl::pop_group_marker_ext` function.
        pub fn pop_group_marker_ext(&self)  { unsafe { crate::dll::AzGl_popGroupMarkerExt(self) } }
        /// Calls the `Gl::debug_message_insert_khr` function.
        pub fn debug_message_insert_khr(&self, source: u32, type_: u32, id: u32, severity: u32, message: Refstr)  { unsafe { crate::dll::AzGl_debugMessageInsertKhr(self, source, type_, id, severity, message) } }
        /// Calls the `Gl::push_debug_group_khr` function.
        pub fn push_debug_group_khr(&self, source: u32, id: u32, message: Refstr)  { unsafe { crate::dll::AzGl_pushDebugGroupKhr(self, source, id, message) } }
        /// Calls the `Gl::pop_debug_group_khr` function.
        pub fn pop_debug_group_khr(&self)  { unsafe { crate::dll::AzGl_popDebugGroupKhr(self) } }
        /// Calls the `Gl::fence_sync` function.
        pub fn fence_sync(&self, condition: u32, flags: u32)  -> crate::gl::GLsyncPtr { unsafe { crate::dll::AzGl_fenceSync(self, condition, flags) } }
        /// Calls the `Gl::client_wait_sync` function.
        pub fn client_wait_sync(&self, sync: GLsyncPtr, flags: u32, timeout: u64)  -> u32 { unsafe { crate::dll::AzGl_clientWaitSync(self, sync, flags, timeout) } }
        /// Calls the `Gl::wait_sync` function.
        pub fn wait_sync(&self, sync: GLsyncPtr, flags: u32, timeout: u64)  { unsafe { crate::dll::AzGl_waitSync(self, sync, flags, timeout) } }
        /// Calls the `Gl::delete_sync` function.
        pub fn delete_sync(&self, sync: GLsyncPtr)  { unsafe { crate::dll::AzGl_deleteSync(self, sync) } }
        /// Calls the `Gl::texture_range_apple` function.
        pub fn texture_range_apple(&self, target: u32, data: U8VecRef)  { unsafe { crate::dll::AzGl_textureRangeApple(self, target, data) } }
        /// Calls the `Gl::gen_fences_apple` function.
        pub fn gen_fences_apple(&self, n: i32)  -> crate::vec::GLuintVec { unsafe { crate::dll::AzGl_genFencesApple(self, n) } }
        /// Calls the `Gl::delete_fences_apple` function.
        pub fn delete_fences_apple(&self, fences: GLuintVecRef)  { unsafe { crate::dll::AzGl_deleteFencesApple(self, fences) } }
        /// Calls the `Gl::set_fence_apple` function.
        pub fn set_fence_apple(&self, fence: u32)  { unsafe { crate::dll::AzGl_setFenceApple(self, fence) } }
        /// Calls the `Gl::finish_fence_apple` function.
        pub fn finish_fence_apple(&self, fence: u32)  { unsafe { crate::dll::AzGl_finishFenceApple(self, fence) } }
        /// Calls the `Gl::test_fence_apple` function.
        pub fn test_fence_apple(&self, fence: u32)  { unsafe { crate::dll::AzGl_testFenceApple(self, fence) } }
        /// Calls the `Gl::test_object_apple` function.
        pub fn test_object_apple(&self, object: u32, name: u32)  -> u8 { unsafe { crate::dll::AzGl_testObjectApple(self, object, name) } }
        /// Calls the `Gl::finish_object_apple` function.
        pub fn finish_object_apple(&self, object: u32, name: u32)  { unsafe { crate::dll::AzGl_finishObjectApple(self, object, name) } }
        /// Calls the `Gl::get_frag_data_index` function.
        pub fn get_frag_data_index(&self, program: u32, name: Refstr)  -> i32 { unsafe { crate::dll::AzGl_getFragDataIndex(self, program, name) } }
        /// Calls the `Gl::blend_barrier_khr` function.
        pub fn blend_barrier_khr(&self)  { unsafe { crate::dll::AzGl_blendBarrierKhr(self) } }
        /// Calls the `Gl::bind_frag_data_location_indexed` function.
        pub fn bind_frag_data_location_indexed(&self, program: u32, color_number: u32, index: u32, name: Refstr)  { unsafe { crate::dll::AzGl_bindFragDataLocationIndexed(self, program, color_number, index, name) } }
        /// Calls the `Gl::get_debug_messages` function.
        pub fn get_debug_messages(&self)  -> crate::vec::DebugMessageVec { unsafe { crate::dll::AzGl_getDebugMessages(self) } }
        /// Calls the `Gl::provoking_vertex_angle` function.
        pub fn provoking_vertex_angle(&self, mode: u32)  { unsafe { crate::dll::AzGl_provokingVertexAngle(self, mode) } }
        /// Calls the `Gl::gen_vertex_arrays_apple` function.
        pub fn gen_vertex_arrays_apple(&self, n: i32)  -> crate::vec::GLuintVec { unsafe { crate::dll::AzGl_genVertexArraysApple(self, n) } }
        /// Calls the `Gl::bind_vertex_array_apple` function.
        pub fn bind_vertex_array_apple(&self, vao: u32)  { unsafe { crate::dll::AzGl_bindVertexArrayApple(self, vao) } }
        /// Calls the `Gl::delete_vertex_arrays_apple` function.
        pub fn delete_vertex_arrays_apple(&self, vertex_arrays: GLuintVecRef)  { unsafe { crate::dll::AzGl_deleteVertexArraysApple(self, vertex_arrays) } }
        /// Calls the `Gl::copy_texture_chromium` function.
        pub fn copy_texture_chromium(&self, source_id: u32, source_level: i32, dest_target: u32, dest_id: u32, dest_level: i32, internal_format: i32, dest_type: u32, unpack_flip_y: u8, unpack_premultiply_alpha: u8, unpack_unmultiply_alpha: u8)  { unsafe { crate::dll::AzGl_copyTextureChromium(self, source_id, source_level, dest_target, dest_id, dest_level, internal_format, dest_type, unpack_flip_y, unpack_premultiply_alpha, unpack_unmultiply_alpha) } }
        /// Calls the `Gl::copy_sub_texture_chromium` function.
        pub fn copy_sub_texture_chromium(&self, source_id: u32, source_level: i32, dest_target: u32, dest_id: u32, dest_level: i32, x_offset: i32, y_offset: i32, x: i32, y: i32, width: i32, height: i32, unpack_flip_y: u8, unpack_premultiply_alpha: u8, unpack_unmultiply_alpha: u8)  { unsafe { crate::dll::AzGl_copySubTextureChromium(self, source_id, source_level, dest_target, dest_id, dest_level, x_offset, y_offset, x, y, width, height, unpack_flip_y, unpack_premultiply_alpha, unpack_unmultiply_alpha) } }
        /// Calls the `Gl::egl_image_target_renderbuffer_storage_oes` function.
        pub fn egl_image_target_renderbuffer_storage_oes(&self, target: u32, image: GlVoidPtrConst)  { unsafe { crate::dll::AzGl_eglImageTargetRenderbufferStorageOes(self, target, image) } }
        /// Calls the `Gl::copy_texture_3d_angle` function.
        pub fn copy_texture_3d_angle(&self, source_id: u32, source_level: i32, dest_target: u32, dest_id: u32, dest_level: i32, internal_format: i32, dest_type: u32, unpack_flip_y: u8, unpack_premultiply_alpha: u8, unpack_unmultiply_alpha: u8)  { unsafe { crate::dll::AzGl_copyTexture3DAngle(self, source_id, source_level, dest_target, dest_id, dest_level, internal_format, dest_type, unpack_flip_y, unpack_premultiply_alpha, unpack_unmultiply_alpha) } }
        /// Calls the `Gl::copy_sub_texture_3d_angle` function.
        pub fn copy_sub_texture_3d_angle(&self, source_id: u32, source_level: i32, dest_target: u32, dest_id: u32, dest_level: i32, x_offset: i32, y_offset: i32, z_offset: i32, x: i32, y: i32, z: i32, width: i32, height: i32, depth: i32, unpack_flip_y: u8, unpack_premultiply_alpha: u8, unpack_unmultiply_alpha: u8)  { unsafe { crate::dll::AzGl_copySubTexture3DAngle(self, source_id, source_level, dest_target, dest_id, dest_level, x_offset, y_offset, z_offset, x, y, z, width, height, depth, unpack_flip_y, unpack_premultiply_alpha, unpack_unmultiply_alpha) } }
        /// Calls the `Gl::buffer_storage` function.
        pub fn buffer_storage(&self, target: u32, size: isize, data: GlVoidPtrConst, flags: u32)  { unsafe { crate::dll::AzGl_bufferStorage(self, target, size, data, flags) } }
        /// Calls the `Gl::flush_mapped_buffer_range` function.
        pub fn flush_mapped_buffer_range(&self, target: u32, offset: isize, length: isize)  { unsafe { crate::dll::AzGl_flushMappedBufferRange(self, target, offset, length) } }
    }

    impl Clone for Gl { fn clone(&self) -> Self { unsafe { crate::dll::AzGl_deepCopy(self) } } }
    impl Drop for Gl { fn drop(&mut self) { unsafe { crate::dll::AzGl_delete(self) } } }
    /// `GlShaderPrecisionFormatReturn` struct
    
#[doc(inline)] pub use crate::dll::AzGlShaderPrecisionFormatReturn as GlShaderPrecisionFormatReturn;
    /// `VertexAttributeType` struct
    
#[doc(inline)] pub use crate::dll::AzVertexAttributeType as VertexAttributeType;
    /// `VertexAttribute` struct
    
#[doc(inline)] pub use crate::dll::AzVertexAttribute as VertexAttribute;
    /// `VertexLayout` struct
    
#[doc(inline)] pub use crate::dll::AzVertexLayout as VertexLayout;
    /// `VertexArrayObject` struct
    
#[doc(inline)] pub use crate::dll::AzVertexArrayObject as VertexArrayObject;
    /// `IndexBufferFormat` struct
    
#[doc(inline)] pub use crate::dll::AzIndexBufferFormat as IndexBufferFormat;
    /// `VertexBuffer` struct
    
#[doc(inline)] pub use crate::dll::AzVertexBuffer as VertexBuffer;
    /// `GlType` struct
    
#[doc(inline)] pub use crate::dll::AzGlType as GlType;
    /// `DebugMessage` struct
    
#[doc(inline)] pub use crate::dll::AzDebugMessage as DebugMessage;
    /// C-ABI stable reexport of `&[u8]`
    
#[doc(inline)] pub use crate::dll::AzU8VecRef as U8VecRef;
    /// C-ABI stable reexport of `&mut [u8]`
    
#[doc(inline)] pub use crate::dll::AzU8VecRefMut as U8VecRefMut;
    /// C-ABI stable reexport of `&[f32]`
    
#[doc(inline)] pub use crate::dll::AzF32VecRef as F32VecRef;
    /// C-ABI stable reexport of `&[i32]`
    
#[doc(inline)] pub use crate::dll::AzI32VecRef as I32VecRef;
    /// C-ABI stable reexport of `&[GLuint]` aka `&[u32]`
    
#[doc(inline)] pub use crate::dll::AzGLuintVecRef as GLuintVecRef;
    /// C-ABI stable reexport of `&[GLenum]` aka `&[u32]`
    
#[doc(inline)] pub use crate::dll::AzGLenumVecRef as GLenumVecRef;
    /// C-ABI stable reexport of `&mut [GLint]` aka `&mut [i32]`
    
#[doc(inline)] pub use crate::dll::AzGLintVecRefMut as GLintVecRefMut;
    /// C-ABI stable reexport of `&mut [GLint64]` aka `&mut [i64]`
    
#[doc(inline)] pub use crate::dll::AzGLint64VecRefMut as GLint64VecRefMut;
    /// C-ABI stable reexport of `&mut [GLboolean]` aka `&mut [u8]`
    
#[doc(inline)] pub use crate::dll::AzGLbooleanVecRefMut as GLbooleanVecRefMut;
    /// C-ABI stable reexport of `&mut [GLfloat]` aka `&mut [f32]`
    
#[doc(inline)] pub use crate::dll::AzGLfloatVecRefMut as GLfloatVecRefMut;
    /// C-ABI stable reexport of `&[Refstr]` aka `&mut [&str]`
    
#[doc(inline)] pub use crate::dll::AzRefstrVecRef as RefstrVecRef;
    /// C-ABI stable reexport of `&str`
    
#[doc(inline)] pub use crate::dll::AzRefstr as Refstr;
    /// C-ABI stable reexport of `(U8Vec, u32)`
    
#[doc(inline)] pub use crate::dll::AzGetProgramBinaryReturn as GetProgramBinaryReturn;
    /// C-ABI stable reexport of `(i32, u32, AzString)`
    
#[doc(inline)] pub use crate::dll::AzGetActiveAttribReturn as GetActiveAttribReturn;
    /// C-ABI stable reexport of `*const gleam::gl::GLsync`
    
#[doc(inline)] pub use crate::dll::AzGLsyncPtr as GLsyncPtr;
    impl Clone for GLsyncPtr { fn clone(&self) -> Self { unsafe { crate::dll::AzGLsyncPtr_deepCopy(self) } } }
    impl Drop for GLsyncPtr { fn drop(&mut self) { unsafe { crate::dll::AzGLsyncPtr_delete(self) } } }
    /// C-ABI stable reexport of `(i32, u32, AzString)`
    
#[doc(inline)] pub use crate::dll::AzGetActiveUniformReturn as GetActiveUniformReturn;
    /// `TextureFlags` struct
    
#[doc(inline)] pub use crate::dll::AzTextureFlags as TextureFlags;
    impl TextureFlags {
        /// Default texture flags (not opaque, not a video texture)
        pub fn default() -> Self { unsafe { crate::dll::AzTextureFlags_default() } }
    }

}

pub mod image {
    #![allow(dead_code, unused_imports)]
    //! Struct definitions for image loading
    use crate::dll::*;
    use core::ffi::c_void;
    use crate::gl::{Texture, U8VecRef};
    use crate::callbacks::{RefAny, RenderImageCallback};
    use crate::window::LayoutSize;
    use crate::svg::{SvgNode, SvgStyle};
    /// `ImageRef` struct
    
#[doc(inline)] pub use crate::dll::AzImageRef as ImageRef;
    impl ImageRef {
        /// Creates an "invalid" image with a width and height that reserves an image key, but does not render anything
        pub fn invalid(width: usize, height: usize, format: RawImageFormat) -> Self { unsafe { crate::dll::AzImageRef_invalid(width, height, format) } }
        /// Creates an image reference from a CPU-backed buffer
        pub fn raw_image(data: RawImage) ->  crate::option::OptionImageRef { unsafe { crate::dll::AzImageRef_rawImage(data) } }
        /// Creates an image reference from an OpenGL texture
        pub fn gl_texture(texture: Texture) -> Self { unsafe { crate::dll::AzImageRef_glTexture(texture) } }
        /// Creates an image reference from a callback that is going to be rendered with the given nodes computed size
        pub fn callback(callback: RenderImageCallback, data: RefAny) -> Self { unsafe { crate::dll::AzImageRef_callback(callback, data) } }
        /// Creates a new copy of the image bytes instead of shallow-copying the reference
        pub fn clone_bytes(&self)  -> crate::image::ImageRef { unsafe { crate::dll::AzImageRef_cloneBytes(self) } }
        /// Returns whether the image is a null (invalid) image
        pub fn is_invalid(&self)  -> bool { unsafe { crate::dll::AzImageRef_isInvalid(self) } }
        /// Returns whether the image is a GL texture
        pub fn is_gl_texture(&self)  -> bool { unsafe { crate::dll::AzImageRef_isGlTexture(self) } }
        /// Returns whether the image is a raw (CPU-decoded) image
        pub fn is_raw_image(&self)  -> bool { unsafe { crate::dll::AzImageRef_isRawImage(self) } }
        /// Returns whether the image is a `RenderImageCallback`
        pub fn is_callback(&self)  -> bool { unsafe { crate::dll::AzImageRef_isCallback(self) } }
    }

    impl Clone for ImageRef { fn clone(&self) -> Self { unsafe { crate::dll::AzImageRef_deepCopy(self) } } }
    impl Drop for ImageRef { fn drop(&mut self) { unsafe { crate::dll::AzImageRef_delete(self) } } }
    /// `RawImage` struct
    
#[doc(inline)] pub use crate::dll::AzRawImage as RawImage;
    impl RawImage {
        /// Returns a zero-sized image
        pub fn empty() -> Self { unsafe { crate::dll::AzRawImage_empty() } }
        /// Allocates a width * height, single-channel image with zeroed bytes
        pub fn allocate_clip_mask(size: LayoutSize) -> Self { unsafe { crate::dll::AzRawImage_allocateClipMask(size) } }
        /// Decodes a RawImage from any supported image format - automatically guesses the format based on magic header
        pub fn decode_image_bytes_any(bytes: U8VecRef) ->  crate::error::ResultRawImageDecodeImageError { unsafe { crate::dll::AzRawImage_decodeImageBytesAny(bytes) } }
        /// Calls the `RawImage::draw_clip_mask` function.
        pub fn draw_clip_mask(&mut self, node: SvgNode, style: SvgStyle)  -> bool { unsafe { crate::dll::AzRawImage_drawClipMask(self, node, style) } }
        /// Encodes the RawImage in the BMP image format
        pub fn encode_bmp(&self)  -> crate::error::ResultU8VecEncodeImageError { unsafe { crate::dll::AzRawImage_encodeBmp(self) } }
        /// Encodes the RawImage in the PNG image format
        pub fn encode_png(&self)  -> crate::error::ResultU8VecEncodeImageError { unsafe { crate::dll::AzRawImage_encodePng(self) } }
        /// Encodes the RawImage in the JPG image format
        pub fn encode_jpeg(&self)  -> crate::error::ResultU8VecEncodeImageError { unsafe { crate::dll::AzRawImage_encodeJpeg(self) } }
        /// Encodes the RawImage in the TGA image format
        pub fn encode_tga(&self)  -> crate::error::ResultU8VecEncodeImageError { unsafe { crate::dll::AzRawImage_encodeTga(self) } }
        /// Encodes the RawImage in the PNM image format
        pub fn encode_pnm(&self)  -> crate::error::ResultU8VecEncodeImageError { unsafe { crate::dll::AzRawImage_encodePnm(self) } }
        /// Encodes the RawImage in the GIF image format
        pub fn encode_gif(&self)  -> crate::error::ResultU8VecEncodeImageError { unsafe { crate::dll::AzRawImage_encodeGif(self) } }
        /// Encodes the RawImage in the TIFF image format
        pub fn encode_tiff(&self)  -> crate::error::ResultU8VecEncodeImageError { unsafe { crate::dll::AzRawImage_encodeTiff(self) } }
    }

    /// `ImageMask` struct
    
#[doc(inline)] pub use crate::dll::AzImageMask as ImageMask;
    /// `RawImageFormat` struct
    
#[doc(inline)] pub use crate::dll::AzRawImageFormat as RawImageFormat;
    /// `EncodeImageError` struct
    
#[doc(inline)] pub use crate::dll::AzEncodeImageError as EncodeImageError;
    /// `DecodeImageError` struct
    
#[doc(inline)] pub use crate::dll::AzDecodeImageError as DecodeImageError;
    /// `RawImageData` struct
    
#[doc(inline)] pub use crate::dll::AzRawImageData as RawImageData;
}

pub mod font {
    #![allow(dead_code, unused_imports)]
    //! Font decoding / parsing module
    use crate::dll::*;
    use core::ffi::c_void;
    /// `ParsedFontDestructorFnType` struct
    
#[doc(inline)] pub use crate::dll::AzParsedFontDestructorFnType as ParsedFontDestructorFnType;
    /// `FontMetrics` struct
    
#[doc(inline)] pub use crate::dll::AzFontMetrics as FontMetrics;
    /// Source data of a font file (bytes)
    
#[doc(inline)] pub use crate::dll::AzFontSource as FontSource;
    /// Atomically reference-counted parsed font data
    
#[doc(inline)] pub use crate::dll::AzFontRef as FontRef;
    impl FontRef {
        /// Parses a new font from bytes. Returns `None` if the font could not be parsed correctly.
        pub fn parse(source: FontSource) ->  crate::option::OptionFontRef { unsafe { crate::dll::AzFontRef_parse(source) } }
        /// Returns the font metrics of the parsed font
        pub fn get_font_metrics(&self)  -> crate::font::FontMetrics { unsafe { crate::dll::AzFontRef_getFontMetrics(self) } }
    }

    impl Clone for FontRef { fn clone(&self) -> Self { unsafe { crate::dll::AzFontRef_deepCopy(self) } } }
    impl Drop for FontRef { fn drop(&mut self) { unsafe { crate::dll::AzFontRef_delete(self) } } }
}

pub mod svg {
    #![allow(dead_code, unused_imports)]
    //! SVG parsing and rendering functions
    use crate::dll::*;
    use core::ffi::c_void;
    use crate::str::String;
    use crate::gl::U8VecRef;
    /// `Svg` struct
    
#[doc(inline)] pub use crate::dll::AzSvg as Svg;
    impl Svg {
        /// Creates a new `Svg` instance.
        pub fn from_string(svg_string: String, parse_options: SvgParseOptions) ->  crate::error::ResultSvgSvgParseError { unsafe { crate::dll::AzSvg_fromString(svg_string, parse_options) } }
        /// Creates a new `Svg` instance.
        pub fn from_bytes(svg_bytes: U8VecRef, parse_options: SvgParseOptions) ->  crate::error::ResultSvgSvgParseError { unsafe { crate::dll::AzSvg_fromBytes(svg_bytes, parse_options) } }
        /// Calls the `Svg::get_root` function.
        pub fn get_root(&self)  -> crate::svg::SvgXmlNode { unsafe { crate::dll::AzSvg_getRoot(self) } }
        /// Calls the `Svg::render` function.
        pub fn render(&self, options: SvgRenderOptions)  -> crate::option::OptionRawImage { unsafe { crate::dll::AzSvg_render(self, options) } }
        /// Calls the `Svg::to_string` function.
        pub fn to_string(&self, options: SvgStringFormatOptions)  -> crate::str::String { unsafe { crate::dll::AzSvg_toString(self, options) } }
    }

    impl Clone for Svg { fn clone(&self) -> Self { unsafe { crate::dll::AzSvg_deepCopy(self) } } }
    impl Drop for Svg { fn drop(&mut self) { unsafe { crate::dll::AzSvg_delete(self) } } }
    /// `SvgXmlNode` struct
    
#[doc(inline)] pub use crate::dll::AzSvgXmlNode as SvgXmlNode;
    impl SvgXmlNode {
        /// Creates a new `SvgXmlNode` instance.
        pub fn parse_from(svg_bytes: U8VecRef, parse_options: SvgParseOptions) ->  crate::error::ResultSvgXmlNodeSvgParseError { unsafe { crate::dll::AzSvgXmlNode_parseFrom(svg_bytes, parse_options) } }
        /// Calls the `SvgXmlNode::render` function.
        pub fn render(&self, options: SvgRenderOptions)  -> crate::option::OptionRawImage { unsafe { crate::dll::AzSvgXmlNode_render(self, options) } }
        /// Calls the `SvgXmlNode::to_string` function.
        pub fn to_string(&self, options: SvgStringFormatOptions)  -> crate::str::String { unsafe { crate::dll::AzSvgXmlNode_toString(self, options) } }
    }

    impl Clone for SvgXmlNode { fn clone(&self) -> Self { unsafe { crate::dll::AzSvgXmlNode_deepCopy(self) } } }
    impl Drop for SvgXmlNode { fn drop(&mut self) { unsafe { crate::dll::AzSvgXmlNode_delete(self) } } }
    /// `SvgMultiPolygon` struct
    
#[doc(inline)] pub use crate::dll::AzSvgMultiPolygon as SvgMultiPolygon;
    impl SvgMultiPolygon {
        /// Calls the `SvgMultiPolygon::tesselate_fill` function.
        pub fn tesselate_fill(&self, fill_style: SvgFillStyle)  -> crate::svg::TesselatedSvgNode { unsafe { crate::dll::AzSvgMultiPolygon_tesselateFill(self, fill_style) } }
        /// Calls the `SvgMultiPolygon::tesselate_stroke` function.
        pub fn tesselate_stroke(&self, stroke_style: SvgStrokeStyle)  -> crate::svg::TesselatedSvgNode { unsafe { crate::dll::AzSvgMultiPolygon_tesselateStroke(self, stroke_style) } }
    }

    /// `SvgNode` struct
    
#[doc(inline)] pub use crate::dll::AzSvgNode as SvgNode;
    impl SvgNode {
        /// Calls the `SvgNode::tesselate_fill` function.
        pub fn tesselate_fill(&self, fill_style: SvgFillStyle)  -> crate::svg::TesselatedSvgNode { unsafe { crate::dll::AzSvgNode_tesselateFill(self, fill_style) } }
        /// Calls the `SvgNode::tesselate_stroke` function.
        pub fn tesselate_stroke(&self, stroke_style: SvgStrokeStyle)  -> crate::svg::TesselatedSvgNode { unsafe { crate::dll::AzSvgNode_tesselateStroke(self, stroke_style) } }
    }

    /// `SvgStyledNode` struct
    
#[doc(inline)] pub use crate::dll::AzSvgStyledNode as SvgStyledNode;
    impl SvgStyledNode {
        /// Calls the `SvgStyledNode::tesselate` function.
        pub fn tesselate(&self)  -> crate::svg::TesselatedSvgNode { unsafe { crate::dll::AzSvgStyledNode_tesselate(self) } }
    }

    /// `SvgCircle` struct
    
#[doc(inline)] pub use crate::dll::AzSvgCircle as SvgCircle;
    impl SvgCircle {
        /// Calls the `SvgCircle::tesselate_fill` function.
        pub fn tesselate_fill(&self, fill_style: SvgFillStyle)  -> crate::svg::TesselatedSvgNode { unsafe { crate::dll::AzSvgCircle_tesselateFill(self, fill_style) } }
        /// Calls the `SvgCircle::tesselate_stroke` function.
        pub fn tesselate_stroke(&self, stroke_style: SvgStrokeStyle)  -> crate::svg::TesselatedSvgNode { unsafe { crate::dll::AzSvgCircle_tesselateStroke(self, stroke_style) } }
    }

    /// `SvgPath` struct
    
#[doc(inline)] pub use crate::dll::AzSvgPath as SvgPath;
    impl SvgPath {
        /// Calls the `SvgPath::tesselate_fill` function.
        pub fn tesselate_fill(&self, fill_style: SvgFillStyle)  -> crate::svg::TesselatedSvgNode { unsafe { crate::dll::AzSvgPath_tesselateFill(self, fill_style) } }
        /// Calls the `SvgPath::tesselate_stroke` function.
        pub fn tesselate_stroke(&self, stroke_style: SvgStrokeStyle)  -> crate::svg::TesselatedSvgNode { unsafe { crate::dll::AzSvgPath_tesselateStroke(self, stroke_style) } }
    }

    /// `SvgPathElement` struct
    
#[doc(inline)] pub use crate::dll::AzSvgPathElement as SvgPathElement;
    /// `SvgLine` struct
    
#[doc(inline)] pub use crate::dll::AzSvgLine as SvgLine;
    /// `SvgPoint` struct
    
#[doc(inline)] pub use crate::dll::AzSvgPoint as SvgPoint;
    /// `SvgQuadraticCurve` struct
    
#[doc(inline)] pub use crate::dll::AzSvgQuadraticCurve as SvgQuadraticCurve;
    /// `SvgCubicCurve` struct
    
#[doc(inline)] pub use crate::dll::AzSvgCubicCurve as SvgCubicCurve;
    /// `SvgRect` struct
    
#[doc(inline)] pub use crate::dll::AzSvgRect as SvgRect;
    impl SvgRect {
        /// Calls the `SvgRect::tesselate_fill` function.
        pub fn tesselate_fill(&self, fill_style: SvgFillStyle)  -> crate::svg::TesselatedSvgNode { unsafe { crate::dll::AzSvgRect_tesselateFill(self, fill_style) } }
        /// Calls the `SvgRect::tesselate_stroke` function.
        pub fn tesselate_stroke(&self, stroke_style: SvgStrokeStyle)  -> crate::svg::TesselatedSvgNode { unsafe { crate::dll::AzSvgRect_tesselateStroke(self, stroke_style) } }
    }

    /// `SvgVertex` struct
    
#[doc(inline)] pub use crate::dll::AzSvgVertex as SvgVertex;
    /// `TesselatedSvgNode` struct
    
#[doc(inline)] pub use crate::dll::AzTesselatedSvgNode as TesselatedSvgNode;
    impl TesselatedSvgNode {
        /// Returns an empty buffer vertices / indices
        pub fn empty() -> Self { unsafe { crate::dll::AzTesselatedSvgNode_empty() } }
        /// Creates a new TesselatedSvgNode by joining all the given nodes together into one array and inserting a `GL_RESTART_INDEX` (`u32::MAX`) into the indices (so that the resulting buffer can be drawn in one draw call).
        pub fn from_nodes(nodes: TesselatedSvgNodeVecRef) -> Self { unsafe { crate::dll::AzTesselatedSvgNode_fromNodes(nodes) } }
    }

    /// Rust wrapper over a `&[TesselatedSvgNode]` or `&Vec<TesselatedSvgNode>`
    
#[doc(inline)] pub use crate::dll::AzTesselatedSvgNodeVecRef as TesselatedSvgNodeVecRef;
    /// `SvgParseOptions` struct
    
#[doc(inline)] pub use crate::dll::AzSvgParseOptions as SvgParseOptions;
    impl SvgParseOptions {
        /// Creates a new `SvgParseOptions` instance.
        pub fn default() -> Self { unsafe { crate::dll::AzSvgParseOptions_default() } }
    }

    /// `ShapeRendering` struct
    
#[doc(inline)] pub use crate::dll::AzShapeRendering as ShapeRendering;
    /// `TextRendering` struct
    
#[doc(inline)] pub use crate::dll::AzTextRendering as TextRendering;
    /// `ImageRendering` struct
    
#[doc(inline)] pub use crate::dll::AzImageRendering as ImageRendering;
    /// `FontDatabase` struct
    
#[doc(inline)] pub use crate::dll::AzFontDatabase as FontDatabase;
    /// `SvgRenderOptions` struct
    
#[doc(inline)] pub use crate::dll::AzSvgRenderOptions as SvgRenderOptions;
    impl SvgRenderOptions {
        /// Creates a new `SvgRenderOptions` instance.
        pub fn default() -> Self { unsafe { crate::dll::AzSvgRenderOptions_default() } }
    }

    /// `SvgStringFormatOptions` struct
    
#[doc(inline)] pub use crate::dll::AzSvgStringFormatOptions as SvgStringFormatOptions;
    /// `Indent` struct
    
#[doc(inline)] pub use crate::dll::AzIndent as Indent;
    /// `SvgFitTo` struct
    
#[doc(inline)] pub use crate::dll::AzSvgFitTo as SvgFitTo;
    /// `SvgStyle` struct
    
#[doc(inline)] pub use crate::dll::AzSvgStyle as SvgStyle;
    /// `SvgFillRule` struct
    
#[doc(inline)] pub use crate::dll::AzSvgFillRule as SvgFillRule;
    /// `SvgTransform` struct
    
#[doc(inline)] pub use crate::dll::AzSvgTransform as SvgTransform;
    /// `SvgFillStyle` struct
    
#[doc(inline)] pub use crate::dll::AzSvgFillStyle as SvgFillStyle;
    /// `SvgStrokeStyle` struct
    
#[doc(inline)] pub use crate::dll::AzSvgStrokeStyle as SvgStrokeStyle;
    /// `SvgLineJoin` struct
    
#[doc(inline)] pub use crate::dll::AzSvgLineJoin as SvgLineJoin;
    /// `SvgLineCap` struct
    
#[doc(inline)] pub use crate::dll::AzSvgLineCap as SvgLineCap;
    /// `SvgDashPattern` struct
    
#[doc(inline)] pub use crate::dll::AzSvgDashPattern as SvgDashPattern;
}

pub mod xml {
    #![allow(dead_code, unused_imports)]
    //! XML parsing / decoding module
    use crate::dll::*;
    use core::ffi::c_void;
    use crate::gl::Refstr;
    /// `Xml` struct
    
#[doc(inline)] pub use crate::dll::AzXml as Xml;
    impl Xml {
        /// Parses an XML document with one or more root nodes
        pub fn from_str(xml_string: Refstr) ->  crate::error::ResultXmlXmlError { unsafe { crate::dll::AzXml_fromStr(xml_string) } }
    }

    /// `XmlNode` struct
    
#[doc(inline)] pub use crate::dll::AzXmlNode as XmlNode;
}

pub mod fs {
    #![allow(dead_code, unused_imports)]
    //! Filesystem / file input and output module
    use crate::dll::*;
    use core::ffi::c_void;
    use crate::str::String;
    use crate::gl::{Refstr, U8VecRef};
    /// **Reference-counted** file handle
    
#[doc(inline)] pub use crate::dll::AzFile as File;
    impl File {
        /// Opens a file at the given path. If the file exists, replaces it with a new file
        pub fn open(path: String) ->  crate::option::OptionFile { unsafe { crate::dll::AzFile_open(path) } }
        /// Creates a file at the given path. If the file exists, replaces it with a new file
        pub fn create(path: String) ->  crate::option::OptionFile { unsafe { crate::dll::AzFile_create(path) } }
        /// Reads the file to a UTF8-encoded String, returns None if the file can't be decoded correctly
        pub fn read_to_string(&mut self)  -> crate::option::OptionString { unsafe { crate::dll::AzFile_readToString(self) } }
        /// Reads the file as bytes, returns None if the file can't be decoded correctly
        pub fn read_to_bytes(&mut self)  -> crate::option::OptionU8Vec { unsafe { crate::dll::AzFile_readToBytes(self) } }
        /// Writes a string to the file, synchronizes the results before returning
        pub fn write_string(&mut self, bytes: Refstr)  -> bool { unsafe { crate::dll::AzFile_writeString(self, bytes) } }
        /// Writes some bytes to the file, synchronizes the results before returning
        pub fn write_bytes(&mut self, bytes: U8VecRef)  -> bool { unsafe { crate::dll::AzFile_writeBytes(self, bytes) } }
        /// Destructor, closes the file handle
        pub fn close(&mut self)  { unsafe { crate::dll::AzFile_close(self) } }
    }

    impl Clone for File { fn clone(&self) -> Self { unsafe { crate::dll::AzFile_deepCopy(self) } } }
    impl Drop for File { fn drop(&mut self) { unsafe { crate::dll::AzFile_delete(self) } } }
}

pub mod dialog {
    #![allow(dead_code, unused_imports)]
    //! Interface for system file selection dialogs / popup message boxes, etc.
    use crate::dll::*;
    use core::ffi::c_void;
    use crate::str::String;
    use crate::option::{OptionColorU, OptionFileTypeList, OptionString};
    /// `MsgBox` struct
    
#[doc(inline)] pub use crate::dll::AzMsgBox as MsgBox;
    impl MsgBox {
        /// Opens an informational message box with only an "OK" button
        pub fn ok(icon: MsgBoxIcon, title: String, message: String) -> bool { unsafe { crate::dll::AzMsgBox_ok(icon, title, message) } }
        /// Opens a ok / cancel message box. Blocks the current thread.
        pub fn ok_cancel(icon: MsgBoxIcon, title: String, message: String, default_value: MsgBoxOkCancel) ->  crate::dialog::MsgBoxOkCancel { unsafe { crate::dll::AzMsgBox_okCancel(icon, title, message, default_value) } }
        /// Opens a yes / no message box. Blocks the current thread.
        pub fn yes_no(icon: MsgBoxIcon, title: String, message: String, default_value: MsgBoxYesNo) ->  crate::dialog::MsgBoxYesNo { unsafe { crate::dll::AzMsgBox_yesNo(icon, title, message, default_value) } }
    }

    /// Type of message box icon
    
#[doc(inline)] pub use crate::dll::AzMsgBoxIcon as MsgBoxIcon;
    /// Value returned from a yes / no message box
    
#[doc(inline)] pub use crate::dll::AzMsgBoxYesNo as MsgBoxYesNo;
    /// Value returned from an ok / cancel message box
    
#[doc(inline)] pub use crate::dll::AzMsgBoxOkCancel as MsgBoxOkCancel;
    /// File picker dialog
    
#[doc(inline)] pub use crate::dll::AzFileDialog as FileDialog;
    impl FileDialog {
        /// Select a single file using the system-native file picker. Blocks the current thread.
        pub fn select_file(title: String, default_path: OptionString, filter_list: OptionFileTypeList) ->  crate::option::OptionString { unsafe { crate::dll::AzFileDialog_selectFile(title, default_path, filter_list) } }
        /// Select multiple files using the system-native file picker. Blocks the current thread.
        pub fn select_multiple_files(title: String, default_path: OptionString, filter_list: OptionFileTypeList) ->  crate::option::OptionStringVec { unsafe { crate::dll::AzFileDialog_selectMultipleFiles(title, default_path, filter_list) } }
        /// Open a dialog prompting the user to select a directory to open. Blocks the current thread.
        pub fn select_folder(title: String, default_path: OptionString) ->  crate::option::OptionString { unsafe { crate::dll::AzFileDialog_selectFolder(title, default_path) } }
        /// Open a dialog prompting the user to save a file. Blocks the current thread.
        pub fn save_file(title: String, default_path: OptionString) ->  crate::option::OptionString { unsafe { crate::dll::AzFileDialog_saveFile(title, default_path) } }
    }

    /// `FileTypeList` struct
    
#[doc(inline)] pub use crate::dll::AzFileTypeList as FileTypeList;
    /// `ColorPickerDialog` struct
    
#[doc(inline)] pub use crate::dll::AzColorPickerDialog as ColorPickerDialog;
    impl ColorPickerDialog {
        /// Opens a system-native color picker dialog
        pub fn open(title: String, default_color: OptionColorU) ->  crate::option::OptionColorU { unsafe { crate::dll::AzColorPickerDialog_open(title, default_color) } }
    }

}

pub mod clipboard {
    #![allow(dead_code, unused_imports)]
    //! Classes to talk to the system clipboard manager
    use crate::dll::*;
    use core::ffi::c_void;
    use crate::str::String;
    /// Connection to the system clipboard, on some systems this connection can be cached
    
#[doc(inline)] pub use crate::dll::AzSystemClipboard as SystemClipboard;
    impl SystemClipboard {
        /// Creates a new connection to the system clipboard manager
        pub fn new() ->  crate::option::OptionSystemClipboard { unsafe { crate::dll::AzSystemClipboard_new() } }
        /// Returns the system clipboard contents or `None` if the clipboard is empty or there was an error
        pub fn get_string_contents(&self)  -> crate::option::OptionString { unsafe { crate::dll::AzSystemClipboard_getStringContents(self) } }
        /// Sets the system clipboard contents to the new string, returns true if the system clipboard was updated
        pub fn set_string_contents(&mut self, contents: String)  -> bool { unsafe { crate::dll::AzSystemClipboard_setStringContents(self, contents) } }
    }

    impl Clone for SystemClipboard { fn clone(&self) -> Self { unsafe { crate::dll::AzSystemClipboard_deepCopy(self) } } }
    impl Drop for SystemClipboard { fn drop(&mut self) { unsafe { crate::dll::AzSystemClipboard_delete(self) } } }
}

pub mod time {
    #![allow(dead_code, unused_imports)]
    //! Rust wrappers for `Instant` / `Duration` classes
    use crate::dll::*;
    use core::ffi::c_void;
    /// `Instant` struct
    
#[doc(inline)] pub use crate::dll::AzInstant as Instant;
    impl Instant {
        /// Returns the duration since and earlier instant or None if the earlier instant is later than self
        pub fn duration_since(&self, earlier: Instant)  -> crate::option::OptionDuration { unsafe { crate::dll::AzInstant_durationSince(self, earlier) } }
        /// Adds a duration to the current time instant, returning the new `Instant`
        pub fn add_duration(&mut self, duration: Duration)  -> crate::time::Instant { unsafe { crate::dll::AzInstant_addDuration(self, duration) } }
        /// Linearly interpolates between [start, end] if the `self` Instant lies between start and end. Returns values between 0.0 and 1.0
        pub fn linear_interpolate(&self, start: Instant, end: Instant)  -> f32 { unsafe { crate::dll::AzInstant_linearInterpolate(self, start, end) } }
    }

    /// `InstantPtr` struct
    
#[doc(inline)] pub use crate::dll::AzInstantPtr as InstantPtr;
    impl Clone for InstantPtr { fn clone(&self) -> Self { unsafe { crate::dll::AzInstantPtr_deepCopy(self) } } }
    impl Drop for InstantPtr { fn drop(&mut self) { unsafe { crate::dll::AzInstantPtr_delete(self) } } }
    /// `InstantPtrCloneFnType` struct
    
#[doc(inline)] pub use crate::dll::AzInstantPtrCloneFnType as InstantPtrCloneFnType;
    /// `InstantPtrCloneFn` struct
    
#[doc(inline)] pub use crate::dll::AzInstantPtrCloneFn as InstantPtrCloneFn;
    /// `InstantPtrDestructorFnType` struct
    
#[doc(inline)] pub use crate::dll::AzInstantPtrDestructorFnType as InstantPtrDestructorFnType;
    /// `InstantPtrDestructorFn` struct
    
#[doc(inline)] pub use crate::dll::AzInstantPtrDestructorFn as InstantPtrDestructorFn;
    /// `SystemTick` struct
    
#[doc(inline)] pub use crate::dll::AzSystemTick as SystemTick;
    /// `Duration` struct
    
#[doc(inline)] pub use crate::dll::AzDuration as Duration;
    /// `SystemTimeDiff` struct
    
#[doc(inline)] pub use crate::dll::AzSystemTimeDiff as SystemTimeDiff;
    /// `SystemTickDiff` struct
    
#[doc(inline)] pub use crate::dll::AzSystemTickDiff as SystemTickDiff;
}

pub mod task {
    #![allow(dead_code, unused_imports)]
    //! Asyncronous timers / task / thread handlers for easy async loading
    use crate::dll::*;
    use core::ffi::c_void;
    use crate::callbacks::{RefAny, TimerCallbackType};
    use crate::time::Duration;
    /// `TimerId` struct
    
#[doc(inline)] pub use crate::dll::AzTimerId as TimerId;
    /// `Timer` struct
    
#[doc(inline)] pub use crate::dll::AzTimer as Timer;
    impl Timer {
        /// Creates a new `Timer` instance.
        pub fn new(timer_data: RefAny, callback: TimerCallbackType, get_system_time_fn: GetSystemTimeFn) -> Self { unsafe { crate::dll::AzTimer_new(timer_data, callback, get_system_time_fn) } }
        /// Calls the `Timer::with_delay` function.
        pub fn with_delay(&self, delay: Duration)  -> crate::task::Timer { unsafe { crate::dll::AzTimer_withDelay(self, delay) } }
        /// Calls the `Timer::with_interval` function.
        pub fn with_interval(&self, interval: Duration)  -> crate::task::Timer { unsafe { crate::dll::AzTimer_withInterval(self, interval) } }
        /// Calls the `Timer::with_timeout` function.
        pub fn with_timeout(&self, timeout: Duration)  -> crate::task::Timer { unsafe { crate::dll::AzTimer_withTimeout(self, timeout) } }
    }

    /// Should a timer terminate or not - used to remove active timers
    
#[doc(inline)] pub use crate::dll::AzTerminateTimer as TerminateTimer;
    /// `ThreadId` struct
    
#[doc(inline)] pub use crate::dll::AzThreadId as ThreadId;
    /// `Thread` struct
    
#[doc(inline)] pub use crate::dll::AzThread as Thread;
    impl Clone for Thread { fn clone(&self) -> Self { unsafe { crate::dll::AzThread_deepCopy(self) } } }
    impl Drop for Thread { fn drop(&mut self) { unsafe { crate::dll::AzThread_delete(self) } } }
    /// `ThreadSender` struct
    
#[doc(inline)] pub use crate::dll::AzThreadSender as ThreadSender;
    impl ThreadSender {
        /// Calls the `ThreadSender::send` function.
        pub fn send(&mut self, msg: ThreadReceiveMsg)  -> bool { unsafe { crate::dll::AzThreadSender_send(self, msg) } }
    }

    impl Clone for ThreadSender { fn clone(&self) -> Self { unsafe { crate::dll::AzThreadSender_deepCopy(self) } } }
    impl Drop for ThreadSender { fn drop(&mut self) { unsafe { crate::dll::AzThreadSender_delete(self) } } }
    /// `ThreadReceiver` struct
    
#[doc(inline)] pub use crate::dll::AzThreadReceiver as ThreadReceiver;
    impl ThreadReceiver {
        /// Calls the `ThreadReceiver::receive` function.
        pub fn receive(&mut self)  -> crate::option::OptionThreadSendMsg { unsafe { crate::dll::AzThreadReceiver_receive(self) } }
    }

    impl Clone for ThreadReceiver { fn clone(&self) -> Self { unsafe { crate::dll::AzThreadReceiver_deepCopy(self) } } }
    impl Drop for ThreadReceiver { fn drop(&mut self) { unsafe { crate::dll::AzThreadReceiver_delete(self) } } }
    /// `ThreadSendMsg` struct
    
#[doc(inline)] pub use crate::dll::AzThreadSendMsg as ThreadSendMsg;
    /// `ThreadReceiveMsg` struct
    
#[doc(inline)] pub use crate::dll::AzThreadReceiveMsg as ThreadReceiveMsg;
    /// `ThreadWriteBackMsg` struct
    
#[doc(inline)] pub use crate::dll::AzThreadWriteBackMsg as ThreadWriteBackMsg;
    /// `CreateThreadFnType` struct
    
#[doc(inline)] pub use crate::dll::AzCreateThreadFnType as CreateThreadFnType;
    /// `CreateThreadFn` struct
    
#[doc(inline)] pub use crate::dll::AzCreateThreadFn as CreateThreadFn;
    /// `GetSystemTimeFnType` struct
    
#[doc(inline)] pub use crate::dll::AzGetSystemTimeFnType as GetSystemTimeFnType;
    /// Get the current system time, equivalent to `std::time::Instant::now()`, except it also works on systems that work with "ticks" instead of timers
    
#[doc(inline)] pub use crate::dll::AzGetSystemTimeFn as GetSystemTimeFn;
    /// Callback that checks whether the thread has finished - the input argument is the `dropcheck` field on the Thread.
    
#[doc(inline)] pub use crate::dll::AzCheckThreadFinishedFnType as CheckThreadFinishedFnType;
    /// Function called to check if the thread has finished
    
#[doc(inline)] pub use crate::dll::AzCheckThreadFinishedFn as CheckThreadFinishedFn;
    /// `LibrarySendThreadMsgFnType` struct
    
#[doc(inline)] pub use crate::dll::AzLibrarySendThreadMsgFnType as LibrarySendThreadMsgFnType;
    /// Function to send a message to the thread
    
#[doc(inline)] pub use crate::dll::AzLibrarySendThreadMsgFn as LibrarySendThreadMsgFn;
    /// `LibraryReceiveThreadMsgFnType` struct
    
#[doc(inline)] pub use crate::dll::AzLibraryReceiveThreadMsgFnType as LibraryReceiveThreadMsgFnType;
    /// Function to receive a message from the thread
    
#[doc(inline)] pub use crate::dll::AzLibraryReceiveThreadMsgFn as LibraryReceiveThreadMsgFn;
    /// `ThreadRecvFnType` struct
    
#[doc(inline)] pub use crate::dll::AzThreadRecvFnType as ThreadRecvFnType;
    /// Function that the running `Thread` can call to receive messages from the main UI thread
    
#[doc(inline)] pub use crate::dll::AzThreadRecvFn as ThreadRecvFn;
    /// `ThreadSendFnType` struct
    
#[doc(inline)] pub use crate::dll::AzThreadSendFnType as ThreadSendFnType;
    /// Function that the running `Thread` can call to receive messages from the main UI thread
    
#[doc(inline)] pub use crate::dll::AzThreadSendFn as ThreadSendFn;
    /// `ThreadDestructorFnType` struct
    
#[doc(inline)] pub use crate::dll::AzThreadDestructorFnType as ThreadDestructorFnType;
    /// Destructor of the `Thread`
    
#[doc(inline)] pub use crate::dll::AzThreadDestructorFn as ThreadDestructorFn;
    /// `ThreadReceiverDestructorFnType` struct
    
#[doc(inline)] pub use crate::dll::AzThreadReceiverDestructorFnType as ThreadReceiverDestructorFnType;
    /// Destructor of the `ThreadReceiver`
    
#[doc(inline)] pub use crate::dll::AzThreadReceiverDestructorFn as ThreadReceiverDestructorFn;
    /// `ThreadSenderDestructorFnType` struct
    
#[doc(inline)] pub use crate::dll::AzThreadSenderDestructorFnType as ThreadSenderDestructorFnType;
    /// Destructor of the `ThreadSender`
    
#[doc(inline)] pub use crate::dll::AzThreadSenderDestructorFn as ThreadSenderDestructorFn;
}

pub mod str {
    #![allow(dead_code, unused_imports)]
    //! Definition of azuls internal `String` wrappers
    use crate::dll::*;
    use core::ffi::c_void;

    use alloc::string;


    impl From<&'static str> for crate::str::String {
        fn from(v: &'static str) -> crate::str::String {
            crate::str::String::from_const_str(v)
        }
    }

    impl From<string::String> for crate::str::String {
        fn from(s: string::String) -> crate::str::String {
            crate::str::String::from_string(s)
        }
    }

    impl AsRef<str> for crate::str::String {
        fn as_ref(&self) -> &str {
            self.as_str()
        }
    }

    impl core::fmt::Display for crate::str::String {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            self.as_str().fmt(f)
        }
    }

    impl crate::str::String {

        #[inline(always)]
        pub fn from_string(s: string::String) -> crate::str::String {
            crate::str::String {
                vec: crate::vec::U8Vec::from_vec(s.into_bytes())
            }
        }

        #[inline(always)]
        pub const fn from_const_str(s: &'static str) -> crate::str::String {
            crate::str::String {
                vec: crate::vec::U8Vec::from_const_slice(s.as_bytes())
            }
        }
    }    use crate::vec::FmtArgVec;
    /// `FmtValue` struct
    
#[doc(inline)] pub use crate::dll::AzFmtValue as FmtValue;
    /// `FmtArg` struct
    
#[doc(inline)] pub use crate::dll::AzFmtArg as FmtArg;
    /// `String` struct
    
#[doc(inline)] pub use crate::dll::AzString as String;
    impl String {
        /// Creates a dynamically formatted String from a fomat string + named arguments
        pub fn format(format: String, args: FmtArgVec) -> Self { unsafe { crate::dll::AzString_format(format, args) } }
        /// Creates a new String from an arbitary pointer, a start offset (bytes from the start pointer, usually 0) and a length (in bytes). The bytes are expected to point to a UTF-8 encoded string, no error checking is performed.
        pub fn copy_from_bytes(ptr: *const u8, start: usize, len: usize) -> Self { unsafe { crate::dll::AzString_copyFromBytes(ptr, start, len) } }
        /// Trims whitespace from the start / end of the string
        pub fn trim(&self)  -> crate::str::String { unsafe { crate::dll::AzString_trim(self) } }
        /// Returns a reference to the string - NOTE: the returned value is a reference to `self`, you MUST NOT drop the `String` object that the `Refstr` references
        pub fn as_refstr(&self)  -> crate::gl::Refstr { unsafe { crate::dll::AzString_asRefstr(self) } }
    }

}

pub mod vec {
    #![allow(dead_code, unused_imports)]
    //! Definition of azuls internal `Vec<*>` wrappers
    use crate::dll::*;
    use core::ffi::c_void;
    use core::iter;
    use core::fmt;
    use core::cmp;

    use alloc::vec::{self, Vec};
    use alloc::slice;
    use alloc::string;

    use crate::gl::{
        GLint as AzGLint,
        GLuint as AzGLuint,
    };

    macro_rules! impl_vec {($struct_type:ident, $struct_name:ident, $destructor_name:ident, $c_destructor_fn_name:ident, $crate_dll_delete_fn:ident) => (

        unsafe impl Send for $struct_name { }
        unsafe impl Sync for $struct_name { }

        impl fmt::Debug for $destructor_name {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                match self {
                    $destructor_name::DefaultRust => write!(f, "DefaultRust"),
                    $destructor_name::NoDestructor => write!(f, "NoDestructor"),
                    $destructor_name::External(_) => write!(f, "External"),
                }
            }
        }

        impl PartialEq for $destructor_name {
            fn eq(&self, rhs: &Self) -> bool {
                match (self, rhs) {
                    ($destructor_name::DefaultRust, $destructor_name::DefaultRust) => true,
                    ($destructor_name::NoDestructor, $destructor_name::NoDestructor) => true,
                    ($destructor_name::External(a), $destructor_name::External(b)) => (a as *const _ as usize).eq(&(b as *const _ as usize)),
                    _ => false,
                }
            }
        }

        impl PartialOrd for $destructor_name {
            fn partial_cmp(&self, _rhs: &Self) -> Option<cmp::Ordering> {
                None
            }
        }

        impl $struct_name {

            #[inline]
            pub fn iter(&self) -> slice::Iter<$struct_type> {
                self.as_ref().iter()
            }

            #[inline]
            pub fn ptr_as_usize(&self) -> usize {
                self.ptr as usize
            }

            #[inline]
            pub fn len(&self) -> usize {
                self.len
            }

            #[inline]
            pub fn capacity(&self) -> usize {
                self.cap
            }

            #[inline]
            pub fn is_empty(&self) -> bool {
                self.len == 0
            }

            pub fn get(&self, index: usize) -> Option<&$struct_type> {
                let v1: &[$struct_type] = self.as_ref();
                let res = v1.get(index);
                res
            }

            #[inline]
            unsafe fn get_unchecked(&self, index: usize) -> &$struct_type {
                let v1: &[$struct_type] = self.as_ref();
                let res = v1.get_unchecked(index);
                res
            }

            pub fn as_slice(&self) -> &[$struct_type] {
                self.as_ref()
            }

            #[inline(always)]
            pub const fn from_const_slice(input: &'static [$struct_type]) -> Self {
                Self {
                    ptr: input.as_ptr(),
                    len: input.len(),
                    cap: input.len(),
                    destructor: $destructor_name::NoDestructor, // because of &'static
                }
            }

            #[inline(always)]
            pub fn from_vec(input: Vec<$struct_type>) -> Self {

                extern "C" fn $c_destructor_fn_name(s: &mut $struct_name) {
                    let _ = unsafe { Vec::from_raw_parts(s.ptr as *mut $struct_type, s.len, s.cap) };
                }

                let ptr = input.as_ptr();
                let len = input.len();
                let cap = input.capacity();

                let _ = ::core::mem::ManuallyDrop::new(input);

                Self {
                    ptr,
                    len,
                    cap,
                    destructor: $destructor_name::External($c_destructor_fn_name),
                }
            }
        }

        impl AsRef<[$struct_type]> for $struct_name {
            fn as_ref(&self) -> &[$struct_type] {
                unsafe { slice::from_raw_parts(self.ptr, self.len) }
            }
        }

        impl iter::FromIterator<$struct_type> for $struct_name {
            fn from_iter<T>(iter: T) -> Self where T: IntoIterator<Item = $struct_type> {
                Self::from_vec(Vec::from_iter(iter))
            }
        }

        impl From<Vec<$struct_type>> for $struct_name {
            fn from(input: Vec<$struct_type>) -> $struct_name {
                Self::from_vec(input)
            }
        }

        impl From<&'static [$struct_type]> for $struct_name {
            fn from(input: &'static [$struct_type]) -> $struct_name {
                Self::from_const_slice(input)
            }
        }

        impl Drop for $struct_name {
            fn drop(&mut self) {
                match self.destructor {
                    $destructor_name::DefaultRust => { unsafe { crate::dll::$crate_dll_delete_fn(self); } },
                    $destructor_name::NoDestructor => { },
                    $destructor_name::External(f) => { f(self); }
                }
            }
        }

        impl fmt::Debug for $struct_name {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                self.as_ref().fmt(f)
            }
        }

        impl PartialOrd for $struct_name {
            fn partial_cmp(&self, rhs: &Self) -> Option<cmp::Ordering> {
                self.as_ref().partial_cmp(rhs.as_ref())
            }
        }

        impl PartialEq for $struct_name {
            fn eq(&self, rhs: &Self) -> bool {
                self.as_ref().eq(rhs.as_ref())
            }
        }
    )}

    #[macro_export]
    macro_rules! impl_vec_clone {($struct_type:ident, $struct_name:ident, $destructor_name:ident) => (
        impl $struct_name {
            /// NOTE: CLONES the memory if the memory is external or &'static
            /// Moves the memory out if the memory is library-allocated
            #[inline(always)]
            pub fn clone_self(&self) -> Self {
                match self.destructor {
                    $destructor_name::NoDestructor => {
                        Self {
                            ptr: self.ptr,
                            len: self.len,
                            cap: self.cap,
                            destructor: $destructor_name::NoDestructor,
                        }
                    }
                    $destructor_name::External(_) | $destructor_name::DefaultRust => {
                        Self::from_vec(self.as_ref().to_vec())
                    }
                }
            }
        }

        impl Clone for $struct_name {
            fn clone(&self) -> Self {
                self.clone_self()
            }
        }
    )}

    impl_vec!(u8,  AzU8Vec,  AzU8VecDestructor, az_u8_vec_destructor, AzU8Vec_delete);
    impl_vec_clone!(u8,  AzU8Vec,  AzU8VecDestructor);
    impl_vec!(u16, AzU16Vec, AzU16VecDestructor, az_u16_vec_destructor, AzU16Vec_delete);
    impl_vec_clone!(u16, AzU16Vec, AzU16VecDestructor);
    impl_vec!(u32, AzU32Vec, AzU32VecDestructor, az_u32_vec_destructor, AzU32Vec_delete);
    impl_vec_clone!(u32, AzU32Vec, AzU32VecDestructor);
    impl_vec!(u32, AzScanCodeVec, AzScanCodeVecDestructor, az_scan_code_vec_destructor, AzScanCodeVec_delete);
    impl_vec_clone!(u32, AzScanCodeVec, AzScanCodeVecDestructor);
    impl_vec!(u32, AzGLuintVec, AzGLuintVecDestructor, az_g_luint_vec_destructor, AzGLuintVec_delete);
    impl_vec_clone!(u32, AzGLuintVec, AzGLuintVecDestructor);
    impl_vec!(i32, AzGLintVec, AzGLintVecDestructor, az_g_lint_vec_destructor, AzGLintVec_delete);
    impl_vec_clone!(i32, AzGLintVec, AzGLintVecDestructor);
    impl_vec!(f32,  AzF32Vec,  AzF32VecDestructor, az_f32_vec_destructor, AzF32Vec_delete);
    impl_vec_clone!(f32,  AzF32Vec,  AzF32VecDestructor);
    impl_vec!(AzXmlNode,  AzXmlNodeVec,  AzXmlNodeVecDestructor, az_xml_node_vec_destructor, AzXmlNodeVec_delete);
    impl_vec_clone!(AzXmlNode,  AzXmlNodeVec,  AzXmlNodeVecDestructor);
    impl_vec!(AzInlineWord,  AzInlineWordVec,  AzInlineWordVecDestructor, az_inline_word_vec_destructor, AzInlineWordVec_delete);
    impl_vec_clone!(AzInlineWord,  AzInlineWordVec,  AzInlineWordVecDestructor);
    impl_vec!(AzInlineGlyph,  AzInlineGlyphVec,  AzInlineGlyphVecDestructor, az_inline_glyph_vec_destructor, AzInlineGlyphVec_delete);
    impl_vec_clone!(AzInlineGlyph,  AzInlineGlyphVec,  AzInlineGlyphVecDestructor);
    impl_vec!(AzInlineLine,  AzInlineLineVec,  AzInlineLineVecDestructor, az_inline_line_vec_destructor, AzInlineLineVec_delete);
    impl_vec_clone!(AzInlineLine,  AzInlineLineVec,  AzInlineLineVecDestructor);
    impl_vec!(AzFmtArg,  AzFmtArgVec,  AzFmtArgVecDestructor, az_fmt_arg_vec_destructor, AzFmtArgVec_delete);
    impl_vec_clone!(AzFmtArg,  AzFmtArgVec,  AzFmtArgVecDestructor);
    impl_vec!(AzInlineTextHit,  AzInlineTextHitVec,  AzInlineTextHitVecDestructor, az_inline_text_hit_vec_destructor, AzInlineTextHitVec_delete);
    impl_vec_clone!(AzInlineTextHit,  AzInlineTextHitVec,  AzInlineTextHitVecDestructor);
    impl_vec!(AzTesselatedSvgNode,  AzTesselatedSvgNodeVec,  AzTesselatedSvgNodeVecDestructor, az_tesselated_svg_node_vec_destructor, AzTesselatedSvgNodeVec_delete);
    impl_vec_clone!(AzTesselatedSvgNode,  AzTesselatedSvgNodeVec,  AzTesselatedSvgNodeVecDestructor);
    impl_vec!(AzNodeDataInlineCssProperty, AzNodeDataInlineCssPropertyVec, NodeDataInlineCssPropertyVecDestructor, az_node_data_inline_css_property_vec_destructor, AzNodeDataInlineCssPropertyVec_delete);
    impl_vec_clone!(AzNodeDataInlineCssProperty, AzNodeDataInlineCssPropertyVec, NodeDataInlineCssPropertyVecDestructor);
    impl_vec!(AzIdOrClass, AzIdOrClassVec, IdOrClassVecDestructor, az_id_or_class_vec_destructor, AzIdOrClassVec_delete);
    impl_vec_clone!(AzIdOrClass, AzIdOrClassVec, IdOrClassVecDestructor);
    impl_vec!(AzStyleTransform, AzStyleTransformVec, AzStyleTransformVecDestructor, az_style_transform_vec_destructor, AzStyleTransformVec_delete);
    impl_vec_clone!(AzStyleTransform, AzStyleTransformVec, AzStyleTransformVecDestructor);
    impl_vec!(AzCssProperty, AzCssPropertyVec, AzCssPropertyVecDestructor, az_css_property_vec_destructor, AzCssPropertyVec_delete);
    impl_vec_clone!(AzCssProperty, AzCssPropertyVec, AzCssPropertyVecDestructor);
    impl_vec!(AzSvgMultiPolygon, AzSvgMultiPolygonVec, AzSvgMultiPolygonVecDestructor, az_svg_multi_polygon_vec_destructor, AzSvgMultiPolygonVec_delete);
    impl_vec_clone!(AzSvgMultiPolygon, AzSvgMultiPolygonVec, AzSvgMultiPolygonVecDestructor);
    impl_vec!(AzSvgPath, AzSvgPathVec, AzSvgPathVecDestructor, az_svg_path_vec_destructor, AzSvgPathVec_delete);
    impl_vec_clone!(AzSvgPath, AzSvgPathVec, AzSvgPathVecDestructor);
    impl_vec!(AzVertexAttribute, AzVertexAttributeVec, AzVertexAttributeVecDestructor, az_vertex_attribute_vec_destructor, AzVertexAttributeVec_delete);
    impl_vec_clone!(AzVertexAttribute, AzVertexAttributeVec, AzVertexAttributeVecDestructor);
    impl_vec!(AzSvgPathElement, AzSvgPathElementVec, AzSvgPathElementVecDestructor, az_svg_path_element_vec_destructor, AzSvgPathElementVec_delete);
    impl_vec_clone!(AzSvgPathElement, AzSvgPathElementVec, AzSvgPathElementVecDestructor);
    impl_vec!(AzSvgVertex, AzSvgVertexVec, AzSvgVertexVecDestructor, az_svg_vertex_vec_destructor, AzSvgVertexVec_delete);
    impl_vec_clone!(AzSvgVertex, AzSvgVertexVec, AzSvgVertexVecDestructor);
    impl_vec!(AzXWindowType, AzXWindowTypeVec, AzXWindowTypeVecDestructor, az_x_window_type_vec_destructor, AzXWindowTypeVec_delete);
    impl_vec_clone!(AzXWindowType, AzXWindowTypeVec, AzXWindowTypeVecDestructor);
    impl_vec!(AzVirtualKeyCode, AzVirtualKeyCodeVec, AzVirtualKeyCodeVecDestructor, az_virtual_key_code_vec_destructor, AzVirtualKeyCodeVec_delete);
    impl_vec_clone!(AzVirtualKeyCode, AzVirtualKeyCodeVec, AzVirtualKeyCodeVecDestructor);
    impl_vec!(AzCascadeInfo, AzCascadeInfoVec, AzCascadeInfoVecDestructor, az_cascade_info_vec_destructor, AzCascadeInfoVec_delete);
    impl_vec_clone!(AzCascadeInfo, AzCascadeInfoVec, AzCascadeInfoVecDestructor);
    impl_vec!(AzCssDeclaration, AzCssDeclarationVec, AzCssDeclarationVecDestructor, az_css_declaration_vec_destructor, AzCssDeclarationVec_delete);
    impl_vec_clone!(AzCssDeclaration, AzCssDeclarationVec, AzCssDeclarationVecDestructor);
    impl_vec!(AzCssPathSelector, AzCssPathSelectorVec, AzCssPathSelectorVecDestructor, az_css_path_selector_vec_destructor, AzCssPathSelectorVec_delete);
    impl_vec_clone!(AzCssPathSelector, AzCssPathSelectorVec, AzCssPathSelectorVecDestructor);
    impl_vec!(AzStylesheet, AzStylesheetVec, AzStylesheetVecDestructor, az_stylesheet_vec_destructor, AzStylesheetVec_delete);
    impl_vec_clone!(AzStylesheet, AzStylesheetVec, AzStylesheetVecDestructor);
    impl_vec!(AzCssRuleBlock, AzCssRuleBlockVec, AzCssRuleBlockVecDestructor, az_css_rule_block_vec_destructor, AzCssRuleBlockVec_delete);
    impl_vec_clone!(AzCssRuleBlock, AzCssRuleBlockVec, AzCssRuleBlockVecDestructor);
    impl_vec!(AzCallbackData, AzCallbackDataVec, AzCallbackDataVecDestructor, az_callback_data_vec_destructor, AzCallbackDataVec_delete);
    impl_vec_clone!(AzCallbackData, AzCallbackDataVec, AzCallbackDataVecDestructor);
    impl_vec!(AzDebugMessage, AzDebugMessageVec, AzDebugMessageVecDestructor, az_debug_message_vec_destructor, AzDebugMessageVec_delete);
    impl_vec_clone!(AzDebugMessage, AzDebugMessageVec, AzDebugMessageVecDestructor);
    impl_vec!(AzDom, AzDomVec, AzDomVecDestructor, az_dom_vec_destructor, AzDomVec_delete);
    impl_vec_clone!(AzDom, AzDomVec, AzDomVecDestructor);
    impl_vec!(AzString, AzStringVec, AzStringVecDestructor, az_string_vec_destructor, AzStringVec_delete);
    impl_vec_clone!(AzString, AzStringVec, AzStringVecDestructor);
    impl_vec!(AzStringPair, AzStringPairVec, AzStringPairVecDestructor, az_string_pair_vec_destructor, AzStringPairVec_delete);
    impl_vec_clone!(AzStringPair, AzStringPairVec, AzStringPairVecDestructor);
    impl_vec!(AzNormalizedLinearColorStop, AzNormalizedLinearColorStopVec, AzNormalizedLinearColorStopVecDestructor, az_normalized_linear_color_stop_vec_destructor, AzNormalizedLinearColorStopVec_delete);
    impl_vec_clone!(AzNormalizedLinearColorStop, AzNormalizedLinearColorStopVec, AzNormalizedLinearColorStopVecDestructor);
    impl_vec!(AzNormalizedRadialColorStop, AzNormalizedRadialColorStopVec, AzNormalizedRadialColorStopVecDestructor, az_normalized_radial_color_stop_vec_destructor, AzNormalizedRadialColorStopVec_delete);
    impl_vec_clone!(AzNormalizedRadialColorStop, AzNormalizedRadialColorStopVec, AzNormalizedRadialColorStopVecDestructor);
    impl_vec!(AzNodeId, AzNodeIdVec, AzNodeIdVecDestructor, az_node_id_vec_destructor, AzNodeIdVec_delete);
    impl_vec_clone!(AzNodeId, AzNodeIdVec, AzNodeIdVecDestructor);
    impl_vec!(AzNode, AzNodeVec, AzNodeVecDestructor, az_node_vec_destructor, AzNodeVec_delete);
    impl_vec_clone!(AzNode, AzNodeVec, AzNodeVecDestructor);
    impl_vec!(AzStyledNode, AzStyledNodeVec, AzStyledNodeVecDestructor, az_styled_node_vec_destructor, AzStyledNodeVec_delete);
    impl_vec_clone!(AzStyledNode, AzStyledNodeVec, AzStyledNodeVecDestructor);
    impl_vec!(AzTagIdToNodeIdMapping, AzTagIdToNodeIdMappingVec, AzTagIdToNodeIdMappingVecDestructor, az_tag_id_to_node_id_mapping_vec_destructor, AzTagIdToNodeIdMappingVec_delete);
    impl_vec_clone!(AzTagIdToNodeIdMapping, AzTagIdToNodeIdMappingVec, AzTagIdToNodeIdMappingVecDestructor);
    impl_vec!(AzParentWithNodeDepth, AzParentWithNodeDepthVec, AzParentWithNodeDepthVecDestructor, az_parent_with_node_depth_vec_destructor, AzParentWithNodeDepthVec_delete);
    impl_vec_clone!(AzParentWithNodeDepth, AzParentWithNodeDepthVec, AzParentWithNodeDepthVecDestructor);
    impl_vec!(AzNodeData, AzNodeDataVec, AzNodeDataVecDestructor, az_node_data_vec_destructor, AzNodeDataVec_delete);
    impl_vec_clone!(AzNodeData, AzNodeDataVec, AzNodeDataVecDestructor);
    impl_vec!(AzStyleBackgroundRepeat, AzStyleBackgroundRepeatVec, AzStyleBackgroundRepeatVecDestructor, az_style_background_repeat_vec_destructor, AzStyleBackgroundRepeatVec_delete);
    impl_vec_clone!(AzStyleBackgroundRepeat, AzStyleBackgroundRepeatVec, AzStyleBackgroundRepeatVecDestructor);
    impl_vec!(AzStyleBackgroundPosition, AzStyleBackgroundPositionVec, AzStyleBackgroundPositionVecDestructor, az_style_background_position_vec_destructor, AzStyleBackgroundPositionVec_delete);
    impl_vec_clone!(AzStyleBackgroundPosition, AzStyleBackgroundPositionVec, AzStyleBackgroundPositionVecDestructor);
    impl_vec!(AzStyleBackgroundSize, AzStyleBackgroundSizeVec, AzStyleBackgroundSizeVecDestructor, az_style_background_size_vec_destructor, AzStyleBackgroundSizeVec_delete);
    impl_vec_clone!(AzStyleBackgroundSize, AzStyleBackgroundSizeVec, AzStyleBackgroundSizeVecDestructor);
    impl_vec!(AzStyleBackgroundContent, AzStyleBackgroundContentVec, AzStyleBackgroundContentVecDestructor, az_style_background_content_vec_destructor, AzStyleBackgroundContentVec_delete);
    impl_vec_clone!(AzStyleBackgroundContent, AzStyleBackgroundContentVec, AzStyleBackgroundContentVecDestructor);
    impl_vec!(AzVideoMode, AzVideoModeVec, AzVideoModeVecDestructor, az_video_mode_vec_destructor, AzVideoModeVec_delete);
    impl_vec_clone!(AzVideoMode, AzVideoModeVec, AzVideoModeVecDestructor);
    impl_vec!(AzMonitor, AzMonitorVec, AzMonitorVecDestructor, az_monitor_vec_destructor, AzMonitorVec_delete);
    impl_vec_clone!(AzMonitor, AzMonitorVec, AzMonitorVecDestructor);
    impl_vec!(AzStyleFontFamily, AzStyleFontFamilyVec, AzStyleFontFamilyVecDestructor, az_style_font_family_vec_destructor, AzStyleFontFamilyVec_delete);
    impl_vec_clone!(AzStyleFontFamily, AzStyleFontFamilyVec, AzStyleFontFamilyVecDestructor);

    impl_vec!(AzAccessibilityState,  AzAccessibilityStateVec,  AzAccessibilityStateVecDestructor, az_accessibility_state_vec_destructor, AzAccessibilityStateVec_delete);
    impl_vec_clone!(AzAccessibilityState,  AzAccessibilityStateVec,  AzAccessibilityStateVecDestructor);

    impl_vec!(AzMenuItem,  AzMenuItemVec,  AzMenuItemVecDestructor, az_menu_item_vec_destructor, AzMenuItemVec_delete);
    impl_vec_clone!(AzMenuItem,  AzMenuItemVec,  AzMenuItemVecDestructor);

    impl From<vec::Vec<string::String>> for crate::vec::StringVec {
        fn from(v: vec::Vec<string::String>) -> crate::vec::StringVec {
            let vec: Vec<AzString> = v.into_iter().map(Into::into).collect();
            vec.into()
            // v dropped here
        }
    }    /// Wrapper over a Rust-allocated `Vec<AccessibilityState>`
    
#[doc(inline)] pub use crate::dll::AzAccessibilityStateVec as AccessibilityStateVec;
    /// Wrapper over a Rust-allocated `Vec<MenuItem>`
    
#[doc(inline)] pub use crate::dll::AzMenuItemVec as MenuItemVec;
    /// Wrapper over a Rust-allocated `Vec<TesselatedSvgNode>`
    
#[doc(inline)] pub use crate::dll::AzTesselatedSvgNodeVec as TesselatedSvgNodeVec;
    impl TesselatedSvgNodeVec {
        /// Returns the `TesselatedSvgNodeVec` as a non-owning slice, NOTE: The `U8Vec` that this slice was borrowed from MUST NOT be deleted before the `U8VecRef`
        pub fn as_ref_vec(&self)  -> crate::svg::TesselatedSvgNodeVecRef { unsafe { crate::dll::AzTesselatedSvgNodeVec_asRefVec(self) } }
    }

    /// Wrapper over a Rust-allocated `Vec<StyleFontFamily>`
    
#[doc(inline)] pub use crate::dll::AzStyleFontFamilyVec as StyleFontFamilyVec;
    /// Wrapper over a Rust-allocated `Vec<XmlNode>`
    
#[doc(inline)] pub use crate::dll::AzXmlNodeVec as XmlNodeVec;
    /// Wrapper over a Rust-allocated `Vec<FmtArg>`
    
#[doc(inline)] pub use crate::dll::AzFmtArgVec as FmtArgVec;
    /// Wrapper over a Rust-allocated `Vec<InlineLine>`
    
#[doc(inline)] pub use crate::dll::AzInlineLineVec as InlineLineVec;
    /// Wrapper over a Rust-allocated `Vec<InlineWord>`
    
#[doc(inline)] pub use crate::dll::AzInlineWordVec as InlineWordVec;
    /// Wrapper over a Rust-allocated `Vec<InlineGlyph>`
    
#[doc(inline)] pub use crate::dll::AzInlineGlyphVec as InlineGlyphVec;
    /// Wrapper over a Rust-allocated `Vec<InlineTextHit>`
    
#[doc(inline)] pub use crate::dll::AzInlineTextHitVec as InlineTextHitVec;
    /// Wrapper over a Rust-allocated `Vec<Monitor>`
    
#[doc(inline)] pub use crate::dll::AzMonitorVec as MonitorVec;
    /// Wrapper over a Rust-allocated `Vec<VideoMode>`
    
#[doc(inline)] pub use crate::dll::AzVideoModeVec as VideoModeVec;
    /// Wrapper over a Rust-allocated `Vec<Dom>`
    
#[doc(inline)] pub use crate::dll::AzDomVec as DomVec;
    /// Wrapper over a Rust-allocated `Vec<IdOrClass>`
    
#[doc(inline)] pub use crate::dll::AzIdOrClassVec as IdOrClassVec;
    /// Wrapper over a Rust-allocated `Vec<NodeDataInlineCssProperty>`
    
#[doc(inline)] pub use crate::dll::AzNodeDataInlineCssPropertyVec as NodeDataInlineCssPropertyVec;
    /// Wrapper over a Rust-allocated `Vec<StyleBackgroundContent>`
    
#[doc(inline)] pub use crate::dll::AzStyleBackgroundContentVec as StyleBackgroundContentVec;
    /// Wrapper over a Rust-allocated `Vec<StyleBackgroundPosition>`
    
#[doc(inline)] pub use crate::dll::AzStyleBackgroundPositionVec as StyleBackgroundPositionVec;
    /// Wrapper over a Rust-allocated `Vec<StyleBackgroundRepeat>`
    
#[doc(inline)] pub use crate::dll::AzStyleBackgroundRepeatVec as StyleBackgroundRepeatVec;
    /// Wrapper over a Rust-allocated `Vec<StyleBackgroundSize>`
    
#[doc(inline)] pub use crate::dll::AzStyleBackgroundSizeVec as StyleBackgroundSizeVec;
    /// Wrapper over a Rust-allocated `Vec<StyleTransform>`
    
#[doc(inline)] pub use crate::dll::AzStyleTransformVec as StyleTransformVec;
    /// Wrapper over a Rust-allocated `Vec<CssProperty>`
    
#[doc(inline)] pub use crate::dll::AzCssPropertyVec as CssPropertyVec;
    /// Wrapper over a Rust-allocated `Vec<SvgMultiPolygon>`
    
#[doc(inline)] pub use crate::dll::AzSvgMultiPolygonVec as SvgMultiPolygonVec;
    /// Wrapper over a Rust-allocated `Vec<SvgPath>`
    
#[doc(inline)] pub use crate::dll::AzSvgPathVec as SvgPathVec;
    /// Wrapper over a Rust-allocated `Vec<VertexAttribute>`
    
#[doc(inline)] pub use crate::dll::AzVertexAttributeVec as VertexAttributeVec;
    /// Wrapper over a Rust-allocated `VertexAttribute`
    
#[doc(inline)] pub use crate::dll::AzSvgPathElementVec as SvgPathElementVec;
    /// Wrapper over a Rust-allocated `SvgVertex`
    
#[doc(inline)] pub use crate::dll::AzSvgVertexVec as SvgVertexVec;
    /// Wrapper over a Rust-allocated `Vec<u32>`
    
#[doc(inline)] pub use crate::dll::AzU32Vec as U32Vec;
    /// Wrapper over a Rust-allocated `XWindowType`
    
#[doc(inline)] pub use crate::dll::AzXWindowTypeVec as XWindowTypeVec;
    /// Wrapper over a Rust-allocated `VirtualKeyCode`
    
#[doc(inline)] pub use crate::dll::AzVirtualKeyCodeVec as VirtualKeyCodeVec;
    /// Wrapper over a Rust-allocated `CascadeInfo`
    
#[doc(inline)] pub use crate::dll::AzCascadeInfoVec as CascadeInfoVec;
    /// Wrapper over a Rust-allocated `ScanCode`
    
#[doc(inline)] pub use crate::dll::AzScanCodeVec as ScanCodeVec;
    /// Wrapper over a Rust-allocated `CssDeclaration`
    
#[doc(inline)] pub use crate::dll::AzCssDeclarationVec as CssDeclarationVec;
    /// Wrapper over a Rust-allocated `CssPathSelector`
    
#[doc(inline)] pub use crate::dll::AzCssPathSelectorVec as CssPathSelectorVec;
    /// Wrapper over a Rust-allocated `Stylesheet`
    
#[doc(inline)] pub use crate::dll::AzStylesheetVec as StylesheetVec;
    /// Wrapper over a Rust-allocated `CssRuleBlock`
    
#[doc(inline)] pub use crate::dll::AzCssRuleBlockVec as CssRuleBlockVec;
    /// Wrapper over a Rust-allocated `Vec<u16>`
    
#[doc(inline)] pub use crate::dll::AzU16Vec as U16Vec;
    /// Wrapper over a Rust-allocated `Vec<f32>`
    
#[doc(inline)] pub use crate::dll::AzF32Vec as F32Vec;
    /// Wrapper over a Rust-allocated `U8Vec`
    
#[doc(inline)] pub use crate::dll::AzU8Vec as U8Vec;
    impl U8Vec {
        /// Creates a new, heap-allocated U8Vec by copying the memory into Rust (heap allocation)
        pub fn copy_from_bytes(ptr: *const u8, start: usize, len: usize) -> Self { unsafe { crate::dll::AzU8Vec_copyFromBytes(ptr, start, len) } }
        /// Returns the `U8Vec` as a non-owning slice, NOTE: The `U8Vec` that this slice was borrowed from MUST NOT be deleted before the `U8VecRef`
        pub fn as_ref_vec(&self)  -> crate::gl::U8VecRef { unsafe { crate::dll::AzU8Vec_asRefVec(self) } }
    }

    /// Wrapper over a Rust-allocated `CallbackData`
    
#[doc(inline)] pub use crate::dll::AzCallbackDataVec as CallbackDataVec;
    /// Wrapper over a Rust-allocated `Vec<DebugMessage>`
    
#[doc(inline)] pub use crate::dll::AzDebugMessageVec as DebugMessageVec;
    /// Wrapper over a Rust-allocated `U32Vec`
    
#[doc(inline)] pub use crate::dll::AzGLuintVec as GLuintVec;
    /// Wrapper over a Rust-allocated `GLintVec`
    
#[doc(inline)] pub use crate::dll::AzGLintVec as GLintVec;
    /// Wrapper over a Rust-allocated `StringVec`
    
#[doc(inline)] pub use crate::dll::AzStringVec as StringVec;
    /// Wrapper over a Rust-allocated `StringPairVec`
    
#[doc(inline)] pub use crate::dll::AzStringPairVec as StringPairVec;
    /// Wrapper over a Rust-allocated `NormalizedLinearColorStopVec`
    
#[doc(inline)] pub use crate::dll::AzNormalizedLinearColorStopVec as NormalizedLinearColorStopVec;
    /// Wrapper over a Rust-allocated `NormalizedRadialColorStopVec`
    
#[doc(inline)] pub use crate::dll::AzNormalizedRadialColorStopVec as NormalizedRadialColorStopVec;
    /// Wrapper over a Rust-allocated `NodeIdVec`
    
#[doc(inline)] pub use crate::dll::AzNodeIdVec as NodeIdVec;
    /// Wrapper over a Rust-allocated `NodeVec`
    
#[doc(inline)] pub use crate::dll::AzNodeVec as NodeVec;
    /// Wrapper over a Rust-allocated `StyledNodeVec`
    
#[doc(inline)] pub use crate::dll::AzStyledNodeVec as StyledNodeVec;
    /// Wrapper over a Rust-allocated `TagIdToNodeIdMappingVec`
    
#[doc(inline)] pub use crate::dll::AzTagIdToNodeIdMappingVec as TagIdToNodeIdMappingVec;
    /// Wrapper over a Rust-allocated `ParentWithNodeDepthVec`
    
#[doc(inline)] pub use crate::dll::AzParentWithNodeDepthVec as ParentWithNodeDepthVec;
    /// Wrapper over a Rust-allocated `NodeDataVec`
    
#[doc(inline)] pub use crate::dll::AzNodeDataVec as NodeDataVec;
    /// `StyleFontFamilyVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzStyleFontFamilyVecDestructor as StyleFontFamilyVecDestructor;
    /// `StyleFontFamilyVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzStyleFontFamilyVecDestructorType as StyleFontFamilyVecDestructorType;
    /// `AccessibilityStateVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzAccessibilityStateVecDestructor as AccessibilityStateVecDestructor;
    /// `AccessibilityStateVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzAccessibilityStateVecDestructorType as AccessibilityStateVecDestructorType;
    /// `MenuItemVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzMenuItemVecDestructor as MenuItemVecDestructor;
    /// `MenuItemVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzMenuItemVecDestructorType as MenuItemVecDestructorType;
    /// `TesselatedSvgNodeVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzTesselatedSvgNodeVecDestructor as TesselatedSvgNodeVecDestructor;
    /// `TesselatedSvgNodeVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzTesselatedSvgNodeVecDestructorType as TesselatedSvgNodeVecDestructorType;
    /// `XmlNodeVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzXmlNodeVecDestructor as XmlNodeVecDestructor;
    /// `XmlNodeVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzXmlNodeVecDestructorType as XmlNodeVecDestructorType;
    /// `FmtArgVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzFmtArgVecDestructor as FmtArgVecDestructor;
    /// `FmtArgVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzFmtArgVecDestructorType as FmtArgVecDestructorType;
    /// `InlineLineVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzInlineLineVecDestructor as InlineLineVecDestructor;
    /// `InlineLineVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzInlineLineVecDestructorType as InlineLineVecDestructorType;
    /// `InlineWordVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzInlineWordVecDestructor as InlineWordVecDestructor;
    /// `InlineWordVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzInlineWordVecDestructorType as InlineWordVecDestructorType;
    /// `InlineGlyphVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzInlineGlyphVecDestructor as InlineGlyphVecDestructor;
    /// `InlineGlyphVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzInlineGlyphVecDestructorType as InlineGlyphVecDestructorType;
    /// `InlineTextHitVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzInlineTextHitVecDestructor as InlineTextHitVecDestructor;
    /// `InlineTextHitVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzInlineTextHitVecDestructorType as InlineTextHitVecDestructorType;
    /// `MonitorVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzMonitorVecDestructor as MonitorVecDestructor;
    /// `MonitorVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzMonitorVecDestructorType as MonitorVecDestructorType;
    /// `VideoModeVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzVideoModeVecDestructor as VideoModeVecDestructor;
    /// `VideoModeVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzVideoModeVecDestructorType as VideoModeVecDestructorType;
    /// `DomVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzDomVecDestructor as DomVecDestructor;
    /// `DomVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzDomVecDestructorType as DomVecDestructorType;
    /// `IdOrClassVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzIdOrClassVecDestructor as IdOrClassVecDestructor;
    /// `IdOrClassVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzIdOrClassVecDestructorType as IdOrClassVecDestructorType;
    /// `NodeDataInlineCssPropertyVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzNodeDataInlineCssPropertyVecDestructor as NodeDataInlineCssPropertyVecDestructor;
    /// `NodeDataInlineCssPropertyVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzNodeDataInlineCssPropertyVecDestructorType as NodeDataInlineCssPropertyVecDestructorType;
    /// `StyleBackgroundContentVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBackgroundContentVecDestructor as StyleBackgroundContentVecDestructor;
    /// `StyleBackgroundContentVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBackgroundContentVecDestructorType as StyleBackgroundContentVecDestructorType;
    /// `StyleBackgroundPositionVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBackgroundPositionVecDestructor as StyleBackgroundPositionVecDestructor;
    /// `StyleBackgroundPositionVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBackgroundPositionVecDestructorType as StyleBackgroundPositionVecDestructorType;
    /// `StyleBackgroundRepeatVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBackgroundRepeatVecDestructor as StyleBackgroundRepeatVecDestructor;
    /// `StyleBackgroundRepeatVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBackgroundRepeatVecDestructorType as StyleBackgroundRepeatVecDestructorType;
    /// `StyleBackgroundSizeVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBackgroundSizeVecDestructor as StyleBackgroundSizeVecDestructor;
    /// `StyleBackgroundSizeVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzStyleBackgroundSizeVecDestructorType as StyleBackgroundSizeVecDestructorType;
    /// `StyleTransformVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzStyleTransformVecDestructor as StyleTransformVecDestructor;
    /// `StyleTransformVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzStyleTransformVecDestructorType as StyleTransformVecDestructorType;
    /// `CssPropertyVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzCssPropertyVecDestructor as CssPropertyVecDestructor;
    /// `CssPropertyVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzCssPropertyVecDestructorType as CssPropertyVecDestructorType;
    /// `SvgMultiPolygonVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzSvgMultiPolygonVecDestructor as SvgMultiPolygonVecDestructor;
    /// `SvgMultiPolygonVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzSvgMultiPolygonVecDestructorType as SvgMultiPolygonVecDestructorType;
    /// `SvgPathVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzSvgPathVecDestructor as SvgPathVecDestructor;
    /// `SvgPathVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzSvgPathVecDestructorType as SvgPathVecDestructorType;
    /// `VertexAttributeVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzVertexAttributeVecDestructor as VertexAttributeVecDestructor;
    /// `VertexAttributeVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzVertexAttributeVecDestructorType as VertexAttributeVecDestructorType;
    /// `SvgPathElementVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzSvgPathElementVecDestructor as SvgPathElementVecDestructor;
    /// `SvgPathElementVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzSvgPathElementVecDestructorType as SvgPathElementVecDestructorType;
    /// `SvgVertexVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzSvgVertexVecDestructor as SvgVertexVecDestructor;
    /// `SvgVertexVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzSvgVertexVecDestructorType as SvgVertexVecDestructorType;
    /// `U32VecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzU32VecDestructor as U32VecDestructor;
    /// `U32VecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzU32VecDestructorType as U32VecDestructorType;
    /// `XWindowTypeVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzXWindowTypeVecDestructor as XWindowTypeVecDestructor;
    /// `XWindowTypeVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzXWindowTypeVecDestructorType as XWindowTypeVecDestructorType;
    /// `VirtualKeyCodeVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzVirtualKeyCodeVecDestructor as VirtualKeyCodeVecDestructor;
    /// `VirtualKeyCodeVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzVirtualKeyCodeVecDestructorType as VirtualKeyCodeVecDestructorType;
    /// `CascadeInfoVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzCascadeInfoVecDestructor as CascadeInfoVecDestructor;
    /// `CascadeInfoVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzCascadeInfoVecDestructorType as CascadeInfoVecDestructorType;
    /// `ScanCodeVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzScanCodeVecDestructor as ScanCodeVecDestructor;
    /// `ScanCodeVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzScanCodeVecDestructorType as ScanCodeVecDestructorType;
    /// `CssDeclarationVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzCssDeclarationVecDestructor as CssDeclarationVecDestructor;
    /// `CssDeclarationVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzCssDeclarationVecDestructorType as CssDeclarationVecDestructorType;
    /// `CssPathSelectorVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzCssPathSelectorVecDestructor as CssPathSelectorVecDestructor;
    /// `CssPathSelectorVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzCssPathSelectorVecDestructorType as CssPathSelectorVecDestructorType;
    /// `StylesheetVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzStylesheetVecDestructor as StylesheetVecDestructor;
    /// `StylesheetVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzStylesheetVecDestructorType as StylesheetVecDestructorType;
    /// `CssRuleBlockVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzCssRuleBlockVecDestructor as CssRuleBlockVecDestructor;
    /// `CssRuleBlockVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzCssRuleBlockVecDestructorType as CssRuleBlockVecDestructorType;
    /// `F32VecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzF32VecDestructor as F32VecDestructor;
    /// `F32VecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzF32VecDestructorType as F32VecDestructorType;
    /// `U16VecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzU16VecDestructor as U16VecDestructor;
    /// `U16VecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzU16VecDestructorType as U16VecDestructorType;
    /// `U8VecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzU8VecDestructor as U8VecDestructor;
    /// `U8VecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzU8VecDestructorType as U8VecDestructorType;
    /// `CallbackDataVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzCallbackDataVecDestructor as CallbackDataVecDestructor;
    /// `CallbackDataVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzCallbackDataVecDestructorType as CallbackDataVecDestructorType;
    /// `DebugMessageVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzDebugMessageVecDestructor as DebugMessageVecDestructor;
    /// `DebugMessageVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzDebugMessageVecDestructorType as DebugMessageVecDestructorType;
    /// `GLuintVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzGLuintVecDestructor as GLuintVecDestructor;
    /// `GLuintVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzGLuintVecDestructorType as GLuintVecDestructorType;
    /// `GLintVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzGLintVecDestructor as GLintVecDestructor;
    /// `GLintVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzGLintVecDestructorType as GLintVecDestructorType;
    /// `StringVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzStringVecDestructor as StringVecDestructor;
    /// `StringVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzStringVecDestructorType as StringVecDestructorType;
    /// `StringPairVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzStringPairVecDestructor as StringPairVecDestructor;
    /// `StringPairVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzStringPairVecDestructorType as StringPairVecDestructorType;
    /// `NormalizedLinearColorStopVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzNormalizedLinearColorStopVecDestructor as NormalizedLinearColorStopVecDestructor;
    /// `NormalizedLinearColorStopVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzNormalizedLinearColorStopVecDestructorType as NormalizedLinearColorStopVecDestructorType;
    /// `NormalizedRadialColorStopVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzNormalizedRadialColorStopVecDestructor as NormalizedRadialColorStopVecDestructor;
    /// `NormalizedRadialColorStopVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzNormalizedRadialColorStopVecDestructorType as NormalizedRadialColorStopVecDestructorType;
    /// `NodeIdVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzNodeIdVecDestructor as NodeIdVecDestructor;
    /// `NodeIdVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzNodeIdVecDestructorType as NodeIdVecDestructorType;
    /// `NodeVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzNodeVecDestructor as NodeVecDestructor;
    /// `NodeVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzNodeVecDestructorType as NodeVecDestructorType;
    /// `StyledNodeVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzStyledNodeVecDestructor as StyledNodeVecDestructor;
    /// `StyledNodeVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzStyledNodeVecDestructorType as StyledNodeVecDestructorType;
    /// `TagIdToNodeIdMappingVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzTagIdToNodeIdMappingVecDestructor as TagIdToNodeIdMappingVecDestructor;
    /// `TagIdToNodeIdMappingVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzTagIdToNodeIdMappingVecDestructorType as TagIdToNodeIdMappingVecDestructorType;
    /// `ParentWithNodeDepthVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzParentWithNodeDepthVecDestructor as ParentWithNodeDepthVecDestructor;
    /// `ParentWithNodeDepthVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzParentWithNodeDepthVecDestructorType as ParentWithNodeDepthVecDestructorType;
    /// `NodeDataVecDestructor` struct
    
#[doc(inline)] pub use crate::dll::AzNodeDataVecDestructor as NodeDataVecDestructor;
    /// `NodeDataVecDestructorType` struct
    
#[doc(inline)] pub use crate::dll::AzNodeDataVecDestructorType as NodeDataVecDestructorType;
}

pub mod option {
    #![allow(dead_code, unused_imports)]
    //! Definition of azuls internal `Option<*>` wrappers
    use crate::dll::*;
    use core::ffi::c_void;
    use crate::dll::*;

    macro_rules! impl_option_inner {
        ($struct_type:ident, $struct_name:ident) => (

        impl Default for $struct_name {
            fn default() -> $struct_name { $struct_name::None }
        }

        impl $struct_name {
            pub fn as_option(&self) -> Option<&$struct_type> {
                match self {
                    $struct_name::None => None,
                    $struct_name::Some(t) => Some(t),
                }
            }
            pub fn replace(&mut self, value: $struct_type) -> $struct_name {
                ::core::mem::replace(self, $struct_name::Some(value))
            }
            pub const fn is_some(&self) -> bool {
                match self {
                    $struct_name::None => false,
                    $struct_name::Some(_) => true,
                }
            }
            pub const fn is_none(&self) -> bool {
                !self.is_some()
            }
            pub const fn as_ref(&self) -> Option<&$struct_type> {
                match *self {
                    $struct_name::Some(ref x) => Some(x),
                    $struct_name::None => None,
                }
            }
        }
    )}

    macro_rules! impl_option {
        ($struct_type:ident, $struct_name:ident, copy = false, clone = false, [$($derive:meta),* ]) => (
            impl_option_inner!($struct_type, $struct_name);
        );
        ($struct_type:ident, $struct_name:ident, copy = false, [$($derive:meta),* ]) => (
            impl_option_inner!($struct_type, $struct_name);

            impl From<$struct_name> for Option<$struct_type> {
                fn from(o: $struct_name) -> Option<$struct_type> {
                    match &o {
                        $struct_name::None => None,
                        $struct_name::Some(t) => Some(t.clone()),
                    }
                }
            }

            impl From<Option<$struct_type>> for $struct_name {
                fn from(o: Option<$struct_type>) -> $struct_name {
                    match &o {
                        None => $struct_name::None,
                        Some(t) => $struct_name::Some(t.clone()),
                    }
                }
            }

            impl $struct_name {
                pub fn into_option(self) -> Option<$struct_type> {
                    self.into()
                }
                pub fn map<U, F: FnOnce($struct_type) -> U>(self, f: F) -> Option<U> {
                    match self.into_option() {
                        None => None,
                        Some(s) => Some(f(s)),
                    }
                }

                pub fn and_then<U, F>(self, f: F) -> Option<U> where F: FnOnce($struct_type) -> Option<U> {
                    match self.into_option() {
                        None => None,
                        Some(s) => f(s),
                    }
                }
            }
        );
        ($struct_type:ident, $struct_name:ident, [$($derive:meta),* ]) => (
            impl_option_inner!($struct_type, $struct_name);

            impl From<$struct_name> for Option<$struct_type> {
                fn from(o: $struct_name) -> Option<$struct_type> {
                    match o {
                        $struct_name::None => None,
                        $struct_name::Some(t) => Some(t),
                    }
                }
            }

            impl From<Option<$struct_type>> for $struct_name {
                fn from(o: Option<$struct_type>) -> $struct_name {
                    match o {
                        None => $struct_name::None,
                        Some(t) => $struct_name::Some(t),
                    }
                }
            }

            impl $struct_name {
                pub fn into_option(self) -> Option<$struct_type> {
                    self.into()
                }
                pub fn map<U, F: FnOnce($struct_type) -> U>(self, f: F) -> Option<U> {
                    match self.into_option() {
                        None => None,
                        Some(s) => Some(f(s)),
                    }
                }

                pub fn and_then<U, F>(self, f: F) -> Option<U> where F: FnOnce($struct_type) -> Option<U> {
                    match self.into_option() {
                        None => None,
                        Some(s) => f(s),
                    }
                }
            }
        );
    }

    pub type AzX11Visual = *const c_void;
    pub type AzHwndHandle = *mut c_void;

    impl_option!(i32, AzOptionI32, [Debug, Copy, Clone]);
    impl_option!(f32, AzOptionF32, [Debug, Copy, Clone]);
    impl_option!(usize, AzOptionUsize, [Debug, Copy, Clone]);
    impl_option!(u32, AzOptionChar, [Debug, Copy, Clone]);

    impl_option!(AzThreadSendMsg, AzOptionThreadSendMsg, [Debug, Copy, Clone]);
    impl_option!(AzLayoutRect, AzOptionLayoutRect, [Debug, Copy, Clone]);
    impl_option!(AzRefAny, AzOptionRefAny, copy = false, clone = false, [Debug, Clone]);
    impl_option!(AzLayoutPoint, AzOptionLayoutPoint, [Debug, Copy, Clone]);
    impl_option!(AzWindowTheme, AzOptionWindowTheme, [Debug, Copy, Clone]);
    impl_option!(AzNodeId, AzOptionNodeId, [Debug, Copy, Clone]);
    impl_option!(AzDomNodeId, AzOptionDomNodeId, [Debug, Copy, Clone]);
    impl_option!(AzColorU, AzOptionColorU, [Debug, Copy, Clone]);
    impl_option!(AzRawImage, AzOptionRawImage, copy = false, [Debug, Clone]);
    impl_option!(AzSvgDashPattern, AzOptionSvgDashPattern, [Debug, Copy, Clone]);
    impl_option!(AzWaylandTheme, AzOptionWaylandTheme, copy = false, [Debug, Clone]);
    impl_option!(AzTaskBarIcon, AzOptionTaskBarIcon, copy = false, [Debug, Clone]);
    impl_option!(AzLogicalPosition, AzOptionLogicalPosition, [Debug, Copy, Clone]);
    impl_option!(AzPhysicalPositionI32, AzOptionPhysicalPositionI32, [Debug, Copy, Clone]);
    impl_option!(AzWindowIcon, AzOptionWindowIcon, copy = false, [Debug, Clone]);
    impl_option!(AzString, AzOptionString, copy = false, [Debug, Clone]);
    impl_option!(AzMouseCursorType, AzOptionMouseCursorType, [Debug, Copy, Clone]);
    impl_option!(AzLogicalSize, AzOptionLogicalSize, [Debug, Copy, Clone]);
    impl_option!(AzVirtualKeyCode, AzOptionVirtualKeyCode, [Debug, Copy, Clone]);
    impl_option!(AzPercentageValue, AzOptionPercentageValue, [Debug, Copy, Clone]);
    impl_option!(AzDom, AzOptionDom, copy = false, clone = false, [Debug, Clone]);
    impl_option!(AzTexture, AzOptionTexture, copy = false, clone = false, [Debug]);
    impl_option!(AzImageMask, AzOptionImageMask, copy = false, [Debug, Clone]);
    impl_option!(AzTabIndex, AzOptionTabIndex, [Debug, Copy, Clone]);
    impl_option!(AzCallback, AzOptionCallback, [Debug, Copy, Clone]);
    impl_option!(AzTagId, AzOptionTagId, [Debug, Copy, Clone]);
    impl_option!(AzDuration, AzOptionDuration, [Debug, Copy, Clone]);
    impl_option!(AzInstant, AzOptionInstant, copy = false, clone = false, [Debug]); // TODO: impl clone!
    impl_option!(AzU8VecRef, AzOptionU8VecRef, copy = false, clone = false, [Debug]);
    impl_option!(AzSystemClipboard, AzOptionSystemClipboard, copy = false,  clone = false, [Debug]);
    impl_option!(AzFileTypeList, AzOptionFileTypeList, copy = false, [Debug, Clone]);
    impl_option!(AzWindowState, AzOptionWindowState, copy = false, [Debug, Clone]);
    impl_option!(AzKeyboardState, AzOptionKeyboardState, copy = false, [Debug, Clone]);
    impl_option!(AzMouseState, AzOptionMouseState, [Debug, Clone]);
    /// `OptionColorInputOnValueChange` struct
    
#[doc(inline)] pub use crate::dll::AzOptionColorInputOnValueChange as OptionColorInputOnValueChange;
    /// `OptionButtonOnClick` struct
    
#[doc(inline)] pub use crate::dll::AzOptionButtonOnClick as OptionButtonOnClick;
    /// `OptionCheckBoxOnToggle` struct
    
#[doc(inline)] pub use crate::dll::AzOptionCheckBoxOnToggle as OptionCheckBoxOnToggle;
    /// `OptionTextInputOnTextInput` struct
    
#[doc(inline)] pub use crate::dll::AzOptionTextInputOnTextInput as OptionTextInputOnTextInput;
    /// `OptionTextInputOnVirtualKeyDown` struct
    
#[doc(inline)] pub use crate::dll::AzOptionTextInputOnVirtualKeyDown as OptionTextInputOnVirtualKeyDown;
    /// `OptionTextInputOnFocusLost` struct
    
#[doc(inline)] pub use crate::dll::AzOptionTextInputOnFocusLost as OptionTextInputOnFocusLost;
    /// `OptionTextInputSelection` struct
    
#[doc(inline)] pub use crate::dll::AzOptionTextInputSelection as OptionTextInputSelection;
    /// `OptionNumberInputOnValueChange` struct
    
#[doc(inline)] pub use crate::dll::AzOptionNumberInputOnValueChange as OptionNumberInputOnValueChange;
    /// `OptionMenuItemIcon` struct
    
#[doc(inline)] pub use crate::dll::AzOptionMenuItemIcon as OptionMenuItemIcon;
    /// `OptionMenuCallback` struct
    
#[doc(inline)] pub use crate::dll::AzOptionMenuCallback as OptionMenuCallback;
    /// `OptionVirtualKeyCodeCombo` struct
    
#[doc(inline)] pub use crate::dll::AzOptionVirtualKeyCodeCombo as OptionVirtualKeyCodeCombo;
    /// `OptionCssProperty` struct
    
#[doc(inline)] pub use crate::dll::AzOptionCssProperty as OptionCssProperty;
    /// `OptionPositionInfo` struct
    
#[doc(inline)] pub use crate::dll::AzOptionPositionInfo as OptionPositionInfo;
    /// `OptionTimerId` struct
    
#[doc(inline)] pub use crate::dll::AzOptionTimerId as OptionTimerId;
    /// `OptionThreadId` struct
    
#[doc(inline)] pub use crate::dll::AzOptionThreadId as OptionThreadId;
    /// `OptionI16` struct
    
#[doc(inline)] pub use crate::dll::AzOptionI16 as OptionI16;
    /// `OptionU16` struct
    
#[doc(inline)] pub use crate::dll::AzOptionU16 as OptionU16;
    /// `OptionU32` struct
    
#[doc(inline)] pub use crate::dll::AzOptionU32 as OptionU32;
    /// `OptionImageRef` struct
    
#[doc(inline)] pub use crate::dll::AzOptionImageRef as OptionImageRef;
    /// `OptionFontRef` struct
    
#[doc(inline)] pub use crate::dll::AzOptionFontRef as OptionFontRef;
    /// `OptionSystemClipboard` struct
    
#[doc(inline)] pub use crate::dll::AzOptionSystemClipboard as OptionSystemClipboard;
    /// `OptionFileTypeList` struct
    
#[doc(inline)] pub use crate::dll::AzOptionFileTypeList as OptionFileTypeList;
    /// `OptionWindowState` struct
    
#[doc(inline)] pub use crate::dll::AzOptionWindowState as OptionWindowState;
    /// `OptionMouseState` struct
    
#[doc(inline)] pub use crate::dll::AzOptionMouseState as OptionMouseState;
    /// `OptionKeyboardState` struct
    
#[doc(inline)] pub use crate::dll::AzOptionKeyboardState as OptionKeyboardState;
    /// `OptionStringVec` struct
    
#[doc(inline)] pub use crate::dll::AzOptionStringVec as OptionStringVec;
    /// `OptionFile` struct
    
#[doc(inline)] pub use crate::dll::AzOptionFile as OptionFile;
    /// `OptionGl` struct
    
#[doc(inline)] pub use crate::dll::AzOptionGl as OptionGl;
    /// `OptionThreadReceiveMsg` struct
    
#[doc(inline)] pub use crate::dll::AzOptionThreadReceiveMsg as OptionThreadReceiveMsg;
    /// `OptionPercentageValue` struct
    
#[doc(inline)] pub use crate::dll::AzOptionPercentageValue as OptionPercentageValue;
    /// `OptionAngleValue` struct
    
#[doc(inline)] pub use crate::dll::AzOptionAngleValue as OptionAngleValue;
    /// `OptionRendererOptions` struct
    
#[doc(inline)] pub use crate::dll::AzOptionRendererOptions as OptionRendererOptions;
    /// `OptionCallback` struct
    
#[doc(inline)] pub use crate::dll::AzOptionCallback as OptionCallback;
    /// `OptionThreadSendMsg` struct
    
#[doc(inline)] pub use crate::dll::AzOptionThreadSendMsg as OptionThreadSendMsg;
    /// `OptionLayoutRect` struct
    
#[doc(inline)] pub use crate::dll::AzOptionLayoutRect as OptionLayoutRect;
    /// `OptionRefAny` struct
    
#[doc(inline)] pub use crate::dll::AzOptionRefAny as OptionRefAny;
    /// `OptionInlineText` struct
    
#[doc(inline)] pub use crate::dll::AzOptionInlineText as OptionInlineText;
    /// `OptionLayoutPoint` struct
    
#[doc(inline)] pub use crate::dll::AzOptionLayoutPoint as OptionLayoutPoint;
    /// `OptionLayoutSize` struct
    
#[doc(inline)] pub use crate::dll::AzOptionLayoutSize as OptionLayoutSize;
    /// `OptionWindowTheme` struct
    
#[doc(inline)] pub use crate::dll::AzOptionWindowTheme as OptionWindowTheme;
    /// `OptionNodeId` struct
    
#[doc(inline)] pub use crate::dll::AzOptionNodeId as OptionNodeId;
    /// `OptionDomNodeId` struct
    
#[doc(inline)] pub use crate::dll::AzOptionDomNodeId as OptionDomNodeId;
    /// `OptionColorU` struct
    
#[doc(inline)] pub use crate::dll::AzOptionColorU as OptionColorU;
    /// `OptionRawImage` struct
    
#[doc(inline)] pub use crate::dll::AzOptionRawImage as OptionRawImage;
    /// `OptionSvgDashPattern` struct
    
#[doc(inline)] pub use crate::dll::AzOptionSvgDashPattern as OptionSvgDashPattern;
    /// `OptionWaylandTheme` struct
    
#[doc(inline)] pub use crate::dll::AzOptionWaylandTheme as OptionWaylandTheme;
    /// `OptionTaskBarIcon` struct
    
#[doc(inline)] pub use crate::dll::AzOptionTaskBarIcon as OptionTaskBarIcon;
    /// `OptionHwndHandle` struct
    
#[doc(inline)] pub use crate::dll::AzOptionHwndHandle as OptionHwndHandle;
    /// `OptionLogicalPosition` struct
    
#[doc(inline)] pub use crate::dll::AzOptionLogicalPosition as OptionLogicalPosition;
    /// `OptionPhysicalPositionI32` struct
    
#[doc(inline)] pub use crate::dll::AzOptionPhysicalPositionI32 as OptionPhysicalPositionI32;
    /// `OptionWindowIcon` struct
    
#[doc(inline)] pub use crate::dll::AzOptionWindowIcon as OptionWindowIcon;
    /// `OptionString` struct
    
#[doc(inline)] pub use crate::dll::AzOptionString as OptionString;
    /// `OptionX11Visual` struct
    
#[doc(inline)] pub use crate::dll::AzOptionX11Visual as OptionX11Visual;
    /// `OptionI32` struct
    
#[doc(inline)] pub use crate::dll::AzOptionI32 as OptionI32;
    /// `OptionF32` struct
    
#[doc(inline)] pub use crate::dll::AzOptionF32 as OptionF32;
    /// `OptionMouseCursorType` struct
    
#[doc(inline)] pub use crate::dll::AzOptionMouseCursorType as OptionMouseCursorType;
    /// `OptionLogicalSize` struct
    
#[doc(inline)] pub use crate::dll::AzOptionLogicalSize as OptionLogicalSize;
    /// Option<char> but the char is a u32, for C FFI stability reasons
    
#[doc(inline)] pub use crate::dll::AzOptionChar as OptionChar;
    /// `OptionVirtualKeyCode` struct
    
#[doc(inline)] pub use crate::dll::AzOptionVirtualKeyCode as OptionVirtualKeyCode;
    /// `OptionDom` struct
    
#[doc(inline)] pub use crate::dll::AzOptionDom as OptionDom;
    /// `OptionTexture` struct
    
#[doc(inline)] pub use crate::dll::AzOptionTexture as OptionTexture;
    /// `OptionImageMask` struct
    
#[doc(inline)] pub use crate::dll::AzOptionImageMask as OptionImageMask;
    /// `OptionTabIndex` struct
    
#[doc(inline)] pub use crate::dll::AzOptionTabIndex as OptionTabIndex;
    /// `OptionTagId` struct
    
#[doc(inline)] pub use crate::dll::AzOptionTagId as OptionTagId;
    /// `OptionDuration` struct
    
#[doc(inline)] pub use crate::dll::AzOptionDuration as OptionDuration;
    /// `OptionInstant` struct
    
#[doc(inline)] pub use crate::dll::AzOptionInstant as OptionInstant;
    /// `OptionUsize` struct
    
#[doc(inline)] pub use crate::dll::AzOptionUsize as OptionUsize;
    /// `OptionU8Vec` struct
    
#[doc(inline)] pub use crate::dll::AzOptionU8Vec as OptionU8Vec;
    /// `OptionU8VecRef` struct
    
#[doc(inline)] pub use crate::dll::AzOptionU8VecRef as OptionU8VecRef;
}

pub mod error {
    #![allow(dead_code, unused_imports)]
    //! Definition of error and `Result<T, E>`  types
    use crate::dll::*;
    use core::ffi::c_void;
    /// `ResultXmlXmlError` struct
    
#[doc(inline)] pub use crate::dll::AzResultXmlXmlError as ResultXmlXmlError;
    /// `ResultRawImageDecodeImageError` struct
    
#[doc(inline)] pub use crate::dll::AzResultRawImageDecodeImageError as ResultRawImageDecodeImageError;
    /// `ResultU8VecEncodeImageError` struct
    
#[doc(inline)] pub use crate::dll::AzResultU8VecEncodeImageError as ResultU8VecEncodeImageError;
    /// `ResultSvgXmlNodeSvgParseError` struct
    
#[doc(inline)] pub use crate::dll::AzResultSvgXmlNodeSvgParseError as ResultSvgXmlNodeSvgParseError;
    /// `ResultSvgSvgParseError` struct
    
#[doc(inline)] pub use crate::dll::AzResultSvgSvgParseError as ResultSvgSvgParseError;
    /// `SvgParseError` struct
    
#[doc(inline)] pub use crate::dll::AzSvgParseError as SvgParseError;
    /// `XmlError` struct
    
#[doc(inline)] pub use crate::dll::AzXmlError as XmlError;
    /// `DuplicatedNamespaceError` struct
    
#[doc(inline)] pub use crate::dll::AzDuplicatedNamespaceError as DuplicatedNamespaceError;
    /// `UnknownNamespaceError` struct
    
#[doc(inline)] pub use crate::dll::AzUnknownNamespaceError as UnknownNamespaceError;
    /// `UnexpectedCloseTagError` struct
    
#[doc(inline)] pub use crate::dll::AzUnexpectedCloseTagError as UnexpectedCloseTagError;
    /// `UnknownEntityReferenceError` struct
    
#[doc(inline)] pub use crate::dll::AzUnknownEntityReferenceError as UnknownEntityReferenceError;
    /// `DuplicatedAttributeError` struct
    
#[doc(inline)] pub use crate::dll::AzDuplicatedAttributeError as DuplicatedAttributeError;
    /// `XmlParseError` struct
    
#[doc(inline)] pub use crate::dll::AzXmlParseError as XmlParseError;
    /// `XmlTextError` struct
    
#[doc(inline)] pub use crate::dll::AzXmlTextError as XmlTextError;
    /// `XmlStreamError` struct
    
#[doc(inline)] pub use crate::dll::AzXmlStreamError as XmlStreamError;
    /// `NonXmlCharError` struct
    
#[doc(inline)] pub use crate::dll::AzNonXmlCharError as NonXmlCharError;
    /// `InvalidCharError` struct
    
#[doc(inline)] pub use crate::dll::AzInvalidCharError as InvalidCharError;
    /// `InvalidCharMultipleError` struct
    
#[doc(inline)] pub use crate::dll::AzInvalidCharMultipleError as InvalidCharMultipleError;
    /// `InvalidQuoteError` struct
    
#[doc(inline)] pub use crate::dll::AzInvalidQuoteError as InvalidQuoteError;
    /// `InvalidSpaceError` struct
    
#[doc(inline)] pub use crate::dll::AzInvalidSpaceError as InvalidSpaceError;
    /// `InvalidStringError` struct
    
#[doc(inline)] pub use crate::dll::AzInvalidStringError as InvalidStringError;
    /// `SvgParseErrorPosition` struct
    
#[doc(inline)] pub use crate::dll::AzSvgParseErrorPosition as SvgParseErrorPosition;
}

