//! `text-overflow`: the marker (e.g. `…`) drawn where a line overflows a clipping block.
//!
//! Truncation is a rendering effect (css-overflow §5.2): layout keeps the full text and only
//! records, per line, how many glyphs paint keeps and where the marker goes.
//!
//! Supported: the inline-end side of left-to-right text, in a block container whose
//! `overflow` is not `visible` (typically with `white-space: nowrap`).
//!
//! TODO: right-to-left text and the inline-start side; following the scroll offset of a
//! scrolled container (paint draws the full text when the container is scrolled
//! horizontally); cutting inside a grapheme that spans several shaping clusters.

use blitz_traits::node_id::NodeId;
use parley::{FontContext, InlineBoxKind, Layout, LayoutContext, PositionedLayoutItem};
use style::computed_values::direction::T as Direction;
use style::properties::ComputedValues;
use style::values::computed::Overflow;
use style::values::specified::box_::DisplayInside;
use style::values::specified::text::TextOverflowSide;

use crate::node::{TextBrush, TextOverflowCut, TextOverflowMarker};
use crate::{BaseDocument, stylo_to_parley};

/// The node whose style decides `text-overflow` for the inline root `node_id`: the root
/// itself, or, for an anonymous block wrapping inline content, the block container that
/// holds it (the box that clips). `None` for flex and grid containers, to which
/// `text-overflow` does not apply.
fn style_node(doc: &BaseDocument, node_id: NodeId) -> Option<NodeId> {
    let node = &doc.nodes[node_id];
    if !node.is_anonymous() {
        return Some(node_id);
    }
    let parent_id = node.parent?;
    let parent_styles = doc.nodes[parent_id].primary_styles()?;
    matches!(
        parent_styles.get_box().display.inside(),
        DisplayInside::Flow | DisplayInside::FlowRoot
    )
    .then_some(parent_id)
}

/// The marker text the style asks for at the inline end, or `None` when overflowing
/// content is just clipped.
fn marker_text(styles: &ComputedValues) -> Option<String> {
    if styles.get_box().overflow_x == Overflow::Visible {
        return None;
    }
    // TODO: right-to-left text.
    if styles.clone_direction() == Direction::Rtl {
        return None;
    }
    // A single value is stored in `second`; with two, `second` is the right (end) side.
    match &styles.get_text().text_overflow.second {
        TextOverflowSide::Clip => None,
        TextOverflowSide::Ellipsis => Some("\u{2026}".to_string()),
        TextOverflowSide::String(s) => Some(s.to_string()),
    }
}

impl BaseDocument {
    /// The `text-overflow` marker for the inline root `node_id`, whose lines have been
    /// broken and aligned, or `None` when no line needs one. `content_width` is the width
    /// of the root's content box in layout (device) pixels.
    pub(crate) fn compute_text_overflow(
        &mut self,
        node_id: NodeId,
        layout: &Layout<TextBrush>,
        content_width: f32,
    ) -> Option<TextOverflowMarker> {
        // Cheap check first: most lines fit.
        if !layout
            .lines()
            .any(|line| line_end(line.metrics()) > content_width)
        {
            return None;
        }

        let style_id = style_node(self, node_id)?;
        let styles = self.nodes[style_id].primary_styles()?;
        let text = marker_text(&styles)?;

        // The marker is styled by the block container (not by the inline content it
        // follows), so it is laid out on its own from the container's style.
        let scale = self.viewport.scale();
        let style = stylo_to_parley::style(style_id, &styles);
        drop(styles);
        let font_ctx = &mut *self.font_ctx.lock().unwrap();
        let marker = layout_marker(&mut self.layout_ctx, font_ctx, scale, &style, &text);

        let marker_advance = marker.width();
        let lines = layout
            .lines()
            .map(|line| {
                (line_end(line.metrics()) > content_width)
                    .then(|| cut_line(&line, content_width - marker_advance))
            })
            .collect();
        Some(TextOverflowMarker {
            layout: marker,
            lines,
        })
    }
}

/// Lay out `text` on a single line in `style`.
fn layout_marker(
    layout_ctx: &mut LayoutContext<TextBrush>,
    font_ctx: &mut FontContext,
    scale: f32,
    style: &parley::TextStyle<'static, 'static, TextBrush>,
    text: &str,
) -> Layout<TextBrush> {
    let mut builder = layout_ctx.tree_builder(font_ctx, scale, true, style);
    builder.push_text(text);
    let mut marker = Layout::new();
    builder.build_into(&mut marker);
    marker.break_all_lines(None);
    marker
}

