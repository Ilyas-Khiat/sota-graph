//! Core of sota-graph: domain types, ontology and registry loading, and the
//! comparison engine. Pure functions only: no network, no database, no LLM,
//! no file system. Callers hand in config text; this crate validates it.

mod arxiv;
mod compare;
mod document;
mod error;
mod eval;
mod extract;
mod gold;
mod ident;
mod methods;
mod normalize;
mod ontology;
mod registry;
mod value;

pub use arxiv::{ArxivId, ArxivIdError};
pub use compare::{Entry, Key, Leaderboard, Report, leaderboards, values_agree};
pub use document::{Cell, Channel, Document, Paragraph, Section, Table};
pub use error::ConfigError;
pub use eval::{Score, score_extraction};
pub use extract::{
    ColumnClaim, ReviewBy, ReviewClaim, ReviewItem, RowClaim, SettingClaim, TableMapping,
    TableOutcome, mentions_value, method_slug, verify_mapping,
};
pub use gold::{
    CellScale, DatasetIn, Expansion, GoldColumn, GoldFile, GoldResult, GoldRow, GoldTable,
    Orientation, Problem, ReviewCell, Role, Scope,
};
pub use methods::{MethodEntry, MethodRef, MethodRegistry};
pub use normalize::{Normalizer, clean_word};
pub use ontology::{NodeType, Ontology, Property, PropertyType, RelationshipType};
pub use registry::{Dataset, Direction, Metric, Registry, Scale, Setting, SettingValue};
pub use value::{ParsedValue, ValueError, is_missing, parse_number, to_canonical};
