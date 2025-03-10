// Win32 event handling
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

use core::{
    char, ptr,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};

use azul_core::window::{ScanCode, VirtualKeyCode};
use winapi::{
    shared::minwindef::{HKL, HKL__, LPARAM, UINT, WPARAM},
    um::winuser,
};

fn key_pressed(vkey: i32) -> bool {
    unsafe { (winuser::GetKeyState(vkey) & (1 << 15)) == (1 << 15) }
}

/*
    pub fn get_key_mods() -> ModifiersState {
        let filter_out_altgr = layout_uses_altgr() && key_pressed(winuser::VK_RMENU);

        let mut mods = ModifiersState::empty();
        mods.set(ModifiersState::SHIFT, key_pressed(winuser::VK_SHIFT));
        mods.set(
            ModifiersState::CTRL,
            key_pressed(winuser::VK_CONTROL) && !filter_out_altgr,
        );
        mods.set(
            ModifiersState::ALT,
            key_pressed(winuser::VK_MENU) && !filter_out_altgr,
        );
        mods.set(
            ModifiersState::LOGO,
            key_pressed(winuser::VK_LWIN) || key_pressed(winuser::VK_RWIN),
        );
        mods
    }

    bitflags! {
        #[derive(Default)]
        pub struct ModifiersStateSide: u32 {
            const LSHIFT = 0b010 << 0;
            const RSHIFT = 0b001 << 0;

            const LCTRL = 0b010 << 3;
            const RCTRL = 0b001 << 3;

            const LALT = 0b010 << 6;
            const RALT = 0b001 << 6;

            const LLOGO = 0b010 << 9;
            const RLOGO = 0b001 << 9;
        }
    }

    impl ModifiersStateSide {
        pub fn filter_out_altgr(&self) -> ModifiersStateSide {
            match layout_uses_altgr() && self.contains(Self::RALT) {
                false => *self,
                true => *self & !(Self::LCTRL | Self::RCTRL | Self::LALT | Self::RALT),
            }
        }
    }

    impl From<ModifiersStateSide> for ModifiersState {
        fn from(side: ModifiersStateSide) -> Self {
            let mut state = ModifiersState::default();
            state.set(
                Self::SHIFT,
                side.intersects(ModifiersStateSide::LSHIFT | ModifiersStateSide::RSHIFT),
            );
            state.set(
                Self::CTRL,
                side.intersects(ModifiersStateSide::LCTRL | ModifiersStateSide::RCTRL),
            );
            state.set(
                Self::ALT,
                side.intersects(ModifiersStateSide::LALT | ModifiersStateSide::RALT),
            );
            state.set(
                Self::LOGO,
                side.intersects(ModifiersStateSide::LLOGO | ModifiersStateSide::RLOGO),
            );
            state
        }
    }
*/

pub fn get_pressed_keys() -> impl Iterator<Item = i32> {
    let mut keyboard_state = vec![0u8; 256];
    unsafe { winuser::GetKeyboardState(keyboard_state.as_mut_ptr()) };
    keyboard_state
        .into_iter()
        .enumerate()
        .filter(|(_, p)| (*p & (1 << 7)) != 0) // whether or not a key is pressed is communicated via the high-order bit
        .map(|(i, _)| i as i32)
}

unsafe fn get_char(keyboard_state: &[u8; 256], v_key: u32, hkl: HKL) -> Option<char> {
    let mut unicode_bytes = [0u16; 5];
    let len = winuser::ToUnicodeEx(
        v_key,
        0,
        keyboard_state.as_ptr(),
        unicode_bytes.as_mut_ptr(),
        unicode_bytes.len() as _,
        0,
        hkl,
    );
    if len >= 1 {
        char::decode_utf16(unicode_bytes.iter().cloned())
            .next()
            .and_then(|c| c.ok())
    } else {
        None
    }
}

