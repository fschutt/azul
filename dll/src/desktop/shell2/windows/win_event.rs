//! Win32 keyboard event handling: VK code translation, scancode processing,
//! and cursor management. Adapted from winit.
//!
//! The translation TABLE itself lives in
//! [`crate::desktop::shell2::common::event::win32_vkey_to_virtual_key`], next to
//! the macOS one, so the shared keycode-coverage manifest
//! (`layout/tests/keycode_table_manifest_is_exhaustive.rs`) can compare all
//! four platform tables against each other. Under `#[cfg(target_os = "windows")]`
//! it was invisible to every test the CI host can run. What stays here is the
//! part that genuinely needs Windows: resolving the seven layout-dependent
//! `VK_OEM_*` codes through `MapVirtualKeyA`.
//
// Copied and modified from:
// https://github.com/rust-windowing/winit/blob/1c4d6e7613c3a3870cecb4cfa0eecc97409d45ff/src/platform_impl/windows/event.rs
//
// We don't need the entire winit crate just to handle events,
// that's why it is copied here

/*
    Apache License
                               Version 2.0, January 2004
                            http://www.apache.org/licenses/

       TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

       1. Definitions.

          "License" shall mean the terms and conditions for use, reproduction,
          and distribution as defined by Sections 1 through 9 of this document.

          "Licensor" shall mean the copyright owner or entity authorized by
          the copyright owner that is granting the License.

          "Legal Entity" shall mean the union of the acting entity and all
          other entities that control, are controlled by, or are under common
          control with that entity. For the purposes of this definition,
          "control" means (i) the power, direct or indirect, to cause the
          direction or management of such entity, whether by contract or
          otherwise, or (ii) ownership of fifty percent (50%) or more of the
          outstanding shares, or (iii) beneficial ownership of such entity.

          "You" (or "Your") shall mean an individual or Legal Entity
          exercising permissions granted by this License.

          "Source" form shall mean the preferred form for making modifications,
          including but not limited to software source code, documentation
          source, and configuration files.

          "Object" form shall mean any form resulting from mechanical
          transformation or translation of a Source form, including but
          not limited to compiled object code, generated documentation,
          and conversions to other media types.

          "Work" shall mean the work of authorship, whether in Source or
          Object form, made available under the License, as indicated by a
          copyright notice that is included in or attached to the work
          (an example is provided in the Appendix below).

          "Derivative Works" shall mean any work, whether in Source or Object
          form, that is based on (or derived from) the Work and for which the
          editorial revisions, annotations, elaborations, or other modifications
          represent, as a whole, an original work of authorship. For the purposes
          of this License, Derivative Works shall not include works that remain
          separable from, or merely link (or bind by name) to the interfaces of,
          the Work and Derivative Works thereof.

          "Contribution" shall mean any work of authorship, including
          the original version of the Work and any modifications or additions
          to that Work or Derivative Works thereof, that is intentionally
          submitted to Licensor for inclusion in the Work by the copyright owner
          or by an individual or Legal Entity authorized to submit on behalf of
          the copyright owner. For the purposes of this definition, "submitted"
          means any form of electronic, verbal, or written communication sent
          to the Licensor or its representatives, including but not limited to
          communication on electronic mailing lists, source code control systems,
          and issue tracking systems that are managed by, or on behalf of, the
          Licensor for the purpose of discussing and improving the Work, but
          excluding communication that is conspicuously marked or otherwise
          designated in writing by the copyright owner as "Not a Contribution."

          "Contributor" shall mean Licensor and any individual or Legal Entity
          on behalf of whom a Contribution has been received by Licensor and
          subsequently incorporated within the Work.

       2. Grant of Copyright License. Subject to the terms and conditions of
          this License, each Contributor hereby grants to You a perpetual,
          worldwide, non-exclusive, no-charge, royalty-free, irrevocable
          copyright license to reproduce, prepare Derivative Works of,
          publicly display, publicly perform, sublicense, and distribute the
          Work and such Derivative Works in Source or Object form.

       3. Grant of Patent License. Subject to the terms and conditions of
          this License, each Contributor hereby grants to You a perpetual,
          worldwide, non-exclusive, no-charge, royalty-free, irrevocable
          (except as stated in this section) patent license to make, have made,
          use, offer to sell, sell, import, and otherwise transfer the Work,
          where such license applies only to those patent claims licensable
          by such Contributor that are necessarily infringed by their
          Contribution(s) alone or by combination of their Contribution(s)
          with the Work to which such Contribution(s) was submitted. If You
          institute patent litigation against any entity (including a
          cross-claim or counterclaim in a lawsuit) alleging that the Work
          or a Contribution incorporated within the Work constitutes direct
          or contributory patent infringement, then any patent licenses
          granted to You under this License for that Work shall terminate
          as of the date such litigation is filed.

       4. Redistribution. You may reproduce and distribute copies of the
          Work or Derivative Works thereof in any medium, with or without
          modifications, and in Source or Object form, provided that You
          meet the following conditions:

          (a) You must give any other recipients of the Work or
              Derivative Works a copy of this License; and

          (b) You must cause any modified files to carry prominent notices
              stating that You changed the files; and

          (c) You must retain, in the Source form of any Derivative Works
              that You distribute, all copyright, patent, trademark, and
              attribution notices from the Source form of the Work,
              excluding those notices that do not pertain to any part of
              the Derivative Works; and

          (d) If the Work includes a "NOTICE" text file as part of its
              distribution, then any Derivative Works that You distribute must
              include a readable copy of the attribution notices contained
              within such NOTICE file, excluding those notices that do not
              pertain to any part of the Derivative Works, in at least one
              of the following places: within a NOTICE text file distributed
              as part of the Derivative Works; within the Source form or
              documentation, if provided along with the Derivative Works; or,
              within a display generated by the Derivative Works, if and
              wherever such third-party notices normally appear. The contents
              of the NOTICE file are for informational purposes only and
              do not modify the License. You may add Your own attribution
              notices within Derivative Works that You distribute, alongside
              or as an addendum to the NOTICE text from the Work, provided
              that such additional attribution notices cannot be construed
              as modifying the License.

          You may add Your own copyright statement to Your modifications and
          may provide additional or different license terms and conditions
          for use, reproduction, or distribution of Your modifications, or
          for any such Derivative Works as a whole, provided Your use,
          reproduction, and distribution of the Work otherwise complies with
          the conditions stated in this License.

       5. Submission of Contributions. Unless You explicitly state otherwise,
          any Contribution intentionally submitted for inclusion in the Work
          by You to the Licensor shall be under the terms and conditions of
          this License, without any additional terms or conditions.
          Notwithstanding the above, nothing herein shall supersede or modify
          the terms of any separate license agreement you may have executed
          with Licensor regarding such Contributions.

       6. Trademarks. This License does not grant permission to use the trade
          names, trademarks, service marks, or product names of the Licensor,
          except as required for reasonable and customary use in describing the
          origin of the Work and reproducing the content of the NOTICE file.

       7. Disclaimer of Warranty. Unless required by applicable law or
          agreed to in writing, Licensor provides the Work (and each
          Contributor provides its Contributions) on an "AS IS" BASIS,
          WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
          implied, including, without limitation, any warranties or conditions
          of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
          PARTICULAR PURPOSE. You are solely responsible for determining the
          appropriateness of using or redistributing the Work and assume any
          risks associated with Your exercise of permissions under this License.

       8. Limitation of Liability. In no event and under no legal theory,
          whether in tort (including negligence), contract, or otherwise,
          unless required by applicable law (such as deliberate and grossly
          negligent acts) or agreed to in writing, shall any Contributor be
          liable to You for damages, including any direct, indirect, special,
          incidental, or consequential damages of any character arising as a
          result of this License or out of the use or inability to use the
          Work (including but not limited to damages for loss of goodwill,
          work stoppage, computer failure or malfunction, or any and all
          other commercial damages or losses), even if such Contributor
          has been advised of the possibility of such damages.

       9. Accepting Warranty or Additional Liability. While redistributing
          the Work or Derivative Works thereof, You may choose to offer,
          and charge a fee for, acceptance of support, warranty, indemnity,
          or other liability obligations and/or rights consistent with this
          License. However, in accepting such obligations, You may act only
          on Your own behalf and on Your sole responsibility, not on behalf
          of any other Contributor, and only if You agree to indemnify,
          defend, and hold each Contributor harmless for any liability
          incurred by, or claims asserted against, such Contributor by reason
          of your accepting any such warranty or additional liability.

       END OF TERMS AND CONDITIONS

       APPENDIX: How to apply the Apache License to your work.

          To apply the Apache License to your work, attach the following
          boilerplate notice, with the fields enclosed by brackets "{}"
          replaced with your own identifying information. (Don't include
          the brackets!)  The text should be enclosed in the appropriate
          comment syntax for the file format. We also recommend that a
          file or class name and description of purpose be included on the
          same "printed page" as the copyright notice for easier
          identification within third-party archives.

       Copyright 2021 The winit contributors

       Licensed under the Apache License, Version 2.0 (the "License");
       you may not use this file except in compliance with the License.
       You may obtain a copy of the License at

           http://www.apache.org/licenses/LICENSE-2.0

       Unless required by applicable law or agreed to in writing, software
       distributed under the License is distributed on an "AS IS" BASIS,
       WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
       See the License for the specific language governing permissions and
       limitations under the License.
*/

