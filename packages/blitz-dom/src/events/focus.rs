use blitz_traits::events::DomEvent;

use crate::BaseDocument;

/// Run a focus change and dispatch the blur and focus events it raised.
///
/// The events are queued by [`BaseDocument::set_focus_to`] and
/// [`BaseDocument::clear_focus`] themselves (so focus moved from code raises them too);
/// this drains the queue straight away so they are dispatched in order with the user
/// action that caused them.
pub(crate) fn generate_focus_events(
    doc: &mut BaseDocument,
    update_focus: &mut dyn FnMut(&mut BaseDocument),
    dispatch_event: &mut dyn FnMut(DomEvent),
) {
    update_focus(doc);
    for event in doc.take_pending_focus_events() {
        dispatch_event(event);
    }
}
