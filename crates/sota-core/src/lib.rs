//! Core of sota-graph: domain types, ontology and registry loading, and the
//! comparison engine. Pure functions only: no network, no database, no LLM,
//! no file system. Callers hand in config text; this crate validates it.

mod arxiv;
mod document;
mod error;
mod gold;
mod ident;
mod normalize;
mod ontology;
mod registry;
mod value;

pub use arxiv::{ArxivId, ArxivIdError};
pub use document::{Cell, Channel, Document, Paragraph, Section, Table};
pub use error::ConfigError;
pub use gold::{
    CellScale, DatasetIn, Expansion, GoldColumn, GoldFile, GoldResult, GoldRow, GoldTable,
    Orientation, ReviewCell, Role,
};
pub use normalize::Normalizer;
pub use ontology::{NodeType, Ontology, Property, PropertyType, RelationshipType};
pub use registry::{Dataset, Direction, Metric, Registry, Scale, Setting, SettingValue};
pub use value::{ParsedValue, ValueError, is_missing, parse_number, to_canonical};
