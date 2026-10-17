//! `BlitzImeEvent::DeleteSurrounding` for a text input.
//!
//! text-input-v3 counts the lengths in UTF-8 bytes, from the cursor, or from the ends of the
//! selection when there is one. A length that ends inside a code point is cut back to the code
//! point before it (inward, towards the cursor) so a character is never split.

use std::num::NonZeroUsize;
use std::ops::Range;

use parley::{FontContext, LayoutContext};

use super::text::{GeneratedTextInputEvent, TextBrush, TextInputData};

/// How many bytes to delete on each side of the selection, after clamping to the text and
/// rounding to char boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeleteLens {
    pub before: usize,
    pub after: usize,
}

/// The bytes to delete before and after `selection` in `text` when `before` and `after` are
/// asked for. Both are clamped to the ends of the text and rounded inward to char boundaries.
pub(crate) fn delete_lens(
    text: &str,
    selection: Range<usize>,
    before: usize,
    after: usize,
) -> DeleteLens {
    let start = selection.start.min(text.len());
    let end = selection.end.clamp(start, text.len());

    let mut from = start.saturating_sub(before);
    while !text.is_char_boundary(from) {
        from += 1;
    }
    let mut to = end.saturating_add(after).min(text.len());
    while !text.is_char_boundary(to) {
        to -= 1;
    }
    DeleteLens {
        before: start - from,
        after: to - end,
    }
}

