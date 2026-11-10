//! Core of sota-graph: domain types, ontology and registry loading, and the
//! comparison engine. Pure functions only: no network, no database, no LLM,
//! no file system. Callers hand in config text; this crate validates it.

mod error;
mod ident;
mod normalize;
mod ontology;
mod registry;

pub use error::ConfigError;
pub use normalize::Normalizer;
pub use ontology::{NodeType, Ontology, Property, PropertyType, RelationshipType};
pub use registry::{Dataset, Direction, Metric, Registry, Scale, Setting, SettingValue};
