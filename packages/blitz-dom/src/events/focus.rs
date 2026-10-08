use blitz_traits::events::{BlitzFocusEvent, DomEvent, DomEventData};

use crate::BaseDocument;

/// Run a focus change made by a user action and dispatch the blur and focus events it caused,
/// from how the focus differs before and after.
///
/// [`BaseDocument::set_focus_to`] queues the same events itself (so focus moved from code
/// raises them); the ones it queued during `update_focus` are dropped here, not dispatched
/// twice. Clearing the focus queues nothing, so the blur of a cleared focus comes from here.
pub(crate) fn generate_focus_events(
    doc: &mut BaseDocument,
    update_focus: &mut dyn FnMut(&mut BaseDocument),
    dispatch_event: &mut dyn FnMut(DomEvent),
) {
    // Update focus, tracking which node was focussed before and after
    let queued = doc.pending_focus_events.len();
    let old_focus = doc.get_focussed_node_id();
    update_focus(doc);
    let new_focus = doc.get_focussed_node_id();
    doc.pending_focus_events.truncate(queued);

    if old_focus == new_focus {
        return;
    }

    if let Some(old_focus) = old_focus {
        dispatch_event(DomEvent::new(
            old_focus,
            DomEventData::Blur(BlitzFocusEvent),
        ));
        dispatch_event(DomEvent::new(
            old_focus,
            DomEventData::FocusOut(BlitzFocusEvent),
        ));
    }

    if let Some(new_focus) = new_focus {
        dispatch_event(DomEvent::new(
            new_focus,
            DomEventData::Focus(BlitzFocusEvent),
        ));
        dispatch_event(DomEvent::new(
            new_focus,
            DomEventData::FocusIn(BlitzFocusEvent),
        ));
    }
}
