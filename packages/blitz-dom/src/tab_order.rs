//! Tab traversal through a document and the sub-documents it hosts.
//!
//! A sub-document host keeps the focus of its own document while a node inside the
//! sub-document holds the focus of that one. Tab walks the sub-document's stops first
//! (without wrapping) and only then returns to the host document's own order.

use std::cmp::Ordering;

use blitz_traits::node_id::NodeId;
use markup5ever::local_name;

use crate::{BaseDocument, Node};

/// Which way Tab moves through the tab order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusDirection {
    Forward,
    Backward,
}

impl BaseDocument {
    /// Whether Tab stops at `node`. A sub-document host is a stop only when the
    /// sub-document has a stop of its own (Tab enters it there), or when the host has an
    /// explicit non-negative `tabindex`; an empty sub-document is skipped.
    fn is_tab_stop(&self, node: &Node) -> bool {
        let Some(sub_doc) = node.subdoc() else {
            return node.is_focussable();
        };
        sub_doc.inner().has_tab_stop()
            || node
                .element_data()
                .and_then(|el| el.attr_parsed(local_name!("tabindex")))
                .is_some_and(|index: i32| index >= 0)
    }

    fn has_tab_stop(&self) -> bool {
        self.try_root_element().is_some_and(|root| {
            self.next_node(root, |node| self.is_tab_stop(node))
                .is_some()
        })
    }

    /// The next Tab stop from `start` in `direction`, wrapping at the ends of the document.
    fn tab_stop_from(&self, start: NodeId, direction: FocusDirection) -> Option<NodeId> {
        let start = &self.nodes[start];
        match direction {
            FocusDirection::Forward => self.next_node(start, |node| self.is_tab_stop(node)),
            FocusDirection::Backward => self.prev_node(start, |node| self.is_tab_stop(node)),
        }
    }

    /// Like [`Self::tab_stop_from`], but `None` where it would wrap.
    fn tab_stop_without_wrap(&self, start: NodeId, direction: FocusDirection) -> Option<NodeId> {
        let id = self.tab_stop_from(start, direction)?;
        let beyond = match direction {
            FocusDirection::Forward => Ordering::Less,
            FocusDirection::Backward => Ordering::Greater,
        };
        (self.compare_document_order(start, id) == beyond).then_some(id)
    }

    /// Focus a Tab stop. Leaving a sub-document host drops the focus held inside it, and
    /// arriving at one enters it at its first stop (its last, going backwards).
    fn focus_tab_stop(&mut self, id: NodeId, direction: FocusDirection) {
        if let Some(old) = self.focus_node_id.filter(|&old| old != id) {
            if let Some(sub_doc) = self.subdoc_mut(old) {
                sub_doc.inner_mut().clear_focus();
            }
        }
        self.set_focus_to(id);
        if let Some(sub_doc) = self.subdoc_mut(id) {
            let mut sub_doc = sub_doc.inner_mut();
            sub_doc.clear_focus();
            match direction {
                FocusDirection::Forward => sub_doc.focus_next_node(),
                FocusDirection::Backward => sub_doc.focus_prev_node(),
            };
        }
    }

    /// Move the focus on inside the focussed sub-document, if it has a stop left that way.
    fn focus_step_inside(&mut self, direction: FocusDirection) -> bool {
        let host = self.focus_node_id;
        host.and_then(|id| self.subdoc_mut(id))
            .is_some_and(|sub_doc| sub_doc.inner_mut().focus_step(direction))
    }

    /// Move the focus to the next Tab stop in `direction` without wrapping; `false` when
    /// there is none left. Used by a sub-document, which hands over to its host then.
    fn focus_step(&mut self, direction: FocusDirection) -> bool {
        if self.focus_step_inside(direction) {
            return true;
        }
        let Some(start) = self.get_focussed_node_id() else {
            return false;
        };
        let Some(id) = self.tab_stop_without_wrap(start, direction) else {
            return false;
        };
        self.focus_tab_stop(id, direction);
        true
    }

    /// Move the focus to the next Tab stop in `direction`, wrapping at the ends of this
    /// document. Returns the node now focussed in this document (the host when the focus
    /// moved on inside its sub-document).
    pub(crate) fn focus_by_tab(&mut self, direction: FocusDirection) -> Option<NodeId> {
        if self.focus_step_inside(direction) {
            return self.focus_node_id;
        }
        let id = self.tab_stop_from(self.get_focussed_node_id()?, direction)?;
        self.focus_tab_stop(id, direction);
        Some(id)
    }
}
