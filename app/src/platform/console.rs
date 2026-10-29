//! Nonblocking console input: cancellation must not strand a blocking stdin worker.
use windows_sys::Win32::System::Console::{
    GetNumberOfConsoleInputEvents, GetStdHandle, INPUT_RECORD, KEY_EVENT, ReadConsoleInputW,
    STD_INPUT_HANDLE,
};

use crate::error::PlatformError;

#[derive(Debug, Eq, PartialEq)]
pub struct ConsoleText {
    pub character: char,
    pub repeat_count: u16,
}

fn decode_text(unit: u16, repeat_count: u16) -> Option<ConsoleText> {
    if unit == 0 {
        return None;
    }
    let character = if repeat_count == 0 {
        '\u{fffd}'
    } else {
        // Unsupported UTF-16 text must invalidate the decision, not disappear.
        char::from_u32(u32::from(unit)).unwrap_or('\u{fffd}')
    };
    Some(ConsoleText {
        character,
        repeat_count: repeat_count.max(1),
    })
}

/// Consumes at most one already available console event without waiting for a line.
/// # Errors
/// Returns an OS error when the caller does not have a working console input handle.
pub fn poll_console_character() -> Result<Option<ConsoleText>, PlatformError> {
    // SAFETY: This borrows the process standard input handle; it is never closed here.
    let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    let mut available = 0;
    // SAFETY: The output pointer is valid and the borrowed handle is checked by Windows.
    if unsafe { GetNumberOfConsoleInputEvents(handle, &raw mut available) } == 0 {
        return Err(PlatformError::Windows(format!(
            "console input is unavailable: {}",
            std::io::Error::last_os_error()
        )));
    }
    if available == 0 {
        return Ok(None);
    }
    let mut record = INPUT_RECORD::default();
    let mut read = 0;
    // SAFETY: The console reader is the only stdin consumer; an event is available.
    // The valid output buffer holds the single event requested by this bounded call.
    if unsafe { ReadConsoleInputW(handle, &raw mut record, 1, &raw mut read) } == 0 {
        return Err(PlatformError::Windows(format!(
            "console event read failed: {}",
            std::io::Error::last_os_error()
        )));
    }
    if read != 1 || u32::from(record.EventType) != KEY_EVENT {
        return Ok(None);
    }
    // SAFETY: EventType identifies the active KeyEvent union member.
    let event = unsafe { record.Event.KeyEvent };
    if event.bKeyDown == 0 {
        return Ok(None);
    }
    // SAFETY: ReadConsoleInputW writes the UnicodeChar member of this union.
    let character = unsafe { event.uChar.UnicodeChar };
    Ok(decode_text(character, event.wRepeatCount))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_and_unsupported_console_text_cannot_collapse_to_y() {
        let inputs = [
            vec![(u16::from(b'y'), 3)],
            vec![(u16::from(b'y'), 1), (0xd83d, 1), (0xde00, 1)],
            vec![(u16::from(b'y'), 1), (0xd800, 1)],
        ];
        for input in inputs {
            let mut line = String::new();
            for (unit, count) in input {
                let text = decode_text(unit, count).expect("text remains represented");
                for _ in 0..text.repeat_count {
                    line.push(text.character);
                }
            }
            assert_ne!(line, "y");
            assert_ne!(line, "yes");
        }
        assert_eq!(
            decode_text(u16::from(b'y'), 1),
            Some(ConsoleText {
                character: 'y',
                repeat_count: 1
            })
        );
        assert_eq!(
            decode_text(u16::from(b'y'), 0),
            Some(ConsoleText {
                character: '\u{fffd}',
                repeat_count: 1
            })
        );
        assert_eq!(decode_text(0, 1), None);
    }
}
