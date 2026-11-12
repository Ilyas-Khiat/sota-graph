use sota_core::{Channel, Document, Table};
use sota_ingest::arxiv_html::{generator, parse, stated_version};

const FIXTURE: &str = include_str!("fixtures/latexml_tables.html");

fn doc() -> Document {
    parse(FIXTURE, "2401.01234", 3)
}

fn table<'a>(doc: &'a Document, id: &str) -> &'a Table {
    doc.table(id)
        .unwrap_or_else(|| panic!("no table {id}; have {:?}", ids(doc)))
}

fn ids(doc: &Document) -> Vec<&str> {
    doc.tables.iter().map(|t| t.id.as_str()).collect()
}

fn text(t: &Table, row: usize, col: usize) -> String {
    t.grid()[row][col]
        .map(|c| c.text.clone())
        .unwrap_or_else(|| "<hole>".into())
}

#[test]
fn document_identity_and_title() {
    let d = doc();
    assert_eq!(d.versioned_id(), "2401.01234v3");
    assert_eq!(d.channel, Channel::ArxivHtml);
    assert_eq!(d.title, "A Synthetic Paper on Link Prediction");
}

#[test]
fn finds_result_tables_only() {
    // No author block (p1.1), no equation (S4.E1), no nested panel as its own table.
    assert_eq!(
        ids(&doc()),
        [
            "S4.T1",
            "S4.T2",
            "S4.T3.t1",
            "S4.T3.t2",
            "S4.T5",
            "S4.T6",
            "S4.SS3.tab1.t1",
            "S4.SS3.tab1.t2",
            "S4.SS3.tab1.t3"
        ]
    );
}

#[test]
fn figures_without_a_table_are_reported() {
    assert_eq!(doc().unparsed_tables, ["S4.T4"]);
}

#[test]
fn caption_is_split_from_its_label() {
    let d = doc();
    let t = table(&d, "S4.T1");
    assert_eq!(t.label, "Table 1");
    assert_eq!(
        t.caption,
        "Results on FB15k-237 and WN18RR. Baselines are taken from [3]."
    );
    // A sub-panel keeps the figure's label; its sub-caption is appended.
    assert_eq!(table(&d, "S4.T6").label, "Table 6");
    assert_eq!(
        table(&d, "S4.T6").caption,
        "Outer caption. (a) Panel caption."
    );
}

#[test]
fn each_table_takes_its_own_caption_when_a_figure_holds_several() {
    let d = doc();
    let label_caption = |id: &str| {
        let t = table(&d, id);
        (t.label.clone(), t.caption.clone())
    };
    assert_eq!(
        label_caption("S4.SS3.tab1.t1"),
        ("Table 7".into(), "All queries.".into())
    );
    assert_eq!(
        label_caption("S4.SS3.tab1.t2"),
        ("Table 8".into(), "Tail prediction only.".into())
    );
    // The "Figure 2" caption in between is not a table caption.
    assert_eq!(
        label_caption("S4.SS3.tab1.t3"),
        ("Table 9".into(), "Large graph.".into())
    );
    // One shared caption still goes to every table of its figure.
    assert_eq!(table(&d, "S4.T3.t2").label, "Table 3");
}

#[test]
fn spans_are_laid_out_on_a_full_grid() {
    let d = doc();
    let t = table(&d, "S4.T1");
    assert_eq!((t.n_rows(), t.n_cols()), (4, 5));
    assert_eq!(t.holes(), 0);
    assert_eq!(text(t, 1, 0), "Method"); // row span
    assert_eq!(text(t, 0, 2), "FB15k-237"); // column span
    assert_eq!(text(t, 0, 3), "WN18RR");
    assert_eq!(text(t, 1, 4), "MRR");
}

#[test]
fn cell_text_flags_and_citations() {
    let d = doc();
    let t = table(&d, "S4.T1");
    let g = t.grid();
    let transe = g[2][0].unwrap();
    assert_eq!(transe.text, "TransE");
    assert_eq!(transe.cites, ["3"]);
    assert!(g[0][1].unwrap().header && !transe.header);
    assert!(g[0][1].unwrap().bold);
    assert!(g[3][1].unwrap().bold && !g[2][1].unwrap().bold);
    assert_eq!(text(t, 3, 0), "Ours (large)"); // nested tabular, one cell
    assert_eq!(text(t, 3, 2), "0.338\\pm 0.002"); // math as its LaTeX source
    assert_eq!(text(t, 3, 3), "-");
    assert_eq!(text(t, 2, 2), ".294");
}

#[test]
fn span_markup_with_spans_as_classes() {
    let d = doc();
    let t = table(&d, "S4.T2");
    assert_eq!((t.n_rows(), t.n_cols(), t.holes()), (3, 3, 0));
    assert_eq!(text(t, 0, 1), "Model"); // ltx_colspan_2
    assert_eq!(text(t, 2, 0), "Path-based"); // ltx_rowspan_2
    assert_eq!(text(t, 2, 1), "DRUM");
    assert_eq!(text(t, 2, 2), "0.343");
    assert!(t.grid()[0][2].unwrap().header);
}

#[test]
fn several_tables_in_one_figure_share_its_anchor() {
    let d = doc();
    let right = table(&d, "S4.T3.t2");
    assert_eq!(right.figure_id, "S4.T3");
    assert_eq!(text(right, 0, 0), "right");
    assert_eq!(
        d.table_url(right),
        "https://arxiv.org/html/2401.01234v3#S4.T3"
    );
}

#[test]
fn conversion_errors_are_flagged() {
    let d = doc();
    assert!(table(&d, "S4.T5").conversion_errors);
    assert!(!table(&d, "S4.T1").conversion_errors);
}

#[test]
fn sections_keep_paragraph_text_without_footnotes() {
    let d = doc();
    let headings: Vec<&str> = d.sections.iter().map(|s| s.heading.as_str()).collect();
    assert_eq!(
        headings,
        [
            "Abstract",
            "1 Introduction",
            "4 Experiments",
            "4.1 Main results"
        ]
    );
    assert_eq!(
        d.sections[0].paragraphs[0].text,
        "We rank entities with a model of dimension d."
    );
    let intro = &d.sections[1].paragraphs[0];
    assert_eq!(intro.id, "S1.p1");
    assert_eq!(intro.text, "Prior work [3] reports filtered metrics.1");
    assert!(d.sections[2].paragraphs.is_empty()); // its paragraph belongs to 4.1
    assert_eq!(
        d.sections[3].paragraphs[0].text,
        "Table 1 shows the results."
    );
}

#[test]
fn page_metadata() {
    assert_eq!(stated_version(FIXTURE, "2401.01234"), Some(3));
    assert_eq!(stated_version(FIXTURE, "2401.99999"), None);
    assert_eq!(
        generator(FIXTURE).as_deref(),
        Some("LaTeXML oxide (version 0.7.6)")
    );
    assert_eq!(generator("<html>not latexml</html>"), None);
}

#[test]
fn garbage_input_gives_an_empty_document() {
    let d = parse("<html><p>nothing</p>", "2401.01234", 1);
    assert!(d.tables.is_empty() && d.sections.is_empty() && d.title.is_empty());
}
