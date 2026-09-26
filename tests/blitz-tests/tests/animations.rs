use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;

/// Regression test for https://github.com/DioxusLabs/blitz/issues/407
///
/// `resolve_stylist` used to panic with "invalid key" when a CSS-animated node
/// was removed between two `resolve` calls. Verify the second resolve is safe.
#[test]
fn resolve_does_not_panic_after_removing_animated_node() {
    let html = r#"
        <style>
          @keyframes pulse { 0% { opacity: 1; } 100% { opacity: 0.5; } }
          .pulse { animation: pulse 2s infinite; }
        </style>
        <div id="pulse-node" class="pulse">animated</div>
    "#;

    let mut doc = HtmlDocument::from_html(html, DocumentConfig::default());
    doc.resolve(0.0);

    let pulse_id = doc.get_element_by_id("pulse-node");
    if let Some(id) = pulse_id {
        doc.mutate().remove_and_drop_node(id);
    }

    // Must not panic: stale animation entry should be skipped safely.
    doc.resolve(0.1);
}

const FADE: &str = r#"
    <style>
      @keyframes fade { from { opacity: 0; } to { opacity: 1; } }
      .fade { animation: fade 1s linear; }
    </style>
    <div id="target" class="fade">animated</div>
"#;

fn opacity_of(doc: &HtmlDocument) -> f32 {
    let id = doc.get_element_by_id("target").unwrap();
    doc.get_node(id)
        .unwrap()
        .primary_styles()
        .unwrap()
        .clone_opacity()
}

fn set_class(doc: &mut HtmlDocument, value: &str) {
    let id = doc.get_element_by_id("target").unwrap();
    let class = blitz_dom::QualName::new(None, blitz_dom::ns!(), blitz_dom::local_name!("class"));
    doc.mutate().set_attribute(id, class, value);
}

/// Removing `animation-name` mid-animation must not leave the animated value behind.
#[test]
fn cancelled_animation_leaves_no_value_behind() {
    let mut doc = HtmlDocument::from_html(FADE, DocumentConfig::default());
    doc.resolve(0.0);
    doc.resolve(0.25);
    assert!((opacity_of(&doc) - 0.25).abs() < 0.01, "animating");

    set_class(&mut doc, "");
    doc.resolve(0.5);
    assert_eq!(opacity_of(&doc), 1.0, "resting value after cancel");

    doc.resolve(0.75);
    assert_eq!(opacity_of(&doc), 1.0, "still at rest");
}
