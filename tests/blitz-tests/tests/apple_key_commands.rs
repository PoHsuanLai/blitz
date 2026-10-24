//! AppKit turns a key into a command (`insertNewline:` for Return) and winit sends the command
//! before the key. The command is the key's default action: a multi-line field takes one new line
//! for Return, not one for the command and another for the key, and a handler that cancels the
//! key's `keydown` (Enter in a field that submits) cancels the command with it.

use blitz_dom::{Document, DocumentConfig, EventDriver, EventHandler, NodeId};
use blitz_html::{HtmlDocument, HtmlProvider};
use blitz_test_harness::{key_event, mouse_pointer_event};
use blitz_traits::SmolStr;
use blitz_traits::events::{
    DomEvent, DomEventData, EventState, KeyState, MouseEventButton, MouseEventButtons, UiEvent,
};
use blitz_traits::shell::{ColorScheme, Viewport};
use keyboard_types::{Key, Modifiers};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

/// Cancels Enter's `keydown` while `cancel` is set.
struct Field {
    cancel: Rc<Cell<bool>>,
}

impl EventHandler for Field {
    fn handle_event(
        &mut self,
        _chain: &[NodeId],
        event: &mut DomEvent,
        _doc: &mut dyn Document,
        state: &mut EventState,
    ) {
        if let DomEventData::KeyDown(key) = &event.data {
            if key.key == Key::Enter && self.cancel.get() {
                state.prevent_default();
            }
        }
    }
}

fn page() -> HtmlDocument {
    let mut doc = HtmlDocument::from_html(
        r#"<html><body style="margin:0">
            <textarea id="field" style="width:200px; height:60px;"></textarea>
        </body></html>"#,
        DocumentConfig {
            viewport: Some(Viewport::new(200, 60, 1.0, ColorScheme::Light)),
            html_parser_provider: Some(Arc::new(HtmlProvider) as _),
            ..Default::default()
        },
    );
    doc.resolve(0.0);
    doc
}

fn text(doc: &HtmlDocument) -> String {
    let id = doc.query_selector("#field").unwrap().unwrap();
    let node = doc.get_node(id).unwrap();
    let input = node.element_data().unwrap().text_input_data().unwrap();
    input.editor.text().to_string()
}

/// One key as macOS delivers it: its command (if any) first, then the key down and up.
fn press(driver: &mut EventDriver<'_, Field>, key: Key, command: Option<&str>) {
    if let Some(command) = command {
        driver.handle_ui_event(UiEvent::AppleStandardKeybinding(SmolStr::new(command)));
    }
    let mods = Modifiers::default();
    driver.handle_ui_event(UiEvent::KeyDown(key_event(
        key.clone(),
        KeyState::Pressed,
        mods,
    )));
    driver.handle_ui_event(UiEvent::KeyUp(key_event(key, KeyState::Released, mods)));
}

fn typed(driver: &mut EventDriver<'_, Field>, text: &str) {
    for ch in text.chars() {
        press(driver, Key::Character(ch.to_string()), None);
    }
}

#[test]
fn return_adds_one_line_and_a_cancelled_return_adds_none() {
    let mut doc = page();
    let cancel = Rc::new(Cell::new(false));
    {
        let mut driver = EventDriver::new(
            &mut doc,
            Field {
                cancel: Rc::clone(&cancel),
            },
        );
        let mut at = mouse_pointer_event(20.0, 20.0);
        at.buttons = MouseEventButtons::from(MouseEventButton::Main);
        driver.handle_ui_event(UiEvent::PointerDown(at.clone()));
        at.buttons = MouseEventButtons::None;
        driver.handle_ui_event(UiEvent::PointerUp(at));

        typed(&mut driver, "a");
        press(&mut driver, Key::Enter, Some("insertNewline:"));
        typed(&mut driver, "b");
        cancel.set(true);
        press(&mut driver, Key::Enter, Some("insertNewline:"));
        cancel.set(false);
        typed(&mut driver, "c");
    }
    assert_eq!(text(&doc), "a\nbc");
}

#[test]
fn a_key_without_a_command_still_edits() {
    let mut doc = page();
    let cancel = Rc::new(Cell::new(false));
    {
        let mut driver = EventDriver::new(&mut doc, Field { cancel });
        let mut at = mouse_pointer_event(20.0, 20.0);
        at.buttons = MouseEventButtons::from(MouseEventButton::Main);
        driver.handle_ui_event(UiEvent::PointerDown(at.clone()));
        at.buttons = MouseEventButtons::None;
        driver.handle_ui_event(UiEvent::PointerUp(at));
        // No command: a host that sends none (tests, other platforms) still gets a new line.
        typed(&mut driver, "a");
        press(&mut driver, Key::Enter, None);
        typed(&mut driver, "b");
    }
    assert_eq!(text(&doc), "a\nb");
}