impl TextInputData {
    /// Apply a text-input-v3 `delete_surrounding_text`: clear any pre-edit (the protocol deletes
    /// around the committed text, and the host sends the new pre-edit afterwards), then delete
    /// `before_bytes` before the selection and `after_bytes` after it with the editor's own
    /// deletions, so the selection moves as it does for a typed deletion.
    ///
    /// Gives `Input` when text went, `PreEditChange` when only a pre-edit was cleared, and
    /// `None` when nothing changed.
    pub(crate) fn delete_surrounding(
        &mut self,
        font_ctx: &mut FontContext,
        layout_ctx: &mut LayoutContext<TextBrush>,
        before_bytes: usize,
        after_bytes: usize,
    ) -> Option<GeneratedTextInputEvent> {
        let was_composing = self.editor.is_composing();
        if was_composing {
            self.editor.driver(font_ctx, layout_ctx).clear_compose();
        }

        let lens = delete_lens(
            self.editor.raw_text(),
            self.editor.raw_selection().text_range(),
            before_bytes,
            after_bytes,
        );
        let mut driver = self.editor.driver(font_ctx, layout_ctx);
        // After first: it leaves the text before the selection where it is.
        if let Some(len) = NonZeroUsize::new(lens.after) {
            driver.delete_bytes_after_selection(len);
        }
        if let Some(len) = NonZeroUsize::new(lens.before) {
            driver.delete_bytes_before_selection(len);
        }

        if lens.before + lens.after > 0 {
            Some(GeneratedTextInputEvent::Input)
        } else if was_composing {
            Some(GeneratedTextInputEvent::PreEditChange)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blitz_traits::events::BlitzImeEvent;

    #[test]
    fn lens_clamps_and_rounds_inward() {
        // (text, selection, asked before, asked after, expected before, expected after)
        type Case = (&'static str, Range<usize>, usize, usize, usize, usize);
        let cases: [Case; 10] = [
            // ASCII
            ("abcdef", 3..3, 1, 1, 1, 1),
            ("abcdef", 3..3, 0, 0, 0, 0),
            // Past the ends of the text
            ("abcdef", 2..2, 10, 10, 2, 4),
            ("abcdef", 0..0, 3, 0, 0, 0),
            ("abcdef", 6..6, 0, 3, 0, 0),
            // A selection: the lengths count from its ends
            ("abcdef", 2..4, 1, 1, 1, 1),
            // CJK, three bytes per char: "你好世界", cursor between 好 and 世
            ("你好世界", 6..6, 3, 3, 3, 3),
            // A cut that lands inside a code point rounds inward: 4 bytes back from byte 6
            // would start at byte 2, inside 你, so it keeps the char and deletes 好 only
            ("你好世界", 6..6, 4, 4, 3, 3),
            // Emoji, four bytes: "a😀b", cursor after the emoji
            ("a😀b", 5..5, 2, 0, 0, 0),
            ("a😀b", 5..5, 4, 1, 4, 1),
        ];
        for (text, selection, before, after, want_before, want_after) in cases {
            assert_eq!(
                delete_lens(text, selection.clone(), before, after),
                DeleteLens {
                    before: want_before,
                    after: want_after
                },
                "{text:?} {selection:?} before={before} after={after}"
            );
        }
    }

    /// An input holding `text`, laid out at scale 1.0, with the selection `start..end`.
    fn input(
        text: &str,
        start: usize,
        end: usize,
    ) -> (TextInputData, FontContext, LayoutContext<TextBrush>) {
        let mut font_ctx = FontContext::new();
        let mut layout_ctx = LayoutContext::new();
        let mut data = TextInputData::new(false);
        data.editor.set_scale(1.0);
        data.editor.set_text(text);
        let mut driver = data.editor.driver(&mut font_ctx, &mut layout_ctx);
        driver.refresh_layout();
        driver.select_byte_range(start, end);
        (data, font_ctx, layout_ctx)
    }

    /// The text and selection after the event.
    fn after(data: &TextInputData) -> (String, Range<usize>) {
        (
            data.editor.raw_text().to_string(),
            data.editor.raw_selection().text_range(),
        )
    }

    fn delete(before_bytes: usize, after_bytes: usize) -> BlitzImeEvent {
        BlitzImeEvent::DeleteSurrounding {
            before_bytes,
            after_bytes,
        }
    }

    #[test]
    fn deletes_around_the_cursor() {
        // (text, selection, before, after, resulting text, resulting selection)
        type Case = (
            &'static str,
            Range<usize>,
            usize,
            usize,
            &'static str,
            Range<usize>,
        );
        let cases: [Case; 7] = [
            ("abcdef", 3..3, 1, 1, "abef", 2..2),
            ("abcdef", 3..3, 2, 0, "adef", 1..1),
            // Past the ends
            ("abcdef", 2..2, 10, 10, "", 0..0),
            ("abcdef", 0..0, 5, 0, "abcdef", 0..0),
            // A selection stays selected, the text on both sides goes
            ("abcdef", 2..4, 1, 1, "acdf", 1..3),
            // CJK
            ("你好世界", 6..6, 3, 3, "你界", 3..3),
            // A cut inside a code point leaves the code point whole
            ("你好世界", 6..6, 4, 4, "你界", 3..3),
        ];
        for (text, selection, before, after_len, want_text, want_selection) in cases {
            let (mut data, mut font_ctx, mut layout_ctx) =
                input(text, selection.start, selection.end);
            let event =
                data.apply_ime_event(&mut font_ctx, &mut layout_ctx, delete(before, after_len));
            assert_eq!(
                after(&data),
                (want_text.to_string(), want_selection.clone()),
                "{text:?} {selection:?} before={before} after={after_len}"
            );
            let changed = want_text != text;
            assert_eq!(
                matches!(event, Some(GeneratedTextInputEvent::Input)),
                changed,
                "{text:?} {selection:?}"
            );
        }
    }

    #[test]
    fn emoji_is_deleted_whole_or_kept_whole() {
        let (mut data, mut font_ctx, mut layout_ctx) = input("a😀b", 5, 5);
        // Two bytes back would end inside the emoji: nothing before goes.
        data.apply_ime_event(&mut font_ctx, &mut layout_ctx, delete(2, 0));
        assert_eq!(after(&data), ("a😀b".to_string(), 5..5));
        // All four bytes go.
        data.apply_ime_event(&mut font_ctx, &mut layout_ctx, delete(4, 0));
        assert_eq!(after(&data), ("ab".to_string(), 1..1));
    }

    #[test]
    fn a_preedit_is_cleared_before_the_deletion() {
        let (mut data, mut font_ctx, mut layout_ctx) = input("abcdef", 3, 3);
        data.apply_ime_event(
            &mut font_ctx,
            &mut layout_ctx,
            BlitzImeEvent::Preedit("ni".to_string(), Some((2, 2))),
        );
        assert!(data.editor.is_composing());
        assert_eq!(data.editor.raw_text(), "abcnidef");

        let event = data.apply_ime_event(&mut font_ctx, &mut layout_ctx, delete(1, 1));
        assert!(matches!(event, Some(GeneratedTextInputEvent::Input)));
        assert!(!data.editor.is_composing());
        assert_eq!(after(&data), ("abef".to_string(), 2..2));
    }

    #[test]
    fn a_preedit_alone_gives_a_preedit_change() {
        let (mut data, mut font_ctx, mut layout_ctx) = input("abc", 3, 3);
        data.apply_ime_event(
            &mut font_ctx,
            &mut layout_ctx,
            BlitzImeEvent::Preedit("ni".to_string(), None),
        );
        let event = data.apply_ime_event(&mut font_ctx, &mut layout_ctx, delete(0, 0));
        assert!(matches!(
            event,
            Some(GeneratedTextInputEvent::PreEditChange)
        ));
        assert_eq!(after(&data), ("abc".to_string(), 3..3));
    }

    #[test]
    fn deleting_nothing_gives_no_event() {
        let (mut data, mut font_ctx, mut layout_ctx) = input("abc", 1, 1);
        let event = data.apply_ime_event(&mut font_ctx, &mut layout_ctx, delete(0, 0));
        assert!(event.is_none());
        assert_eq!(after(&data), ("abc".to_string(), 1..1));
    }
}
