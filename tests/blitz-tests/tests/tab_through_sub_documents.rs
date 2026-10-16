//! Tab walks through the focussable nodes of a sub-document without wrapping inside it,
//! and carries on into the host document before and after the frame.

use blitz_dom::{DocumentConfig, local_name};
use blitz_html::{HtmlDocument, HtmlProvider};
use blitz_test_harness::Harness;
use blitz_traits::shell::{ColorScheme, Viewport};
use keyboard_types::{Key, Modifiers};
use std::sync::Arc;

const PAGE: &str = r#"<html><body>
    <input id="before"><iframe id="host"></iframe><input id="after">
</body></html>"#;

fn with_frame(inner_html: &str) -> Harness {
    let mut harness = Harness::from_html(PAGE);
    let inner = HtmlDocument::from_html(
        inner_html,
        DocumentConfig {
            viewport: Some(Viewport::new(300, 150, 1.0, ColorScheme::Light)),
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

fn two_inputs() -> Harness {
    with_frame(r#"<html><body><input id="i1"><input id="i2"></body></html>"#)
}

/// The id of the focussed element, looking inside a focussed frame
fn focused(harness: &Harness) -> String {
    let base = harness.base();
    let node = base.get_node(base.get_focussed_node_id().unwrap()).unwrap();
    let inner = node.subdoc().map(|sub| sub.inner());
    let node = match &inner {
        Some(inner) => inner
            .get_node(inner.get_focussed_node_id().unwrap())
            .unwrap(),
        None => node,
    };
    node.attr(local_name!("id")).unwrap_or_default().to_owned()
}

fn tab_through(harness: &mut Harness, modifiers: Modifiers, presses: usize) -> Vec<String> {
    (0..presses)
        .map(|_| {
            harness.press_with(Key::Tab, modifiers);
            focused(harness)
        })
        .collect()
}

#[test]
fn tab_visits_the_frame_in_order_and_wraps_only_at_the_end_of_the_host_document() {
    let mut harness = two_inputs();
    let order = tab_through(&mut harness, Modifiers::default(), 5);
    assert_eq!(order, ["before", "i1", "i2", "after", "before"]);
}

#[test]
fn shift_tab_visits_the_frame_in_reverse() {
    let mut harness = two_inputs();
    let order = tab_through(&mut harness, Modifiers::SHIFT, 5);
    assert_eq!(order, ["after", "i2", "i1", "before", "after"]);
}

#[test]
fn a_frame_without_focussable_nodes_is_skipped() {
    let mut harness = with_frame("<html><body>text only</body></html>");
    let forward = tab_through(&mut harness, Modifiers::default(), 3);
    assert_eq!(forward, ["before", "after", "before"]);
    let backward = tab_through(&mut harness, Modifiers::SHIFT, 2);
    assert_eq!(backward, ["after", "before"]);
}
