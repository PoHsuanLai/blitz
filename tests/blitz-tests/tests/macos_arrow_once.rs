//! On macOS one arrow press in a text field reaches the document twice: AppKit's
//! `interpretKeyEvents` turns it into a standard key binding (`moveRight:`), and winit still
//! forwards the key itself. The caret moves once.

#![cfg(target_os = "macos")]

use blitz_dom::{Document, DocumentConfig, EventDriver, NoopEventHandler};
use blitz_html::{HtmlDocument, HtmlProvider};
use blitz_traits::events::{BlitzKeyEvent, KeyState, UiEvent};
use blitz_traits::shell::{ColorScheme, Viewport};
use keyboard_types::{Code, Key, Location, Modifiers};
use std::sync::Arc;

fn page() -> HtmlDocument {
    let mut doc = HtmlDocument::from_html(
        r#"<html><body><input id="field" value="ab.cd"></body></html>"#,
        DocumentConfig {
            viewport: Some(Viewport::new(300, 60, 1.0, ColorScheme::Light)),
            html_parser_provider: Some(Arc::new(HtmlProvider) as _),
            ..Default::default()
        },
    );
    doc.resolve(0.0);
    doc
}

fn key(key: Key, code: Code, mods: Modifiers, state: KeyState) -> BlitzKeyEvent {
    BlitzKeyEvent {
        key: key.clone(),
        key_without_modifiers: key,
        code,
        modifiers: mods,
        location: Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state,
        text: None,
    }
}

/// One press as AppKit delivers it: the binding, then the key down and up.
fn press(doc: &mut HtmlDocument, k: Key, code: Code, mods: Modifiers, binding: &str) {
    let mut driver = EventDriver::new(doc, NoopEventHandler);
    driver.handle_ui_event(UiEvent::AppleStandardKeybinding(binding.into()));
    driver.handle_ui_event(UiEvent::KeyDown(key(
        k.clone(),
        code,
        mods,
        KeyState::Pressed,
    )));
    driver.handle_ui_event(UiEvent::KeyUp(key(k, code, mods, KeyState::Released)));
}

fn caret(doc: &HtmlDocument) -> usize {
    let id = doc.query_selector("#field").unwrap().unwrap();
    let input = doc.get_node(id).unwrap().element_data().unwrap();
    input
        .text_input_data()
        .unwrap()
        .editor
        .raw_selection()
        .focus()
        .index()
}

#[test]
fn an_arrow_moves_the_caret_once() {
    let mut doc = page();
    let id = doc.query_selector("#field").unwrap().unwrap();
    doc.set_focus_to(id);
    press(
        &mut doc,
        Key::Home,
        Code::Home,
        Modifiers::empty(),
        "scrollToBeginningOfDocument:",
    );
    assert_eq!(caret(&doc), 0, "Home");

    for at in 1..=3 {
        press(
            &mut doc,
            Key::ArrowRight,
            Code::ArrowRight,
            Modifiers::empty(),
            "moveRight:",
        );
        assert_eq!(caret(&doc), at, "after {at} presses of Right");
    }
    press(
        &mut doc,
        Key::ArrowLeft,
        Code::ArrowLeft,
        Modifiers::empty(),
        "moveLeft:",
    );
    assert_eq!(caret(&doc), 2, "Left");
}
