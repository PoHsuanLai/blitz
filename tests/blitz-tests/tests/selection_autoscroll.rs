//! Dragging a text selection towards or past the edge of a scroll container scrolls it, as in
//! a browser, and keeps scrolling while the button stays down; what a container cannot scroll
//! falls to the next one outward, from a sub-document into its host's scroll ancestors.

use blitz_dom::DocumentConfig;
use blitz_html::{HtmlDocument, HtmlProvider};
use blitz_test_harness::{Harness, HarnessOptions, mouse_pointer_event, pointer_event};
use blitz_traits::events::{BlitzPointerId, MouseEventButton, MouseEventButtons, UiEvent};
use blitz_traits::shell::{ColorScheme, Viewport};
use keyboard_types::Modifiers;
use std::sync::Arc;

fn lines(count: u32) -> String {
    (0..count)
        .map(|i| format!(r#"<div style="height:20px; width:180px">line {i}</div>"#))
        .collect()
}

fn options() -> HarnessOptions {
    HarnessOptions {
        width: 300,
        height: 300,
        ..Default::default()
    }
}

/// A 200x100 container with 30 lines in it, and the rest of the window below it
fn with_container() -> Harness {
    let mut harness = Harness::from_html_with(
        &format!(
            r#"<html><body style="margin:0">
            <div id="container" style="width:200px; height:100px; overflow-y:scroll; font-size:16px">{}</div>
            <div style="height:150px">below the container</div>
        </body></html>"#,
            lines(30)
        ),
        options(),
    );
    harness.pump();
    harness
}

/// A 200x100 container holding a frame whose body is `content_height` tall, then more lines
fn with_frame(content_height: u32) -> Harness {
    let mut harness = Harness::from_html_with(
        &format!(
            r#"<html><body style="margin:0">
            <div id="container" style="width:200px; height:100px; overflow-y:scroll">
                <iframe id="host" style="display:block; width:200px; height:100px; border:0;"></iframe>
                {}
            </div>
        </body></html>"#,
            lines(30)
        ),
        options(),
    );
    let inner = HtmlDocument::from_html(
        &format!(
            r#"<html><body style="margin:0">{}</body></html>"#,
            lines(content_height / 20)
        ),
        DocumentConfig {
            viewport: Some(Viewport::new(200, 100, 1.0, ColorScheme::Light)),
            html_parser_provider: Some(Arc::new(HtmlProvider) as _),
            ..Default::default()
        },
    );
    let host = harness.node("#host");
    harness
        .base_mut()
        .mutate()
        .set_sub_document(host, Box::new(inner));
    harness.pump();
    harness.pump();
    harness
}

fn press(harness: &mut Harness, x: f32, y: f32) {
    harness.dispatch(UiEvent::PointerDown(mouse_pointer_event(x, y)));
    harness.pump();
}

fn drag_to(harness: &mut Harness, x: f32, y: f32) {
    harness.dispatch(UiEvent::PointerMove(mouse_pointer_event(x, y)));
    harness.pump();
}

fn release(harness: &mut Harness, x: f32, y: f32) {
    harness.dispatch(UiEvent::PointerUp(pointer_event(
        BlitzPointerId::Mouse,
        x,
        y,
        MouseEventButton::Main,
        MouseEventButtons::empty(),
        Modifiers::default(),
    )));
    harness.pump();
}

/// Let `seconds` pass in 50ms frames, the pointer not moving
fn hold(harness: &mut Harness, seconds: f64) {
    for _ in 0..(seconds / 0.05).round() as u32 {
        harness.tick(0.05);
    }
}

fn container_scroll(harness: &Harness) -> f64 {
    let container = harness.node("#container");
    harness
        .base()
        .get_node(container)
        .unwrap()
        .scroll_offset()
        .y
}

fn frame_scroll(harness: &Harness) -> f64 {
    let host = harness.node("#host");
    let base = harness.base();
    let sub_doc = base.get_node(host).unwrap().subdoc().unwrap();
    sub_doc.inner().viewport_scroll().y
}

fn frame_selection(harness: &Harness) -> String {
    let host = harness.node("#host");
    let base = harness.base();
    let sub_doc = base.get_node(host).unwrap().subdoc().unwrap();
    sub_doc.inner().get_selected_text().unwrap_or_default()
}

fn selection(harness: &Harness) -> String {
    harness.base().get_selected_text().unwrap_or_default()
}

#[test]
fn a_drag_in_the_middle_of_a_container_does_not_scroll_it() {
    let mut harness = with_container();
    press(&mut harness, 10.0, 10.0);
    drag_to(&mut harness, 10.0, 50.0);
    hold(&mut harness, 1.0);
    assert_eq!(container_scroll(&harness), 0.0);
    assert!(!selection(&harness).contains("line 5"));
}

#[test]
fn a_drag_below_a_container_scrolls_it_and_extends_the_selection() {
    let mut harness = with_container();
    press(&mut harness, 10.0, 10.0);
    drag_to(&mut harness, 10.0, 140.0);
    hold(&mut harness, 0.5);
    let scrolled = container_scroll(&harness);
    assert!(scrolled > 0.0, "the container scrolled");
    // The pointer is below the container, over the text after it; the selection runs through
    // the container's lines that scrolled into view.
    assert!(
        selection(&harness).contains("line 8"),
        "{}",
        selection(&harness)
    );
}

#[test]
fn a_pointer_held_still_past_the_edge_keeps_scrolling() {
    let mut harness = with_container();
    press(&mut harness, 10.0, 10.0);
    drag_to(&mut harness, 10.0, 140.0);
    hold(&mut harness, 0.3);
    let first = container_scroll(&harness);
    hold(&mut harness, 0.3);
    let second = container_scroll(&harness);
    assert!(first > 0.0 && second > first);
}

#[test]
fn the_further_past_the_edge_the_faster() {
    let mut near = with_container();
    press(&mut near, 10.0, 10.0);
    drag_to(&mut near, 10.0, 110.0);
    hold(&mut near, 0.4);
    let mut far = with_container();
    press(&mut far, 10.0, 10.0);
    drag_to(&mut far, 10.0, 290.0);
    hold(&mut far, 0.4);
    assert!(container_scroll(&far) > container_scroll(&near));
}

#[test]
fn the_selection_follows_the_content_while_the_pointer_holds_still() {
    let mut harness = with_container();
    press(&mut harness, 10.0, 10.0);
    drag_to(&mut harness, 10.0, 95.0);
    hold(&mut harness, 0.2);
    let early = selection(&harness).len();
    hold(&mut harness, 1.0);
    assert!(selection(&harness).len() > early);
}

#[test]
fn releasing_the_button_stops_the_scroll() {
    let mut harness = with_container();
    press(&mut harness, 10.0, 10.0);
    drag_to(&mut harness, 10.0, 140.0);
    hold(&mut harness, 0.3);
    release(&mut harness, 10.0, 140.0);
    let stopped = container_scroll(&harness);
    hold(&mut harness, 0.5);
    assert_eq!(container_scroll(&harness), stopped);
}

#[test]
fn cancelling_the_pointer_stops_the_scroll() {
    let mut harness = with_container();
    press(&mut harness, 10.0, 10.0);
    drag_to(&mut harness, 10.0, 140.0);
    hold(&mut harness, 0.3);
    harness.dispatch(UiEvent::PointerCancel(mouse_pointer_event(10.0, 140.0)));
    harness.pump();
    let stopped = container_scroll(&harness);
    hold(&mut harness, 0.5);
    assert_eq!(container_scroll(&harness), stopped);
}

#[test]
fn a_drag_stops_scrolling_at_the_end_of_the_container() {
    let mut harness = with_container();
    press(&mut harness, 10.0, 10.0);
    drag_to(&mut harness, 10.0, 290.0);
    hold(&mut harness, 5.0);
    // 30 lines of 20px in a 100px container
    assert_eq!(container_scroll(&harness), 500.0);
}

#[test]
fn a_drag_past_a_containers_right_edge_scrolls_it_sideways() {
    let mut harness = Harness::from_html_with(
        r#"<html><body style="margin:0">
            <div id="container" style="width:200px; height:100px; overflow-x:scroll; overflow-y:hidden; white-space:nowrap">
                <span>aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll mmmm nnnn oooo pppp qqqq rrrr ssss tttt uuuu</span>
            </div>
        </body></html>"#,
        options(),
    );
    harness.pump();
    press(&mut harness, 10.0, 10.0);
    drag_to(&mut harness, 290.0, 10.0);
    hold(&mut harness, 0.5);
    let container = harness.node("#container");
    let scroll_x = harness
        .base()
        .get_node(container)
        .unwrap()
        .scroll_offset()
        .x;
    assert!(scroll_x > 0.0);
    assert!(
        selection(&harness).contains("eeee"),
        "{}",
        selection(&harness)
    );
}

#[test]
fn a_drag_in_a_frame_past_the_hosts_container_scrolls_the_container() {
    let mut harness = with_frame(80);
    press(&mut harness, 10.0, 10.0);
    drag_to(&mut harness, 10.0, 140.0);
    hold(&mut harness, 0.5);
    // The frame fits, so the container around it scrolls, and the pointer being below the
    // frame the selection still runs to the end of the frame's text.
    assert_eq!(frame_scroll(&harness), 0.0);
    assert!(container_scroll(&harness) > 0.0);
    assert!(
        frame_selection(&harness).contains("line 2"),
        "{}",
        frame_selection(&harness)
    );
}

#[test]
fn a_drag_in_a_scrollable_frame_scrolls_the_frame_before_the_container() {
    let mut harness = with_frame(400);
    press(&mut harness, 10.0, 10.0);
    drag_to(&mut harness, 10.0, 140.0);
    hold(&mut harness, 0.3);
    assert!(frame_scroll(&harness) > 0.0);
    assert_eq!(container_scroll(&harness), 0.0);

    // Once the frame is at its end the container takes over.
    hold(&mut harness, 5.0);
    assert_eq!(frame_scroll(&harness), 300.0);
    assert!(container_scroll(&harness) > 0.0);
    assert!(
        frame_selection(&harness).contains("line 18"),
        "{}",
        frame_selection(&harness)
    );
}

#[test]
fn a_frame_drag_ended_outside_the_frame_stops_the_scroll() {
    let mut harness = with_frame(400);
    press(&mut harness, 10.0, 10.0);
    drag_to(&mut harness, 10.0, 140.0);
    hold(&mut harness, 0.3);
    release(&mut harness, 10.0, 140.0);
    let (frame, container) = (frame_scroll(&harness), container_scroll(&harness));
    hold(&mut harness, 0.5);
    assert_eq!(
        (frame_scroll(&harness), container_scroll(&harness)),
        (frame, container)
    );
}