/// Figures out if the keyboard layout has an AltGr key instead of an Alt key.
///
/// Unfortunately, the Windows API doesn't give a way for us to conveniently figure that out. So,
/// we use a technique blatantly stolen from [the Firefox source code][source]: iterate over every
/// possible virtual key and compare the `char` output when AltGr is pressed vs when it isn't. If
/// pressing AltGr outputs characters that are different from the standard characters, the layout
/// uses AltGr. Otherwise, it doesn't.
///
/// [source]: https://github.com/mozilla/gecko-dev/blob/265e6721798a455604328ed5262f430cfcc37c2f/widget/windows/KeyboardLayout.cpp#L4356-L4416
fn layout_uses_altgr() -> bool {
    unsafe {
        static ACTIVE_LAYOUT: AtomicPtr<HKL__> = AtomicPtr::new(ptr::null_mut());
        static USES_ALTGR: AtomicBool = AtomicBool::new(false);

        let hkl = winuser::GetKeyboardLayout(0);
        let old_hkl = ACTIVE_LAYOUT.swap(hkl, Ordering::SeqCst);

        if hkl == old_hkl {
            return USES_ALTGR.load(Ordering::SeqCst);
        }

        let mut keyboard_state_altgr = [0u8; 256];
        // AltGr is an alias for Ctrl+Alt for... some reason. Whatever it is, those are the
        // keypresses we have to emulate to do an AltGr test.
        keyboard_state_altgr[winuser::VK_MENU as usize] = 0x80;
        keyboard_state_altgr[winuser::VK_CONTROL as usize] = 0x80;

        let keyboard_state_empty = [0u8; 256];

        for v_key in 0..=255 {
            let key_noaltgr = get_char(&keyboard_state_empty, v_key, hkl);
            let key_altgr = get_char(&keyboard_state_altgr, v_key, hkl);
            if let (Some(noaltgr), Some(altgr)) = (key_noaltgr, key_altgr) {
                if noaltgr != altgr {
                    USES_ALTGR.store(true, Ordering::SeqCst);
                    return true;
                }
            }
        }

        USES_ALTGR.store(false, Ordering::SeqCst);
        false
    }
}

