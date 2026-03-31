/// A motion in normal mode. Corresponds to TS motion key handling in src/vim/motions.ts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    StartOfLine,       // 0 / ^  (first non-blank via WordStart)
    EndOfLine,         // $
    WordForward,       // w
    WordBackward,      // b
    WordEndForward,    // e
    FirstNonBlank,     // ^
    Gg,                // gg
    G,                 // G
}

/// Resolve a motion to a new cursor offset within a single line.
/// `offset` is the current byte offset in `text`, `count` is the repeat count.
/// Returns the new offset (clamped to `[0, text.len()]`).
///
/// NOTE: For multi-line motions (Up/Down/G/Gg), callers must handle those
/// outside this function using line-level context. This function handles
/// single-line motions only and returns None for unhandled motions.
pub fn resolve_single_line_motion(
    motion: Motion,
    offset: usize,
    count: u32,
    text: &str,
) -> Option<usize> {
    let len = text.len();
    let count = count as usize;
    match motion {
        Motion::Right => {
            let new = (offset + count).min(len);
            Some(new)
        }
        Motion::Left => {
            let new = offset.saturating_sub(count);
            Some(new)
        }
        Motion::StartOfLine => Some(0),
        Motion::EndOfLine => Some(len),
        Motion::FirstNonBlank => {
            let pos = text
                .char_indices()
                .find(|(_, c)| !c.is_whitespace())
                .map(|(i, _)| i)
                .unwrap_or(0);
            Some(pos)
        }
        Motion::WordForward => {
            let mut pos = offset;
            for _ in 0..count {
                pos = next_word_start(text, pos);
            }
            Some(pos)
        }
        Motion::WordBackward => {
            let mut pos = offset;
            for _ in 0..count {
                pos = prev_word_start(text, pos);
            }
            Some(pos)
        }
        Motion::WordEndForward => {
            let mut pos = offset;
            for _ in 0..count {
                pos = next_word_end(text, pos);
            }
            Some(pos)
        }
        // Multi-line motions not handled here
        Motion::Up | Motion::Down | Motion::G | Motion::Gg => None,
    }
}

/// Advance to the start of the next word.
fn next_word_start(text: &str, offset: usize) -> usize {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = chars.partition_point(|(idx, _)| *idx <= offset);
    // skip current word chars
    while i < chars.len() && !chars[i].1.is_whitespace() {
        i += 1;
    }
    // skip whitespace
    while i < chars.len() && chars[i].1.is_whitespace() {
        i += 1;
    }
    chars.get(i).map(|(idx, _)| *idx).unwrap_or(text.len())
}

/// Move to the start of the previous word.
fn prev_word_start(text: &str, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = chars.partition_point(|(idx, _)| *idx < offset);
    if i == 0 {
        return 0;
    }
    i -= 1;
    // skip whitespace backwards
    while i > 0 && chars[i].1.is_whitespace() {
        i -= 1;
    }
    // skip word chars backwards
    while i > 0 && !chars[i - 1].1.is_whitespace() {
        i -= 1;
    }
    chars.get(i).map(|(idx, _)| *idx).unwrap_or(0)
}

/// Advance to the end of the current/next word.
fn next_word_end(text: &str, offset: usize) -> usize {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = chars.partition_point(|(idx, _)| *idx <= offset);
    // skip whitespace
    while i < chars.len() && chars[i].1.is_whitespace() {
        i += 1;
    }
    // advance through word
    while i + 1 < chars.len() && !chars[i + 1].1.is_whitespace() {
        i += 1;
    }
    chars.get(i).map(|(idx, _)| *idx).unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_right_basic() {
        assert_eq!(resolve_single_line_motion(Motion::Right, 0, 1, "hello"), Some(1));
    }

    #[test]
    fn move_right_clamps_to_len() {
        assert_eq!(resolve_single_line_motion(Motion::Right, 5, 1, "hello"), Some(5));
    }

    #[test]
    fn move_left_clamps_to_zero() {
        assert_eq!(resolve_single_line_motion(Motion::Left, 0, 1, "hello"), Some(0));
    }

    #[test]
    fn move_to_end_of_line() {
        assert_eq!(resolve_single_line_motion(Motion::EndOfLine, 0, 1, "hello"), Some(5));
    }

    #[test]
    fn move_to_start_of_line() {
        assert_eq!(resolve_single_line_motion(Motion::StartOfLine, 3, 1, "hello"), Some(0));
    }

    #[test]
    fn first_non_blank_skips_spaces() {
        assert_eq!(resolve_single_line_motion(Motion::FirstNonBlank, 5, 1, "   hi"), Some(3));
    }

    #[test]
    fn word_forward() {
        // "hello world" — from 0, w → 6 (start of "world")
        assert_eq!(resolve_single_line_motion(Motion::WordForward, 0, 1, "hello world"), Some(6));
    }

    #[test]
    fn word_backward() {
        // "hello world" — from 6, b → 0 (start of "hello")
        assert_eq!(resolve_single_line_motion(Motion::WordBackward, 6, 1, "hello world"), Some(0));
    }

    #[test]
    fn multi_count_right() {
        assert_eq!(resolve_single_line_motion(Motion::Right, 0, 3, "hello"), Some(3));
    }

    #[test]
    fn multiline_motion_returns_none() {
        assert_eq!(resolve_single_line_motion(Motion::Up, 0, 1, "hello"), None);
        assert_eq!(resolve_single_line_motion(Motion::G, 0, 1, "hello"), None);
    }
}
