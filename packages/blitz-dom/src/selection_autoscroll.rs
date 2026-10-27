//! Autoscroll of a text-selection drag.
//!
//! Dragging a selection towards, or past, the edge of a scroll container scrolls it, as in every
//! browser: the speed grows with the distance past the edge, the scrolling goes on while the
//! button stays down even if the pointer does not move, and the selection is extended to the
//! pointer's position in the content that scrolled under it. What a container cannot scroll
//! further falls to the next scroller outward, from a sub-document up through its host into the
//! host document's scroll ancestors (as a wheel over a sub-document does).
//!
//! The document that receives the pointer events (the one no other document hosts) records the
//! pointer and drives the scroll from [`BaseDocument::resolve`]; its `is_animating` keeps the
//! frame loop running while something scrolls.

use blitz_traits::node_id::NodeId;

use crate::BaseDocument;
use crate::events::DragMode;
use crate::scrolling::ScrollTarget;

/// How far inside a container's edge the pointer starts scrolling it, in CSS pixels.
const EDGE_ZONE: f32 = 14.0;
/// The speed at the edge zone's inner boundary, in CSS pixels per second.
const MIN_SPEED: f64 = 60.0;
/// How much the speed grows per CSS pixel the pointer is past the zone's inner boundary.
const SPEED_PER_PIXEL: f64 = 12.0;
const MAX_SPEED: f64 = 1600.0;
/// The longest step one tick scrolls for, so a stalled frame does not jump.
const MAX_STEP_SECONDS: f64 = 0.1;
/// How far (in steps of [`PROBE_STEP`]) a pointer outside a frame looks for text to extend to.
const PROBE_STEPS: u32 = 32;
const PROBE_STEP: f32 = 4.0;

#[derive(Clone, Copy, PartialEq)]
enum Axis {
    X,
    Y,
}

/// A rectangle in the client coordinates of the document receiving the pointer events.
#[derive(Clone, Copy, Debug)]
struct Rect {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl Rect {
    fn intersect(self, other: Rect) -> Rect {
        Rect {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        }
    }

    /// How far the pointer is past the edge zone along `axis`: negative before the start edge's
    /// zone, positive after the end edge's, zero between them.
    fn overshoot(self, axis: Axis, x: f32, y: f32) -> f32 {
        let (p, start, end) = match axis {
            Axis::X => (x, self.left, self.right),
            Axis::Y => (y, self.top, self.bottom),
        };
        // A container too small to have both zones scrolls towards the nearer edge.
        let zone = EDGE_ZONE.min((end - start) / 2.0).max(0.0);
        if p < start + zone {
            p - (start + zone)
        } else if p > end - zone {
            p - (end - zone)
        } else {
            0.0
        }
    }
}

/// A container the drag can scroll, with where it shows.
struct Scroller {
    /// The host nodes leading from the receiving document to the document holding the scroller.
    path: Vec<NodeId>,
    target: ScrollTarget,
    rect: Rect,
}

fn speed_for(overshoot: f32) -> f64 {
    (MIN_SPEED + SPEED_PER_PIXEL * overshoot.abs() as f64).min(MAX_SPEED)
}

impl BaseDocument {
    /// Record the pointer of a selection drag, or forget it once the button is up.
    pub(crate) fn track_selection_pointer(&mut self, page_x: f32, page_y: f32, primary_held: bool) {
        if !primary_held {
            self.stop_selection_autoscroll();
            return;
        }
        self.selection_pointer = Some((
            page_x - self.viewport_scroll.x as f32,
            page_y - self.viewport_scroll.y as f32,
        ));
        // The frame that resolves next starts or retargets the scroll.
        self.shell_provider.request_redraw();
    }

    /// Forget the pointer: the button went up or the drag was cancelled.
    pub(crate) fn stop_selection_autoscroll(&mut self) {
        self.selection_pointer = None;
        self.selection_autoscroll_time = None;
        self.selection_autoscrolling = false;
    }

    /// End a drag which started in a sub-document when the button went up outside it: the frame
    /// gets no pointer events there, so nothing else tells it.
    pub(crate) fn end_sub_document_drag(&mut self) {
        if self.hover_node_id == self.mousedown_node_id {
            return;
        }
        let Some(host) = self.mousedown_node_id else {
            return;
        };
        if let Some(sub_doc) = self.nodes.get_mut(host).and_then(|n| n.subdoc_mut()) {
            let mut inner = sub_doc.inner_mut();
            inner.drag_mode = DragMode::None;
            inner.end_sub_document_drag();
        }
    }

