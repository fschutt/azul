//! Debugging utilities for the azul-css crate.

use crate::{AzString, LayoutDebugMessage};
use alloc::vec::Vec;
use core::fmt;

/// Logs a debug message if a debug message vector is provided.
///
/// # Example
///
/// ```
/// use azul_css::debug::css_debug_log;
/// use azul_css::{LayoutDebugMessage, AzString};
/// use alloc::vec::Vec;
///
/// let mut debug_messages: Option<Vec<LayoutDebugMessage>> = Some(Vec::new());
/// css_debug_log!(debug_messages, "This is a test message");
/// css_debug_log!(debug_messages, "Another message with value: {}", 42);
///
/// if let Some(messages) = debug_messages {
///     assert_eq!(messages.len(), 2);
///     assert_eq!(messages[0].message.as_str(), "This is a test message");
///     assert_eq!(messages[0].location.as_str(), concat!(file!(), ":", line!() - 4)); // Adjust line number based on macro call
///     assert_eq!(messages[1].message.as_str(), "Another message with value: 42");
/// }
/// ```
#[macro_export]
macro_rules! css_debug_log {
    ($debug_messages:expr, $($arg:tt)*) => {
        if let Some(messages) = $debug_messages.as_mut() {
            messages.push($crate::LayoutDebugMessage {
                message: $crate::AzString::from_string(alloc::format!($($arg)*)),
                location: $crate::AzString::from_string(alloc::format!("{}:{}", file!(), line!())),
            });
        }
    }
}

/// Clears all debug messages from the provided vector.
pub fn clear_debug_logs(debug_messages: &mut Option<Vec<LayoutDebugMessage>>) {
    if let Some(messages) = debug_messages {
        messages.clear();
    }
}

/// Formats all debug messages into a single string.
pub fn format_debug_logs(debug_messages: &Option<Vec<LayoutDebugMessage>>) -> String {
    let mut output = String::new();
    if let Some(messages) = debug_messages {
        if messages.is_empty() {
            output.push_str("No debug messages.\n");
        } else {
            output.push_str("Debug Messages:\n");
            for msg in messages {
                output.push_str(&alloc::format!("- {}: {}\n", msg.location, msg.message));
            }
        }
    } else {
        output.push_str("Debug messages container not initialized.\n");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn test_css_debug_log_some() {
        let mut messages_opt: Option<Vec<LayoutDebugMessage>> = Some(Vec::new());
        let line = line!() + 1;
        css_debug_log!(messages_opt, "Test log 1");
        let line2 = line!() + 1;
        css_debug_log!(messages_opt, "Test log {} with number", 2);

        let messages = messages_opt.unwrap();
        assert_eq!(messages.len(), 2);

        assert_eq!(messages[0].message.as_str(), "Test log 1");
        assert_eq!(messages[0].location.as_str(), alloc::format!("{}:{}", file!(), line));

        assert_eq!(messages[1].message.as_str(), "Test log 2 with number");
        assert_eq!(messages[1].location.as_str(), alloc::format!("{}:{}", file!(), line2));
    }

    #[test]
    fn test_css_debug_log_none() {
        let mut messages_opt: Option<Vec<LayoutDebugMessage>> = None;
        css_debug_log!(messages_opt, "This should not be logged");
        assert!(messages_opt.is_none());
    }

    #[test]
    fn test_clear_debug_logs_some() {
        let mut messages_opt: Option<Vec<LayoutDebugMessage>> = Some(vec![
            LayoutDebugMessage { message: "msg1".into(), location: "loc1".into() },
        ]);
        clear_debug_logs(&mut messages_opt);
        assert!(messages_opt.as_ref().unwrap().is_empty());
    }

    #[test]
    fn test_clear_debug_logs_none() {
        let mut messages_opt: Option<Vec<LayoutDebugMessage>> = None;
        clear_debug_logs(&mut messages_opt);
        assert!(messages_opt.is_none());
    }

    #[test]
    fn test_format_debug_logs_some_empty() {
        let messages_opt: Option<Vec<LayoutDebugMessage>> = Some(Vec::new());
        assert_eq!(format_debug_logs(&messages_opt), "No debug messages.\n");
    }

    #[test]
    fn test_format_debug_logs_some_with_messages() {
        let line = line!(); // Store line before creating messages for accurate location
        let messages_opt: Option<Vec<LayoutDebugMessage>> = Some(vec![
            LayoutDebugMessage {
                message: AzString::from_string("Message 1".to_string()),
                location: AzString::from_string(alloc::format!("{}:{}", file!(), line + 1)),
            },
            LayoutDebugMessage {
                message: AzString::from_string("Message 2".to_string()),
                location: AzString::from_string(alloc::format!("{}:{}", file!(), line + 5)),
            },
        ]);
        let expected = alloc::format!(
            "Debug Messages:\n- {}:{}: Message 1\n- {}:{}: Message 2\n",
            file!(), line + 1, file!(), line + 5
        );
        assert_eq!(format_debug_logs(&messages_opt), expected);
    }

    #[test]
    fn test_format_debug_logs_none() {
        let messages_opt: Option<Vec<LayoutDebugMessage>> = None;
        assert_eq!(format_debug_logs(&messages_opt), "Debug messages container not initialized.\n");
    }
}
