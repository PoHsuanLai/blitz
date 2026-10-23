use blitz_test_harness::Harness;

/// In the automatic table layout algorithm a cell's `width` is its column's width unless the
/// cell's content cannot fit in it: the column is never narrower than the cell's min-content
/// width, nor than the width given.
#[test]
fn an_auto_table_cell_is_at_least_its_width_and_its_min_content_width() {
    let harness = Harness::from_html(
        r#"<div style="width:200px">
        <table style="border-spacing:0"><tr>
          <td id="block" style="width:46px;padding:0"><span style="display:inline-block;width:100px;height:10px"></span></td>
          <td id="after-block" style="padding:0">x</td>
        </tr></table>
        <table style="border-spacing:0"><tr>
          <td id="empty" style="width:60px;padding:0"></td>
          <td id="after-empty" style="padding:0">x</td>
        </tr></table>
        <table style="border-spacing:0"><tr>
          <td id="wide" style="width:300px;padding:0">x</td>
          <td id="after-wide" style="padding:0">x</td>
        </tr></table>
        </div>"#,
    );

    // Its content is wider than its width: the column holds the content.
    assert_eq!(harness.layout_rect("#block").width, 100.0);
    assert_eq!(
        harness.layout_rect("#after-block").x - harness.layout_rect("#block").x,
        100.0
    );
    // Its content is narrower: the column is the width given.
    assert_eq!(harness.layout_rect("#empty").width, 60.0);
    assert_eq!(
        harness.layout_rect("#after-empty").x - harness.layout_rect("#empty").x,
        60.0
    );
    // Wider than the space the table has: the table overflows, as browsers' do.
    assert_eq!(harness.layout_rect("#wide").width, 300.0);
    assert_eq!(
        harness.layout_rect("#after-wide").x - harness.layout_rect("#wide").x,
        300.0
    );
}

/// A sentence in a cell with a width wraps at that width rather than widening its column.
#[test]
fn an_auto_table_cell_wraps_its_text_at_its_width() {
    let harness = Harness::from_html(
        r#"<table style="border-spacing:0"><tr>
          <td id="text" style="width:120px;padding:0;font-size:10px">the quick brown fox jumps over the lazy dog</td>
          <td style="padding:0">x</td>
        </tr></table>"#,
    );

    assert_eq!(harness.layout_rect("#text").width, 120.0);
}
