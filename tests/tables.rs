//! `NodeRef::table_rows` and `to_csv`.

use domtree::{parse, to_csv};

fn rows(html: &str) -> Vec<Vec<String>> {
    let dom = parse(html);
    let table = dom.find_by_tag("table").next().expect("a table");
    table.table_rows()
}

macro_rules! grid {
    ($([$($cell:expr),* $(,)?]),* $(,)?) => {
        vec![$(vec![$($cell.to_string()),*]),*] as Vec<Vec<String>>
    };
}

#[test]
fn simple_table() {
    assert_eq!(
        rows("<table><tr><th>a</th><th>b</th></tr><tr><td>1</td><td>2</td></tr></table>"),
        grid![["a", "b"], ["1", "2"]]
    );
    // Omitted end tags, as in real pages.
    assert_eq!(
        rows("<table><tr><td>1<td>2<tr><td>3<td>4</table>"),
        grid![["1", "2"], ["3", "4"]]
    );
}

#[test]
fn sections_in_document_order() {
    assert_eq!(
        rows(
            "<table><caption>Ignored caption</caption>\
             <thead><tr><th>h</th></tr></thead>\
             <tbody><tr><td>b1</td></tr><tr><td>b2</td></tr></tbody>\
             <tr><td>direct</td></tr>\
             <tfoot><tr><td>f</td></tr></tfoot></table>"
        ),
        grid![["h"], ["b1"], ["b2"], ["direct"], ["f"]]
    );
}

#[test]
fn colspan_and_rowspan_are_expanded() {
    assert_eq!(
        rows(
            "<table>\
             <tr><td rowspan=2>A</td><td colspan=2>B</td></tr>\
             <tr><td>C</td><td>D</td></tr>\
             <tr><td colspan=3>E</td></tr>\
             </table>"
        ),
        grid![["A", "B", "B"], ["A", "C", "D"], ["E", "E", "E"]]
    );
    // A block spanning both ways, in the middle of a row.
    assert_eq!(
        rows(
            "<table>\
             <tr><td>1</td><td rowspan=2 colspan=2>X</td><td>2</td></tr>\
             <tr><td>3</td><td>4</td></tr>\
             </table>"
        ),
        grid![["1", "X", "X", "2"], ["3", "X", "X", "4"]]
    );
}

#[test]
fn rowspan_past_a_short_row_pads_the_gap() {
    assert_eq!(
        rows("<table><tr><td>a</td><td>b</td><td rowspan=2>R</td></tr><tr><td>c</td></tr></table>"),
        grid![["a", "b", "R"], ["c", "", "R"]]
    );
}

#[test]
fn rows_are_not_padded_otherwise() {
    assert_eq!(
        rows("<table><tr><td>1<td>2<td>3<tr><td>4</table>"),
        grid![["1", "2", "3"], ["4"]]
    );
}

#[test]
fn spans_stop_at_the_row_group() {
    assert_eq!(
        rows(
            "<table><thead><tr><th rowspan=5>H</th><th>x</th></tr></thead>\
             <tbody><tr><td>1</td><td>2</td></tr></tbody></table>"
        ),
        grid![["H", "x"], ["1", "2"]]
    );
    // rowspan=0: to the end of the row group.
    assert_eq!(
        rows(
            "<table><tbody><tr><td rowspan=0>Z</td><td>1</td></tr><tr><td>2</td></tr><tr><td>3</td></tr></tbody>\
             <tbody><tr><td>4</td></tr></tbody></table>"
        ),
        grid![["Z", "1"], ["Z", "2"], ["Z", "3"], ["4"]]
    );
}

#[test]
fn span_values_follow_html_parsing_rules() {
    assert_eq!(
        rows(
            "<table><tr><td colspan='2px'>a</td><td colspan=0>b</td><td colspan=-3>c</td><td colspan=x>d</td><td colspan=' 2'>e</td></tr></table>"
        ),
        grid![["a", "a", "b", "c", "d", "e", "e"]]
    );
    // Huge spans are clamped at 1000.
    let r = rows("<table><tr><td colspan=99999999999999999999999>w</td></tr></table>");
    assert_eq!(r[0].len(), 1000);
}

#[test]
fn cells_are_readable_text() {
    assert_eq!(
        rows(
            "<table><tr><td>  a   <b>bold</b>\n </td><td>line<br>break</td>\
             <td><script>x()</script>visible</td><td><p>p1</p><p>p2</p></td></tr></table>"
        ),
        grid![["a bold", "line\nbreak", "visible", "p1\n\np2"]]
    );
}

#[test]
fn nested_tables_stay_inside_their_cell() {
    let html = "<table id=outer><tr><td>o1</td><td><table><tr><td>i1</td><td>i2</td></tr><tr><td>i3</td></tr></table></td></tr>\
                <tr><td>o2</td></tr></table>";
    let dom = parse(html);
    let outer = dom.find_by_id("outer").unwrap();
    assert_eq!(outer.table_rows(), grid![["o1", "i1\ti2\ni3"], ["o2"]]);
    let inner = dom.find_by_tag("table").nth(1).unwrap();
    assert_eq!(inner.table_rows(), grid![["i1", "i2"], ["i3"]]);
}

