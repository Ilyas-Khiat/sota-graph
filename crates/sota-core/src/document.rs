//! The document model both input channels produce: sections of text and
//! tables of cells, every cell addressable by a locator.

/// Where a document came from. Trust differs: HTML tables are parsed by code,
/// PDF tables are read by a model and checked against the page text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    ArxivHtml,
    Pdf,
}

#[derive(Debug, Clone)]
pub struct Document {
    pub arxiv_id: String,
    pub version: u32,
    pub channel: Channel,
    pub title: String,
    pub sections: Vec<Section>,
    pub tables: Vec<Table>,
    /// Table figures that held no parseable table (e.g. a table shipped as an image).
    pub unparsed_tables: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Section {
    pub id: String,
    pub heading: String,
    pub paragraphs: Vec<Paragraph>,
}

#[derive(Debug, Clone)]
pub struct Paragraph {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Table {
    /// Unique within the document: the figure id, suffixed `.tN` when a figure holds several tables.
    pub id: String,
    /// The anchor of the enclosing figure, used in URLs.
    pub figure_id: String,
    /// "Table 5".
    pub label: String,
    /// Verbatim caption, label excluded.
    pub caption: String,
    pub cells: Vec<Cell>,
    /// The converter marked part of this table as failed: its cells deserve less trust.
    pub conversion_errors: bool,
}

/// One cell at its logical position: `row`/`col` already account for the
/// spans of cells above and to the left.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub row: usize,
    pub col: usize,
    pub row_span: usize,
    pub col_span: usize,
    /// Visible text with whitespace collapsed, math as its LaTeX source,
    /// citation markers removed (they are in `cites`).
    pub text: String,
    pub bold: bool,
    pub header: bool,
    /// Citation markers found in the cell, e.g. "6" or "Bordes et al., 2013".
    pub cites: Vec<String>,
}

impl Document {
    /// `2106.06935v4`.
    pub fn versioned_id(&self) -> String {
        format!("{}v{}", self.arxiv_id, self.version)
    }

    /// `2106.06935v4#S4.T3/r5/c3`: one cell, stable across re-parses of the same version.
    pub fn locator(&self, table: &Table, row: usize, col: usize) -> String {
        format!("{}#{}/r{row}/c{col}", self.versioned_id(), table.id)
    }

    /// A link a human can open, scrolled to the table.
    pub fn table_url(&self, table: &Table) -> String {
        match self.channel {
            Channel::ArxivHtml | Channel::Pdf => format!(
                "https://arxiv.org/html/{}#{}",
                self.versioned_id(),
                table.figure_id
            ),
        }
    }

    pub fn table(&self, id: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.id == id)
    }
}

impl Table {
    pub fn n_rows(&self) -> usize {
        self.cells
            .iter()
            .map(|c| c.row + c.row_span)
            .max()
            .unwrap_or(0)
    }

    pub fn n_cols(&self) -> usize {
        self.cells
            .iter()
            .map(|c| c.col + c.col_span)
            .max()
            .unwrap_or(0)
    }

    /// Row-major grid where every slot points to the cell covering it.
    pub fn grid(&self) -> Vec<Vec<Option<&Cell>>> {
        let mut grid = vec![vec![None; self.n_cols()]; self.n_rows()];
        for cell in &self.cells {
            for row in grid.iter_mut().skip(cell.row).take(cell.row_span) {
                for slot in row.iter_mut().skip(cell.col).take(cell.col_span) {
                    *slot = Some(cell);
                }
            }
        }
        grid
    }

    /// Slots no cell covers. A well-parsed table has none.
    pub fn holes(&self) -> usize {
        self.grid().iter().flatten().filter(|s| s.is_none()).count()
    }

    /// Each cell's text at its origin slot; slots covered by a span stay empty.
    /// The first row is used as the Markdown header row.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        for (r, row) in self.grid().iter().enumerate() {
            let texts: Vec<String> = row
                .iter()
                .enumerate()
                .map(|(c, slot)| match slot {
                    Some(cell) if cell.row == r && cell.col == c => {
                        let text = cell.text.replace('|', "\\|");
                        if cell.bold && !text.is_empty() {
                            format!("**{text}**")
                        } else {
                            text
                        }
                    }
                    _ => String::new(),
                })
                .collect();
            out.push_str(&format!("| {} |\n", texts.join(" | ")));
            if r == 0 {
                out.push_str(&format!("|{}\n", " --- |".repeat(row.len())));
            }
        }
        out
    }
}
