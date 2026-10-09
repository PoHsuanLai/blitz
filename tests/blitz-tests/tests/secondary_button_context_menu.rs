//! A secondary-button press that jitters a few pixels still ends in a context menu:
//! only the primary button starts a text-selection drag, and a selection drag is
//! what suppresses the click and context-menu events on release.

use blitz_test_harness::{Harness, HarnessOptions, pointer_event};
use blitz_traits::events::{BlitzPointerId, MouseEventButton, MouseEventButtons, UiEvent};
use keyboard_types::Modifiers;

fn harness() -> Harness {
    Harness::from_html_with(
        r#"<html><body style="margin:0">
        <p id="text" style="margin:0; font-size:20px;">Selectable words to jitter over</p>
    </body></html>"#,
        HarnessOptions {
            width: 300,
            height: 100,
            ..Default::default()
        },
    )
}

fn at(
    x: f32,
    y: f32,
    button: MouseEventButton,
    held: MouseEventButtons,
) -> blitz_traits::events::BlitzPointerEvent {
    pointer_event(
        BlitzPointerId::Mouse,
        x,
        y,
        button,
        held,
        Modifiers::default(),
    )
}

fn press_jitter_release(button: MouseEventButton) -> Vec<String> {
    let mut harness = harness();
    let held = MouseEventButtons::from(button);
    harness.dispatch_recorded([
        UiEvent::PointerDown(at(20.0, 10.0, button, held)),
        UiEvent::PointerMove(at(26.0, 12.0, button, held)),
        UiEvent::PointerUp(at(26.0, 12.0, button, MouseEventButtons::None)),
    ])
}

#[test]
fn a_jittery_right_click_over_text_opens_the_context_menu() {
    let names = press_jitter_release(MouseEventButton::Secondary);
    assert!(
        names.contains(&"contextmenu".to_string()),
        "expected a contextmenu event, got {names:?}"
    );
}

#[test]
fn a_primary_drag_over_text_still_selects_and_sends_no_click() {
    let names = press_jitter_release(MouseEventButton::Main);
    assert!(
        !names.contains(&"click".to_string()),
        "a selection drag must not click, got {names:?}"
    );
}
