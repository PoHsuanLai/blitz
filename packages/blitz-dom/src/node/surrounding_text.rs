//! The text around a text input's caret, for an input method.
//!
//! text-input-v3's `set_surrounding_text` takes the text around the cursor (at most 4000 bytes),
//! and the cursor and anchor as byte offsets into that string. A host reads it here after the
//! focused input changes. A pre-edit is not part of it: the string leaves the pre-edit out and the
//! cursor stands where the pre-edit starts, as the protocol describes.

use std::ops::Range;

use super::text::TextInputData;
use crate::BaseDocument;

/// The most bytes `zwp_text_input_v3.set_surrounding_text` takes.
pub const SURROUNDING_TEXT_MAX_BYTES: usize = 4000;

/// The text around the caret and where the caret and the far end of the selection are in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurroundingText {
    /// A window of the text, cut on char boundaries, at most [`SURROUNDING_TEXT_MAX_BYTES`].
    pub text: String,
    /// Byte offset of the caret (the moving end of the selection) into `text`.
    pub cursor: usize,
    /// Byte offset of the selection's fixed end into `text`; equal to `cursor` when there is no
    /// selection.
    pub anchor: usize,
}

/// The window of `text` of at most `max` bytes (at least 4) that keeps `cursor` and `anchor` in
/// view where it can, centred on the span between them, cut on char boundaries. A span longer
/// than the window is cut at its low end.
fn window(text: &str, cursor: usize, anchor: usize, max: usize) -> Range<usize> {
    if text.len() <= max {
        return 0..text.len();
    }
    let low = cursor.min(anchor).min(text.len());
    let high = cursor.max(anchor).min(text.len());
    let pad = max.saturating_sub(high - low) / 2;

    let mut start = low.saturating_sub(pad).min(text.len() - max);
    let mut end = start + max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    start..end
}

/// The [`SurroundingText`] for `cursor` and `anchor` (byte offsets into `text`), windowed to at
/// most `max` bytes. An offset outside the window is moved to its nearest edge.
pub(crate) fn surrounding_of(
    text: &str,
    cursor: usize,
    anchor: usize,
    max: usize,
) -> SurroundingText {
    let range = window(text, cursor, anchor, max);
    let inside = |offset: usize| offset.clamp(range.start, range.end) - range.start;
    SurroundingText {
        cursor: inside(cursor),
        anchor: inside(anchor),
        text: text[range].to_string(),
    }
}

impl TextInputData {
    /// The text around the caret, for the input method (see [`SurroundingText`]).
    pub fn surrounding_text(&self) -> SurroundingText {
        let selection = self.editor.raw_selection();
        match self.editor.raw_compose() {
            None => surrounding_of(
                self.editor.raw_text(),
                selection.focus().index(),
                selection.anchor().index(),
                SURROUNDING_TEXT_MAX_BYTES,
            ),
            Some(pre_edit) => {
                let text = self.editor.text().to_string();
                surrounding_of(
                    &text,
                    pre_edit.start,
                    pre_edit.start,
                    SURROUNDING_TEXT_MAX_BYTES,
                )
            }
        }
    }
}

