//! Plain-text messages use their own Agent action, never the clipboard channel.
//! Success acknowledges acceptance into the receiver's bounded background queue.

#[cfg(any(target_os = "windows", test))]
mod inbox;
#[cfg(any(target_os = "windows", test))]
mod layout;
#[cfg(target_os = "windows")]
mod windows;

const MAX_MESSAGE_BYTES: usize = 16 * 1024;
const MAX_HISTORY_BYTES: usize = 128 * 1024;
const TIME_SEPARATOR: &str = "  ";

/// Only the currently open window owns this history; closing drops every entry.
#[cfg(any(target_os = "windows", test))]
#[derive(Default)]
struct MessageHistory {
    text: String,
    time_ranges: Vec<(i32, i32)>,
    utf16_length: i32,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug)]
struct MessageAppend {
    text: String,
    start: i32,
    end: i32,
    time_range: (i32, i32),
}

#[cfg(any(target_os = "windows", test))]
impl MessageHistory {
    fn append(&mut self, entry: &str) -> Result<MessageAppend, String> {
        let entry = entry.replace("\r\n", "\n").replace('\r', "\n");
        let separator = if self.text.is_empty() { "" } else { "\n\n" };
        if self.text.len() + separator.len() + entry.len() > MAX_HISTORY_BYTES {
            return Err("remote_message_busy".into());
        }
        let start = self.utf16_length;
        let time_start = start + separator.len() as i32;
        let time_length = entry
            .split_once(TIME_SEPARATOR)
            .map(|(time, _)| time)
            .unwrap_or_default()
            .encode_utf16()
            .count();
        let time_range = (time_start, time_start + time_length as i32);
        self.time_ranges.push(time_range);
        self.utf16_length += separator.len() as i32 + entry.encode_utf16().count() as i32;
        self.text.push_str(separator);
        self.text.push_str(&entry);
        Ok(MessageAppend {
            text: format!("{separator}{entry}"),
            start,
            end: self.utf16_length,
            time_range,
        })
    }
}

fn received_entry(text: &str, time: &str) -> String {
    format!("{time}{TIME_SEPARATOR}{text}")
}

pub(crate) fn show(text: &str) -> Result<(), String> {
    show_with(text, present)
}

fn show_with(text: &str, present: impl FnOnce(&str) -> Result<(), String>) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("remote_message_empty".into());
    }
    if text.contains('\0') {
        return Err("remote_message_invalid".into());
    }
    if text.len() > MAX_MESSAGE_BYTES {
        return Err("remote_message_too_large".into());
    }
    let time = chrono::Local::now().format("%H:%M:%S").to_string();
    present(&received_entry(text, &time))
}

#[cfg(target_os = "macos")]
fn present(text: &str) -> Result<(), String> {
    unsafe extern "C" {
        fn llremote_show_message(text: *const std::ffi::c_char, max_bytes: usize) -> i32;
    }
    let text = std::ffi::CString::new(text).map_err(|_| "remote_message_invalid".to_string())?;
    match unsafe { llremote_show_message(text.as_ptr(), MAX_HISTORY_BYTES) } {
        1 => Ok(()),
        2 => Err("remote_message_busy".into()),
        _ => Err("remote_message_display_failed".into()),
    }
}

#[cfg(target_os = "windows")]
fn present(text: &str) -> Result<(), String> {
    if crate::local_vnc_server_platform::windows_desktop::is_service_worker() {
        return Err("agent_user_desktop_unavailable".into());
    }
    windows::show(text)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn present(_text: &str) -> Result<(), String> {
    Err("remote_message_unsupported".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_nul_and_large_utf8_messages_before_opening_a_window() {
        for (text, code) in [
            (" \n\t".to_string(), "remote_message_empty"),
            ("hi\0there".to_string(), "remote_message_invalid"),
            (
                "你".repeat(MAX_MESSAGE_BYTES / 3 + 1),
                "remote_message_too_large",
            ),
        ] {
            assert_eq!(
                show_with(&text, |_| panic!("must not present")).unwrap_err(),
                code
            );
        }
    }

    #[test]
    fn preserves_multiline_plain_text_and_acknowledges_receiver_acceptance() {
        let text = "你好\n<script>alert('plain text')</script> 🐱";
        assert!(show_with(text, |received| {
            let (time, body) = received.split_once(TIME_SEPARATOR).unwrap();
            assert_eq!(time.len(), 8);
            assert!(chrono::NaiveTime::parse_from_str(time, "%H:%M:%S").is_ok());
            assert_eq!(body, text);
            Ok(())
        })
        .is_ok());
        assert_eq!(
            show_with(text, |_| Err("remote_message_busy".into())).unwrap_err(),
            "remote_message_busy"
        );
        assert!(show_with(&"a".repeat(MAX_MESSAGE_BYTES), |_| Ok(())).is_ok());
    }

    #[test]
    fn appends_distinct_timestamped_entries_without_replacing_unread_text() {
        let mut history = MessageHistory::default();
        history
            .append(&received_entry("第一条\n第二行", "10:00:00"))
            .unwrap();
        history
            .append(&received_entry("再次发送", "10:00:01"))
            .unwrap();
        assert_eq!(
            history.text,
            "10:00:00  第一条\n第二行\n\n10:00:01  再次发送"
        );
        // A reopened window starts with a new history.
        assert!(MessageHistory::default().text.is_empty());
    }

    #[test]
    fn a_full_unread_history_rejects_new_entries_without_losing_existing_messages() {
        let mut history = MessageHistory::default();
        let accepted = "a".repeat(MAX_HISTORY_BYTES);
        history.append(&accepted).unwrap();
        assert_eq!(history.append("next").unwrap_err(), "remote_message_busy");
        assert_eq!(history.text, accepted);
    }

    #[test]
    fn timestamp_spans_exclude_message_content_and_use_normalized_utf16_offsets() {
        let mut history = MessageHistory::default();
        history
            .append("10:00:00  你好🐱\r\n10:01:00 is message content")
            .unwrap();
        let first_length = history.text.encode_utf16().count() as i32;
        history.append("10:00:01  第二条").unwrap();
        assert_eq!(
            history.time_ranges,
            vec![(0, 8), (first_length + 2, first_length + 10)]
        );
        assert!(!history.text.contains('\r'));
    }

    #[test]
    fn render_deltas_contain_only_new_text_with_correct_utf16_positions() {
        let mut history = MessageHistory::default();
        let first = history.append("10:00:00  🐱\r\nhello").unwrap();
        assert_eq!(first.text, "10:00:00  🐱\nhello");
        assert_eq!(first.start, 0);
        let next = history.append("10:00:01  next").unwrap();
        assert_eq!(next.text, "\n\n10:00:01  next");
        assert_eq!(next.start, first.end);
        assert_eq!(next.time_range, (first.end + 2, first.end + 10));
        assert_eq!(next.end as usize, history.text.encode_utf16().count());
    }
}
