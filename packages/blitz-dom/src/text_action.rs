//! Mapping of key chords to text-editing actions.
//!
//! Which modifier means "the action key" (Cmd on macOS, Ctrl elsewhere) is a platform and
//! embedder decision. [`BaseDocument`](crate::BaseDocument) asks a [`TextActionResolver`]
//! what a key press means in a focused text input, and embedders can supply their own through
//! [`DocumentConfig::text_action_resolver`](crate::DocumentConfig::text_action_resolver).

use keyboard_types::{Key, Modifiers};

/// A text-editing action that Blitz performs in a focused text input or on a text selection.
///
/// The Shift modifier is not part of the action: Blitz reads it from the key event and extends
/// the selection for the movement actions (and collapses it for [`TextAction::SelectAll`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TextAction {
    /// Copy the selection to the clipboard.
    Copy,
    /// Copy the selection to the clipboard and delete it.
    Cut,
    /// Replace the selection with the clipboard text.
    Paste,
    /// Select all of the text.
    SelectAll,
    /// Move to the previous word boundary.
    WordLeft,
    /// Move to the next word boundary.
    WordRight,
    /// Move to the start of the text.
    TextStart,
    /// Move to the end of the text.
    TextEnd,
    /// Delete the word after the cursor.
    DeleteWord,
    /// Delete the word before the cursor.
    BackdeleteWord,
}

/// Decides what a key press means to a text input.
pub trait TextActionResolver: Send + Sync {
    /// The action for `key` pressed with `modifiers`, or `None` if the press is not an action
    /// (plain navigation and typing do not go through the resolver).
    fn resolve(&self, key: &Key, modifiers: Modifiers) -> Option<TextAction>;

    /// Whether this chord is a command rather than text: a character key for which this returns
    /// `true` is never inserted into a text input. Defaults to Control, Super or Meta being held.
    fn is_command_chord(&self, _key: &Key, modifiers: Modifiers) -> bool {
        modifiers.intersects(Modifiers::CONTROL | Modifiers::SUPER | Modifiers::META)
    }
}

/// The resolver used when none is configured.
///
/// The action key is Super (or Meta) on macOS, and Control, Super or Meta elsewhere.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultTextActionResolver;

/// The modifiers that count as the action key for [`DefaultTextActionResolver`].
#[cfg(target_os = "macos")]
const ACTION_MODS: Modifiers = Modifiers::SUPER.union(Modifiers::META);
#[cfg(not(target_os = "macos"))]
const ACTION_MODS: Modifiers = Modifiers::CONTROL
    .union(Modifiers::SUPER)
    .union(Modifiers::META);

impl TextActionResolver for DefaultTextActionResolver {
    fn resolve(&self, key: &Key, modifiers: Modifiers) -> Option<TextAction> {
        if !modifiers.intersects(ACTION_MODS) {
            return None;
        }
        match key {
            Key::Character(c) => match c.to_lowercase().as_str() {
                "c" => Some(TextAction::Copy),
                "x" => Some(TextAction::Cut),
                "v" => Some(TextAction::Paste),
                "a" => Some(TextAction::SelectAll),
                _ => None,
            },
            Key::ArrowLeft => Some(TextAction::WordLeft),
            Key::ArrowRight => Some(TextAction::WordRight),
            Key::Home => Some(TextAction::TextStart),
            Key::End => Some(TextAction::TextEnd),
            Key::Delete => Some(TextAction::DeleteWord),
            Key::Backspace => Some(TextAction::BackdeleteWord),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn character(c: &str) -> Key {
        Key::Character(c.into())
    }

    #[test]
    fn default_resolver_maps_action_chords() {
        let r = DefaultTextActionResolver;
        assert_eq!(
            r.resolve(&character("c"), Modifiers::SUPER),
            Some(TextAction::Copy)
        );
        assert_eq!(
            r.resolve(&character("C"), Modifiers::META),
            Some(TextAction::Copy)
        );
        assert_eq!(
            r.resolve(&character("v"), Modifiers::SUPER | Modifiers::SHIFT),
            Some(TextAction::Paste)
        );
        assert_eq!(
            r.resolve(&Key::ArrowLeft, Modifiers::SUPER),
            Some(TextAction::WordLeft)
        );
        assert_eq!(
            r.resolve(&Key::Home, Modifiers::SUPER),
            Some(TextAction::TextStart)
        );
        assert_eq!(r.resolve(&character("c"), Modifiers::empty()), None);
        assert_eq!(r.resolve(&Key::ArrowLeft, Modifiers::SHIFT), None);
        assert_eq!(r.resolve(&character("z"), Modifiers::SUPER), None);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn default_resolver_uses_control_off_macos() {
        let r = DefaultTextActionResolver;
        assert_eq!(
            r.resolve(&character("a"), Modifiers::CONTROL),
            Some(TextAction::SelectAll)
        );
        assert_eq!(
            r.resolve(&Key::Backspace, Modifiers::CONTROL),
            Some(TextAction::BackdeleteWord)
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn default_resolver_ignores_control_on_macos() {
        let r = DefaultTextActionResolver;
        assert_eq!(r.resolve(&character("c"), Modifiers::CONTROL), None);
    }

    #[test]
    fn default_command_chord_is_control_super_or_meta() {
        let r = DefaultTextActionResolver;
        let k = character("a");
        assert!(r.is_command_chord(&k, Modifiers::CONTROL));
        assert!(r.is_command_chord(&k, Modifiers::SUPER));
        assert!(r.is_command_chord(&k, Modifiers::META | Modifiers::SHIFT));
        assert!(!r.is_command_chord(&k, Modifiers::SHIFT));
        assert!(!r.is_command_chord(&k, Modifiers::ALT));
        assert!(!r.is_command_chord(&k, Modifiers::empty()));
    }

    /// A resolver for an embedder where only Super is the action key.
    struct SuperOnly;

    impl TextActionResolver for SuperOnly {
        fn resolve(&self, key: &Key, modifiers: Modifiers) -> Option<TextAction> {
            match (key, modifiers.contains(Modifiers::SUPER)) {
                (Key::Character(c), true) if c == "c" => Some(TextAction::Copy),
                _ => None,
            }
        }

        fn is_command_chord(&self, _key: &Key, modifiers: Modifiers) -> bool {
            modifiers.contains(Modifiers::SUPER)
        }
    }

    #[test]
    fn injected_resolver_decides() {
        let r: &dyn TextActionResolver = &SuperOnly;
        assert_eq!(
            r.resolve(&character("c"), Modifiers::SUPER),
            Some(TextAction::Copy)
        );
        assert_eq!(r.resolve(&character("c"), Modifiers::CONTROL), None);
        assert!(!r.is_command_chord(&character("c"), Modifiers::CONTROL));
        assert!(r.is_command_chord(&character("c"), Modifiers::SUPER));
    }
}