    /// The host nodes from this document down to the document whose selection is being dragged
    /// (empty when this document's own is), if a selection drag is going on.
    fn selection_drag_path(&self) -> Option<Vec<NodeId>> {
        if self.drag_mode == DragMode::Selecting {
            return Some(Vec::new());
        }
        let host = self.mousedown_node_id?;
        let sub_doc = self.nodes.get(host)?.subdoc()?;
        let mut path = sub_doc.inner().selection_drag_path()?;
        path.insert(0, host);
        Some(path)
    }

    /// Run `f` on the document at the end of `path`, if the path still leads to one.
    fn with_doc_at<R>(
        &mut self,
        path: &[NodeId],
        f: impl FnOnce(&mut BaseDocument) -> R,
    ) -> Option<R> {
        match path.split_first() {
            None => Some(f(self)),
            Some((&host, rest)) => {
                let sub_doc = self.nodes.get_mut(host)?.subdoc_mut()?;
                let mut inner = sub_doc.inner_mut();
                inner.with_doc_at(rest, f)
            }
        }
    }

    /// A node's border box in client coordinates of this document.
    fn client_rect_of(&self, node_id: NodeId) -> Option<Rect> {
        let node = self.nodes.get(node_id)?;
        // `absolute_position` takes the node's own scroll offset off, which its box does not move by.
        let own = node.scroll_offset();
        let pos = node.absolute_position(0.0, 0.0);
        let left = pos.x + own.x as f32 - self.viewport_scroll.x as f32;
        let top = pos.y + own.y as f32 - self.viewport_scroll.y as f32;
        let size = node.final_layout().size;
        Some(Rect {
            left,
            top,
            right: left + size.width,
            bottom: top + size.height,
        })
    }

    fn viewport_client_rect(&self) -> Rect {
        let scale = self.viewport.scale();
        Rect {
            left: 0.0,
            top: 0.0,
            right: self.viewport.window_size.0 as f32 / scale,
            bottom: self.viewport.window_size.1 as f32 / scale,
        }
    }

    /// Whether `node_id` scrolls an overflow range of its own (the root element's goes to the
    /// viewport).
    fn is_scroll_container(&self, node_id: NodeId) -> bool {
        if self.try_root_element().is_some_and(|el| el.id == node_id) {
            return false;
        }
        let (_, max) = self.scroll_state(ScrollTarget::Node(node_id), false);
        max.x > 0.0 || max.y > 0.0
    }

    /// Collect the scrollers from the one holding `start` outward: its scrolling ancestors, then
    /// the viewport, then those of the host nodes in `path`'s documents above. `origin` is where
    /// this document's client space starts in the receiving document's.
    fn collect_drag_scrollers(
        &self,
        path: &[NodeId],
        walked: &mut Vec<NodeId>,
        origin: (f32, f32),
        out: &mut Vec<Scroller>,
    ) {
        // Where the walk outward begins: the host's parent for a frame above, else the selection's
        // ends (the focus alone would not do: dragged out of the container it is in, it is in
        // whatever is under the pointer).
        let starts: Vec<NodeId> = match path.split_first() {
            Some((&host, rest)) => {
                let Some(rect) = self.client_rect_of(host) else {
                    return;
                };
                if let Some(sub_doc) = self.nodes.get(host).and_then(|n| n.subdoc()) {
                    walked.push(host);
                    sub_doc.inner().collect_drag_scrollers(
                        rest,
                        walked,
                        (origin.0 + rect.left, origin.1 + rect.top),
                        out,
                    );
                    walked.pop();
                }
                self.nodes
                    .get(host)
                    .and_then(|node| node.parent)
                    .into_iter()
                    .collect()
            }
            None => [
                self.text_selection.anchor.node_or_parent,
                self.text_selection.focus.node_or_parent,
            ]
            .into_iter()
            .flatten()
            .collect(),
        };

        let at = |rect: Rect| Rect {
            left: rect.left + origin.0,
            top: rect.top + origin.1,
            right: rect.right + origin.0,
            bottom: rect.bottom + origin.1,
        };
        let first_of_this_document = out.len();
        for start in starts {
            let mut next = Some(start);
            while let Some(id) = next {
                let seen = out[first_of_this_document..]
                    .iter()
                    .any(|s| s.target == ScrollTarget::Node(id));
                if !seen && self.is_scroll_container(id) {
                    if let Some(rect) = self.client_rect_of(id) {
                        out.push(Scroller {
                            path: walked.clone(),
                            target: ScrollTarget::Node(id),
                            rect: at(rect),
                        });
                    }
                }
                next = self.nodes.get(id).and_then(|node| node.parent);
            }
        }
        out.push(Scroller {
            path: walked.clone(),
            target: ScrollTarget::Viewport,
            rect: at(self.viewport_client_rect()),
        });
    }

