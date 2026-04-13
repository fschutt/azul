//! Native OS dialog wrappers (message boxes, file open/save, color picker)
//! built on top of the `tfd` (tiny-file-dialogs) crate.

use tfd::MessageBoxIcon;

/// "Ok" MsgBox (title, message, icon)
///
/// Note: quotes are stripped from the message to work around `tfd`
/// misinterpreting them as shell metacharacters on some platforms.
pub fn msg_box_ok(title: &str, message: &str, icon: MessageBoxIcon) {
    let mut msg = message.to_string();

    msg = msg.replace('\"', "");
    msg = msg.replace('\'', "");

    tfd::MessageBox::new(title, &msg)
        .with_icon(icon)
        .run_modal();
}

/// Wrapper around `message_box_ok` with the default title "Info" + an info icon.
pub fn msg_box(content: &str) {
    msg_box_ok("Info", content, MessageBoxIcon::Info);
}
