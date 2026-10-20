//! Tables as data: [`NodeRef::table_rows`] and [`to_csv`].

use alloc::string::String;
use alloc::vec::Vec;

use crate::node::NodeId;
use crate::serialize::walk;
use crate::tags::{self, Display};
use crate::tree::NodeRef;

/// `colspan` and `rowspan` above this are clamped (HTML clamps `colspan` at
/// 1000 too).
const MAX_SPAN: usize = 1000;

/// Cells that spans (and the padding a `rowspan` forces into a short row) may
/// add beyond the table's own cells: this plus [`SPAN_FACTOR`] per cell.
const SPAN_CELLS: usize = 100_000;

/// Bytes of cell text that spans may duplicate: this plus [`SPAN_FACTOR`]
/// times the table's own text.
const SPAN_BYTES: usize = 1 << 22;

const SPAN_FACTOR: usize = 4;

impl NodeRef<'_> {
    /// The rows of a `<table>` as cell text, with `colspan` and `rowspan`
    /// expanded — ready for CSV ([`to_csv`]), a data frame or an LLM prompt.
    ///
    /// Rows come from the table's `<thead>`, `<tbody>`, `<tfoot>` and direct
    /// `<tr>`s in document order (stray cells outside any row form a row of
    /// their own). Each cell is its readable text ([`NodeRef::to_text`]), so
    /// a nested table's content appears as text inside its cell, but its rows
    /// are not mixed into this table's. Returns an empty list if this node is
    /// not a `<table>`.
    ///
    /// A cell spanning several columns or rows is **repeated** in every slot
    /// it covers, so each row lists the value under each column. `rowspan=0`
    /// spans the rest of the row group; spans stop at the end of their row
    /// group, as in HTML. Rows are not padded to equal length, except where a
    /// `rowspan` from an earlier row reaches past a row's own cells.
    ///
    /// Bounded work on hostile input: spans are clamped at 1000, and once
    /// the spans of a table have added about four times the table's own
    /// cells and text (plus a fixed allowance of 100,000 cells and 4 MiB),
    /// further spans count as 1.
    ///
    /// ```
    /// let dom = domtree::parse(
    ///     "<table>
    ///        <thead><tr><th>Region</th><th>Q1</th><th>Q2</th></tr></thead>
    ///        <tbody>
    ///          <tr><td rowspan=2>EU</td><td>10</td><td>12</td></tr>
    ///          <tr><td colspan=2>n/a</td></tr>
    ///        </tbody>
    ///      </table>",
    /// );
    /// let table = dom.find_by_tag("table").next().unwrap();
    /// assert_eq!(table.table_rows(), [
    ///     ["Region", "Q1", "Q2"],
    ///     ["EU", "10", "12"],
    ///     ["EU", "n/a", "n/a"],
    /// ]);
    /// ```
    pub fn table_rows(&self) -> Vec<Vec<String>> {
        if self.tag_name() != Some("table") {
            return Vec::new();
        }
        let rows = collect_rows(*self);

        let mut texts: Vec<Vec<String>> = Vec::with_capacity(rows.len());
        let (mut cells, mut bytes) = (0usize, 0usize);
        for row in &rows {
            let row_texts: Vec<String> = row.cells.iter().map(NodeRef::to_text).collect();
            cells = cells.saturating_add(row_texts.len());
            bytes = row_texts
                .iter()
                .fold(bytes, |b, t| b.saturating_add(t.len()));
            texts.push(row_texts);
        }
        let mut budget = Budget {
            cells: SPAN_CELLS.saturating_add(cells.saturating_mul(SPAN_FACTOR)),
            bytes: SPAN_BYTES.saturating_add(bytes.saturating_mul(SPAN_FACTOR)),
        };

        // Where each row's group ends (for `rowspan=0` and span cut-off).
        let mut group_end = alloc::vec![rows.len(); rows.len()];
        for i in (0..rows.len().saturating_sub(1)).rev() {
            let same = rows.get(i).map(|r| r.group) == rows.get(i + 1).map(|r| r.group);
            let next = group_end.get(i + 1).copied().unwrap_or(rows.len());
            if let Some(end) = group_end.get_mut(i) {
                *end = if same { next } else { i + 1 };
            }
        }

        let mut out: Vec<Vec<String>> = Vec::with_capacity(rows.len());
        // Per column: rows still covered by a `rowspan`, and the cell's text.
        let mut pending: Vec<Option<(usize, String)>> = Vec::new();
        let mut group = None;
        for (i, (row, row_texts)) in rows.iter().zip(texts).enumerate() {
            if group != Some(row.group) {
                pending.clear();
                group = Some(row.group);
            }
            let rows_left = group_end.get(i).copied().unwrap_or(i + 1).saturating_sub(i);
            let mut line: Vec<String> = Vec::with_capacity(row_texts.len());
            for (cell, text) in row.cells.iter().zip(row_texts) {
                take_pending(&mut line, &mut pending);
                let colspan = span(cell.attr("colspan")).clamp(1, MAX_SPAN);
                let rowspan = match span(cell.attr("rowspan")) {
                    0 => rows_left,
                    n => n.min(rows_left),
                }
                .clamp(1, MAX_SPAN);
                let (colspan, rowspan) = budget.allow(colspan, rowspan, text.len());
                for _ in 0..colspan {
                    let col = line.len();
                    if pending.len() <= col && rowspan > 1 {
                        pending.resize(col + 1, None);
                    }
                    // A cell overlapping an earlier `rowspan` takes the column.
                    if let Some(slot) = pending.get_mut(col) {
                        *slot = (rowspan > 1).then(|| (rowspan - 1, text.clone()));
                    }
                    line.push(text.clone());
                }
            }
            take_pending(&mut line, &mut pending);
            // A `rowspan` beyond this row's own cells: pad the gap.
            if let Some(last) = pending.iter().rposition(Option::is_some) {
                while line.len() <= last {
                    if pending.get(line.len()).is_some_and(Option::is_some) {
                        take_pending(&mut line, &mut pending);
                    } else if budget.cells > 0 {
                        budget.cells -= 1;
                        line.push(String::new());
                    } else {
                        pending.clear();
                        break;
                    }
                }
            }
            let used = pending
                .iter()
                .rposition(Option::is_some)
                .map_or(0, |i| i + 1);
            pending.truncate(used);
            out.push(line);
        }
        out
    }
}