impl BaseDocument {
    /// The surrounding text of the focused node, when it is a text input. Beside the IME cursor
    /// area the shell is given when such a node takes focus.
    pub fn focused_surrounding_text(&self) -> Option<SurroundingText> {
        let node = self.get_node(self.focus_node_id?)?;
        let input = node.element_data()?.text_input_data()?;
        Some(input.surrounding_text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blitz_traits::events::BlitzImeEvent;
    use parley::{FontContext, LayoutContext};

    use crate::node::TextBrush;

    /// The window text with the cursor and anchor in it, cut to 8 bytes.
    fn surrounding(text: &str, cursor: usize, anchor: usize) -> (String, usize, usize) {
        let s = surrounding_of(text, cursor, anchor, 8);
        (s.text, s.cursor, s.anchor)
    }

    #[test]
    fn short_text_is_sent_whole() {
        assert_eq!(surrounding("abc", 1, 1), ("abc".to_string(), 1, 1));
        assert_eq!(surrounding("你好", 3, 0), ("你好".to_string(), 3, 0));
        assert_eq!(surrounding("", 0, 0), (String::new(), 0, 0));
    }

    #[test]
    fn long_text_is_windowed_around_the_cursor() {
        const ASCII: &str = "0123456789abcdef";
        // (text, cursor, anchor, window text, cursor in window, anchor in window)
        let cases: [(&str, usize, usize, &str, usize, usize); 8] = [
            // Four bytes either side
            (ASCII, 8, 8, "456789ab", 4, 4),
            // Near the ends the window stops at the text
            (ASCII, 1, 1, "01234567", 1, 1),
            (ASCII, 16, 16, "89abcdef", 8, 8),
            // A selection is centred, and keeps its direction
            (ASCII, 9, 7, "456789ab", 5, 3),
            // A selection longer than the window is cut at its low end
            (ASCII, 14, 2, "23456789", 8, 0),
            // CJK: the window 8..16 would split two chars, so it shrinks to 9..15
            ("你好世界你好世界", 12, 12, "界你", 3, 3),
            // Emoji: the end at byte 8 is inside the second emoji, so it stops at 6
            ("ab😀😀cd", 2, 2, "ab😀", 2, 2),
            ("a😀😀😀b", 5, 5, "😀😀", 4, 4),
        ];
        for (text, cursor, anchor, want_text, want_cursor, want_anchor) in cases {
            assert_eq!(
                surrounding(text, cursor, anchor),
                (want_text.to_string(), want_cursor, want_anchor),
                "{text:?} cursor={cursor} anchor={anchor}"
            );
        }
    }

    #[test]
    fn the_window_never_exceeds_the_protocol_limit() {
        let text = "你".repeat(5000);
        for cursor in [0, 3, 7500, 14997, 15000] {
            let s = surrounding_of(&text, cursor, cursor, SURROUNDING_TEXT_MAX_BYTES);
            assert!(
                s.text.len() <= SURROUNDING_TEXT_MAX_BYTES,
                "cursor={cursor}"
            );
            assert!(s.text.is_char_boundary(s.cursor), "cursor={cursor}");
        }
    }

    /// An input holding `text`, laid out at scale 1.0, with the selection `anchor..cursor`.
    fn input(text: &str, anchor: usize, cursor: usize) -> TextInputData {
        let mut font_ctx = FontContext::new();
        let mut layout_ctx: LayoutContext<TextBrush> = LayoutContext::new();
        let mut data = TextInputData::new(false);
        data.editor.set_scale(1.0);
        data.editor.set_text(text);
        let mut driver = data.editor.driver(&mut font_ctx, &mut layout_ctx);
        driver.refresh_layout();
        driver.select_byte_range(anchor, cursor);
        data
    }

    #[test]
    fn the_input_reports_its_caret_and_selection() {
        let caret = input("你好abc", 6, 6).surrounding_text();
        assert_eq!(
            caret,
            SurroundingText {
                text: "你好abc".to_string(),
                cursor: 6,
                anchor: 6
            }
        );

        // Selected from the end backwards: the caret is the end the selection moved to.
        let selected = input("abcdef", 4, 2).surrounding_text();
        assert_eq!((selected.cursor, selected.anchor), (2, 4));
    }

    #[test]
    fn a_pre_edit_is_left_out_and_the_caret_stands_where_it_starts() {
        let mut data = input("abcdef", 3, 3);
        data.apply_ime_event(
            &mut FontContext::new(),
            &mut LayoutContext::new(),
            BlitzImeEvent::Preedit("ni".to_string(), Some((2, 2))),
        );
        assert_eq!(data.editor.raw_text(), "abcnidef");
        assert_eq!(
            data.surrounding_text(),
            SurroundingText {
                text: "abcdef".to_string(),
                cursor: 3,
                anchor: 3
            }
        );
    }
}
