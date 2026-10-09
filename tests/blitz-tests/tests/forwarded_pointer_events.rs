//! Pointer events forwarded to a custom widget or a sub-document also take the normal
//! pointer path on the host element: `click` and `contextmenu` are synthesized there and
//! bubble to its ancestors (an app opens its context menu from an ancestor's handler).

use blitz_dom::{DocumentConfig, Widget};
use blitz_html::{HtmlDocument, HtmlProvider};
use blitz_test_harness::{Harness, HarnessOptions, pointer_event};
use blitz_traits::events::{
    BlitzPointerEvent, BlitzPointerId, MouseEventButton, MouseEventButtons, UiEvent,
};
use blitz_traits::shell::{ColorScheme, Viewport};
use keyboard_types::Modifiers;
use std::sync::Arc;

struct Probe;
impl Widget for Probe {}

const PAGE: &str = r#"<html><body style="margin:0">
    <div id="outer" style="width:200px; height:100px;">
        <div id="host" style="width:200px; height:100px;"></div>
    </div>
</body></html>"#;

fn at(button: MouseEventButton, held: MouseEventButtons) -> BlitzPointerEvent {
    pointer_event(
        BlitzPointerId::Mouse,
        50.0,
        50.0,
        button,
        held,
        Modifiers::default(),
    )
}

fn press_release(button: MouseEventButton) -> [UiEvent; 2] {
    let held = MouseEventButtons::from(button);
    [
        UiEvent::PointerDown(at(button, held)),
        UiEvent::PointerUp(at(button, MouseEventButtons::None)),
    ]
}

fn with_widget() -> Harness {
    let mut harness = Harness::from_html_with(
        PAGE,
        HarnessOptions {
            width: 200,
            height: 100,
            ..Default::default()
        },
    );
    let host = harness.node("#host");
    harness
        .base_mut()
        .mutate()
        .set_custom_widget(host, Box::new(Probe));
    harness.pump();
    harness
}

fn with_subdocument() -> Harness {
    let mut harness = Harness::from_html_with(
        PAGE,
        HarnessOptions {
            width: 200,
            height: 100,
            ..Default::default()
        },
    );
    let inner = HtmlDocument::from_html(
        "<html><body>inner</body></html>",
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
    harness
}

fn bubbles_to_outer(harness: &mut Harness, button: MouseEventButton, name: &str) -> bool {
    let outer = harness.node("#outer");
    harness
        .dispatch_recorded_chains(press_release(button))
        .iter()
        .any(|(n, chain)| n == name && chain.contains(&outer))
}

#[test]
fn a_right_click_over_a_custom_widget_reaches_its_ancestors() {
    let mut harness = with_widget();
    assert!(bubbles_to_outer(
        &mut harness,
        MouseEventButton::Secondary,
        "contextmenu"
    ));
}

#[test]
fn a_left_click_over_a_custom_widget_reaches_its_ancestors() {
    let mut harness = with_widget();
    assert!(bubbles_to_outer(
        &mut harness,
        MouseEventButton::Main,
        "click"
    ));
}

#[test]
fn a_right_click_over_a_sub_document_reaches_its_ancestors() {
    let mut harness = with_subdocument();
    assert!(bubbles_to_outer(
        &mut harness,
        MouseEventButton::Secondary,
        "contextmenu"
    ));
}

#[test]
fn a_left_click_over_a_sub_document_reaches_its_ancestors() {
    let mut harness = with_subdocument();
    assert!(bubbles_to_outer(
        &mut harness,
        MouseEventButton::Main,
        "click"
    ));
}

#[test]
fn a_jittery_drag_over_a_custom_widget_starts_no_selection() {
    let mut harness = with_widget();
    let held = MouseEventButtons::from(MouseEventButton::Secondary);
    let moved = pointer_event(
        BlitzPointerId::Mouse,
        60.0,
        58.0,
        MouseEventButton::Secondary,
        held,
        Modifiers::default(),
    );
    let outer = harness.node("#outer");
    let names = harness.dispatch_recorded_chains([
        UiEvent::PointerDown(at(MouseEventButton::Secondary, held)),
        UiEvent::PointerMove(moved.clone()),
        UiEvent::PointerUp(moved),
    ]);
    assert!(
        names
            .iter()
            .any(|(n, chain)| n == "contextmenu" && chain.contains(&outer))
    );
}

const LINKED: &str = r#"<html>
    <body style="margin:0"><a id="l" href="../msg?id=7" style="display:block; height:100px">
    <span id="word">link text</span></a></body></html>"#;

/// A secondary press on a link inside a sub-document still reaches the host's ancestors
/// as `contextmenu`, and the link under the pointer is the nearest enclosing `a[href]`
/// of the sub-document, resolved against the sub-document's own base URL (Blitz takes it from the document's
/// configuration, as for a `srcdoc` frame; it does not read a `<base>` element).
#[test]
fn a_right_click_on_a_link_in_a_sub_document_reaches_the_host_with_the_link_under_it() {
    let mut harness = with_subdocument();
    let inner = HtmlDocument::from_html(
        LINKED,
        DocumentConfig {
            base_url: Some("https://mail.example/inbox/".to_owned()),
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

    assert!(bubbles_to_outer(
        &mut harness,
        MouseEventButton::Secondary,
        "contextmenu"
    ));

    let mut base = harness.base_mut();
    let sub = base
        .get_node_mut(host)
        .and_then(|n| n.subdoc_mut())
        .expect("the sub-document");
    sub.inner_mut().resolve(0.0);
    let inner = sub.inner();
    let hit = inner.hit(10.0, 10.0).expect("something under the pointer");
    let (mut id, mut href) = (Some(hit.node_id), None);
    while let Some(node) = id.and_then(|i| inner.get_node(i)) {
        if let Some(h) = node.attr(blitz_dom::local_name!("href")) {
            href = inner.url().join(h).ok().map(|u| u.to_string());
            break;
        }
        id = node.parent;
    }
    assert_eq!(href.as_deref(), Some("https://mail.example/msg?id=7"));
}
