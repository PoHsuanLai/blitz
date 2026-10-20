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
/// holds it (the box that clips: a block, flow root or table cell). `None` for flex and
/// grid containers, to which
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
        DisplayInside::Flow | DisplayInside::FlowRoot | DisplayInside::TableCell
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
        if !layout.lines().any(|line| overflows(&line, content_width)) {
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
                overflows(&line, content_width)
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

/// How far a line may end past its content box and still count as fitting: sizes are
/// rounded to whole CSS pixels at fractional scales, so text that fits can measure a
/// hair wider than the box it sized.
const OVERFLOW_TOLERANCE: f32 = 0.5;

/// Whether `line` ends past `content_width` (both in layout pixels).
fn overflows(line: &parley::Line<'_, TextBrush>, content_width: f32) -> bool {
    line_end(line.metrics()) > content_width + OVERFLOW_TOLERANCE
}

/// The x at which a line's content ends, not counting trailing whitespace, so that a line
/// starting further in (`text-indent`, alignment) is measured where it actually ends.
fn line_end(metrics: &parley::LineMetrics) -> f32 {
    metrics.inline_min_coord + metrics.offset + metrics.advance - metrics.trailing_whitespace
}

/// Keep the longest run of whole clusters at the start of `line` that ends at or before
/// `limit`. Content after an inline box that does not fit is dropped with it.
fn cut_line(line: &parley::Line<'_, TextBrush>, limit: f32) -> TextOverflowCut {
    let mut glyphs_kept = 0;
    let mut marker_x = line.metrics().inline_min_coord + line.metrics().offset;
    // A text run is split into consecutive glyph runs by style (runs and style segments
    // without glyphs yield none), so each run's clusters are measured once, from the
    // offset of its first glyph run, identified by the run's index in the line.
    let mut measured_run = None;

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
                let run = glyph_run.run();
                if measured_run == Some(run.index()) {
                    continue;
                }
                measured_run = Some(run.index());
                let mut x = glyph_run.offset();
                for (advance, glyphs) in cluster_units(run) {
                    let end = x + advance;
                    if end > limit {
                        return TextOverflowCut {
                            glyphs_kept,
                            marker_x,
                        };
                    }
                    glyphs_kept += glyphs;
                    marker_x = end;
                    x = end;
                }
            }
        }
    }

    TextOverflowCut {
        glyphs_kept,
        marker_x,
    }
}

