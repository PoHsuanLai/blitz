//! Moving focus from code raises focus and blur events.

use std::cell::RefCell;
use std::rc::Rc;

use blitz_dom::{Document, EventDriver, EventHandler, NodeId};
use blitz_test_harness::Harness;
use blitz_traits::events::{DomEvent, EventState};

#[derive(Clone, Default)]
struct Recorder(Rc<RefCell<Vec<(String, NodeId)>>>);

impl EventHandler for Recorder {
    fn handle_event(
        &mut self,
        chain: &[NodeId],
        event: &mut DomEvent,
        _doc: &mut dyn Document,
        _event_state: &mut EventState,
    ) {
        self.0
            .borrow_mut()
            .push((event.name().to_string(), chain[0]));
    }
}

fn harness() -> Harness {
    Harness::from_html(r#"<html><body><input id="a"><input id="b"></body></html>"#)
}

/// Dispatch the pending focus events, returning each event's name and target.
fn flush(harness: &mut Harness) -> Vec<(String, NodeId)> {
    let recorder = Recorder::default();
    let mut doc = harness.base_mut();
    EventDriver::new(&mut *doc, recorder.clone()).flush_pending_events();
    drop(doc);
    recorder.0.take()
}

fn names(events: &[(String, NodeId)]) -> Vec<&str> {
    events.iter().map(|(name, _)| name.as_str()).collect()
}

#[test]
fn focusing_from_code_fires_focus_events() {
    let mut harness = harness();
    let a = harness.node("#a");
    harness.base_mut().set_focus_to(a);
    let events = flush(&mut harness);
    assert_eq!(names(&events), ["focus", "focusin"]);
    assert!(events.iter().all(|(_, target)| *target == a));
}

#[test]
fn moving_focus_from_code_blurs_the_old_element_first() {
    let mut harness = harness();
    let (a, b) = (harness.node("#a"), harness.node("#b"));
    harness.base_mut().set_focus_to(a);
    flush(&mut harness);

    harness.base_mut().set_focus_to(b);
    let events = flush(&mut harness);
    assert_eq!(names(&events), ["blur", "focusout", "focus", "focusin"]);
    assert_eq!(events[0].1, a);
    assert_eq!(events[2].1, b);
}

#[test]
fn clearing_focus_from_code_stays_silent() {
    let mut harness = harness();
    let a = harness.node("#a");
    harness.base_mut().set_focus_to(a);
    flush(&mut harness);

    harness.base_mut().clear_focus();
    assert!(flush(&mut harness).is_empty());
}

#[test]
fn user_focus_does_not_fire_twice() {
    let mut harness = harness();
    harness.click("#a");
    assert!(flush(&mut harness).is_empty());
}