pub fn vkey_to_winit_vkey(vkey: i32) -> Option<VirtualKeyCode> {
    // VK_* codes are documented here https://msdn.microsoft.com/en-us/library/windows/desktop/dd375731(v=vs.85).aspx
    match vkey {
        //winuser::VK_LBUTTON => Some(VirtualKeyCode::Lbutton),
        //winuser::VK_RBUTTON => Some(VirtualKeyCode::Rbutton),
        //winuser::VK_CANCEL => Some(VirtualKeyCode::Cancel),
        //winuser::VK_MBUTTON => Some(VirtualKeyCode::Mbutton),
        //winuser::VK_XBUTTON1 => Some(VirtualKeyCode::Xbutton1),
        //winuser::VK_XBUTTON2 => Some(VirtualKeyCode::Xbutton2),
        winuser::VK_BACK => Some(VirtualKeyCode::Back),
        winuser::VK_TAB => Some(VirtualKeyCode::Tab),
        //winuser::VK_CLEAR => Some(VirtualKeyCode::Clear),
        winuser::VK_RETURN => Some(VirtualKeyCode::Return),
        winuser::VK_LSHIFT => Some(VirtualKeyCode::LShift),
        winuser::VK_RSHIFT => Some(VirtualKeyCode::RShift),
        winuser::VK_LCONTROL => Some(VirtualKeyCode::LControl),
        winuser::VK_RCONTROL => Some(VirtualKeyCode::RControl),
        winuser::VK_LMENU => Some(VirtualKeyCode::LAlt),
        winuser::VK_RMENU => Some(VirtualKeyCode::RAlt),
        winuser::VK_PAUSE => Some(VirtualKeyCode::Pause),
        winuser::VK_CAPITAL => Some(VirtualKeyCode::Capital),
        winuser::VK_KANA => Some(VirtualKeyCode::Kana),
        //winuser::VK_HANGUEL => Some(VirtualKeyCode::Hanguel),
        //winuser::VK_HANGUL => Some(VirtualKeyCode::Hangul),
        //winuser::VK_JUNJA => Some(VirtualKeyCode::Junja),
        //winuser::VK_FINAL => Some(VirtualKeyCode::Final),
        //winuser::VK_HANJA => Some(VirtualKeyCode::Hanja),
        winuser::VK_KANJI => Some(VirtualKeyCode::Kanji),
        winuser::VK_ESCAPE => Some(VirtualKeyCode::Escape),
        winuser::VK_CONVERT => Some(VirtualKeyCode::Convert),
        winuser::VK_NONCONVERT => Some(VirtualKeyCode::NoConvert),
        //winuser::VK_ACCEPT => Some(VirtualKeyCode::Accept),
        //winuser::VK_MODECHANGE => Some(VirtualKeyCode::Modechange),
        winuser::VK_SPACE => Some(VirtualKeyCode::Space),
        winuser::VK_PRIOR => Some(VirtualKeyCode::PageUp),
        winuser::VK_NEXT => Some(VirtualKeyCode::PageDown),
        winuser::VK_END => Some(VirtualKeyCode::End),
        winuser::VK_HOME => Some(VirtualKeyCode::Home),
        winuser::VK_LEFT => Some(VirtualKeyCode::Left),
        winuser::VK_UP => Some(VirtualKeyCode::Up),
        winuser::VK_RIGHT => Some(VirtualKeyCode::Right),
        winuser::VK_DOWN => Some(VirtualKeyCode::Down),
        //winuser::VK_SELECT => Some(VirtualKeyCode::Select),
        //winuser::VK_PRINT => Some(VirtualKeyCode::Print),
        //winuser::VK_EXECUTE => Some(VirtualKeyCode::Execute),
        winuser::VK_SNAPSHOT => Some(VirtualKeyCode::Snapshot),
        winuser::VK_INSERT => Some(VirtualKeyCode::Insert),
        winuser::VK_DELETE => Some(VirtualKeyCode::Delete),
        //winuser::VK_HELP => Some(VirtualKeyCode::Help),
        0x30 => Some(VirtualKeyCode::Key0),
        0x31 => Some(VirtualKeyCode::Key1),
        0x32 => Some(VirtualKeyCode::Key2),
        0x33 => Some(VirtualKeyCode::Key3),
        0x34 => Some(VirtualKeyCode::Key4),
        0x35 => Some(VirtualKeyCode::Key5),
        0x36 => Some(VirtualKeyCode::Key6),
        0x37 => Some(VirtualKeyCode::Key7),
        0x38 => Some(VirtualKeyCode::Key8),
        0x39 => Some(VirtualKeyCode::Key9),
        0x41 => Some(VirtualKeyCode::A),
        0x42 => Some(VirtualKeyCode::B),
        0x43 => Some(VirtualKeyCode::C),
        0x44 => Some(VirtualKeyCode::D),
        0x45 => Some(VirtualKeyCode::E),
        0x46 => Some(VirtualKeyCode::F),
        0x47 => Some(VirtualKeyCode::G),
        0x48 => Some(VirtualKeyCode::H),
        0x49 => Some(VirtualKeyCode::I),
        0x4A => Some(VirtualKeyCode::J),
        0x4B => Some(VirtualKeyCode::K),
        0x4C => Some(VirtualKeyCode::L),
        0x4D => Some(VirtualKeyCode::M),
        0x4E => Some(VirtualKeyCode::N),
        0x4F => Some(VirtualKeyCode::O),
        0x50 => Some(VirtualKeyCode::P),
        0x51 => Some(VirtualKeyCode::Q),
        0x52 => Some(VirtualKeyCode::R),
        0x53 => Some(VirtualKeyCode::S),
        0x54 => Some(VirtualKeyCode::T),
        0x55 => Some(VirtualKeyCode::U),
        0x56 => Some(VirtualKeyCode::V),
        0x57 => Some(VirtualKeyCode::W),
        0x58 => Some(VirtualKeyCode::X),
        0x59 => Some(VirtualKeyCode::Y),
        0x5A => Some(VirtualKeyCode::Z),
        winuser::VK_LWIN => Some(VirtualKeyCode::LWin),
        winuser::VK_RWIN => Some(VirtualKeyCode::RWin),
        winuser::VK_APPS => Some(VirtualKeyCode::Apps),
        winuser::VK_SLEEP => Some(VirtualKeyCode::Sleep),
        winuser::VK_NUMPAD0 => Some(VirtualKeyCode::Numpad0),
        winuser::VK_NUMPAD1 => Some(VirtualKeyCode::Numpad1),
        winuser::VK_NUMPAD2 => Some(VirtualKeyCode::Numpad2),
        winuser::VK_NUMPAD3 => Some(VirtualKeyCode::Numpad3),
        winuser::VK_NUMPAD4 => Some(VirtualKeyCode::Numpad4),
        winuser::VK_NUMPAD5 => Some(VirtualKeyCode::Numpad5),
        winuser::VK_NUMPAD6 => Some(VirtualKeyCode::Numpad6),
        winuser::VK_NUMPAD7 => Some(VirtualKeyCode::Numpad7),
        winuser::VK_NUMPAD8 => Some(VirtualKeyCode::Numpad8),
        winuser::VK_NUMPAD9 => Some(VirtualKeyCode::Numpad9),
        winuser::VK_MULTIPLY => Some(VirtualKeyCode::NumpadMultiply),
        winuser::VK_ADD => Some(VirtualKeyCode::NumpadAdd),
        //winuser::VK_SEPARATOR => Some(VirtualKeyCode::Separator),
        winuser::VK_SUBTRACT => Some(VirtualKeyCode::NumpadSubtract),
        winuser::VK_DECIMAL => Some(VirtualKeyCode::NumpadDecimal),
        winuser::VK_DIVIDE => Some(VirtualKeyCode::NumpadDivide),
        winuser::VK_F1 => Some(VirtualKeyCode::F1),
        winuser::VK_F2 => Some(VirtualKeyCode::F2),
        winuser::VK_F3 => Some(VirtualKeyCode::F3),
        winuser::VK_F4 => Some(VirtualKeyCode::F4),
        winuser::VK_F5 => Some(VirtualKeyCode::F5),
        winuser::VK_F6 => Some(VirtualKeyCode::F6),
        winuser::VK_F7 => Some(VirtualKeyCode::F7),
        winuser::VK_F8 => Some(VirtualKeyCode::F8),
        winuser::VK_F9 => Some(VirtualKeyCode::F9),
        winuser::VK_F10 => Some(VirtualKeyCode::F10),
        winuser::VK_F11 => Some(VirtualKeyCode::F11),
        winuser::VK_F12 => Some(VirtualKeyCode::F12),
        winuser::VK_F13 => Some(VirtualKeyCode::F13),
        winuser::VK_F14 => Some(VirtualKeyCode::F14),
        winuser::VK_F15 => Some(VirtualKeyCode::F15),
        winuser::VK_F16 => Some(VirtualKeyCode::F16),
        winuser::VK_F17 => Some(VirtualKeyCode::F17),
        winuser::VK_F18 => Some(VirtualKeyCode::F18),
        winuser::VK_F19 => Some(VirtualKeyCode::F19),
        winuser::VK_F20 => Some(VirtualKeyCode::F20),
        winuser::VK_F21 => Some(VirtualKeyCode::F21),
        winuser::VK_F22 => Some(VirtualKeyCode::F22),
        winuser::VK_F23 => Some(VirtualKeyCode::F23),
        winuser::VK_F24 => Some(VirtualKeyCode::F24),
        winuser::VK_NUMLOCK => Some(VirtualKeyCode::Numlock),
        winuser::VK_SCROLL => Some(VirtualKeyCode::Scroll),
        winuser::VK_BROWSER_BACK => Some(VirtualKeyCode::NavigateBackward),
        winuser::VK_BROWSER_FORWARD => Some(VirtualKeyCode::NavigateForward),
        winuser::VK_BROWSER_REFRESH => Some(VirtualKeyCode::WebRefresh),
        winuser::VK_BROWSER_STOP => Some(VirtualKeyCode::WebStop),
        winuser::VK_BROWSER_SEARCH => Some(VirtualKeyCode::WebSearch),
        winuser::VK_BROWSER_FAVORITES => Some(VirtualKeyCode::WebFavorites),
        winuser::VK_BROWSER_HOME => Some(VirtualKeyCode::WebHome),
        winuser::VK_VOLUME_MUTE => Some(VirtualKeyCode::Mute),
        winuser::VK_VOLUME_DOWN => Some(VirtualKeyCode::VolumeDown),
        winuser::VK_VOLUME_UP => Some(VirtualKeyCode::VolumeUp),
        winuser::VK_MEDIA_NEXT_TRACK => Some(VirtualKeyCode::NextTrack),
        winuser::VK_MEDIA_PREV_TRACK => Some(VirtualKeyCode::PrevTrack),
        winuser::VK_MEDIA_STOP => Some(VirtualKeyCode::MediaStop),
        winuser::VK_MEDIA_PLAY_PAUSE => Some(VirtualKeyCode::PlayPause),
        winuser::VK_LAUNCH_MAIL => Some(VirtualKeyCode::Mail),
        winuser::VK_LAUNCH_MEDIA_SELECT => Some(VirtualKeyCode::MediaSelect),
        /*winuser::VK_LAUNCH_APP1 => Some(VirtualKeyCode::Launch_app1),
        winuser::VK_LAUNCH_APP2 => Some(VirtualKeyCode::Launch_app2),*/
        winuser::VK_OEM_PLUS => Some(VirtualKeyCode::Equals),
        winuser::VK_OEM_COMMA => Some(VirtualKeyCode::Comma),
        winuser::VK_OEM_MINUS => Some(VirtualKeyCode::Minus),
        winuser::VK_OEM_PERIOD => Some(VirtualKeyCode::Period),
        winuser::VK_OEM_1 => map_text_keys(vkey),
        winuser::VK_OEM_2 => map_text_keys(vkey),
        winuser::VK_OEM_3 => map_text_keys(vkey),
        winuser::VK_OEM_4 => map_text_keys(vkey),
        winuser::VK_OEM_5 => map_text_keys(vkey),
        winuser::VK_OEM_6 => map_text_keys(vkey),
        winuser::VK_OEM_7 => map_text_keys(vkey),
        /* winuser::VK_OEM_8 => Some(VirtualKeyCode::Oem_8), */
        winuser::VK_OEM_102 => Some(VirtualKeyCode::OEM102),
        /*winuser::VK_PROCESSKEY => Some(VirtualKeyCode::Processkey),
        winuser::VK_PACKET => Some(VirtualKeyCode::Packet),
        winuser::VK_ATTN => Some(VirtualKeyCode::Attn),
        winuser::VK_CRSEL => Some(VirtualKeyCode::Crsel),
        winuser::VK_EXSEL => Some(VirtualKeyCode::Exsel),
        winuser::VK_EREOF => Some(VirtualKeyCode::Ereof),
        winuser::VK_PLAY => Some(VirtualKeyCode::Play),
        winuser::VK_ZOOM => Some(VirtualKeyCode::Zoom),
        winuser::VK_NONAME => Some(VirtualKeyCode::Noname),
        winuser::VK_PA1 => Some(VirtualKeyCode::Pa1),
        winuser::VK_OEM_CLEAR => Some(VirtualKeyCode::Oem_clear),*/
        _ => None,
    }
}