/// The units a run can be cut between, in visual order, as `(advance, glyph count)`: its
/// clusters, except that a ligature and the clusters it continues into form one unit, as
/// its single glyph spans all of their advances.
fn cluster_units<'a>(
    run: &'a parley::Run<'a, TextBrush>,
) -> impl Iterator<Item = (f32, usize)> + 'a {
    let mut clusters = run.visual_clusters().peekable();
    let rtl = run.is_rtl();
    std::iter::from_fn(move || {
        let first = clusters.next()?;
        let mut advance = first.advance();
        let mut glyphs = first.glyphs().count();
        // Visually, continuations follow the ligature left-to-right and precede it
        // right-to-left.
        let mut open = if rtl {
            first.is_ligature_continuation()
        } else {
            first.is_ligature_start()
        };
        while open {
            let Some(next) = clusters.next_if(|next| {
                if rtl {
                    next.is_ligature_continuation() || next.is_ligature_start()
                } else {
                    next.is_ligature_continuation()
                }
            }) else {
                break;
            };
            advance += next.advance();
            glyphs += next.glyphs().count();
            if rtl && next.is_ligature_start() {
                open = false;
            }
        }
        Some((advance, glyphs))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Attribute, DocumentConfig, DocumentMutator, qual_name};
    use blitz_traits::shell::{ColorScheme, Viewport};
    use std::io::Write as _;

    const LONG_TEXT: &str = "A label far too long to fit in its box";

    fn style(value: &str) -> Attribute {
        Attribute {
            name: qual_name!("style"),
            value: value.to_string(),
        }
    }

    /// Lay out `<div style="{div_style}">{text}</div>` at 2x scale and return the document
    /// and the div.
    fn make_doc(div_style: &str, text: &str) -> (BaseDocument, NodeId) {
        make_doc_with(2.0, div_style, |mutator| {
            vec![mutator.create_text_node(text)]
        })
    }

    /// Lay out a div styled `div_style` holding the nodes `children` creates, at `scale`
    /// (the HTML parser lives in blitz-html, which would be a circular dev-dependency),
    /// and return the document and the div.
    fn make_doc_with(
        scale: f32,
        div_style: &str,
        children: impl FnOnce(&mut DocumentMutator<'_>) -> Vec<NodeId>,
    ) -> (BaseDocument, NodeId) {
        let mut doc = BaseDocument::new(DocumentConfig {
            viewport: Some(Viewport::new(800, 600, scale, ColorScheme::Light)),
            ..Default::default()
        });
        let root_id = doc.root_node().id;

        let mut mutator = doc.mutate();
        // No user-agent stylesheet is loaded, so every element is styled as a block here.
        let html = mutator.create_element(qual_name!("html"), vec![style("display:block")]);
        let body = mutator.create_element(qual_name!("body"), vec![style("display:block")]);
        let div_style = format!("display:block; {div_style}");
        let div = mutator.create_element(qual_name!("div"), vec![style(&div_style)]);
        let children = children(&mut mutator);
        mutator.append_children(div, &children);
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

    /// Without a usable font (no `system-fonts` feature, or no fonts installed) text
    /// measures 0x0, which would make these tests vacuous, so they skip. The skip is
    /// written straight to stderr, past the test harness's output capture, so it shows.
    fn no_font(doc: &BaseDocument, div: NodeId) -> bool {
        let skip = text_layout(doc, div).layout.width() == 0.0;
        if skip {
            let test = std::thread::current().name().unwrap_or("test").to_string();
            let _ = writeln!(
                std::io::stderr(),
                "{test}: SKIPPED, no usable font (text measures 0x0)"
            );
        }
        skip
    }

    /// Where the first `glyphs_kept` glyphs of `line` end, walking its glyph runs as paint
    /// draws them.
    fn kept_end(line: &parley::Line<'_, TextBrush>, glyphs_kept: usize) -> f32 {
        let mut left = glyphs_kept;
        let mut end = line.metrics().inline_min_coord + line.metrics().offset;
        for item in line.items() {
            if let PositionedLayoutItem::GlyphRun(run) = item {
                let mut x = run.offset();
                for glyph in run.glyphs() {
                    if left == 0 {
                        return end;
                    }
                    x += glyph.advance;
                    end = x;
                    left -= 1;
                }
            }
        }
        end
    }

    /// The only line's cut, checking that the kept glyphs end where the marker starts and
    /// the marker fits in `content_width`.
    fn checked_cut(doc: &BaseDocument, div: NodeId, content_width: f32) -> TextOverflowCut {
        let text_layout = text_layout(doc, div);
        let marker = text_layout.text_overflow.as_ref().expect("a marker");
        let cut = marker.lines[0].expect("the line is cut");
        let line = text_layout.layout.lines().next().unwrap();
        let end = kept_end(&line, cut.glyphs_kept);
        assert!(
            (end - cut.marker_x).abs() < 0.01,
            "kept glyphs end at {end}, marker at {}",
            cut.marker_x
        );
        assert!(cut.marker_x + marker.layout.width() <= content_width);
        cut
    }

    const ELLIPSIS: &str =
        "width:100px; padding:0 7px; overflow:hidden; white-space:nowrap; text-overflow:ellipsis";

    #[test]
    fn overflowing_line_ends_with_the_ellipsis_and_fits() {
        let (doc, div) = make_doc(ELLIPSIS, LONG_TEXT);
        if no_font(&doc, div) {
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
        checked_cut(&doc, div, content_width);
    }

    #[test]
    fn text_sized_to_fit_at_fractional_scales_has_no_marker() {
        let inner_style = "display:block; overflow:hidden; white-space:nowrap; text-overflow:ellipsis; font-size:13px";
        for scale in [1.25, 1.5, 1.75] {
            // Sized by its content, as a flex item.
            let (doc, outer) = make_doc_with(scale, "display:flex", |mutator| {
                let inner = mutator.create_element(qual_name!("div"), vec![style(inner_style)]);
                let text = mutator.create_text_node(LONG_TEXT);
                mutator.append_children(inner, &[text]);
                vec![inner]
            });
            let inner = doc.nodes[outer].children[0];
            if no_font(&doc, inner) {
                return;
            }
            let fitted = text_layout(&doc, inner);
            assert!(fitted.text_overflow.is_none(), "scale {scale}");

            // Given a width a fraction of a device pixel narrower than the text, as rounding
            // at a fractional scale can, it still fits; a whole pixel narrower, it does not.
            let text_width = fitted.layout.width();
            for (shortfall, cut) in [(0.3, false), (2.0, true)] {
                let width = (text_width - shortfall) / scale;
                let div_style = format!("width:{width}px; {inner_style}");
                let (doc, div) = make_doc_with(scale, &div_style, |mutator| {
                    vec![mutator.create_text_node(LONG_TEXT)]
                });
                assert_eq!(
                    text_layout(&doc, div).text_overflow.is_some(),
                    cut,
                    "scale {scale}, {shortfall} device px short"
                );
            }
        }
    }

    #[test]
    fn several_runs_and_an_empty_span_on_one_line_are_cut_where_paint_draws() {
        let div_style = "width:250px; overflow:hidden; white-space:nowrap; text-overflow:ellipsis";
        let (doc, div) = make_doc_with(2.0, div_style, |mutator| {
            let before = mutator.create_text_node("Plain ");
            let bold = mutator.create_element(qual_name!("b"), vec![style("font-weight:bold")]);
            let bold_text = mutator.create_text_node("bold");
            mutator.append_children(bold, &[bold_text]);
            let empty = mutator.create_element(qual_name!("span"), vec![]);
            let big = mutator.create_element(qual_name!("span"), vec![style("font-size:20px")]);
            let big_text = mutator.create_text_node(" bigger");
            mutator.append_children(big, &[big_text]);
            let after = mutator.create_text_node(" and plain text that runs well past the box");
            vec![before, bold, empty, big, after]
        });
        if no_font(&doc, div) {
            return;
        }
        let cut = checked_cut(&doc, div, 250.0 * 2.0);
        assert!(cut.glyphs_kept > "Plain bold bigger".len());
    }

    #[test]
    fn a_ligature_is_kept_or_dropped_whole() {
        // Wherever the cut lands, the kept glyphs (a ligature glyph spans every cluster it
        // joins) end where the marker starts. Fonts without these ligatures pass trivially.
        let text = "office affine fluffier waffle offline effigy fjord";
        for width in (40..=160).step_by(6) {
            let style = format!(
                "width:{width}px; overflow:hidden; white-space:nowrap; text-overflow:ellipsis"
            );
            let (doc, div) =
                make_doc_with(2.0, &style, |mutator| vec![mutator.create_text_node(text)]);
            if no_font(&doc, div) {
                return;
            }
            checked_cut(&doc, div, width as f32 * 2.0);
        }
    }

    #[test]
    fn an_anonymous_root_in_a_table_cell_takes_the_cells_marker() {
        let div_style = "display:table-cell; width:100px; overflow:hidden; white-space:nowrap; text-overflow:ellipsis";
        let (doc, div) = make_doc_with(2.0, div_style, |mutator| {
            let text = mutator.create_text_node(LONG_TEXT);
            let block = mutator.create_element(qual_name!("div"), vec![style("display:block")]);
            vec![text, block]
        });
        let cell = &doc.nodes[div];
        let anon = cell
            .layout_children
            .borrow()
            .as_ref()
            .and_then(|children| {
                children
                    .iter()
                    .copied()
                    .find(|id| doc.nodes[*id].is_anonymous())
            })
            .expect("the text is wrapped in an anonymous block");
        if no_font(&doc, anon) {
            return;
        }
        assert_eq!(style_node(&doc, anon), Some(div));
    }

    #[test]
    fn text_that_fits_has_no_marker() {
        let (doc, div) = make_doc(ELLIPSIS, "Short");
        if no_font(&doc, div) {
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
            if no_font(&doc, div) {
                return;
            }
            assert!(text_layout(&doc, div).text_overflow.is_none(), "{style}");
        }
    }

    #[test]
    fn string_marker_is_laid_out() {
        let style = "width:100px; overflow:hidden; white-space:nowrap; text-overflow:'>>'";
        let (doc, div) = make_doc(style, LONG_TEXT);
        if no_font(&doc, div) {
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
        if no_font(&doc, div) {
            return;
        }
        assert!(text_layout(&doc, div).text_overflow.is_none());
    }
}
