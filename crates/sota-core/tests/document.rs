use sota_core::{Cell, Channel, Document, Table};

fn cell(row: usize, col: usize, row_span: usize, col_span: usize, text: &str) -> Cell {
    Cell {
        row,
        col,
        row_span,
        col_span,
        text: text.to_owned(),
        bold: false,
        header: row < 2,
        cites: Vec::new(),
    }
}

/// | Method | FB15k-237 (spans 2) |
/// |        | MR  | MRR           |
/// | RotatE | 177 | **.338**      |
fn table() -> Table {
    let mut best = cell(2, 2, 1, 1, ".338");
    best.bold = true;
    Table {
        id: "S4.T5".into(),
        figure_id: "S4.T5".into(),
        label: "Table 5".into(),
        caption: "Results on FB15k-237.".into(),
        cells: vec![
            cell(0, 0, 2, 1, "Method"),
            cell(0, 1, 1, 2, "FB15k-237"),
            cell(1, 1, 1, 1, "MR"),
            cell(1, 2, 1, 1, "MRR"),
            cell(2, 0, 1, 1, "RotatE"),
            cell(2, 1, 1, 1, "177"),
            best,
        ],
        conversion_errors: false,
    }
}

fn doc() -> Document {
    Document {
        arxiv_id: "1902.10197".into(),
        version: 1,
        channel: Channel::ArxivHtml,
        title: "RotatE".into(),
        sections: Vec::new(),
        tables: vec![table()],
        unparsed_tables: Vec::new(),
    }
}

#[test]
fn grid_expands_spans_to_their_origin_cell() {
    let t = table();
    assert_eq!((t.n_rows(), t.n_cols()), (3, 3));
    let g = t.grid();
    assert_eq!(g[1][0].unwrap().text, "Method"); // covered by the row span
    assert_eq!(g[0][2].unwrap().text, "FB15k-237"); // covered by the column span
    assert_eq!(g[2][2].unwrap().text, ".338");
    assert_eq!(t.holes(), 0);
}

#[test]
fn holes_count_uncovered_slots() {
    let mut t = table();
    t.cells.retain(|c| c.text != "177");
    assert_eq!(t.holes(), 1);
    assert!(t.grid()[2][1].is_none());
}

#[test]
fn locators_and_urls_point_into_the_pinned_version() {
    let d = doc();
    let t = &d.tables[0];
    assert_eq!(d.locator(t, 2, 2), "1902.10197v1#S4.T5/r2/c2");
    assert_eq!(d.table_url(t), "https://arxiv.org/html/1902.10197v1#S4.T5");
    assert_eq!(d.versioned_id(), "1902.10197v1");
}

#[test]
fn markdown_shows_each_cell_once_with_bold() {
    let md = table().to_markdown();
    let lines: Vec<&str> = md.lines().collect();
    assert_eq!(lines[0], "| Method | FB15k-237 |  |");
    assert_eq!(lines[1], "| --- | --- | --- |");
    assert_eq!(lines[2], "|  | MR | MRR |");
    assert_eq!(lines[3], "| RotatE | 177 | **.338** |");
}

#[test]
fn markdown_escapes_pipes() {
    let mut t = table();
    t.cells[4].text = "A|B".into();
    assert!(t.to_markdown().contains("A\\|B"));
}