/// Format rows as CSV ([RFC 4180](https://www.rfc-editor.org/rfc/rfc4180)):
/// fields separated by commas, a field quoted (with `"` doubled) when it
/// contains a comma, a quote or a line break, and every record ended by
/// `\r\n`. Rows may have different lengths.
///
/// Takes anything iterable of iterables of strings — such as the
/// [`NodeRef::table_rows`] of a table, by value or by reference.
///
/// ```
/// let dom = domtree::parse(
///     r#"<table><tr><th>Name<th>Note<tr><td>Ann<td>says "hi", twice</table>"#,
/// );
/// let rows = dom.find_by_tag("table").next().unwrap().table_rows();
/// assert_eq!(
///     domtree::to_csv(&rows),
///     "Name,Note\r\nAnn,\"says \"\"hi\"\", twice\"\r\n",
/// );
/// assert_eq!(domtree::to_csv([["a", "b\nc"]]), "a,\"b\nc\"\r\n");
/// ```
pub fn to_csv<R, C, S>(rows: R) -> String
where
    R: IntoIterator<Item = C>,
    C: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out = String::new();
    for row in rows {
        for (i, field) in row.into_iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let field = field.as_ref();
            if field.contains([',', '"', '\r', '\n']) {
                out.push('"');
                for c in field.chars() {
                    if c == '"' {
                        out.push('"');
                    }
                    out.push(c);
                }
                out.push('"');
            } else {
                out.push_str(field);
            }
        }
        out.push_str("\r\n");
    }
    out
}