use core::char;

use azul_core::window::VirtualKeyCode;
use winapi::um::winuser;

use crate::desktop::shell2::common::event::{win32_vk, win32_vkey_to_virtual_key};

/// The shared table's `VK_*` constants must be `winapi`'s, digit for digit.
///
/// `common/event.rs` is compiled on every platform and cannot depend on
/// `winapi`, so it transcribes the codes. This makes a mistyped transcription a
/// BUILD ERROR on Windows instead of one key that quietly stops working.
const _: () = {
    macro_rules! transcribed_correctly {
        ($($name:ident),* $(,)?) => { $( assert!(win32_vk::$name == winuser::$name); )* };
    }
    transcribed_correctly!(
        VK_BACK,
        VK_TAB,
        VK_RETURN,
        VK_SHIFT,
        VK_CONTROL,
        VK_MENU,
        VK_PAUSE,
        VK_CAPITAL,
        VK_KANA,
        VK_KANJI,
        VK_ESCAPE,
        VK_CONVERT,
        VK_NONCONVERT,
        VK_SPACE,
        VK_PRIOR,
        VK_NEXT,
        VK_END,
        VK_HOME,
        VK_LEFT,
        VK_UP,
        VK_RIGHT,
        VK_DOWN,
        VK_SNAPSHOT,
        VK_INSERT,
        VK_DELETE,
        VK_LWIN,
        VK_RWIN,
        VK_APPS,
        VK_SLEEP,
        VK_NUMPAD0,
        VK_NUMPAD1,
        VK_NUMPAD2,
        VK_NUMPAD3,
        VK_NUMPAD4,
        VK_NUMPAD5,
        VK_NUMPAD6,
        VK_NUMPAD7,
        VK_NUMPAD8,
        VK_NUMPAD9,
        VK_MULTIPLY,
        VK_ADD,
        VK_SUBTRACT,
        VK_DECIMAL,
        VK_DIVIDE,
        VK_F1,
        VK_F2,
        VK_F3,
        VK_F4,
        VK_F5,
        VK_F6,
        VK_F7,
        VK_F8,
        VK_F9,
        VK_F10,
        VK_F11,
        VK_F12,
        VK_F13,
        VK_F14,
        VK_F15,
        VK_F16,
        VK_F17,
        VK_F18,
        VK_F19,
        VK_F20,
        VK_F21,
        VK_F22,
        VK_F23,
        VK_F24,
        VK_NUMLOCK,
        VK_SCROLL,
        VK_LSHIFT,
        VK_RSHIFT,
        VK_LCONTROL,
        VK_RCONTROL,
        VK_LMENU,
        VK_RMENU,
        VK_BROWSER_BACK,
        VK_BROWSER_FORWARD,
        VK_BROWSER_REFRESH,
        VK_BROWSER_STOP,
        VK_BROWSER_SEARCH,
        VK_BROWSER_FAVORITES,
        VK_BROWSER_HOME,
        VK_VOLUME_MUTE,
        VK_VOLUME_DOWN,
        VK_VOLUME_UP,
        VK_MEDIA_NEXT_TRACK,
        VK_MEDIA_PREV_TRACK,
        VK_MEDIA_STOP,
        VK_MEDIA_PLAY_PAUSE,
        VK_LAUNCH_MAIL,
        VK_LAUNCH_MEDIA_SELECT,
        VK_OEM_1,
        VK_OEM_PLUS,
        VK_OEM_COMMA,
        VK_OEM_MINUS,
        VK_OEM_PERIOD,
        VK_OEM_2,
        VK_OEM_3,
        VK_OEM_4,
        VK_OEM_5,
        VK_OEM_6,
        VK_OEM_7,
        VK_OEM_102,
    );
};

/// Translate a Win32 VK code to a [`VirtualKeyCode`] against the ACTIVE layout.
///
/// The table is shared (`common::event::win32_vkey_to_virtual_key`); all this
/// adds is the one thing only Windows can answer — which character the active
/// layout puts on a `VK_OEM_*` position.
pub fn vkey_to_winit_vkey(vkey: i32) -> Option<VirtualKeyCode> {
    win32_vkey_to_virtual_key(vkey, active_layout_char(vkey))
}

/// The character the active keyboard layout produces for `vkey`, if any.
///
/// This is needed as windows doesn't properly distinguish
/// some virtual key codes for different keyboard layouts.
fn active_layout_char(win_virtual_key: i32) -> Option<char> {
    let char_key =
        unsafe { winuser::MapVirtualKeyA(win_virtual_key as u32, winuser::MAPVK_VK_TO_CHAR) }
            & 0x7FFF;
    char::from_u32(char_key)
}
