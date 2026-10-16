//! A wheel over a sub-document scrolls the sub-document first and passes what it could not
//! scroll on to the host element's scroll ancestors (scroll chaining), per axis.

use blitz_dom::DocumentConfig;
use blitz_html::{HtmlDocument, HtmlProvider};
use blitz_test_harness::Harness;
use blitz_traits::shell::{ColorScheme, Viewport};
use std::sync::Arc;

const PAGE: &str = r#"<html><body style="margin:0">
    <div id="container" style="width:200px; height:100px; overflow-y:scroll;">
        <iframe id="host" style="display:block; width:200px; height:100px; border:0;"></iframe>
        <div style="width:50px; height:600px;"></div>
    </div>
</body></html>"#;

/// The page with a frame whose body is `content_height` tall inside the 100px frame
fn with_frame(content_height: u32) -> Harness {
    let mut harness = Harness::from_html(PAGE);
    let inner = HtmlDocument::from_html(
        &format!(
            r#"<html><body style="margin:0"><div style="height:{content_height}px"></div></body></html>"#
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

fn container_scroll(harness: &Harness) -> f64 {
    let container = harness.node("#container");
    harness.base().get_node(container).unwrap().scroll_offset().y
}

fn frame_scroll(harness: &Harness) -> f64 {
    let host = harness.node("#host");
    let base = harness.base();
    let sub_doc = base.get_node(host).unwrap().subdoc().unwrap();
    sub_doc.inner().viewport_scroll().y
}

/// Roll the wheel over the frame by `pixels` (positive scrolls the content down)
fn wheel_down(harness: &mut Harness, pixels: f64) {
    harness.wheel_at(50.0, 50.0, 0.0, -pixels);
}

#[test]
fn a_wheel_over_a_frame_that_cannot_scroll_scrolls_the_container_around_it() {
    let mut harness = with_frame(50);
    wheel_down(&mut harness, 30.0);
    assert_eq!(frame_scroll(&harness), 0.0);
    assert_eq!(container_scroll(&harness), 30.0);
}

#[test]
fn a_wheel_over_a_scrollable_frame_scrolls_the_frame_and_not_the_container() {
    let mut harness = with_frame(400);
    wheel_down(&mut harness, 30.0);
    assert_eq!(frame_scroll(&harness), 30.0);
    assert_eq!(container_scroll(&harness), 0.0);
}

#[test]
fn a_wheel_over_a_frame_at_its_end_scrolls_the_container() {
    let mut harness = with_frame(400);
    wheel_down(&mut harness, 300.0);
    assert_eq!(frame_scroll(&harness), 300.0);
    assert_eq!(container_scroll(&harness), 0.0);

    wheel_down(&mut harness, 40.0);
    assert_eq!(frame_scroll(&harness), 300.0);
    assert_eq!(container_scroll(&harness), 40.0);
}

#[test]
fn a_wheel_that_runs_past_the_end_of_the_frame_passes_on_only_the_rest() {
    let mut harness = with_frame(400);
    wheel_down(&mut harness, 350.0);
    assert_eq!(frame_scroll(&harness), 300.0);
    assert_eq!(container_scroll(&harness), 50.0);
}