pub fn handle_extended_keys(vkey: i32, mut scancode: UINT, extended: bool) -> Option<(i32, UINT)> {
    // Welcome to hell https://blog.molecular-matters.com/2011/09/05/properly-handling-keyboard-input/
    scancode = if extended { 0xE000 } else { 0x0000 } | scancode;
    let vkey = match vkey {
        winuser::VK_SHIFT => unsafe {
            winuser::MapVirtualKeyA(scancode, winuser::MAPVK_VSC_TO_VK_EX) as _
        },
        winuser::VK_CONTROL => {
            if extended {
                winuser::VK_RCONTROL
            } else {
                winuser::VK_LCONTROL
            }
        }
        winuser::VK_MENU => {
            if extended {
                winuser::VK_RMENU
            } else {
                winuser::VK_LMENU
            }
        }
        _ => {
            match scancode {
                // When VK_PAUSE is pressed it emits a LeftControl + NumLock scancode event
                // sequence, but reports VK_PAUSE as the virtual key on both events,
                // or VK_PAUSE on the first event or 0xFF when using raw input.
                // Don't emit anything for the LeftControl event in the pair...
                0xE01D if vkey == winuser::VK_PAUSE => return None,
                // ...and emit the Pause event for the second event in the pair.
                0x45 if vkey == winuser::VK_PAUSE || vkey == 0xFF as _ => {
                    scancode = 0xE059;
                    winuser::VK_PAUSE
                }
                // VK_PAUSE has an incorrect vkey value when used with modifiers. VK_PAUSE also
                // reports a different scancode when used with modifiers than when
                // used without
                0xE046 => {
                    scancode = 0xE059;
                    winuser::VK_PAUSE
                }
                // VK_SCROLL has an incorrect vkey value when used with modifiers.
                0x46 => winuser::VK_SCROLL,
                _ => vkey,
            }
        }
    };
    Some((vkey, scancode))
}

