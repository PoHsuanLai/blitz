//! Moving focus from code raises the same focus and blur events as user focus.

use blitz_test_harness::Harness;

fn harness() -> Harness {
    Harness::from_html(
        r#"<html><body>
        <input id="a"><input id="b">
    </body></html>"#,
    )
}

fn names(events: &[(String, blitz_dom::NodeId)]) -> Vec<&str> {
    events.iter().map(|(n, _)| n.as_str()).collect()
}

#[test]
fn focusing_from_code_fires_focus_events() {
    let mut harness = harness();
    let a = harness.node("#a");
    harness.base_mut().set_focus_to(a);
    let events = harness.flush_recorded_focus_events();
    assert_eq!(names(&events), ["focus", "focusin"]);
    assert!(events.iter().all(|(_, target)| *target == a));
}

#[test]
fn moving_focus_from_code_blurs_the_old_element_first() {
    let mut harness = harness();
    let (a, b) = (harness.node("#a"), harness.node("#b"));
    harness.base_mut().set_focus_to(a);
    harness.flush_recorded_focus_events();

    harness.base_mut().set_focus_to(b);
    let events = harness.flush_recorded_focus_events();
    assert_eq!(names(&events), ["blur", "focusout", "focus", "focusin"]);
    assert_eq!(events[0].1, a);
    assert_eq!(events[2].1, b);
}

#[test]
fn clearing_focus_from_code_fires_blur() {
    let mut harness = harness();
    let a = harness.node("#a");
    harness.base_mut().set_focus_to(a);
    harness.flush_recorded_focus_events();

    harness.base_mut().clear_focus();
    let events = harness.flush_recorded_focus_events();
    assert_eq!(names(&events), ["blur", "focusout"]);
}

#[test]
fn focusing_the_focussed_element_fires_nothing() {
    let mut harness = harness();
    let a = harness.node("#a");
    harness.base_mut().set_focus_to(a);
    harness.flush_recorded_focus_events();
    harness.base_mut().set_focus_to(a);
    assert!(harness.flush_recorded_focus_events().is_empty());
}

#[test]
fn user_focus_does_not_fire_twice() {
    let mut harness = harness();
    harness.click("#a");
    // The click already dispatched its focus events; nothing is left over
    assert!(harness.flush_recorded_focus_events().is_empty());
}