    /// Advance the drag's autoscroll to `now` (seconds): scroll the first container which the
    /// pointer is at the edge of or past and which can scroll that way, then extend the selection
    /// to the pointer in the content that moved.
    pub(crate) fn tick_selection_autoscroll(&mut self, now: f64) {
        let Some(pointer) = self.selection_pointer else {
            return self.stop_selection_autoscroll();
        };
        let Some(path) = self.selection_drag_path() else {
            return self.stop_selection_autoscroll();
        };
        let last = self.selection_autoscroll_time.replace(now);
        let dt = last.map_or(0.0, |last| (now - last).clamp(0.0, MAX_STEP_SECONDS));

        let mut scrollers = Vec::new();
        self.collect_drag_scrollers(&path, &mut Vec::new(), (0.0, 0.0), &mut scrollers);
        // What shows of a container is what its scrolling ancestors let through.
        for i in (0..scrollers.len().saturating_sub(1)).rev() {
            scrollers[i].rect = scrollers[i].rect.intersect(scrollers[i + 1].rect);
        }

        let mut active = false;
        let mut moved = false;
        for axis in [Axis::X, Axis::Y] {
            for scroller in &scrollers {
                let overshoot = scroller.rect.overshoot(axis, pointer.0, pointer.1);
                if overshoot == 0.0 {
                    continue;
                }
                let toward_end = overshoot > 0.0;
                let step = speed_for(overshoot) * dt * if toward_end { 1.0 } else { -1.0 };
                let target = scroller.target;
                let result = self.with_doc_at(&scroller.path, |doc| {
                    let (current, max) = doc.scroll_state(target, false);
                    let (cur, max) = match axis {
                        Axis::X => (current.x, max.x),
                        Axis::Y => (current.y, max.y),
                    };
                    // Nothing left that way: the next scroller outward takes it.
                    if (toward_end && cur >= max) || (!toward_end && cur <= 0.0) {
                        return None;
                    }
                    let mut offset = current;
                    let to = (cur + step).clamp(0.0, max);
                    match axis {
                        Axis::X => offset.x = to,
                        Axis::Y => offset.y = to,
                    }
                    Some(doc.write_scroll_offset(target, offset, &mut |_| {}))
                });
                if let Some(Some(changed)) = result {
                    active = true;
                    moved |= changed;
                    break;
                }
            }
        }
        self.selection_autoscrolling = active;
        if moved || active {
            self.extend_selection_drag_to_pointer();
        }
    }

    /// Extend the dragged selection to the recorded pointer, also when the pointer is not over
    /// the sub-document the selection is in or is past its edge.
    pub(crate) fn extend_selection_drag_to_pointer(&mut self) -> bool {
        let Some(pointer) = self.selection_pointer else {
            return false;
        };
        let Some(path) = self.selection_drag_path() else {
            return false;
        };
        self.extend_selection_through(&path, pointer.0, pointer.1)
    }

    fn extend_selection_through(&mut self, path: &[NodeId], client_x: f32, client_y: f32) -> bool {
        match path.split_first() {
            Some((&host, rest)) => {
                let Some(node) = self.nodes.get_mut(host) else {
                    return false;
                };
                let pos = node.absolute_position(0.0, 0.0);
                let x = client_x + self.viewport_scroll.x as f32 - pos.x;
                let y = client_y + self.viewport_scroll.y as f32 - pos.y;
                let Some(sub_doc) = self.nodes.get_mut(host).and_then(|n| n.subdoc_mut()) else {
                    return false;
                };
                sub_doc.inner_mut().extend_selection_through(rest, x, y)
            }
            None => self.extend_selection_to_nearest_text(client_x, client_y),
        }
    }

    /// Extend the selection to the text at the client point, clamped into the viewport; a point
    /// past the end looks back towards the content for the last text there is.
    fn extend_selection_to_nearest_text(&mut self, client_x: f32, client_y: f32) -> bool {
        let rect = self.viewport_client_rect();
        let x = client_x.clamp(rect.left, (rect.right - 1.0).max(rect.left));
        let y = client_y.clamp(rect.top, (rect.bottom - 1.0).max(rect.top));
        let toward_content = match y.partial_cmp(&client_y) {
            Some(std::cmp::Ordering::Greater) => 1.0,
            Some(std::cmp::Ordering::Less) => -1.0,
            _ => 0.0,
        };
        let page_x = x + self.viewport_scroll.x as f32;
        let page_y = y + self.viewport_scroll.y as f32;
        let steps = if toward_content == 0.0 {
            0
        } else {
            PROBE_STEPS
        };
        for step in 0..=steps {
            let probe_y = page_y + toward_content * PROBE_STEP * step as f32;
            if self.extend_text_selection_to_point(page_x, probe_y) {
                return true;
            }
        }
        false
    }
}
