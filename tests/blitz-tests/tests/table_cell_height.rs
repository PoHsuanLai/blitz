use blitz_test_harness::Harness;

/// A cell's `height` is the least its row is, not a fixed height: a row grows to hold what its
/// cells hold, and the rows after it start below, as browsers lay a table out (CSS 2.1 §17.5.3).
#[test]
fn a_table_cell_taller_than_its_height_grows_its_row() {
    let harness = Harness::from_html(
        r#"<table style="border-spacing:0">
          <tr><td id="tall" height="50" style="padding:0"><div style="height:300px"></div></td></tr>
          <tr><td id="after" style="padding:0">x</td></tr>
        </table>"#,
    );

    assert_eq!(harness.layout_rect("#tall").height, 300.0);
    assert_eq!(
        harness.layout_rect("#after").y - harness.layout_rect("#tall").y,
        300.0
    );
}

/// A cell whose content is shorter than its `height` is that tall, as before.
#[test]
fn a_table_cell_shorter_than_its_height_keeps_it() {
    let harness = Harness::from_html(
        r#"<table style="border-spacing:0">
          <tr><td id="short" style="height:80px;padding:0">x</td></tr>
          <tr><td id="after" style="padding:0">x</td></tr>
        </table>"#,
    );

    assert_eq!(harness.layout_rect("#short").height, 80.0);
    assert_eq!(
        harness.layout_rect("#after").y - harness.layout_rect("#short").y,
        80.0
    );
}
