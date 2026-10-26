//! Core Data Models
//!
//! Defines strongly typed representations for paths, spans, names,
//! value classifications, evidence kinds, definitions, and references.

pub mod definition;
pub mod evidence;
pub mod name;
pub mod path;
pub mod reference;
pub mod span;
pub mod value;

pub use definition::Definition;
pub use evidence::EvidenceKind;
pub use name::EnvName;
pub use path::RelPath;
pub use reference::Reference;
pub use span::{Location, Span};
pub use value::ValueKind;
