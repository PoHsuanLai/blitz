//! A press whose default action is cancelled (a menu item cancels `mousedown` so the press does not
//! move focus or start a text selection) still ends in a `click`, even when the pointer moves while
//! the button is held and an earlier press happened somewhere else.
//!
//! The cancelled press skipped `handle_pointerdown`, so its position and drag state were those of the
//! earlier press: the first move with the button held read as a long drag from the old position, the
//! drag was a text selection, and a selection drag takes the `click` away.

use blitz_dom::{Document, DocumentConfig, EventDriver, EventHandler, NodeId};
use blitz_html::{HtmlDocument, HtmlProvider};
use blitz_traits::events::{
    BlitzPointerEvent, BlitzPointerId, DomEvent, DomEventData, EventState, MouseEventButton,
    MouseEventButtons, Point, PointerCoords, PointerDetails, UiEvent,
};
use blitz_traits::shell::{ColorScheme, Viewport};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

fn pointer_event(x: f32, y: f32, buttons: MouseEventButtons) -> BlitzPointerEvent {
    BlitzPointerEvent {
        id: BlitzPointerId::Mouse,
        is_primary: true,
        coords: PointerCoords {
            page_x: x,
            page_y: y,
            screen_x: x,
            screen_y: y,
            client_x: x,
            client_y: y,
        },
        button: MouseEventButton::Main,
        buttons,
        mods: Default::default(),
        details: PointerDetails::default(),
        element: Point::default(),
        active_pointers: Default::default(),
    }
}

/// Cancels every press at or right of `x = 100` (the "menu item") and writes down each click's x.
struct Item {
    clicks: Rc<RefCell<Vec<f32>>>,
}

impl EventHandler for Item {
    fn handle_event(
        &mut self,
        _chain: &[NodeId],
        event: &mut DomEvent,
        _doc: &mut dyn Document,
        state: &mut EventState,
    ) {
        match &event.data {
            DomEventData::PointerDown(press) if press.page_x() >= 100.0 => state.prevent_default(),
            DomEventData::Click(click) => self.clicks.borrow_mut().push(click.page_x()),
            _ => {}
        }
    }
}

fn page() -> HtmlDocument {
    let mut doc = HtmlDocument::from_html(
        r#"<html><body style="margin:0">
            <div style="display:flex; width:200px; height:40px;">
                <div id="plain" style="width:100px;">plain text</div>
                <div id="item" style="width:100px;">item text</div>
            </div>
        </body></html>"#,
        DocumentConfig {
            viewport: Some(Viewport::new(200, 40, 1.0, ColorScheme::Light)),
            html_parser_provider: Some(Arc::new(HtmlProvider) as _),
            ..Default::default()
        },
    );
    doc.resolve(0.0);
    doc
}

/// Press and release at `(x, 20)`, the pointer moving to `(x + travel, 20)` while the button is held.
fn click(driver: &mut EventDriver<'_, Item>, x: f32, travel: f32) {
    let held = MouseEventButtons::Primary;
    driver.handle_ui_event(UiEvent::PointerMove(pointer_event(
        x,
        20.0,
        MouseEventButtons::None,
    )));
    driver.handle_ui_event(UiEvent::PointerDown(pointer_event(x, 20.0, held)));
    driver.handle_ui_event(UiEvent::PointerMove(pointer_event(x + travel, 20.0, held)));
    driver.handle_ui_event(UiEvent::PointerUp(pointer_event(
        x + travel,
        20.0,
        MouseEventButtons::None,
    )));
}

#[test]
fn a_cancelled_press_still_clicks_after_a_press_elsewhere() {
    let mut doc = page();
    let clicks = Rc::new(RefCell::new(Vec::new()));
    let mut driver = EventDriver::new(
        &mut doc,
        Item {
            clicks: Rc::clone(&clicks),
        },
    );

    // An ordinary click on the left, so the document remembers where a press last began.
    click(&mut driver, 20.0, 0.0);
    assert_eq!(*clicks.borrow(), [20.0], "the ordinary click");

    // The cancelled press on the right, the pointer moving 3px as a hand does.
    click(&mut driver, 150.0, 3.0);
    assert_eq!(
        *clicks.borrow(),
        [20.0, 153.0],
        "the click on the item whose press was cancelled"
    );
}