/// One `<tr>` (or run of stray cells): its row group and its cells.
struct Row<'a> {
    group: NodeId,
    cells: Vec<NodeRef<'a>>,
}

/// The table's rows in document order, without descending into nested tables
/// or into cells.
fn collect_rows(table: NodeRef<'_>) -> Vec<Row<'_>> {
    let mut rows: Vec<Row<'_>> = Vec::new();
    // Open `<thead>`/`<tbody>`/`<tfoot>` and `<tr>` elements (`tr` with the
    // index of its row).
    let mut groups: Vec<NodeId> = Vec::new();
    let mut open: Vec<(NodeId, usize)> = Vec::new();
    // The implicit row collecting stray cells, if one is open.
    let mut stray: Option<usize> = None;
    walk(table, |node, enter| {
        let Some(tag) = node.tag_name() else {
            return false;
        };
        if node.id() == table.id() {
            return true;
        }
        let group = groups.last().copied().unwrap_or(table.id());
        if !enter {
            match tag {
                "tr" if open.last().is_some_and(|&(id, _)| id == node.id()) => {
                    open.pop();
                }
                "thead" | "tbody" | "tfoot" if groups.last() == Some(&node.id()) => {
                    groups.pop();
                    stray = None;
                }
                _ => {}
            }
            return false;
        }
        match tag {
            "table" => false,
            "tr" => {
                open.push((node.id(), rows.len()));
                rows.push(Row {
                    group,
                    cells: Vec::new(),
                });
                stray = None;
                true
            }
            "thead" | "tbody" | "tfoot" => {
                groups.push(node.id());
                stray = None;
                true
            }
            "td" | "th" => {
                let index = match open.last() {
                    Some(&(_, index)) => index,
                    None => *stray.get_or_insert_with(|| {
                        rows.push(Row {
                            group,
                            cells: Vec::new(),
                        });
                        rows.len() - 1
                    }),
                };
                if let Some(row) = rows.get_mut(index) {
                    row.cells.push(node);
                }
                false
            }
            _ => tags::display(tag) != Display::Hidden,
        }
    });
    rows
}

/// Fill `line` from the `rowspan`s covering its next columns.
fn take_pending(line: &mut Vec<String>, pending: &mut [Option<(usize, String)>]) {
    while let Some(slot) = pending.get_mut(line.len()) {
        match slot {
            Some((left, text)) if *left > 1 => {
                *left -= 1;
                line.push(text.clone());
            }
            Some(_) => {
                if let Some((_, text)) = slot.take() {
                    line.push(text);
                }
            }
            None => break,
        }
    }
}

/// A `colspan`/`rowspan` value, by the HTML rules for non-negative integers
/// (leading digits; `"2px"` is 2). Missing or invalid: 1.
fn span(value: Option<&str>) -> usize {
    let Some(value) = value else {
        return 1;
    };
    let digits = value.trim_start().bytes().take_while(u8::is_ascii_digit);
    let mut n: usize = 0;
    let mut any = false;
    for d in digits {
        any = true;
        n = n.saturating_mul(10).saturating_add(usize::from(d - b'0'));
    }
    if any { n } else { 1 }
}

/// What spans may still add to a table.
struct Budget {
    cells: usize,
    bytes: usize,
}

impl Budget {
    /// The spans to apply for a cell with `len` bytes of text: as asked, or
    /// `(1, 1)` once the budget cannot cover the copies.
    fn allow(&mut self, colspan: usize, rowspan: usize, len: usize) -> (usize, usize) {
        let extra = colspan.saturating_mul(rowspan).saturating_sub(1);
        let bytes = extra.saturating_mul(len);
        if extra <= self.cells && bytes <= self.bytes {
            self.cells -= extra;
            self.bytes -= bytes;
            (colspan, rowspan)
        } else {
            (1, 1)
        }
    }
}