/// The x at which a line's content ends, not counting trailing whitespace, so that a line
/// starting further in (`text-indent`, alignment) is measured where it actually ends.
fn line_end(metrics: &parley::LineMetrics) -> f32 {
    metrics.inline_min_coord + metrics.offset + metrics.advance - metrics.trailing_whitespace
}

/// Keep the longest run of whole clusters at the start of `line` that ends at or before
/// `limit`. Content after an inline box that does not fit is dropped with it.
fn cut_line(line: &parley::Line<'_, TextBrush>, limit: f32) -> TextOverflowCut {
    let mut runs = line.runs();
    let mut glyphs_kept = 0;
    let mut marker_x = line.metrics().inline_min_coord + line.metrics().offset;
    // Glyphs of the current text run not yet covered by the glyph runs walked so far. A
    // text run is split into consecutive glyph runs by style; its clusters are measured
    // once, when its first glyph run is reached.
    let mut glyphs_left_in_run = 0;

    for item in line.items() {
        match item {
            PositionedLayoutItem::InlineBox(ibox) => {
                if ibox.kind != InlineBoxKind::InFlow {
                    continue;
                }
                if ibox.x + ibox.width > limit {
                    break;
                }
                marker_x = ibox.x + ibox.width;
            }
            PositionedLayoutItem::GlyphRun(glyph_run) => {
                if glyphs_left_in_run == 0 {
                    let Some(run) = runs.next() else { break };
                    let mut x = glyph_run.offset();
                    for cluster in run.visual_clusters() {
                        let end = x + cluster.advance();
                        if end > limit {
                            return TextOverflowCut {
                                glyphs_kept,
                                marker_x,
                            };
                        }
                        glyphs_kept += cluster.glyphs().count();
                        glyphs_left_in_run += cluster.glyphs().count();
                        marker_x = end;
                        x = end;
                    }
                }
                glyphs_left_in_run = glyphs_left_in_run.saturating_sub(glyph_run.glyphs().count());
            }
        }
    }

    TextOverflowCut {
        glyphs_kept,
        marker_x,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Attribute, DocumentConfig, qual_name};
    use blitz_traits::shell::{ColorScheme, Viewport};

    const LONG_TEXT: &str = "A label far too long to fit in its box";

    /// Lay out `<div style="{div_style}">{text}</div>` at 2x scale (the HTML parser lives
    /// in blitz-html, which would be a circular dev-dependency) and return the document
    /// and the div.
    fn make_doc(div_style: &str, text: &str) -> (BaseDocument, NodeId) {
        let mut doc = BaseDocument::new(DocumentConfig {
            viewport: Some(Viewport::new(800, 600, 2.0, ColorScheme::Light)),
            ..Default::default()
        });
        let root_id = doc.root_node().id;
        let style = |value: &str| Attribute {
            name: qual_name!("style"),
            value: value.to_string(),
        };

        let mut mutator = doc.mutate();
        // No user-agent stylesheet is loaded, so every element is styled as a block here.
        let html = mutator.create_element(qual_name!("html"), vec![style("display:block")]);
        let body = mutator.create_element(qual_name!("body"), vec![style("display:block")]);
        let div_style = format!("display:block; {div_style}");
        let div = mutator.create_element(qual_name!("div"), vec![style(&div_style)]);
        let text = mutator.create_text_node(text);
        mutator.append_children(div, &[text]);
        mutator.append_children(body, &[div]);
        mutator.append_children(html, &[body]);
        mutator.append_children(root_id, &[html]);
        drop(mutator);

        doc.resolve(0.0);
        (doc, div)
    }

    fn text_layout(doc: &BaseDocument, div: NodeId) -> &crate::node::TextLayout {
        doc.nodes[div]
            .element_data()
            .unwrap()
            .inline_layout_data
            .as_deref()
            .expect("the div is an inline root")
    }

    /// Without the `system-fonts` feature text measures 0x0, which would make these tests
    /// vacuous.
    fn has_font(doc: &BaseDocument, div: NodeId) -> bool {
        text_layout(doc, div).layout.width() > 0.0
    }

    const ELLIPSIS: &str =
        "width:100px; padding:0 7px; overflow:hidden; white-space:nowrap; text-overflow:ellipsis";

    #[test]
    fn overflowing_line_ends_with_the_ellipsis_and_fits() {
        let (doc, div) = make_doc(ELLIPSIS, LONG_TEXT);
        if !has_font(&doc, div) {
            eprintln!("skipping: no usable font (text measures 0x0)");
            return;
        }
        let text_layout = text_layout(&doc, div);
        let scale = doc.viewport.scale();
        let content_width = 100.0 * scale;
        assert!(
            text_layout.layout.width() > content_width,
            "the text overflows"
        );

        let marker = text_layout.text_overflow.as_ref().expect("a marker");
        assert_eq!(marker.lines.len(), 1);
        let cut = marker.lines[0].expect("the only line is cut");

        // The marker is the single `…` glyph of the block's font.
        let marker_line = marker.layout.lines().next().unwrap();
        let glyphs: Vec<_> = marker_line
            .items()
            .filter_map(|item| match item {
                PositionedLayoutItem::GlyphRun(run) => Some(run.glyphs().collect::<Vec<_>>()),
                PositionedLayoutItem::InlineBox(_) => None,
            })
            .flatten()
            .collect();
        assert_eq!(glyphs.len(), 1, "one glyph for U+2026");
        assert_ne!(glyphs[0].id, 0, "the font has U+2026 (not .notdef)");

        // Some text is kept, not all of it, and the marker drawn after it fits the box.
        let line = text_layout.layout.lines().next().unwrap();
        let total_glyphs: usize = line
            .items()
            .map(|item| match item {
                PositionedLayoutItem::GlyphRun(run) => run.glyphs().count(),
                PositionedLayoutItem::InlineBox(_) => 0,
            })
            .sum();
        assert!(cut.glyphs_kept > 0 && cut.glyphs_kept < total_glyphs);
        assert!(cut.marker_x > 0.0);
        assert!(cut.marker_x + marker.layout.width() <= content_width);

        // The kept glyphs end exactly where the marker starts.
        let kept_end = line
            .items()
            .filter_map(|item| match item {
                PositionedLayoutItem::GlyphRun(run) => Some(
                    run.positioned_glyphs()
                        .map(|g| g.x + g.advance)
                        .collect::<Vec<_>>(),
                ),
                PositionedLayoutItem::InlineBox(_) => None,
            })
            .flatten()
            .nth(cut.glyphs_kept - 1)
            .unwrap();
        assert!((kept_end - cut.marker_x).abs() < 0.01);
    }

    #[test]
    fn text_that_fits_has_no_marker() {
        let (doc, div) = make_doc(ELLIPSIS, "Short");
        if !has_font(&doc, div) {
            eprintln!("skipping: no usable font (text measures 0x0)");
            return;
        }
        assert!(text_layout(&doc, div).text_overflow.is_none());
    }

    #[test]
    fn clip_and_visible_overflow_have_no_marker() {
        for style in [
            "width:100px; overflow:hidden; white-space:nowrap; text-overflow:clip",
            "width:100px; overflow:hidden; white-space:nowrap",
            "width:100px; white-space:nowrap; text-overflow:ellipsis",
        ] {
            let (doc, div) = make_doc(style, LONG_TEXT);
            if !has_font(&doc, div) {
                eprintln!("skipping: no usable font (text measures 0x0)");
                return;
            }
            assert!(text_layout(&doc, div).text_overflow.is_none(), "{style}");
        }
    }

    #[test]
    fn string_marker_is_laid_out() {
        let style = "width:100px; overflow:hidden; white-space:nowrap; text-overflow:'>>'";
        let (doc, div) = make_doc(style, LONG_TEXT);
        if !has_font(&doc, div) {
            eprintln!("skipping: no usable font (text measures 0x0)");
            return;
        }
        let marker = text_layout(&doc, div)
            .text_overflow
            .as_ref()
            .expect("a marker");
        let cut = marker.lines[0].expect("the only line is cut");
        let marker_glyphs: usize = marker
            .layout
            .lines()
            .flat_map(|line| line.items().collect::<Vec<_>>())
            .map(|item| match item {
                PositionedLayoutItem::GlyphRun(run) => run.glyphs().count(),
                PositionedLayoutItem::InlineBox(_) => 0,
            })
            .sum();
        assert_eq!(marker_glyphs, 2);
        assert!(cut.marker_x + marker.layout.width() <= 100.0 * doc.viewport.scale());
    }

    #[test]
    fn right_to_left_text_is_not_cut_yet() {
        let style = format!("{ELLIPSIS}; direction:rtl");
        let (doc, div) = make_doc(&style, LONG_TEXT);
        if !has_font(&doc, div) {
            eprintln!("skipping: no usable font (text measures 0x0)");
            return;
        }
        assert!(text_layout(&doc, div).text_overflow.is_none());
    }
}
