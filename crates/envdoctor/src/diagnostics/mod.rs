//! Diagnostic codes, severities, report models, and deterministic sorting.

pub mod code;
pub mod diagnostic;
pub mod report;
pub mod severity;
pub mod summary;

pub use code::DiagnosticCode;
pub use diagnostic::Diagnostic;
pub use report::Report;
pub use severity::Severity;
pub use summary::Summary;
