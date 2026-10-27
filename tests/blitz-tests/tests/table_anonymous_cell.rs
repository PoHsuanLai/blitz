use blitz_test_harness::{Harness, HarnessOptions};

/// A row's child that is not a table cell (a `td` set to `display: block`, as a mobile
/// stylesheet of a newsletter does) is wrapped in an anonymous cell and laid out, not dropped
/// (CSS 2.1 §17.2.1).
#[test]
fn a_block_td_in_a_row_is_laid_out_in_an_anonymous_cell() {
    let harness = Harness::from_html(
        r#"<style>td { display: block }</style>
        <table width="100%" style="border-spacing:0"><tr><td id="c" style="padding:0"><p id="p" style="margin:0">hello</p></td></tr></table>"#,
    );

    let cell = harness.layout_rect("#c");
    let para = harness.layout_rect("#p");
    assert!(para.width > 0.0, "the paragraph has no width");
    assert!(para.height > 0.0, "the paragraph has no height");
    assert_eq!(cell.width, para.width);
}

/// The same rule under a media query that matches the viewport.
#[test]
fn a_block_td_under_a_matching_media_query_is_laid_out() {
    let html = r#"<style>@media (max-width: 590px) { td.column { display: block; width: 100% !important } }</style>
        <table width="100%" style="border-spacing:0"><tr><td class="column" style="padding:0"><p id="p" style="margin:0">hello</p></td></tr></table>"#;
    let narrow = Harness::from_html_with(
        html,
        HarnessOptions {
            width: 500,
            ..Default::default()
        },
    );
    assert!(narrow.layout_rect("#p").width > 0.0);
    let wide = Harness::from_html(html);
    assert!(wide.layout_rect("#p").width > 0.0);
}

/// A row with one block cell and one real cell: both are laid out, the block one beside the
/// other as cells of the row.
#[test]
fn a_block_cell_beside_a_real_cell_keeps_both() {
    let harness = Harness::from_html(
        r#"<table style="border-spacing:0"><tr>
          <td id="a" style="padding:0;display:block"><p id="pa" style="margin:0">one</p></td>
          <td id="b" style="padding:0"><p id="pb" style="margin:0">two</p></td>
        </tr></table>"#,
    );

    assert!(harness.layout_rect("#pa").width > 0.0);
    assert!(harness.layout_rect("#pb").width > 0.0);
    assert!(harness.layout_rect("#b").x > harness.layout_rect("#a").x);
}

/// A block child of a row group (a `tr` set to `display: block`; the parser would lift any other
/// element out of the table) gets an anonymous row, below the rows before it.
#[test]
fn a_block_child_of_a_table_gets_its_own_row() {
    let harness = Harness::from_html(
        r#"<table style="border-spacing:0">
          <tr><td id="first" style="padding:0">x</td></tr>
          <tr id="loose" style="display:block"><td style="padding:0">loose</td></tr>
        </table>"#,
    );

    assert!(harness.layout_rect("#loose").width > 0.0);
    assert!(harness.layout_rect("#loose").y >= harness.layout_rect("#first").y + 1.0);
}

/// A real caption is still a caption: not a cell, and the rows' columns are the cells' alone.
#[test]
fn a_caption_is_not_made_a_cell() {
    let harness = Harness::from_html(
        r#"<table style="border-spacing:0">
          <caption id="cap">title</caption>
          <tr><td id="a" style="padding:0">x</td><td id="b" style="padding:0">y</td></tr>
        </table>"#,
    );

    assert!(harness.layout_rect("#a").width > 0.0);
    assert!(harness.layout_rect("#b").x > harness.layout_rect("#a").x);
    // As before the fix, a caption is left out of the grid.
    assert_eq!(harness.layout_rect("#cap").width, 0.0);
}