#[test]
fn stray_cells_form_a_row() {
    assert_eq!(rows("<table><td>a<td>b</table>"), grid![["a", "b"]]);
    assert_eq!(
        rows("<table><td>a</td><tr><td>b</td></tr><td>c</td></table>"),
        grid![["a"], ["b"], ["c"]]
    );
}

#[test]
fn wrappers_inside_tables_are_looked_through() {
    // Forms and other junk between table parts are common in old markup.
    assert_eq!(
        rows("<table><form><tr><td>a</td></tr></form><div><tr><td>b</td></tr></div></table>"),
        grid![["a"], ["b"]]
    );
    // Hidden parts are not.
    assert_eq!(
        rows("<table><template><tr><td>t</td></tr></template><tr><td>v</td></tr></table>"),
        grid![["v"]]
    );
}

#[test]
fn non_tables_have_no_rows() {
    let dom = parse("<div><tr><td>x</td></tr></div><table></table>");
    assert!(dom.root().unwrap().table_rows().is_empty());
    assert!(
        dom.find_by_tag("tr")
            .next()
            .unwrap()
            .table_rows()
            .is_empty()
    );
    assert!(
        dom.find_by_tag("table")
            .next()
            .unwrap()
            .table_rows()
            .is_empty()
    );
}

#[test]
fn page_fixture_table() {
    let dom = parse(include_str!("pages/blog.html"));
    let table = dom.find_by_tag("table").next().unwrap();
    assert_eq!(
        table.table_rows(),
        grid![
            ["Parser", "Size (gzip)", "Deps"],
            ["dom-tree-rs", "19.5 KB", "0"],
            ["html5ever", "~180 KB", "12"],
            ["a | b", "pipes are escaped", "pipes are escaped"],
        ]
    );
}

#[test]
fn spans_are_budgeted() {
    // One cell asking for a million slots, many times over: the expansion
    // budget runs out and the output stays proportional to the input.
    let cell = "<td colspan=1000 rowspan=1000>xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx</td>";
    let html = format!(
        "<table>{}</table>",
        format!("<tr>{}</tr>", cell.repeat(10)).repeat(200)
    );
    let r = rows(&html);
    assert_eq!(r.len(), 200);
    let cells: usize = r.iter().map(Vec::len).sum();
    let bytes: usize = r.iter().flatten().map(String::len).sum();
    assert!(cells < 400_000, "{cells} cells");
    assert!(bytes < 32 << 20, "{bytes} bytes");
}

#[test]
fn wide_rowspans_are_budgeted() {
    // A rowspan far to the right forces padding into every following row.
    let html = format!(
        "<table><tr>{}<td rowspan=1000>r</td></tr>{}</table>",
        "<td>w</td>".repeat(5_000),
        "<tr><td>x</td></tr>".repeat(1_000)
    );
    let r = rows(&html);
    let cells: usize = r.iter().map(Vec::len).sum();
    assert!(cells < 200_000, "{cells} cells");
}

#[test]
fn overlapping_spans_do_not_panic() {
    let r = rows(
        "<table><tr><td rowspan=3>a</td><td>b</td></tr>\
         <tr><td colspan=3>c</td></tr><tr><td rowspan=2 colspan=2>d</td></tr><tr></tr></table>",
    );
    assert_eq!(r.len(), 4);
}

#[test]
fn deep_and_wide_tables() {
    let depth = 50_000;
    let mut html = "<table><tr><td>".repeat(depth);
    html.push_str("core");
    let dom = parse(&html);
    // Each nested table's text is part of its parent's single cell.
    let outer = dom.find_by_tag("table").next().unwrap();
    assert_eq!(outer.table_rows().len(), 1);
    let inner = dom.find_by_tag("table").last().unwrap();
    assert_eq!(inner.table_rows(), grid![["core"]]);
    let html = format!("<table>{}</table>", "<tr><td>a<td>b</tr>".repeat(50_000));
    assert_eq!(rows(&html).len(), 50_000);
}

// --- CSV ---------------------------------------------------------------------

#[test]
fn csv_quoting() {
    assert_eq!(
        to_csv([[
            "plain",
            "with,comma",
            "with \"quote\"",
            "line\nbreak",
            "cr\r",
            ""
        ]]),
        "plain,\"with,comma\",\"with \"\"quote\"\"\",\"line\nbreak\",\"cr\r\",\r\n"
    );
}

#[test]
fn csv_rows() {
    let rows: Vec<Vec<String>> = vec![vec!["a".into(), "b".into()], vec![], vec!["c".into()]];
    assert_eq!(to_csv(&rows), "a,b\r\n\r\nc\r\n");
    assert_eq!(to_csv(rows), "a,b\r\n\r\nc\r\n");
    assert_eq!(to_csv(Vec::<Vec<&str>>::new()), "");
}

#[test]
fn table_to_csv() {
    let dom = parse(include_str!("pages/blog.html"));
    let table = dom.find_by_tag("table").next().unwrap();
    assert_eq!(
        to_csv(table.table_rows()),
        "Parser,Size (gzip),Deps\r\ndom-tree-rs,19.5 KB,0\r\nhtml5ever,~180 KB,12\r\na | b,pipes are escaped,pipes are escaped\r\n"
    );
}