pub fn process_key_params(
    wparam: WPARAM,
    lparam: LPARAM,
) -> Option<(ScanCode, Option<VirtualKeyCode>)> {
    let scancode = ((lparam >> 16) & 0xff) as UINT;
    let extended = (lparam & 0x01000000) != 0;
    handle_extended_keys(wparam as _, scancode, extended)
        .map(|(vkey, scancode)| (scancode, vkey_to_winit_vkey(vkey)))
}

// This is needed as windows doesn't properly distinguish
// some virtual key codes for different keyboard layouts
fn map_text_keys(win_virtual_key: i32) -> Option<VirtualKeyCode> {
    let char_key =
        unsafe { winuser::MapVirtualKeyA(win_virtual_key as u32, winuser::MAPVK_VK_TO_CHAR) }
            & 0x7FFF;
    match char::from_u32(char_key) {
        Some(';') => Some(VirtualKeyCode::Semicolon),
        Some('/') => Some(VirtualKeyCode::Slash),
        Some('`') => Some(VirtualKeyCode::Grave),
        Some('[') => Some(VirtualKeyCode::LBracket),
        Some(']') => Some(VirtualKeyCode::RBracket),
        Some('\'') => Some(VirtualKeyCode::Apostrophe),
        Some('\\') => Some(VirtualKeyCode::Backslash),
        _ => None,
    }
}
